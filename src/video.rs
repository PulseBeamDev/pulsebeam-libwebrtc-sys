use std::{fmt, rc::Rc};

use crate::{
    CodecError, VideoFrame, VideoResolution, ffi,
    peer::{FactoryInner, PeerError, PeerErrorKind, PeerInner, error_kind},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RtpTransceiverDirection {
    SendReceive = 0,
    SendOnly = 1,
    ReceiveOnly = 2,
    Inactive = 3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VideoSourceState {
    Live,
    Ended,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VideoTrackState {
    Live,
    Ended,
}

/// A sequence-bound caller-fed video source.
///
/// ```compile_fail
/// fn assert_send<T: Send>() {}
/// assert_send::<pulsebeam_webrtc_sys::VideoSource>();
/// ```
///
/// ```compile_fail
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<pulsebeam_webrtc_sys::VideoSource>();
/// ```
pub struct VideoSource {
    inner: Rc<SourceInner>,
}

struct SourceInner {
    native: cxx::UniquePtr<ffi::NativeVideoSource>,
    _factory: Rc<FactoryInner>,
}

impl VideoSource {
    pub(crate) fn from_native(
        native: cxx::UniquePtr<ffi::NativeVideoSource>,
        factory: Rc<FactoryInner>,
    ) -> Self {
        Self {
            inner: Rc::new(SourceInner {
                native,
                _factory: factory,
            }),
        }
    }

    pub fn state(&self) -> VideoSourceState {
        match ffi::video_source_state(self.native()) {
            1 => VideoSourceState::Live,
            2 => VideoSourceState::Ended,
            value => unreachable!("native adapter returned invalid video source state {value}"),
        }
    }

    pub fn push_frame(&self, frame: &VideoFrame) -> Result<(), CodecError> {
        ffi::video_source_push_frame(
            self.native(),
            frame.buffer.as_bytes(),
            frame.width,
            frame.height,
            frame.timestamp_us,
            frame.rtp_timestamp,
        )
        .then_some(())
        .ok_or(CodecError::Released)
    }

    pub fn close(&mut self) -> Result<(), CodecError> {
        ffi::close_video_source(self.native())
            .then_some(())
            .ok_or(CodecError::Released)
    }

    pub(crate) fn native(&self) -> &ffi::NativeVideoSource {
        self.inner.native.as_ref().expect("validated video source")
    }
}

/// A sequence-bound local or remote video track.
///
/// ```compile_fail
/// fn assert_send<T: Send>() {}
/// assert_send::<pulsebeam_webrtc_sys::VideoTrack>();
/// ```
///
/// ```compile_fail
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<pulsebeam_webrtc_sys::VideoTrack>();
/// ```
pub struct VideoTrack {
    inner: Rc<TrackInner>,
}

struct TrackInner {
    native: cxx::UniquePtr<ffi::NativeVideoTrack>,
    _factory: Option<Rc<FactoryInner>>,
    _peer: Option<Rc<PeerInner>>,
    _source: Option<Rc<SourceInner>>,
}

impl VideoTrack {
    pub(crate) fn local(
        native: cxx::UniquePtr<ffi::NativeVideoTrack>,
        factory: Rc<FactoryInner>,
        source: &VideoSource,
    ) -> Self {
        Self {
            inner: Rc::new(TrackInner {
                native,
                _factory: Some(factory),
                _peer: None,
                _source: Some(source.inner.clone()),
            }),
        }
    }

    fn remote(native: cxx::UniquePtr<ffi::NativeVideoTrack>, peer: Rc<PeerInner>) -> Self {
        Self {
            inner: Rc::new(TrackInner {
                native,
                _factory: None,
                _peer: Some(peer),
                _source: None,
            }),
        }
    }

    pub fn id(&self) -> String {
        ffi::video_track_id(self.native())
    }

    pub fn enabled(&self) -> bool {
        ffi::video_track_enabled(self.native())
    }

    pub fn set_enabled(&self, enabled: bool) -> bool {
        ffi::video_track_set_enabled(self.native(), enabled)
    }

    pub fn state(&self) -> VideoTrackState {
        match ffi::video_track_state(self.native()) {
            0 => VideoTrackState::Live,
            1 => VideoTrackState::Ended,
            value => unreachable!("native adapter returned invalid video track state {value}"),
        }
    }

    pub fn attach_sink(&self) -> Result<VideoSink, PeerError> {
        let native = ffi::video_track_attach_sink(self.native());
        if native.is_null() {
            Err(native_error("failed to attach video sink"))
        } else {
            Ok(VideoSink {
                native,
                _track: self.inner.clone(),
            })
        }
    }

    pub(crate) fn native(&self) -> &ffi::NativeVideoTrack {
        self.inner.native.as_ref().expect("validated video track")
    }
}

/// A sequence-bound caller-polled video sink.
///
/// At most four frames and 16 MiB of I420 data are retained. When full, the
/// oldest frames are discarded; frames larger than the budget are discarded.
/// Observe cumulative intentional loss with [`Self::dropped_frames`].
///
/// ```compile_fail
/// fn assert_send<T: Send>() {}
/// assert_send::<pulsebeam_webrtc_sys::VideoSink>();
/// ```
///
/// ```compile_fail
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<pulsebeam_webrtc_sys::VideoSink>();
/// ```
pub struct VideoSink {
    native: cxx::UniquePtr<ffi::NativeVideoSink>,
    _track: Rc<TrackInner>,
}

impl VideoSink {
    /// Number of frames discarded by this sink's retention policy, saturating
    /// at `u64::MAX`. Closed sinks retain the last count.
    pub fn dropped_frames(&self) -> u64 {
        ffi::video_sink_dropped_frames(self.native.as_ref().expect("validated video sink"))
    }

    pub fn try_next_frame(&self) -> Option<VideoFrame> {
        let native =
            ffi::video_sink_take_frame(self.native.as_ref().expect("validated video sink"));
        let native = native.as_ref()?;
        Some(
            VideoFrame::i420(
                ffi::native_video_frame_width(native),
                ffi::native_video_frame_height(native),
                ffi::native_video_frame_i420(native),
                ffi::native_video_frame_timestamp_us(native),
                ffi::native_video_frame_rtp_timestamp(native),
            )
            .expect("native adapter returned an invalid I420 frame"),
        )
    }

    pub fn close(&mut self) -> Result<(), PeerError> {
        ffi::close_video_sink(self.native.as_ref().expect("validated video sink"))
            .then_some(())
            .ok_or_else(|| native_error("failed to detach video sink"))
    }
}

/// A sequence-bound RTP sender.
///
/// ```compile_fail
/// fn assert_send<T: Send>() {}
/// assert_send::<pulsebeam_webrtc_sys::RtpSender>();
/// ```
///
/// ```compile_fail
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<pulsebeam_webrtc_sys::RtpSender>();
/// ```
pub struct RtpSender {
    native: cxx::UniquePtr<ffi::NativeRtpSender>,
    peer: Rc<PeerInner>,
}

/// A mutable subset of an RTP sender encoding. RID is fixed by negotiation.
#[derive(Clone, Debug, PartialEq)]
pub struct RtpSenderEncoding {
    pub rid: String,
    pub active: bool,
    pub max_bitrate_bps: Option<u32>,
    pub max_framerate: Option<f64>,
    pub scale_resolution_down_by: Option<f64>,
    pub scale_resolution_down_to: Option<VideoResolution>,
    pub scalability_mode: Option<String>,
}

/// An owned snapshot. Pass it back to the same sender; stale transactions fail.
#[derive(Clone, Debug, PartialEq)]
pub struct RtpSenderParameters {
    transaction_id: String,
    pub encodings: Vec<RtpSenderEncoding>,
}

impl RtpSender {
    pub fn id(&self) -> String {
        ffi::rtp_sender_id(self.native())
    }

    pub fn track(&self) -> Option<VideoTrack> {
        let native = ffi::rtp_sender_track(self.native());
        (!native.is_null()).then(|| VideoTrack::remote(native, self.peer.clone()))
    }

    pub fn parameters(&self) -> Result<RtpSenderParameters, PeerError> {
        let mut snapshot = ffi::FfiSenderParameters {
            transaction_id: String::new(),
            encodings: Vec::new(),
        };
        if !ffi::rtp_sender_get_parameters(self.native(), &mut snapshot) {
            return Err(PeerError {
                kind: PeerErrorKind::InvalidState,
                message: "RTP sender parameters are unavailable".into(),
            });
        }
        Ok(RtpSenderParameters {
            transaction_id: snapshot.transaction_id,
            encodings: snapshot
                .encodings
                .into_iter()
                .map(|encoding| RtpSenderEncoding {
                    rid: encoding.rid,
                    active: encoding.active,
                    max_bitrate_bps: encoding
                        .max_bitrate_bps
                        .try_into()
                        .ok()
                        .filter(|_| encoding.has_max_bitrate),
                    max_framerate: encoding.has_max_framerate.then_some(encoding.max_framerate),
                    scale_resolution_down_by: encoding.has_scale_by.then_some(encoding.scale_by),
                    scale_resolution_down_to: encoding.has_scale_to.then_some(VideoResolution {
                        width: encoding.scale_to_width as u32,
                        height: encoding.scale_to_height as u32,
                    }),
                    scalability_mode: encoding
                        .has_scalability_mode
                        .then_some(encoding.scalability_mode),
                })
                .collect(),
        })
    }

    /// Updates supported fields using the latest parameters snapshot. Unexposed
    /// fields are preserved, and native validation rejects unsupported modes.
    pub fn set_parameters(&self, parameters: RtpSenderParameters) -> Result<(), PeerError> {
        let invalid = |message: &str| PeerError {
            kind: PeerErrorKind::InvalidParameter,
            message: message.into(),
        };
        let mut encodings = Vec::with_capacity(parameters.encodings.len());
        for encoding in parameters.encodings {
            let max_bitrate = encoding
                .max_bitrate_bps
                .map(i32::try_from)
                .transpose()
                .map_err(|_| invalid("maximum bitrate exceeds i32 range"))?;
            if max_bitrate == Some(0) {
                return Err(invalid("maximum bitrate must be positive"));
            }
            if encoding
                .max_framerate
                .is_some_and(|v| !v.is_finite() || v < 0.0)
            {
                return Err(invalid("maximum framerate must be finite and nonnegative"));
            }
            if encoding
                .scale_resolution_down_by
                .is_some_and(|v| !v.is_finite() || v < 1.0)
            {
                return Err(invalid("resolution scale must be finite and at least one"));
            }
            if encoding.scale_resolution_down_to.is_some_and(|r| {
                r.width == 0
                    || r.height == 0
                    || r.width > i32::MAX as u32
                    || r.height > i32::MAX as u32
            }) {
                return Err(invalid("target resolution is outside the supported range"));
            }
            if encoding.scalability_mode.as_deref() == Some("") {
                return Err(invalid("scalability mode must not be empty"));
            }
            let target = encoding.scale_resolution_down_to;
            encodings.push(ffi::FfiSenderEncoding {
                rid: encoding.rid,
                active: encoding.active,
                has_max_bitrate: max_bitrate.is_some(),
                max_bitrate_bps: max_bitrate.unwrap_or_default(),
                has_max_framerate: encoding.max_framerate.is_some(),
                max_framerate: encoding.max_framerate.unwrap_or_default(),
                has_scale_by: encoding.scale_resolution_down_by.is_some(),
                scale_by: encoding.scale_resolution_down_by.unwrap_or_default(),
                has_scale_to: target.is_some(),
                scale_to_width: target.map(|r| r.width as i32).unwrap_or_default(),
                scale_to_height: target.map(|r| r.height as i32).unwrap_or_default(),
                has_scalability_mode: encoding.scalability_mode.is_some(),
                scalability_mode: encoding.scalability_mode.unwrap_or_default(),
            });
        }
        let mut error_type = 0;
        let mut message = String::new();
        ffi::rtp_sender_set_parameters(
            self.native(),
            &ffi::FfiSenderParameters {
                transaction_id: parameters.transaction_id,
                encodings,
            },
            &mut error_type,
            &mut message,
        )
        .then_some(())
        .ok_or_else(|| PeerError {
            kind: error_kind(error_type),
            message,
        })
    }

    pub(crate) fn native(&self) -> &ffi::NativeRtpSender {
        self.native.as_ref().expect("validated RTP sender")
    }
}

/// A sequence-bound RTP receiver.
///
/// ```compile_fail
/// fn assert_send<T: Send>() {}
/// assert_send::<pulsebeam_webrtc_sys::RtpReceiver>();
/// ```
///
/// ```compile_fail
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<pulsebeam_webrtc_sys::RtpReceiver>();
/// ```
pub struct RtpReceiver {
    native: cxx::UniquePtr<ffi::NativeRtpReceiver>,
    peer: Rc<PeerInner>,
}

impl fmt::Debug for RtpReceiver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RtpReceiver")
            .field("id", &self.id())
            .finish()
    }
}

impl RtpReceiver {
    pub(crate) fn from_native(
        native: cxx::UniquePtr<ffi::NativeRtpReceiver>,
        peer: Rc<PeerInner>,
    ) -> Self {
        Self { native, peer }
    }

    pub fn id(&self) -> String {
        ffi::rtp_receiver_id(self.native())
    }

    pub fn track(&self) -> Option<VideoTrack> {
        let native = ffi::rtp_receiver_track(self.native());
        (!native.is_null()).then(|| VideoTrack::remote(native, self.peer.clone()))
    }

    fn native(&self) -> &ffi::NativeRtpReceiver {
        self.native.as_ref().expect("validated RTP receiver")
    }
}

/// A sequence-bound RTP transceiver.
///
/// ```compile_fail
/// fn assert_send<T: Send>() {}
/// assert_send::<pulsebeam_webrtc_sys::RtpTransceiver>();
/// ```
///
/// ```compile_fail
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<pulsebeam_webrtc_sys::RtpTransceiver>();
/// ```
pub struct RtpTransceiver {
    native: cxx::UniquePtr<ffi::NativeRtpTransceiver>,
    peer: Rc<PeerInner>,
}

impl fmt::Debug for RtpTransceiver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RtpTransceiver")
            .field("direction", &self.direction())
            .field("current_direction", &self.current_direction())
            .field("stopped", &self.stopped())
            .finish()
    }
}

impl RtpTransceiver {
    pub(crate) fn from_native(
        native: cxx::UniquePtr<ffi::NativeRtpTransceiver>,
        peer: Rc<PeerInner>,
    ) -> Self {
        Self { native, peer }
    }

    pub fn sender(&self) -> RtpSender {
        RtpSender {
            native: ffi::rtp_transceiver_sender(self.native()),
            peer: self.peer.clone(),
        }
    }

    pub fn receiver(&self) -> RtpReceiver {
        RtpReceiver::from_native(
            ffi::rtp_transceiver_receiver(self.native()),
            self.peer.clone(),
        )
    }

    pub fn direction(&self) -> RtpTransceiverDirection {
        direction(ffi::rtp_transceiver_direction(self.native()))
    }

    pub fn current_direction(&self) -> Option<RtpTransceiverDirection> {
        u8::try_from(ffi::rtp_transceiver_current_direction(self.native()))
            .ok()
            .map(direction)
    }

    pub fn stopped(&self) -> bool {
        ffi::rtp_transceiver_stopped(self.native())
    }

    pub fn set_direction(&self, direction: RtpTransceiverDirection) -> Result<(), PeerError> {
        let mut error_type = 0;
        let mut message = String::new();
        ffi::rtp_transceiver_set_direction(
            self.native(),
            direction as u8,
            &mut error_type,
            &mut message,
        )
        .then_some(())
        .ok_or_else(|| PeerError {
            kind: error_kind(error_type),
            message,
        })
    }

    fn native(&self) -> &ffi::NativeRtpTransceiver {
        self.native.as_ref().expect("validated RTP transceiver")
    }
}

fn direction(value: u8) -> RtpTransceiverDirection {
    match value {
        0 => RtpTransceiverDirection::SendReceive,
        1 => RtpTransceiverDirection::SendOnly,
        2 => RtpTransceiverDirection::ReceiveOnly,
        3 => RtpTransceiverDirection::Inactive,
        _ => unreachable!("native adapter returned an invalid RTP direction"),
    }
}

fn native_error(message: &str) -> PeerError {
    PeerError {
        kind: crate::PeerErrorKind::NativeConstruction,
        message: message.into(),
    }
}
