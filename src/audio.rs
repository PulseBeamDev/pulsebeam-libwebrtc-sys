//! Owned, validated CPU audio frames for injected media.
//!
//! The initial input format is interleaved signed 16-bit PCM in 10 ms blocks.
//! Unsupported rates, channels, or partial blocks are rejected before crossing
//! the native bridge. Source-specific processing and device APIs are separate.

use std::{cell::Cell, fmt, rc::Rc};

use crate::{
    CodecParameter, ffi,
    peer::{FactoryInner, PeerError, PeerErrorKind, PeerInner},
};

/// An audio RTP capability reported by the selected peer factory. Opus uses
/// SDP channel count 2 even for mono; its `stereo` fmtp selects send channels.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioCodecCapability {
    name: String,
    parameters: Vec<CodecParameter>,
    clock_rate: Option<u32>,
    channels: Option<u8>,
}

impl AudioCodecCapability {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn parameters(&self) -> &[CodecParameter] {
        &self.parameters
    }
    pub fn clock_rate(&self) -> Option<u32> {
        self.clock_rate
    }
    pub fn channels(&self) -> Option<u8> {
        self.channels
    }
    pub(crate) fn from_ffi(value: ffi::FfiAudioCodecCapability) -> Self {
        Self {
            name: value.format.name,
            parameters: value
                .format
                .parameters
                .into_iter()
                .map(|item| CodecParameter {
                    key: item.key,
                    value: item.value,
                })
                .collect(),
            clock_rate: u32::try_from(value.clock_rate).ok(),
            channels: u8::try_from(value.channels).ok(),
        }
    }
    pub(crate) fn ffi_format(&self) -> ffi::FfiCodecFormat {
        ffi::FfiCodecFormat {
            name: self.name.clone(),
            scalability_modes: Vec::new(),
            parameters: self
                .parameters
                .iter()
                .map(|item| ffi::FfiCodecParameter {
                    key: item.key.clone(),
                    value: item.value.clone(),
                })
                .collect(),
        }
    }
}

/// A platform audio device reported by the native artifact. Device lists can
/// change; re-enumerate before using an index, and handle selection failure.
#[cfg(feature = "native")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioDevice {
    pub index: u16,
    pub name: String,
    pub id: String,
}

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

/// Owned decoded PCM with its recipient and receiver identity. `observed_at`
/// is the controlled timeline at the explicit playout/poll invocation, not a
/// wall-clock profiling value. Capture timing remains in `frame` when supplied.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceivedAudioFrame {
    pub peer_id: Option<u64>,
    pub receiver_id: String,
    pub observed_at: Option<std::time::Duration>,
    pub frame: DecodedAudioFrame,
}

/// A bounded decoded PCM receiver queue. Closing removes the native sink on
/// the signaling thread before releasing callback memory. Dropped frames are
/// accounted for; at most eight blocks are retained. On headless peers,
/// `try_next_frame` pulls one 10 ms playout block when the queue is empty;
/// it does not open an operating-system audio output device.
pub struct AudioSink {
    native: cxx::UniquePtr<ffi::NativeAudioSink>,
    _peer: Rc<PeerInner>,
    receiver_id: String,
}

impl AudioSink {
    pub(crate) fn from_native(
        native: cxx::UniquePtr<ffi::NativeAudioSink>,
        peer: Rc<PeerInner>,
        receiver_id: String,
    ) -> Self {
        Self {
            native,
            _peer: peer,
            receiver_id,
        }
    }

    pub fn try_next_received_frame(&self) -> Option<ReceivedAudioFrame> {
        self.try_next_frame().map(|frame| ReceivedAudioFrame {
            peer_id: self._peer.controlled_id,
            receiver_id: self.receiver_id.clone(),
            observed_at: self._peer.controlled_time(),
            frame,
        })
    }

    pub fn try_next_frame(&self) -> Option<DecodedAudioFrame> {
        if self._peer.closed.get() {
            return None;
        }
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

    /// Detach on the signaling thread and release the receiver reservation.
    /// Either audio receive mode may then be attached again. Repeated calls
    /// and dropping this closed handle do not affect a replacement sink.
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

/// A complete encoded Opus packet after RTP depacketization. `data` holds
/// exactly the Opus packet (RFC 6716), without RTP headers. Duration is
/// `samples_per_channel` at the 48 kHz Opus clock. A negotiated RED or
/// non-Opus payload is not exposed as an Opus packet.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedAudioFrame {
    pub data: Vec<u8>,
    pub rtp_timestamp: u32,
    pub ssrc: u32,
    pub payload_type: u8,
    /// Sequence number supplied by the native audio-frame callback, when known.
    pub sequence_number: Option<u16>,
    /// Native received RFC6464 level in -dBov, from 0 to 127. Missing means the
    /// extension was not present.
    pub audio_level_dbov: Option<u8>,
    /// Received RFC6464 V bit, when the audio-level extension is present.
    /// The pinned native incoming frame projects this bit through its `Type`
    /// getter. This is not inferred from PCM or codec speech classification.
    pub voice_activity: Option<bool>,
    pub samples_per_channel: u32,
    pub capture_time_us: Option<i64>,
    pub receive_time_us: Option<i64>,
}

/// Bounded, decoder-free Opus receiver. Attaching this sink selects encoded
/// delivery for its receiver and excludes simultaneous decoded delivery.
pub struct EncodedAudioSink {
    native: cxx::UniquePtr<ffi::NativeEncodedAudioSink>,
    _peer: Rc<PeerInner>,
}

impl EncodedAudioSink {
    pub(crate) fn from_native(
        native: cxx::UniquePtr<ffi::NativeEncodedAudioSink>,
        peer: Rc<PeerInner>,
    ) -> Self {
        Self {
            native,
            _peer: peer,
        }
    }

    pub fn try_next_frame(&self) -> Option<EncodedAudioFrame> {
        if self._peer.closed.get() {
            return None;
        }
        let frame = ffi::encoded_audio_sink_take_frame(self.native());
        frame.available.then_some(EncodedAudioFrame {
            data: frame.data,
            rtp_timestamp: frame.rtp_timestamp,
            ssrc: frame.ssrc,
            payload_type: frame.payload_type,
            sequence_number: frame.has_sequence_number.then_some(frame.sequence_number),
            audio_level_dbov: frame.has_audio_level.then_some(frame.audio_level_dbov),
            voice_activity: frame.has_voice_activity.then_some(frame.voice_activity),
            samples_per_channel: frame.samples_per_channel,
            capture_time_us: frame.has_capture_time.then_some(frame.capture_time_us),
            receive_time_us: frame.has_receive_time.then_some(frame.receive_time_us),
        })
    }

    pub fn dropped_frames(&self) -> u64 {
        ffi::encoded_audio_sink_dropped_frames(self.native())
    }

    /// Stop encoded delivery, discard queued output, and restore native
    /// decoding for future packets. Either receive mode may be attached again.
    /// Stop reception before closing if opaque packets must never be decoded.
    /// Idempotent; dropping this closed handle does not close a replacement.
    pub fn close(&mut self) -> Result<(), PeerError> {
        ffi::close_encoded_audio_sink(self.native())
            .then_some(())
            .ok_or_else(|| PeerError {
                kind: PeerErrorKind::Internal,
                message: "failed to close encoded audio sink".into(),
            })
    }

    fn native(&self) -> &ffi::NativeEncodedAudioSink {
        self.native.as_ref().expect("validated encoded audio sink")
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

/// One RFC 6716 mono Opus packet, without RTP headers. The RTP clock is
/// 48 kHz; supported packet durations are 10, 20, 30, 40, 50, or 60 ms.
/// The packet's TOC duration must agree with `samples_per_channel`.
/// The payload is at most 1200 bytes and must fit in 960 bytes per 10 ms
/// slot, less 36 internal envelope bytes per slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpusInputFrame {
    pub data: Vec<u8>,
    pub rtp_timestamp: u32,
    pub samples_per_channel: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpusInputError {
    InvalidPacket,
    InvalidDuration,
    InvalidTimestamp,
    InvalidAudioLevel,
    Backpressure,
    Released,
}
impl fmt::Display for OpusInputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for OpusInputError {}

/// Explicit producer RFC6464 level and voice-activity bit, not derived from
/// packet bytes, carrier PCM, or decoder speech classification. The native
/// sender serializes these only when the audio-level extension is negotiated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpusAudioLevel {
    level_dbov: u8,
    voice_activity: bool,
}
impl OpusAudioLevel {
    /// Level is the positive magnitude in -dBov: 0 is full scale, 127 silence.
    /// Voice activity is independently declared, not inferred from this value.
    pub fn new(level_dbov: u8, voice_activity: bool) -> Result<Self, OpusInputError> {
        if level_dbov > 127 {
            return Err(OpusInputError::InvalidAudioLevel);
        }
        Ok(Self {
            level_dbov,
            voice_activity,
        })
    }

    pub fn level_dbov(self) -> u8 {
        self.level_dbov
    }
    pub fn voice_activity(self) -> bool {
        self.voice_activity
    }
}

/// A sequence-bound Opus input source. Create its factory with
/// [`crate::AudioEncoderFactory::with_opus_frames`]. Frames bypass Opus
/// encoding; each source carries its own bytes through a 48 kHz bridge
/// matching its configured mono or stereo format.
/// The factory accepts Opus-only tracks, not raw PCM tracks. At most 24
/// in-flight 10 ms input copies are admitted per source, counting every attached
/// sender. Retry on `Backpressure`. Detachment does not count as consumption;
/// if native input is discarded before encoding, capacity is conservatively
/// retained until source close. No frame is queued without an attached sender.
/// ```compile_fail
/// fn assert_send<T: Send>() {}
/// assert_send::<pulsebeam_webrtc_sys::EncodedAudioSource>();
/// ```
pub struct EncodedAudioSource {
    source: AudioSource,
    channels: u8,
    last_timestamp: Cell<Option<u32>>,
}
impl EncodedAudioSource {
    pub(crate) fn from_source(source: AudioSource, channels: u8) -> Self {
        Self {
            source,
            channels,
            last_timestamp: Cell::new(None),
        }
    }

    pub fn channels(&self) -> u8 {
        self.channels
    }

    /// Supply a packet with capture timing on the controlled world's timeline.
    /// Capture metadata is forwarded at millisecond precision; RTP timestamps
    /// remain caller-supplied 48 kHz ticks. Ordinary production clocks do not
    /// provide this mapping and are rejected by this explicit-timing API.
    pub fn push_opus_at(
        &self,
        frame: &OpusInputFrame,
        capture_time: std::time::Duration,
    ) -> Result<(), OpusInputError> {
        if !self.source.inner._factory.controlled_media {
            return Err(OpusInputError::InvalidTimestamp);
        }
        self.push_opus_with_time(frame, Some(capture_time), None)
    }

    /// In the controlled media profile, use the current world time for capture.
    /// Otherwise preserve the original timestamp-free production carrier API.
    pub fn push_opus(&self, frame: &OpusInputFrame) -> Result<(), OpusInputError> {
        let capture_time = self.source.inner._factory.controlled_media.then(|| {
            self.source
                .inner
                ._factory
                .controlled_time()
                .expect("controlled media clock")
        });
        self.push_opus_with_time(frame, capture_time, None)
    }

    /// Submit an independent producer level/V pair alongside an unchanged Opus
    /// packet. Native encoder speech and the supported outgoing level setter
    /// carry it to native RTP extension serialization. Capture timing follows
    /// the same rules as `push_opus`. Missing negotiation produces no extension.
    pub fn push_opus_with_audio_level(
        &self,
        frame: &OpusInputFrame,
        level: OpusAudioLevel,
    ) -> Result<(), OpusInputError> {
        let capture_time = self.source.inner._factory.controlled_media.then(|| {
            self.source
                .inner
                ._factory
                .controlled_time()
                .expect("controlled media clock")
        });
        self.push_opus_with_time(frame, capture_time, Some(level))
    }

    /// Combine explicit controlled capture timing with a declared level/V pair.
    /// Ordinary production clock domains remain unsupported by this timing API.
    pub fn push_opus_at_with_audio_level(
        &self,
        frame: &OpusInputFrame,
        capture_time: std::time::Duration,
        level: OpusAudioLevel,
    ) -> Result<(), OpusInputError> {
        if !self.source.inner._factory.controlled_media {
            return Err(OpusInputError::InvalidTimestamp);
        }
        self.push_opus_with_time(frame, Some(capture_time), Some(level))
    }

    fn push_opus_with_time(
        &self,
        frame: &OpusInputFrame,
        capture_time: Option<std::time::Duration>,
        audio_level: Option<OpusAudioLevel>,
    ) -> Result<(), OpusInputError> {
        if self.source.inner.closed.get() {
            return Err(OpusInputError::Released);
        }
        let bytes = &frame.data;
        if bytes.is_empty() || bytes.len() > 1200 || (bytes[0] & 4 != 0) != (self.channels == 2) {
            return Err(OpusInputError::InvalidPacket);
        }
        let config = bytes[0] >> 3;
        let count = match bytes[0] & 3 {
            0 => 1,
            3 => bytes.get(1).map(|b| b & 63).unwrap_or(0) as u32,
            _ => 2,
        };
        let samples = (if config < 12 {
            if config & 3 == 3 {
                2880
            } else {
                480u32 << (config & 3)
            }
        } else if config < 16 {
            480u32 << (config & 1)
        } else {
            120u32 << (config & 3)
        }) * count;
        if samples != frame.samples_per_channel
            || !matches!(samples, 480 | 960 | 1440 | 1920 | 2400 | 2880)
            || bytes.len() > (samples as usize / 480) * (960 * self.channels as usize - 36)
        {
            return Err(OpusInputError::InvalidDuration);
        }
        if let Some(last) = self.last_timestamp.get()
            && (frame.rtp_timestamp.wrapping_sub(last) == 0
                || frame.rtp_timestamp.wrapping_sub(last) >= (1 << 31))
        {
            return Err(OpusInputError::InvalidTimestamp);
        }
        let capture_us = if let Some(time) = capture_time {
            let duration = std::time::Duration::from_micros(
                u64::from(frame.samples_per_channel) * 1_000_000 / 48_000,
            );
            if time
                .checked_add(duration)
                .is_none_or(|end| end > crate::MAX_CONTROLLED_TIME)
            {
                return Err(OpusInputError::InvalidTimestamp);
            }
            i64::try_from(time.as_micros()).map_err(|_| OpusInputError::InvalidTimestamp)?
        } else {
            -1
        };
        if !ffi::audio_source_push_opus_with_level_at(
            self.source.native(),
            bytes,
            frame.rtp_timestamp,
            frame.samples_per_channel,
            capture_us,
            audio_level.is_some(),
            audio_level.map_or(0, OpusAudioLevel::level_dbov),
            audio_level.is_some_and(OpusAudioLevel::voice_activity),
        ) {
            return Err(OpusInputError::Backpressure);
        }
        self.last_timestamp.set(Some(frame.rtp_timestamp));
        Ok(())
    }

    pub fn close(&mut self) -> Result<(), AudioFrameError> {
        self.source.close()
    }

    pub(crate) fn source(&self) -> &AudioSource {
        &self.source
    }
}

/// A sequence-bound local audio track retaining its source and factory.
#[derive(Clone)]
pub struct AudioTrack {
    inner: Rc<TrackInner>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AudioInputKind {
    Pcm,
    OpusFrames,
}

struct TrackInner {
    native: cxx::UniquePtr<ffi::NativeAudioTrack>,
    _source: Option<Rc<SourceInner>>,
    _factory: Rc<FactoryInner>,
    input_kind: AudioInputKind,
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
                _source: Some(source.inner.clone()),
                _factory: factory,
                input_kind: AudioInputKind::Pcm,
            }),
        }
    }

    #[cfg(feature = "native")]
    pub(crate) fn microphone(
        native: cxx::UniquePtr<ffi::NativeAudioTrack>,
        factory: Rc<FactoryInner>,
    ) -> Self {
        Self {
            inner: Rc::new(TrackInner {
                native,
                _source: None,
                _factory: factory,
                input_kind: AudioInputKind::Pcm,
            }),
        }
    }

    pub(crate) fn mark_opus_frames(mut self) -> Self {
        Rc::get_mut(&mut self.inner)
            .expect("new audio track has one owner")
            .input_kind = AudioInputKind::OpusFrames;
        self
    }

    pub(crate) fn is_opus_frames(&self) -> bool {
        self.inner.input_kind == AudioInputKind::OpusFrames
    }

    /// Request upstream AEC, NS and AGC choices for a local PCM track. The
    /// engine is factory-wide: requests from different tracks can conflict.
    /// A stored request does not prove an effect is active; inspect the
    /// factory's `audio_processing_state` after starting capture.
    pub fn set_audio_processing_options(
        &self,
        options: crate::AudioProcessingOptions,
    ) -> Result<(), crate::PeerError> {
        if self.is_opus_frames() {
            return Err(crate::PeerError {
                kind: crate::PeerErrorKind::InvalidParameter,
                message: "encoded Opus bypasses PCM audio processing".into(),
            });
        }
        let mut error = String::new();
        if !ffi::audio_track_set_processing_options(
            self.native(),
            options.echo_cancellation as u8,
            options.noise_suppression as u8,
            options.gain_control as u8,
            &mut error,
        ) {
            return Err(crate::PeerError {
                kind: crate::PeerErrorKind::InvalidParameter,
                message: error,
            });
        }
        Ok(())
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
