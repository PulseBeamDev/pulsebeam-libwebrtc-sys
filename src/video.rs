use std::{cell::Cell, fmt, rc::Rc};

use crate::{
    AudioCodecCapability, CodecError, CodecParameter, VideoCodecFormat, VideoFrame,
    VideoResolution, ffi,
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

/// An actual video RTP capability reported by the pinned peer engine.
/// Resiliency codecs (such as RTX) may also appear in the list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VideoCodecCapability {
    format: VideoCodecFormat,
    clock_rate: Option<u32>,
    preferred_payload_type: Option<u8>,
    rtcp_feedback: Vec<String>,
    scalability_modes: Vec<String>,
}

impl VideoCodecCapability {
    pub fn format(&self) -> &VideoCodecFormat {
        &self.format
    }
    pub fn clock_rate(&self) -> Option<u32> {
        self.clock_rate
    }
    pub fn preferred_payload_type(&self) -> Option<u8> {
        self.preferred_payload_type
    }
    pub fn rtcp_feedback(&self) -> &[String] {
        &self.rtcp_feedback
    }
    pub fn scalability_modes(&self) -> &[String] {
        &self.scalability_modes
    }

    pub(crate) fn from_ffi(value: ffi::FfiVideoCodecCapability) -> Self {
        Self {
            format: VideoCodecFormat {
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
            },
            clock_rate: u32::try_from(value.clock_rate).ok(),
            preferred_payload_type: u8::try_from(value.preferred_payload_type).ok(),
            rtcp_feedback: value.rtcp_feedback,
            scalability_modes: value.scalability_modes,
        }
    }

    fn ffi_format(&self) -> ffi::FfiCodecFormat {
        ffi::FfiCodecFormat {
            name: self.format.name.clone(),
            parameters: self
                .format
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

/// A platform screen identified by WebRTC's desktop capture module.
#[cfg(feature = "native")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScreenSource {
    pub id: i64,
    pub name: String,
}

/// A window shares the desktop capturer's ID and title representation. On
/// Wayland, IDs can be portal placeholders rather than unrestricted windows.
#[cfg(feature = "native")]
pub type WindowSource = ScreenSource;

/// A window uses the same caller-paced capture and lifecycle contract.
#[cfg(feature = "native")]
pub type WindowCapture = ScreenCapture;

/// Native screen capture lifecycle. On Wayland, the selection UI is owned by
/// the desktop portal and may remain pending until the user responds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(feature = "native")]
pub enum ScreenCaptureStatus {
    PendingSelection,
    Selected,
    Streaming,
    /// The portal reported user cancellation (which may include a denial).
    CancelledOrDenied,
    /// Upstream's generic error; missing service, denial, and capture errors
    /// cannot be distinguished through this callback.
    Failed,
    /// The Wayland portal closed an active session (not the same as `stop`).
    PortalSessionClosed,
    Closed,
}

/// Caller-paced native screen capture. Call [`Self::capture_next_frame`] to
/// request each frame, then use [`Self::source`] to attach a video track.
#[cfg(feature = "native")]
pub struct ScreenCapture {
    native: cxx::UniquePtr<ffi::NativeScreenCapture>,
    source: VideoSource,
}

#[cfg(feature = "native")]
impl ScreenCapture {
    pub(crate) fn new(
        native: cxx::UniquePtr<ffi::NativeScreenCapture>,
        source: VideoSource,
    ) -> Self {
        Self { native, source }
    }

    pub fn source(&self) -> &VideoSource {
        &self.source
    }

    /// Capture and conversion failures are accumulated by [`Self::failed_frames`].
    pub fn capture_next_frame(&self) -> Result<(), PeerError> {
        let native = self.native.as_ref().expect("validated desktop capture");
        match self.status() {
            ScreenCaptureStatus::Closed => {
                return Err(PeerError {
                    kind: PeerErrorKind::Closed,
                    message: "desktop capture is closed".into(),
                });
            }
            ScreenCaptureStatus::CancelledOrDenied => {
                return Err(PeerError {
                    kind: PeerErrorKind::Operation,
                    message: "desktop selection was cancelled or denied".into(),
                });
            }
            ScreenCaptureStatus::Failed => {
                return Err(PeerError {
                    kind: PeerErrorKind::Operation,
                    message: "desktop capture failed".into(),
                });
            }
            ScreenCaptureStatus::PortalSessionClosed => {
                return Err(PeerError {
                    kind: PeerErrorKind::Operation,
                    message: "desktop portal session closed".into(),
                });
            }
            _ => {}
        }
        if !ffi::screen_capture_next_frame(native) {
            return Err(PeerError {
                kind: PeerErrorKind::Closed,
                message: "desktop capture is closed".into(),
            });
        }
        match self.status() {
            ScreenCaptureStatus::Failed => Err(PeerError {
                kind: PeerErrorKind::Operation,
                message: "desktop capture failed".into(),
            }),
            ScreenCaptureStatus::PortalSessionClosed => Err(PeerError {
                kind: PeerErrorKind::Operation,
                message: "desktop portal session closed".into(),
            }),
            _ => Ok(()),
        }
    }

    /// Portal selection and capture state. `CancelledOrDenied`, `Failed`, and
    /// `PortalSessionClosed` are terminal; check with [`Self::failed_frames`] after requesting
    /// frames. Headless X11 selections begin in `Selected`.
    pub fn status(&self) -> ScreenCaptureStatus {
        match ffi::screen_capture_status(self.native.as_ref().expect("validated screen capture")) {
            0 => ScreenCaptureStatus::PendingSelection,
            1 => ScreenCaptureStatus::Selected,
            2 => ScreenCaptureStatus::Streaming,
            3 => ScreenCaptureStatus::CancelledOrDenied,
            4 => ScreenCaptureStatus::Failed,
            6 => ScreenCaptureStatus::PortalSessionClosed,
            _ => ScreenCaptureStatus::Closed,
        }
    }

    /// Number of failed capture callbacks or frames rejected before delivery.
    pub fn failed_frames(&self) -> u64 {
        ffi::screen_capture_failed_frames(self.native.as_ref().expect("validated screen capture"))
    }

    /// Stop capture and end the associated video source. Repeated stops succeed.
    pub fn stop(&mut self) -> Result<(), PeerError> {
        let stopped = self.native.as_ref().is_some_and(ffi::close_screen);
        let source_stopped = self.source.close().is_ok();
        (stopped && source_stopped)
            .then_some(())
            .ok_or_else(|| PeerError {
                kind: PeerErrorKind::Operation,
                message: "failed to stop desktop capture or end its video source".into(),
            })
    }
}

#[cfg(feature = "native")]
impl Drop for ScreenCapture {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// Pixel layout offered by a platform capture device. Formats other than
/// I420/NV12 are converted to I420 by WebRTC before track delivery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(feature = "native")]
pub enum CameraPixelFormat {
    Unknown,
    I420,
    Iyuv,
    Rgb24,
    Bgr24,
    Argb,
    Abgr,
    Rgb565,
    Yuy2,
    Yv12,
    Uyvy,
    Mjpeg,
    Bgra,
    Nv12,
}

#[cfg(feature = "native")]
impl CameraPixelFormat {
    pub(crate) fn from_native(value: i32) -> Self {
        match value {
            1 => Self::I420,
            2 => Self::Iyuv,
            3 => Self::Rgb24,
            4 => Self::Bgr24,
            5 => Self::Argb,
            6 => Self::Abgr,
            7 => Self::Rgb565,
            8 => Self::Yuy2,
            9 => Self::Yv12,
            10 => Self::Uyvy,
            11 => Self::Mjpeg,
            12 => Self::Bgra,
            13 => Self::Nv12,
            _ => Self::Unknown,
        }
    }
}

/// A capture format advertised by a native camera.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(feature = "native")]
pub struct CameraFormat {
    pub width: u32,
    pub height: u32,
    pub max_fps: u32,
    pub pixel_format: CameraPixelFormat,
}

/// A native camera name and stable device identifier reported by WebRTC.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(feature = "native")]
pub struct CameraDevice {
    pub name: String,
    pub id: String,
}

/// Observable camera delivery state. `Stalled` means no frame arrived within
/// the caller's timeout, not a diagnosed reason for device loss.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(feature = "native")]
pub enum CameraCaptureStatus {
    Starting,
    Streaming,
    Stalled,
    Closed,
    Failed,
}

/// A running camera bound to a caller-owned video source. Capture stops on
/// drop; the source remains usable for a track while this handle is alive.
#[cfg(feature = "native")]
pub struct CameraCapture {
    native: cxx::UniquePtr<ffi::NativeCamera>,
    source: VideoSource,
}

#[cfg(feature = "native")]
impl CameraCapture {
    pub(crate) fn new(native: cxx::UniquePtr<ffi::NativeCamera>, source: VideoSource) -> Self {
        Self { native, source }
    }

    pub fn source(&self) -> &VideoSource {
        &self.source
    }

    /// Detect stopped delivery after `stale_after` without assuming the cause.
    /// A capture may be `Starting` until its first frame, or `Stalled` if no
    /// frame arrives in time (including after a device disappears).
    pub fn status(&self, stale_after: std::time::Duration) -> CameraCaptureStatus {
        let millis = stale_after.as_millis().min(u128::from(u64::MAX)) as u64;
        match ffi::camera_capture_status(self.native.as_ref().expect("validated camera"), millis) {
            0 => CameraCaptureStatus::Starting,
            1 => CameraCaptureStatus::Streaming,
            2 => CameraCaptureStatus::Stalled,
            3 => CameraCaptureStatus::Closed,
            _ => CameraCaptureStatus::Failed,
        }
    }

    /// Stop the device and end its video source. Reports a device stop failure.
    pub fn stop(&mut self) -> Result<(), PeerError> {
        let stopped = self.native.as_ref().is_some_and(ffi::close_camera);
        let source_stopped = self.source.close().is_ok();
        (stopped && source_stopped)
            .then_some(())
            .ok_or_else(|| PeerError {
                kind: PeerErrorKind::Operation,
                message: "failed to stop camera or end its video source".into(),
            })
    }
}

#[cfg(feature = "native")]
impl Drop for CameraCapture {
    fn drop(&mut self) {
        let _ = self.stop();
    }
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
            frame.rotation as u16,
        )
        .then_some(())
        .ok_or(CodecError::Released)
    }

    pub fn close(&mut self) -> Result<(), CodecError> {
        ffi::close_video_source(self.native())
            .then_some(())
            .ok_or(CodecError::Released)
    }

    pub(crate) fn belongs_to(&self, factory: &Rc<FactoryInner>) -> bool {
        Rc::ptr_eq(&self.inner._factory, factory)
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
#[derive(Clone)]
pub struct VideoTrack {
    inner: Rc<TrackInner>,
}

struct TrackInner {
    native: cxx::UniquePtr<ffi::NativeVideoTrack>,
    _factory: Option<Rc<FactoryInner>>,
    _peer: Option<Rc<PeerInner>>,
    _source: Option<Rc<SourceInner>>,
    encoded_h264: Cell<bool>,
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
                encoded_h264: Cell::new(false),
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
                encoded_h264: Cell::new(false),
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

    pub(crate) fn mark_encoded_h264(&self) {
        self.inner.encoded_h264.set(true);
    }

    pub(crate) fn is_encoded_h264(&self) -> bool {
        self.inner.encoded_h264.get()
    }

    pub(crate) fn is_local_to(&self, factory: &Rc<FactoryInner>) -> bool {
        self.inner
            ._factory
            .as_ref()
            .is_some_and(|owner| Rc::ptr_eq(owner, factory))
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
            .expect("native adapter returned an invalid I420 frame")
            .with_rotation(crate::VideoRotation::from_degrees(
                ffi::native_video_frame_rotation(native),
            )),
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
    pub(crate) peer: Rc<PeerInner>,
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
        if native.is_null() {
            return None;
        }
        self.peer
            .sender_tracks
            .borrow()
            .get(&self.id())
            .cloned()
            .or_else(|| Some(VideoTrack::remote(native, self.peer.clone())))
    }

    /// Replace or detach this sender's local video track without replacing the
    /// sender or its negotiated transceiver. Only tracks from the same factory
    /// are accepted. Renegotiate if the remote track identity must change.
    pub fn set_track(&self, track: Option<&VideoTrack>) -> Result<(), PeerError> {
        if self.peer.closed.get() {
            return Err(PeerError {
                kind: PeerErrorKind::Closed,
                message: "peer is closed".into(),
            });
        }
        if let Some(track) = track {
            if track.is_encoded_h264() && self.parameters()?.encodings.len() > 1 {
                return Err(PeerError {
                    kind: PeerErrorKind::UnsupportedParameter,
                    message: "encoded H264 input cannot replace a simulcast sender".into(),
                });
            }
            if !track.is_local_to(&self.peer._factory) || track.state() != VideoTrackState::Live {
                return Err(PeerError {
                    kind: PeerErrorKind::InvalidParameter,
                    message: "sender replacement requires a live track from the same factory"
                        .into(),
                });
            }
        }
        let accepted = match track {
            Some(track) => ffi::rtp_sender_set_video_track(self.native(), track.native()),
            None => ffi::rtp_sender_clear_track(self.native()),
        };
        if !accepted {
            return Err(PeerError {
                kind: PeerErrorKind::InvalidState,
                message: "sender rejected the video track change".into(),
            });
        }
        let mut retained = self.peer.sender_tracks.borrow_mut();
        match track {
            Some(track) => {
                retained.insert(self.id(), track.clone());
            }
            None => {
                retained.remove(&self.id());
            }
        }
        Ok(())
    }

    /// Ask the active video sender to produce a keyframe for the listed RIDs.
    /// An empty slice targets all configured encodings. Successful submission
    /// is not proof that a frame was produced: the sender must have a live
    /// sending media channel and an encoder that honors keyframe requests.
    pub fn request_keyframe(&self, rids: &[String]) -> Result<(), PeerError> {
        if self.peer.closed.get() {
            return Err(PeerError {
                kind: PeerErrorKind::Closed,
                message: "peer is closed".into(),
            });
        }
        if self.track().is_none() {
            return Err(PeerError {
                kind: PeerErrorKind::InvalidState,
                message: "video sender has no track".into(),
            });
        }
        let mut error_type = 0;
        let mut message = String::new();
        ffi::rtp_sender_request_keyframe(
            self.native(),
            self.peer.native(),
            rids,
            &mut error_type,
            &mut message,
        )
        .then_some(())
        .ok_or_else(|| PeerError {
            kind: error_kind(error_type),
            message,
        })
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
        if self.track().is_some_and(|track| track.is_encoded_h264())
            && (parameters.encodings.len() > 1
                || parameters.encodings.iter().any(|encoding| {
                    encoding.scalability_mode.is_some()
                        || encoding.scale_resolution_down_to.is_some()
                        || encoding
                            .scale_resolution_down_by
                            .is_some_and(|scale| scale != 1.0)
                }))
        {
            return Err(PeerError {
                kind: PeerErrorKind::UnsupportedParameter,
                message: "encoded H264 input does not support scaling, simulcast, or SVC".into(),
            });
        }
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

    /// Attach a bounded decoded PCM sink to a receiver on this peer.
    pub fn attach_audio_sink(&self) -> Result<crate::AudioSink, PeerError> {
        if self.peer.closed.get() {
            return Err(PeerError {
                kind: PeerErrorKind::Closed,
                message: "peer is closed".into(),
            });
        }
        let native = ffi::rtp_receiver_attach_audio_sink(self.peer.native(), self.native());
        if native.is_null() {
            return Err(PeerError {
                kind: PeerErrorKind::InvalidState,
                message: "receiver is foreign, ended or not an audio receiver".into(),
            });
        }
        Ok(crate::AudioSink::from_native(native, self.peer.clone()))
    }

    /// Receive encoded Opus without decoding. Select this instead of a decoded
    /// audio sink before media arrives. A receiver permits only one encoded
    /// sink during its lifetime, including after that sink closes.
    pub fn attach_encoded_audio_sink(&self) -> Result<crate::EncodedAudioSink, PeerError> {
        if self.peer.closed.get() {
            return Err(PeerError {
                kind: PeerErrorKind::Closed,
                message: "peer is closed".into(),
            });
        }
        let native = ffi::rtp_receiver_attach_encoded_audio_sink(self.peer.native(), self.native());
        if native.is_null() {
            return Err(PeerError {
                kind: PeerErrorKind::InvalidState,
                message:
                    "receiver is foreign, not negotiated for Opus, or already has an encoded sink"
                        .into(),
            });
        }
        Ok(crate::EncodedAudioSink::from_native(
            native,
            self.peer.clone(),
        ))
    }

    /// Request a fresh remote video keyframe through the negotiated receiver.
    /// Success means the request was submitted, not that the sender delivered
    /// a frame. A foreign, ended or non-video receiver is rejected.
    pub fn request_keyframe(&self) -> Result<(), PeerError> {
        if self.peer.closed.get() {
            return Err(PeerError {
                kind: PeerErrorKind::Closed,
                message: "peer is closed".into(),
            });
        }
        ffi::rtp_receiver_request_keyframe(self.peer.native(), self.native())
            .then_some(())
            .ok_or_else(|| PeerError {
                kind: PeerErrorKind::InvalidState,
                message: "receiver is foreign, ended or not a remote video receiver".into(),
            })
    }

    /// Consume depacketized encoded video before decoding. Only one sink can
    /// ever be attached to each receiver during a peer's lifetime. Closing
    /// the sink restores pass-through decoding; it cannot be reattached.
    pub fn attach_encoded_sink(&self) -> Result<EncodedVideoSink, PeerError> {
        if self.peer.closed.get() {
            return Err(PeerError {
                kind: PeerErrorKind::Closed,
                message: "peer is closed".into(),
            });
        }
        let native = ffi::rtp_receiver_attach_encoded_video_sink(self.peer.native(), self.native());
        if native.is_null() {
            return Err(PeerError {
                kind: PeerErrorKind::InvalidState,
                message: "receiver is foreign or already has an encoded sink".into(),
            });
        }
        Ok(EncodedVideoSink {
            native,
            _peer: self.peer.clone(),
        })
    }

    fn native(&self) -> &ffi::NativeRtpReceiver {
        self.native.as_ref().expect("validated RTP receiver")
    }
}

/// An encoded access unit after RTP reassembly, before WebRTC decoding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedReceivedVideoFrame {
    pub data: Vec<u8>,
    pub mime_type: String,
    pub rtp_timestamp: u32,
    pub ssrc: u32,
    pub payload_type: u8,
    pub key_frame: bool,
    pub rid: Option<String>,
    /// Remote capture clock microseconds, when the absolute capture-time
    /// header extension provides it. Not a local wall-clock timestamp.
    pub capture_time_us: Option<i64>,
    /// Local receive clock microseconds when supplied by upstream.
    pub receive_time_us: Option<i64>,
    /// Dependency descriptor frame id, if present for this access unit.
    pub frame_id: Option<i64>,
    pub spatial_index: Option<i32>,
    pub temporal_index: Option<i32>,
    pub dependencies: Vec<i64>,
    pub decode_target_indications: Vec<DecodeTargetIndication>,
}

/// Decode target indication from the negotiated dependency descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodeTargetIndication {
    NotPresent,
    Discardable,
    Switch,
    Required,
}

/// A bounded receive-only queue, exclusive to one receiver. A full queue
/// discards the oldest frame. A frame too large for the 4 MiB cap is dropped.
/// No transformed frame is forwarded to the built-in decoder while active.
pub struct EncodedVideoSink {
    native: cxx::UniquePtr<ffi::NativeEncodedVideoSink>,
    _peer: Rc<PeerInner>,
}

impl EncodedVideoSink {
    pub fn try_next_frame(&self) -> Option<EncodedReceivedVideoFrame> {
        let frame =
            ffi::encoded_video_sink_take_frame(self.native.as_ref().expect("validated sink"));
        frame.available.then_some(EncodedReceivedVideoFrame {
            data: frame.data,
            mime_type: frame.mime_type,
            rtp_timestamp: frame.rtp_timestamp,
            ssrc: frame.ssrc,
            payload_type: frame.payload_type,
            key_frame: frame.key_frame,
            rid: frame.has_rid.then_some(frame.rid),
            capture_time_us: frame.has_capture_time.then_some(frame.capture_time_us),
            receive_time_us: frame.has_receive_time.then_some(frame.receive_time_us),
            frame_id: frame.has_frame_id.then_some(frame.frame_id),
            spatial_index: frame.has_frame_id.then_some(frame.spatial_index),
            temporal_index: frame.has_frame_id.then_some(frame.temporal_index),
            dependencies: frame.dependencies,
            decode_target_indications: frame
                .decode_target_indications
                .into_iter()
                .map(|value| match value {
                    0 => DecodeTargetIndication::NotPresent,
                    1 => DecodeTargetIndication::Discardable,
                    2 => DecodeTargetIndication::Switch,
                    3 => DecodeTargetIndication::Required,
                    _ => unreachable!("upstream emitted invalid decode target indication"),
                })
                .collect(),
        })
    }

    /// Saturating count of dropped access units, including oversize frames.
    pub fn dropped_frames(&self) -> u64 {
        ffi::encoded_video_sink_dropped_frames(self.native.as_ref().expect("validated sink"))
    }

    pub fn close(&mut self) {
        ffi::close_encoded_video_sink(self.native.as_ref().expect("validated sink"));
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

    /// The negotiated media ID, absent before negotiation or after rollback.
    pub fn mid(&self) -> Option<String> {
        let mut mid = String::new();
        ffi::rtp_transceiver_mid(self.native(), &mut mid).then_some(mid)
    }

    /// Order supported video codecs for this transceiver's next negotiation.
    /// An empty list restores upstream defaults. Pass capabilities returned by
    /// the peer, rather than inventing payload profiles or fmtp parameters.
    pub fn set_codec_preferences(&self, codecs: &[VideoCodecCapability]) -> Result<(), PeerError> {
        if self.peer.closed.get() {
            return Err(PeerError {
                kind: PeerErrorKind::Closed,
                message: "peer is closed".into(),
            });
        }
        let formats: Vec<_> = codecs
            .iter()
            .map(VideoCodecCapability::ffi_format)
            .collect();
        let mut error_type = 0;
        let mut message = String::new();
        ffi::rtp_transceiver_set_video_codec_preferences(
            self.native(),
            self.peer.factory_native(),
            &formats,
            &mut error_type,
            &mut message,
        )
        .then_some(())
        .ok_or_else(|| PeerError {
            kind: error_kind(error_type),
            message,
        })
    }

    /// Order supported audio codecs for this transceiver's next negotiation.
    /// Stereo Opus also requires the receiver to answer with `stereo=1` on
    /// the matching audio m-line; preferences alone do not request stereo.
    pub fn set_audio_codec_preferences(
        &self,
        codecs: &[AudioCodecCapability],
    ) -> Result<(), PeerError> {
        if self.peer.closed.get() {
            return Err(PeerError {
                kind: PeerErrorKind::Closed,
                message: "peer is closed".into(),
            });
        }
        let formats: Vec<_> = codecs
            .iter()
            .map(AudioCodecCapability::ffi_format)
            .collect();
        let mut error_type = 0;
        let mut message = String::new();
        ffi::rtp_transceiver_set_audio_codec_preferences(
            self.native(),
            self.peer.factory_native(),
            &formats,
            &mut error_type,
            &mut message,
        )
        .then_some(())
        .ok_or_else(|| PeerError {
            kind: error_kind(error_type),
            message,
        })
    }

    /// Start standard transceiver stopping; negotiate again to complete it.
    pub fn stop(&self) -> Result<(), PeerError> {
        if self.peer.closed.get() {
            return Err(PeerError {
                kind: PeerErrorKind::Closed,
                message: "peer is closed".into(),
            });
        }
        let mut error_type = 0;
        let mut message = String::new();
        ffi::rtp_transceiver_stop(self.native(), &mut error_type, &mut message)
            .then_some(())
            .ok_or_else(|| PeerError {
                kind: error_kind(error_type),
                message,
            })
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
