//! Direct H.264 Annex-B input using an internal encoder adapter. No caller
//! codec implementation is required for sending encoded access units.

use std::{
    cell::Cell,
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use crate::{
    CodecError, CodecSupport, EncodedImageCallback, EncodedVideoFrame, PeerConnectionFactory,
    PeerError, PeerErrorKind, VideoCodecFormat, VideoEncoder, VideoEncoderFactory,
    VideoEncoderFactoryHandle, VideoEncoderInfo, VideoEncoderSettings, VideoFrame, VideoFrameType,
    VideoRateControl, VideoResolution, VideoSource, VideoTrack, ffi,
};

const MAX_PENDING: usize = 16;
const MAX_PENDING_BYTES: usize = 8 * 1024 * 1024;

/// One encoded H.264 access unit, represented as start-code-delimited Annex-B
/// NAL units. Timestamps use caller monotonic microseconds; WebRTC assigns the
/// wire RTP timestamp from its capture clock, rather than trusting an input RTP
/// clock. Each source is a separate stream.
#[derive(Clone, Debug)]
pub struct H264AccessUnit {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub timestamp_us: i64,
    pub key_frame: bool,
    pub qp: Option<u8>,
}

#[derive(Default)]
struct SourceFeedback {
    keyframe_requested: AtomicBool,
    rates: Mutex<Option<VideoRateControl>>,
}

struct Pending {
    source_id: u64,
    dropped: Arc<AtomicU64>,
    feedback: Arc<SourceFeedback>,
    frame: H264AccessUnit,
}

#[derive(Default)]
struct Broker {
    next_token: i64,
    next_source: u64,
    bytes: usize,
    pending: HashMap<i64, Pending>,
    order: VecDeque<i64>,
}

impl Broker {
    fn insert(
        &mut self,
        source_id: u64,
        dropped: Arc<AtomicU64>,
        feedback: Arc<SourceFeedback>,
        frame: H264AccessUnit,
    ) -> Result<i64, CodecError> {
        let size = frame.data.len();
        if size > MAX_PENDING_BYTES || self.next_token == i64::MAX {
            return Err(CodecError::InvalidFrame);
        }
        while self.pending.len() >= MAX_PENDING || self.bytes > MAX_PENDING_BYTES - size {
            let Some(old) = self.order.pop_front() else {
                return Err(CodecError::InvalidFrame);
            };
            if let Some(previous) = self.pending.remove(&old) {
                self.bytes -= previous.frame.data.len();
                previous.dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
        self.next_token += 1;
        let token = self.next_token;
        self.bytes += size;
        self.order.push_back(token);
        self.pending.insert(
            token,
            Pending {
                source_id,
                dropped,
                feedback,
                frame,
            },
        );
        Ok(token)
    }

    fn take(&mut self, token: i64) -> Option<Pending> {
        let pending = self.pending.remove(&token)?;
        self.bytes -= pending.frame.data.len();
        // Older tokens lost upstream are pruned by insert's bounded FIFO.
        self.order.retain(|queued| *queued != token);
        Some(pending)
    }

    fn remove_source(&mut self, id: u64) {
        let tokens: Vec<_> = self
            .pending
            .iter()
            .filter_map(|(&token, pending)| (pending.source_id == id).then_some(token))
            .collect();
        for token in tokens {
            let _ = self.take(token);
        }
    }
}

/// Codec setup for encoded-only H.264 sending. Supply `encoder_factory()` to
/// the peer factory builder, then create sources using that same factory.
/// One format is advertised: constrained-baseline H.264, level 3.1, packetization
/// mode 1. This does not provide an H.264 decoder.
#[derive(Clone)]
pub struct EncodedH264Input {
    broker: Arc<Mutex<Broker>>,
    encoder: VideoEncoderFactoryHandle,
}

impl EncodedH264Input {
    pub fn new() -> Result<Self, CodecError> {
        let broker = Arc::new(Mutex::new(Broker::default()));
        let encoder = VideoEncoderFactoryHandle::new(InputFactory {
            broker: broker.clone(),
        })?;
        Ok(Self { broker, encoder })
    }

    pub fn encoder_factory(&self) -> VideoEncoderFactoryHandle {
        self.encoder.clone()
    }

    pub fn create_source(
        &self,
        factory: &PeerConnectionFactory,
    ) -> Result<EncodedH264Source, PeerError> {
        if !factory.uses_video_encoder(&self.encoder) {
            return Err(PeerError {
                kind: PeerErrorKind::InvalidParameter,
                message: "factory is not configured with this encoded H.264 input".into(),
            });
        }
        let source = factory.create_video_source()?;
        let id = {
            let mut broker = self
                .broker
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            broker.next_source = broker.next_source.checked_add(1).ok_or_else(|| PeerError {
                kind: PeerErrorKind::InvalidState,
                message: "encoded source identifier exhausted".into(),
            })?;
            broker.next_source
        };
        Ok(EncodedH264Source {
            source,
            broker: self.broker.clone(),
            id,
            closed: false,
            last_timestamp_us: Cell::new(None),
            seen_keyframe: Cell::new(false),
            dropped: Arc::new(AtomicU64::new(0)),
            feedback: Arc::new(SourceFeedback::default()),
        })
    }
}

/// A sequence-bound encoded-video source; use `create_track` to retain the
/// underlying source in the returned video track. Dropping this handle stops
/// accepting input and discards any queued access units for this source.
pub struct EncodedH264Source {
    source: VideoSource,
    broker: Arc<Mutex<Broker>>,
    id: u64,
    closed: bool,
    last_timestamp_us: Cell<Option<i64>>,
    seen_keyframe: Cell<bool>,
    dropped: Arc<AtomicU64>,
    feedback: Arc<SourceFeedback>,
}

impl EncodedH264Source {
    pub fn stream_id(&self) -> u64 {
        self.id
    }

    pub fn create_track(
        &self,
        factory: &PeerConnectionFactory,
        id: &str,
    ) -> Result<VideoTrack, PeerError> {
        if self.closed || !factory.owns_video_source(&self.source) {
            return Err(PeerError {
                kind: PeerErrorKind::InvalidParameter,
                message: "encoded source is closed or belongs to another factory".into(),
            });
        }
        factory.create_video_track(id, &self.source)
    }

    pub fn push(&self, frame: H264AccessUnit) -> Result<(), CodecError> {
        if self.closed {
            return Err(CodecError::Released);
        }
        if frame.width == 0
            || frame.height == 0
            || frame.width > 4096
            || frame.height > 4096
            || frame.timestamp_us < 0
            || self
                .last_timestamp_us
                .get()
                .is_some_and(|last| frame.timestamp_us <= last)
            || (!self.seen_keyframe.get() && !frame.key_frame)
            || frame.qp.is_some_and(|qp| qp > 51)
            || !valid_annex_b(&frame.data, frame.key_frame)
        {
            return Err(CodecError::InvalidFrame);
        }
        let width = frame.width;
        let height = frame.height;
        let time = frame.timestamp_us;
        let key_frame = frame.key_frame;
        let token = self
            .broker
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(self.id, self.dropped.clone(), self.feedback.clone(), frame)?;
        if !ffi::video_source_push_encoded_trigger(self.source.native(), width, height, time, token)
        {
            let _ = self
                .broker
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take(token);
            return Err(CodecError::Released);
        }
        self.last_timestamp_us.set(Some(time));
        if key_frame {
            self.seen_keyframe.set(true);
        }
        Ok(())
    }

    /// Returns and clears keyframe feedback signaled by the sending encoder.
    /// Feedback becomes visible when WebRTC tries to encode the next frame;
    /// an already-submitted delta frame can therefore be rejected.
    pub fn take_keyframe_request(&self) -> bool {
        self.feedback
            .keyframe_requested
            .swap(false, Ordering::AcqRel)
    }

    /// Most recent rate control observed while processing this stream, if any.
    pub fn latest_rate_control(&self) -> Option<VideoRateControl> {
        *self
            .feedback
            .rates
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn pending_frames(&self) -> usize {
        self.broker
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pending
            .values()
            .filter(|pending| pending.source_id == self.id)
            .count()
    }

    pub fn dropped_frames(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    pub fn close(&mut self) -> Result<(), CodecError> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        self.broker
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove_source(self.id);
        self.source.close()
    }
}

impl Drop for EncodedH264Source {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

fn valid_annex_b(data: &[u8], key: bool) -> bool {
    if data.is_empty() || data.len() > MAX_PENDING_BYTES {
        return false;
    }
    let mut nal_types = Vec::new();
    let mut cursor = 0;
    while cursor < data.len() {
        let prefix = if data[cursor..].starts_with(&[0, 0, 0, 1]) {
            4
        } else if data[cursor..].starts_with(&[0, 0, 1]) {
            3
        } else {
            return false;
        };
        let start = cursor + prefix;
        if start >= data.len() {
            return false;
        }
        let next = (start..data.len()).find(|&at| data[at..].starts_with(&[0, 0, 1]));
        let end = next.unwrap_or(data.len());
        if start == end || data[start] & 0x80 != 0 {
            return false;
        }
        let nal_type = data[start] & 0x1f;
        if nal_type == 7
            && (end - start < 4
                || data[start + 1] != 0x42
                || data[start + 2] & 0xc0 != 0xc0
                || data[start + 3] > 0x1f)
        {
            return false;
        }
        if !matches!(nal_type, 1 | 5 | 6 | 7 | 8 | 9) {
            return false;
        }
        nal_types.push(nal_type);
        cursor = end;
    }
    let idr = nal_types.contains(&5);
    if key {
        idr && nal_types.contains(&7) && nal_types.contains(&8)
    } else {
        !idr && nal_types.contains(&1)
    }
}

fn format() -> VideoCodecFormat {
    VideoCodecFormat::new("H264")
        .with_parameter("level-asymmetry-allowed", "1")
        .with_parameter("packetization-mode", "1")
        .with_parameter("profile-level-id", "42e01f")
}

struct InputFactory {
    broker: Arc<Mutex<Broker>>,
}
impl VideoEncoderFactory for InputFactory {
    fn supported_formats(&self) -> Vec<VideoCodecFormat> {
        vec![format()]
    }
    fn query_support(
        &self,
        format: &VideoCodecFormat,
        mode: Option<&str>,
        _: Option<VideoResolution>,
    ) -> CodecSupport {
        CodecSupport {
            supported: format == &self::format() && mode.is_none(),
            power_efficient: false,
        }
    }
    fn create(&self, format: &VideoCodecFormat) -> Result<Box<dyn VideoEncoder>, CodecError> {
        if format != &self::format() {
            return Err(CodecError::UnsupportedFormat);
        }
        Ok(Box::new(InputEncoder {
            broker: self.broker.clone(),
            released: false,
            rates: None,
        }))
    }
}

struct InputEncoder {
    broker: Arc<Mutex<Broker>>,
    released: bool,
    rates: Option<VideoRateControl>,
}
impl VideoEncoder for InputEncoder {
    fn initialize(&mut self, _: VideoEncoderSettings) -> Result<(), CodecError> {
        Ok(())
    }
    fn encode(
        &mut self,
        _: VideoFrame,
        _: &[VideoFrameType],
        _: EncodedImageCallback,
    ) -> Result<(), CodecError> {
        Err(CodecError::InvalidFrame)
    }
    fn encode_with_presentation_token(
        &mut self,
        frame: VideoFrame,
        frame_types: &[VideoFrameType],
        token: Option<i64>,
        callback: EncodedImageCallback,
    ) -> Result<(), CodecError> {
        if self.released {
            return Err(CodecError::Released);
        }
        let token = token.ok_or(CodecError::InvalidFrame)?;
        let pending = self
            .broker
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take(token)
            .ok_or(CodecError::InvalidFrame)?;
        if let Some(rates) = self.rates {
            *pending
                .feedback
                .rates
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(rates);
        }
        let unit = pending.frame;
        if frame_types.contains(&VideoFrameType::Key) && !unit.key_frame {
            pending
                .feedback
                .keyframe_requested
                .store(true, Ordering::Release);
            return Err(CodecError::InvalidFrame);
        }
        if (unit.width, unit.height) != (frame.width, frame.height) {
            return Err(CodecError::InvalidFrame);
        }
        callback.emit(&EncodedVideoFrame {
            data: unit.data,
            width: unit.width,
            height: unit.height,
            rtp_timestamp: frame.rtp_timestamp,
            frame_type: if unit.key_frame {
                VideoFrameType::Key
            } else {
                VideoFrameType::Delta
            },
            qp: unit.qp,
        })
    }
    fn set_rates(&mut self, rates: VideoRateControl) -> Result<(), CodecError> {
        self.rates = Some(rates);
        Ok(())
    }
    fn release(&mut self) -> Result<(), CodecError> {
        self.released = true;
        Ok(())
    }
    fn info(&self) -> VideoEncoderInfo {
        VideoEncoderInfo {
            implementation_name: "encoded H264 input".into(),
            hardware_accelerated: false,
            supports_native_handle: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::valid_annex_b;
    #[test]
    fn rejects_malformed_access_units() {
        assert!(!valid_annex_b(&[], true));
        assert!(!valid_annex_b(&[0, 0, 0, 1, 0x65], false));
        assert!(!valid_annex_b(&[0, 0, 0, 1, 0x65], true));
        assert!(valid_annex_b(
            &[
                0, 0, 0, 1, 0x67, 0x42, 0xe0, 0x1f, 0, 0, 1, 0x68, 1, 0, 0, 1, 0x65, 1
            ],
            true
        ));
    }
}
