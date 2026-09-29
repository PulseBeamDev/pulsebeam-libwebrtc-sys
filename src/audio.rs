//! Owned, validated CPU audio frames for injected media.
//!
//! The initial input format is interleaved signed 16-bit PCM in 10 ms blocks.
//! Unsupported rates, channels, or partial blocks are rejected before crossing
//! the native bridge. Source-specific processing and device APIs are separate.

use std::{cell::Cell, fmt, rc::Rc};

use crate::{
    ffi,
    peer::{FactoryInner, PeerError, PeerErrorKind, PeerInner},
};

/// The layout and representation of a CPU audio frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioSampleFormat {
    /// Signed 16-bit, native-endian, interleaved samples.
    I16Interleaved,
}

/// An invalid or unsupported PCM frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioFrameError {
    InvalidSampleRate,
    InvalidChannels,
    InvalidSampleCount,
    InvalidTimestamp,
    Released,
}

impl fmt::Display for AudioFrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for AudioFrameError {}

/// Owned, timestamped PCM ready for injection into an audio source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioPcmFrame {
    pub format: AudioSampleFormat,
    pub sample_rate_hz: u32,
    pub channels: u8,
    pub samples_per_channel: u32,
    /// Caller capture timestamp in microseconds. The current raw source does
    /// not forward this value as an absolute capture clock or RTP timestamp.
    pub timestamp_us: i64,
    pub samples: Vec<i16>,
}

impl AudioPcmFrame {
    /// Constructs one 10 ms PCM block. Supported rates are 8, 16, 32, and
    /// 48 kHz; mono and stereo are supported. No padding or truncation occurs.
    pub fn i16_interleaved(
        sample_rate_hz: u32,
        channels: u8,
        timestamp_us: i64,
        samples: Vec<i16>,
    ) -> Result<Self, AudioFrameError> {
        let samples_per_channel =
            Self::validate(sample_rate_hz, channels, timestamp_us, samples.len())?;
        Ok(Self {
            format: AudioSampleFormat::I16Interleaved,
            sample_rate_hz,
            channels,
            samples_per_channel,
            timestamp_us,
            samples,
        })
    }

    fn validate(
        sample_rate_hz: u32,
        channels: u8,
        timestamp_us: i64,
        sample_count: usize,
    ) -> Result<u32, AudioFrameError> {
        if !matches!(sample_rate_hz, 8_000 | 16_000 | 32_000 | 48_000) {
            return Err(AudioFrameError::InvalidSampleRate);
        }
        if !matches!(channels, 1 | 2) {
            return Err(AudioFrameError::InvalidChannels);
        }
        if timestamp_us < 0 {
            return Err(AudioFrameError::InvalidTimestamp);
        }
        let samples_per_channel = sample_rate_hz / 100;
        if sample_count != samples_per_channel as usize * channels as usize {
            return Err(AudioFrameError::InvalidSampleCount);
        }
        Ok(samples_per_channel)
    }
}

/// A decoded audio block received from an RTP audio track.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedAudioFrame {
    pub sample_rate_hz: u32,
    pub channels: u8,
    pub samples_per_channel: u32,
    /// Absolute capture time on the upstream TimeMillis clock, if supplied.
    pub capture_time_us: Option<i64>,
    /// Signed 16-bit, native-endian, interleaved samples.
    pub samples: Vec<i16>,
}

/// A bounded decoded PCM receiver queue. Closing removes the native sink on
/// the signaling thread before releasing callback memory. Dropped frames are
/// accounted for; at most eight blocks are retained. On headless peers,
/// `try_next_frame` pulls one 10 ms playout block when the queue is empty;
/// it does not open an operating-system audio output device.
pub struct AudioSink {
    native: cxx::UniquePtr<ffi::NativeAudioSink>,
    _peer: Rc<PeerInner>,
}

impl AudioSink {
    pub(crate) fn from_native(
        native: cxx::UniquePtr<ffi::NativeAudioSink>,
        peer: Rc<PeerInner>,
    ) -> Self {
        Self {
            native,
            _peer: peer,
        }
    }

    pub fn try_next_frame(&self) -> Option<DecodedAudioFrame> {
        let frame = ffi::audio_sink_take_frame(self.native());
        frame.valid.then_some(DecodedAudioFrame {
            sample_rate_hz: frame.sample_rate_hz,
            channels: frame.channels,
            samples_per_channel: frame.samples_per_channel,
            capture_time_us: frame.has_capture_time.then_some(frame.capture_time_us),
            samples: frame.samples,
        })
    }

    pub fn dropped_frames(&self) -> u64 {
        ffi::audio_sink_dropped_frames(self.native())
    }

    /// Detach on the signaling thread. Repeated calls succeed.
    pub fn close(&mut self) -> Result<(), PeerError> {
        ffi::close_audio_sink(self.native())
            .then_some(())
            .ok_or_else(|| PeerError {
                kind: PeerErrorKind::Internal,
                message: "failed to detach audio sink".into(),
            })
    }

    fn native(&self) -> &ffi::NativeAudioSink {
        self.native.as_ref().expect("validated audio sink")
    }
}

/// A caller-fed, sequence-bound raw audio source.
///
/// ```compile_fail
/// fn assert_send<T: Send>() {}
/// assert_send::<pulsebeam_webrtc_sys::AudioSource>();
/// ```
pub struct AudioSource {
    inner: Rc<SourceInner>,
}

struct SourceInner {
    native: cxx::UniquePtr<ffi::NativeAudioSource>,
    _factory: Rc<FactoryInner>,
    closed: Cell<bool>,
}

impl AudioSource {
    pub(crate) fn from_native(
        native: cxx::UniquePtr<ffi::NativeAudioSource>,
        factory: Rc<FactoryInner>,
    ) -> Self {
        Self {
            inner: Rc::new(SourceInner {
                native,
                _factory: factory,
                closed: Cell::new(false),
            }),
        }
    }

    pub fn push_frame(&self, frame: &AudioPcmFrame) -> Result<(), AudioFrameError> {
        if self.inner.closed.get() {
            return Err(AudioFrameError::Released);
        }
        AudioPcmFrame::validate(
            frame.sample_rate_hz,
            frame.channels,
            frame.timestamp_us,
            frame.samples.len(),
        )?;
        ffi::audio_source_push_pcm(
            self.native(),
            &frame.samples,
            frame.sample_rate_hz,
            frame.channels,
            frame.timestamp_us,
        )
        .then_some(())
        .ok_or(AudioFrameError::Released)
    }

    pub fn close(&mut self) -> Result<(), AudioFrameError> {
        if !self.inner.closed.replace(true) && !ffi::close_audio_source(self.native()) {
            return Err(AudioFrameError::Released);
        }
        Ok(())
    }

    pub(crate) fn belongs_to(&self, factory: &Rc<FactoryInner>) -> bool {
        Rc::ptr_eq(&self.inner._factory, factory)
    }

    pub(crate) fn native(&self) -> &ffi::NativeAudioSource {
        self.inner.native.as_ref().expect("validated audio source")
    }
}

/// A sequence-bound local audio track retaining its source and factory.
#[derive(Clone)]
pub struct AudioTrack {
    inner: Rc<TrackInner>,
}

struct TrackInner {
    native: cxx::UniquePtr<ffi::NativeAudioTrack>,
    _source: Rc<SourceInner>,
    _factory: Rc<FactoryInner>,
}

impl AudioTrack {
    pub(crate) fn local(
        native: cxx::UniquePtr<ffi::NativeAudioTrack>,
        factory: Rc<FactoryInner>,
        source: &AudioSource,
    ) -> Self {
        Self {
            inner: Rc::new(TrackInner {
                native,
                _source: source.inner.clone(),
                _factory: factory,
            }),
        }
    }

    pub fn id(&self) -> String {
        ffi::audio_track_id(self.native())
    }

    pub fn enabled(&self) -> bool {
        ffi::audio_track_enabled(self.native())
    }

    pub fn set_enabled(&self, enabled: bool) -> bool {
        ffi::audio_track_set_enabled(self.native(), enabled)
    }

    pub(crate) fn belongs_to(&self, factory: &Rc<FactoryInner>) -> bool {
        Rc::ptr_eq(&self.inner._factory, factory)
    }

    pub(crate) fn native(&self) -> &ffi::NativeAudioTrack {
        self.inner.native.as_ref().expect("validated audio track")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_frame_requires_exact_ten_millisecond_interleaved_block() {
        for rate in [8_000, 16_000, 32_000, 48_000] {
            for channels in [1, 2] {
                let data = vec![0; (rate / 100 * channels as u32) as usize];
                let frame =
                    AudioPcmFrame::i16_interleaved(rate, channels, 42, data.clone()).unwrap();
                assert_eq!(frame.samples_per_channel, rate / 100);
                assert_eq!(frame.samples, data);
            }
        }
        assert_eq!(
            AudioPcmFrame::i16_interleaved(44_100, 1, 0, vec![]),
            Err(AudioFrameError::InvalidSampleRate)
        );
        assert_eq!(
            AudioPcmFrame::i16_interleaved(48_000, 3, 0, vec![]),
            Err(AudioFrameError::InvalidChannels)
        );
        assert_eq!(
            AudioPcmFrame::i16_interleaved(48_000, 1, -1, vec![0; 480]),
            Err(AudioFrameError::InvalidTimestamp)
        );
        assert_eq!(
            AudioPcmFrame::i16_interleaved(48_000, 1, 0, vec![0; 479]),
            Err(AudioFrameError::InvalidSampleCount)
        );
        assert_eq!(
            AudioPcmFrame::i16_interleaved(48_000, 1, 0, vec![0; 481]),
            Err(AudioFrameError::InvalidSampleCount)
        );
    }
}
