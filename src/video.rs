use std::{cell::Cell, fmt, rc::Rc};

use crate::{
    AudioCodecCapability, CodecError, VideoCodecFormat, VideoFrame, VideoResolution, ffi,
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

/// Native direction of a negotiable RTP header extension.
/// `Stopped` disables advertisement without stopping the transceiver.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RtpHeaderExtensionDirection {
    SendReceive = 0,
    SendOnly = 1,
    ReceiveOnly = 2,
    Inactive = 3,
    Stopped = 4,
}

/// Native extension capability snapshot. Only its direction is configurable.
/// Preserve the complete ordered vector when applying negotiation preferences.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RtpHeaderExtensionCapability {
    uri: String,
    preferred_id: Option<i32>,
    preferred_encrypt: bool,
    pub direction: RtpHeaderExtensionDirection,
}

impl RtpHeaderExtensionCapability {
    pub fn uri(&self) -> &str {
        &self.uri
    }
    pub fn preferred_id(&self) -> Option<i32> {
        self.preferred_id
    }
    pub fn preferred_encrypt(&self) -> bool {
        self.preferred_encrypt
    }
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

    pub(crate) fn from_ffi(value: ffi::FfiVideoCodecCapability) -> Result<Self, crate::PeerError> {
        let owned = crate::SdpVideoFormat::from_native(value.format);
        let format = VideoCodecFormat::try_from(&owned).map_err(|error| crate::PeerError {
            kind: crate::PeerErrorKind::UnrepresentableCodecCapability,
            message: format!("legacy capability projection failed: {error}; use get_rtp_sender_capabilities/get_rtp_receiver_capabilities"),
        })?;
        let scalability_modes = format
            .scalability_modes
            .iter()
            .map(|mode| mode.as_str().to_owned())
            .collect();
        Ok(Self {
            format,
            clock_rate: u32::try_from(value.clock_rate).ok(),
            preferred_payload_type: u8::try_from(value.preferred_payload_type).ok(),
            rtcp_feedback: value.rtcp_feedback,
            scalability_modes,
        })
    }

    fn ffi_format(&self) -> ffi::FfiCodecFormat {
        ffi::FfiCodecFormat {
            name: self.format.name.clone(),
            scalability_modes: self
                .format
                .scalability_modes
                .iter()
                .map(|mode| mode.as_str().into())
                .collect(),
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

    pub(crate) fn controlled_media(&self) -> bool {
        self.inner._factory.controlled_media
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

/// Provenance assigned only by the crate's direct encoded-input adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DirectEncodedVideo {
    H264,
    H264L1T3,
    H264Simulcast,
    Vp8,
    Vp9,
    Av1,
    H265,
}

struct TrackInner {
    native: cxx::UniquePtr<ffi::NativeVideoTrack>,
    _factory: Option<Rc<FactoryInner>>,
    _peer: Option<Rc<PeerInner>>,
    _source: Option<Rc<SourceInner>>,
    encoded_codec: Cell<Option<DirectEncodedVideo>>,
}

impl VideoTrack {
    pub(crate) fn factory_context(&self) -> &Rc<FactoryInner> {
        self.inner
            ._factory
            .as_ref()
            .unwrap_or_else(|| &self.inner._peer.as_ref().unwrap()._factory)
    }
    pub(crate) fn media_track_owner(&self) -> crate::media_track::MediaTrackOwner {
        match &self.inner._peer {
            Some(peer) => {
                let actual = ffi::video_track_reference(self.native());
                for lease in peer.stream_inputs.borrow().values() {
                    if ffi::media_track_same_resource(&actual, lease.track.native()) {
                        return lease.track.source_owner();
                    }
                }
                for lease in peer.sender_tracks.borrow().values() {
                    let candidate = ffi::video_track_reference(lease.track.native());
                    if ffi::media_track_same_resource(&actual, &candidate) {
                        return crate::media_track::MediaTrackOwner::Video {
                            _track: lease.track.clone(),
                        };
                    }
                }
                crate::media_track::MediaTrackOwner::Factory {
                    _factory: peer._factory.clone(),
                }
            }
            None => crate::media_track::MediaTrackOwner::Video {
                _track: self.clone(),
            },
        }
    }
    pub fn as_media_track(&self) -> crate::MediaTrack {
        crate::MediaTrack::from_native(
            ffi::video_track_reference(self.native()),
            self.media_track_owner(),
        )
    }
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
                encoded_codec: Cell::new(None),
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
                encoded_codec: Cell::new(None),
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
        if self
            .inner
            ._peer
            .as_ref()
            .is_some_and(|peer| peer._factory.encoded_video_receive_only())
        {
            return Err(PeerError {
                kind: PeerErrorKind::UnsupportedOperation,
                message: "factory only receives encoded video; decoded sinks are unavailable"
                    .into(),
            });
        }
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

    pub(crate) fn mark_direct_encoded(&self, codec: DirectEncodedVideo) {
        self.inner.encoded_codec.set(Some(codec));
    }

    pub(crate) fn is_direct_encoded(&self) -> bool {
        self.inner.encoded_codec.get().is_some()
    }

    pub(crate) fn is_direct_l1t3(&self) -> bool {
        self.inner.encoded_codec.get() == Some(DirectEncodedVideo::H264L1T3)
    }

    pub(crate) fn is_direct_simulcast(&self) -> bool {
        self.inner.encoded_codec.get() == Some(DirectEncodedVideo::H264Simulcast)
    }

    pub(crate) fn is_admitted_controlled_video(&self) -> bool {
        matches!(
            self.inner.encoded_codec.get(),
            Some(
                DirectEncodedVideo::Vp8
                    | DirectEncodedVideo::H264
                    | DirectEncodedVideo::H264L1T3
                    | DirectEncodedVideo::H264Simulcast
            )
        )
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

/// Owned CPU frame with its recipient and wire track identity. Diagnostic
/// observation timing follows the controlled world, without advancing it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceivedVideoFrame {
    pub peer_id: Option<u64>,
    pub track_id: String,
    pub observed_at: Option<std::time::Duration>,
    pub frame: VideoFrame,
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

    pub fn try_next_received_frame(&self) -> Option<ReceivedVideoFrame> {
        self.try_next_frame().map(|frame| ReceivedVideoFrame {
            peer_id: self
                ._track
                ._peer
                .as_ref()
                .and_then(|peer| peer.controlled_id),
            track_id: ffi::video_track_id(
                self._track.native.as_ref().expect("validated video track"),
            ),
            observed_at: self
                ._track
                ._peer
                .as_ref()
                .and_then(|peer| peer.controlled_time()),
            frame,
        })
    }

    pub fn try_next_frame(&self) -> Option<VideoFrame> {
        if self
            ._track
            ._peer
            .as_ref()
            .is_some_and(|peer| peer.closed.get())
        {
            return None;
        }
        if ffi::video_track_state(self._track.native.as_ref().expect("validated video track")) == 1
        {
            return None;
        }
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

/// Full owning native sender-operation errors, distinct from binding failures.
#[derive(Debug)]
pub enum RtpSenderOperationError {
    Binding(PeerError),
    Native(crate::RtcError),
}
impl From<PeerError> for RtpSenderOperationError {
    fn from(error: PeerError) -> Self {
        Self::Binding(error)
    }
}
impl std::fmt::Display for RtpSenderOperationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Binding(error) => std::fmt::Display::fmt(error, f),
            Self::Native(error) => write!(f, "native RTP sender operation failed: {error:?}"),
        }
    }
}
impl std::error::Error for RtpSenderOperationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Binding(error) => Some(error),
            Self::Native(error) => Some(error),
        }
    }
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

/// Exact pinned native media-type alternatives, including non-RTP categories.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum MediaType {
    Audio = 0,
    Video = 1,
    Data = 2,
    Unsupported = 3,
    Any = 4,
    Other(i32),
}

impl MediaType {
    pub(crate) fn from_native(value: i32) -> Self {
        match value {
            0 => Self::Audio,
            1 => Self::Video,
            2 => Self::Data,
            3 => Self::Unsupported,
            4 => Self::Any,
            value => Self::Other(value),
        }
    }
    pub(crate) fn to_native(self) -> i32 {
        match self {
            Self::Audio => 0,
            Self::Video => 1,
            Self::Data => 2,
            Self::Unsupported => 3,
            Self::Any => 4,
            Self::Other(value) => value,
        }
    }

    /// Forward native conversion for Audio/Video/Data. Unsupported and Any
    /// enter a native assertion branch and are explicitly rejected as `None`.
    pub fn native_name(self) -> Option<String> {
        let mut name = String::new();
        ffi::media_type_to_string(self.to_native(), &mut name).then_some(name)
    }
    /// SDK AbslStringify, with the same native conversion safety boundary.
    pub fn native_string(self) -> Option<String> {
        let mut valid = false;
        let text = ffi::rtp_media_type_string(self.to_native(), &mut valid);
        valid.then_some(text)
    }
}

impl crate::PeerConnection {
    /// A single native owning GetSenders snapshot, including Plan B senders.
    pub fn senders(&self) -> Vec<RtpSender> {
        self.inner.sender_snapshot()
    }
    /// A single native owning GetReceivers snapshot, not a transceiver-derived approximation.
    pub fn receivers(&self) -> Vec<RtpReceiver> {
        let list = ffi::peer_endpoints(self.inner.native());
        (0..ffi::endpoint_list_len(&list, 1))
            .map(|i| {
                RtpReceiver::from_native(ffi::endpoint_list_receiver(&list, i), self.inner.clone())
            })
            .collect()
    }
    pub fn transceivers(&self) -> Result<Vec<RtpTransceiver>, PeerError> {
        let list = ffi::peer_endpoints(self.inner.native());
        if !ffi::endpoint_list_unified(&list) {
            return Err(PeerError {
                kind: PeerErrorKind::UnsupportedOperation,
                message: "GetTransceivers requires Unified Plan semantics".into(),
            });
        }
        Ok((0..ffi::endpoint_list_len(&list, 2))
            .map(|i| {
                RtpTransceiver::from_native(
                    ffi::endpoint_list_transceiver(&list, i),
                    self.inner.clone(),
                )
            })
            .collect())
    }
}
impl PeerInner {
    pub(crate) fn sender_snapshot(self: &Rc<Self>) -> Vec<RtpSender> {
        let list = ffi::peer_endpoints(self.native());
        (0..ffi::endpoint_list_len(&list, 0))
            .map(|i| RtpSender {
                native: ffi::endpoint_list_sender(&list, i),
                peer: self.clone(),
            })
            .collect()
    }
}

impl RtpSender {
    /// SDK creation can change the owning transceiver and request negotiation
    /// even when it returns an error. The selected stock SDK does not implement
    /// the encryption pipeline; errors are returned without fake handles.
    pub fn create_sframe_encrypter(
        &self,
        init: &crate::SframeEncrypterInit,
    ) -> Result<crate::SframeEncrypter, crate::SframeCreateError> {
        crate::SframeEncrypter::from_creation(
            ffi::rtp_sender_create_sframe(self.native(), self.peer.native(), init.native()),
            self.peer.clone(),
        )
    }
    pub fn dtls_transport(&self) -> Option<crate::DtlsTransport> {
        crate::DtlsTransport::from_native(
            ffi::rtp_sender_dtls(self.native(), self.peer.native()),
            self.peer.clone(),
        )
    }

    /// Complete native initial encoding values, copied through the signaling
    /// proxy. Their SDK fields/optionality and comparison are not a partial DTO.
    pub fn init_send_encodings(&self) -> Vec<crate::RtpEncodingParameters> {
        let list = ffi::rtp_sender_initial_encodings(self.native());
        let list = list.as_ref().unwrap();
        (0..ffi::rtp_encoding_list_len(list))
            .map(|index| {
                crate::RtpEncodingParameters::from_native(ffi::rtp_encoding_list_get(list, index))
            })
            .collect()
    }

    /// Primary native SSRC; zero preserves the native not-yet-determined value.
    pub fn ssrc(&self) -> u32 {
        ffi::rtp_sender_ssrc(self.native())
    }

    pub fn media_type(&self) -> MediaType {
        MediaType::from_native(ffi::rtp_sender_media_type(self.native()))
    }

    /// UTF-8 convenience view. Use `stream_id_bytes` for the complete native
    /// string domain, including IDs installed with arbitrary bytes.
    pub fn stream_ids(&self) -> Vec<String> {
        ffi::rtp_sender_stream_ids(self.native())
    }

    /// One actual SDK snapshot, preserving byte strings and native ordering.
    pub fn stream_id_bytes(&self) -> Vec<Vec<u8>> {
        ffi::rtp_sender_stream_id_bytes(self.native())
            .into_iter()
            .map(|id| id.bytes)
            .collect()
    }

    /// Invoke native SetStreams with the supplied IDs/order/duplicates unchanged.
    /// Native de-duplicates by first occurrence, including empty IDs. Read back
    /// native state through `stream_ids`; this void call is not an SDP or
    /// renegotiation acknowledgement.
    /// The native void identity update, preserving opaque bytes without UTF-8 conversion.
    pub fn set_stream_id_bytes(&self, ids: &[Vec<u8>]) {
        let ids: Vec<_> = ids
            .iter()
            .map(|bytes| ffi::FfiRtpByteString {
                bytes: bytes.clone(),
            })
            .collect();
        ffi::rtp_sender_set_stream_id_bytes(self.native(), &ids);
    }
    pub fn set_streams(&self, ids: &[String]) {
        ffi::rtp_sender_set_streams(self.native(), ids);
    }

    /// Shared owning SDK first-packet-sent observer slot with a bounded queue.
    pub fn packet_observer(&self) -> crate::RtpSenderObserver {
        crate::RtpSenderObserver::from_native(
            ffi::rtp_sender_observer(self.native()),
            self.peer.clone(),
        )
    }

    /// Native DTMF access for audio RTP senders. Video senders return `None`.
    /// Repeated getters share one owning handle and one observer registration.
    pub fn dtmf_sender(&self) -> Option<crate::DtmfSender> {
        let native = ffi::rtp_sender_dtmf(self.native(), self.peer.native());
        (!native.is_null()).then(|| crate::DtmfSender::from_native(native, self.peer.clone()))
    }

    pub fn track_reference(&self) -> Option<crate::MediaTrack> {
        let native = ffi::rtp_sender_track_reference(self.native());
        if native.is_null() {
            return None;
        }
        let key = self.resource_key();
        if let Some(lease) = self.peer.stream_inputs.borrow().get(&key) {
            if ffi::media_track_same_resource(&native, lease.track.native()) {
                return Some(lease.track.clone());
            }
        }
        let owner = if let Some(lease) = self.peer.sender_tracks.borrow().get(&key) {
            lease.track.media_track_owner()
        } else if let Some(lease) = self.peer.audio_sender_tracks.borrow().get(&key) {
            crate::media_track::MediaTrackOwner::Audio {
                _track: lease.track.clone(),
            }
        } else {
            crate::media_track::MediaTrackOwner::Factory {
                _factory: self.peer._factory.clone(),
            }
        };
        Some(crate::MediaTrack::from_native(native, owner))
    }
    pub(crate) fn resource_key(&self) -> usize {
        ffi::rtp_sender_identity(self.native())
    }

    /// Actual SDK interface identity, independent of a track/protocol ID.
    pub fn same_resource(&self, other: &Self) -> bool {
        ffi::rtp_sender_same_resource(self.native(), other.native())
    }

    pub fn id(&self) -> String {
        ffi::rtp_sender_id(self.native())
    }
    pub fn id_bytes(&self) -> Vec<u8> {
        ffi::rtp_sender_id_bytes(self.native())
    }

    pub fn track(&self) -> Option<VideoTrack> {
        let native = ffi::rtp_sender_track(self.native());
        if native.is_null() {
            return None;
        }
        self.peer
            .sender_tracks
            .borrow()
            .get(&self.resource_key())
            .map(|lease| lease.track.clone())
            .or_else(|| Some(VideoTrack::remote(native, self.peer.clone())))
    }

    /// SDK SetTrack for an existing audio or video sender. A false return is
    /// the SDK rejection (for example a kind mismatch or stopped sender).
    /// Accepted tracks retain their original Rust source and factory owners.
    pub fn set_media_track(&self, track: Option<&crate::MediaTrack>) -> Result<bool, PeerError> {
        if self.peer.closed.get() {
            return Err(PeerError {
                kind: PeerErrorKind::Closed,
                message: "peer is closed".into(),
            });
        }
        if let Some(track) = track {
            if self.peer._factory.uses_controlled_execution()
                && (!self.peer._factory.controlled_media || !track.is_admitted_controlled_video())
            {
                return Err(PeerError { kind: PeerErrorKind::UnsupportedOperation, message: "controlled replacement requires an admitted video source".into() });
            }
            if !Rc::ptr_eq(track.factory(), &self.peer._factory) {
                return Err(PeerError {
                    kind: PeerErrorKind::InvalidParameter,
                    message: "track must belong to this peer factory".into(),
                });
            }
            if let Some(video) = track.local_video() {
                self.validate_video_replacement(video)?;
            }
            if self.peer._factory.controlled_media
                && track.local_video().is_none()
                && track.local_audio().is_none()
            {
                return Err(PeerError {
                    kind: PeerErrorKind::UnsupportedOperation,
                    message: "controlled replacement requires an admitted local media source"
                        .into(),
                });
            }
        }
        let accepted = match track {
            Some(track) => ffi::rtp_sender_set_media_track(self.native(), track.native()),
            None => ffi::rtp_sender_clear_track(self.native()),
        };
        if !accepted {
            return Ok(false);
        }
        let key = self.resource_key();
        self.peer.sender_tracks.borrow_mut().remove(&key);
        self.peer.audio_sender_tracks.borrow_mut().remove(&key);
        match track {
            Some(track) => {
                self.peer
                    .stream_inputs
                    .borrow_mut()
                    .insert(key, crate::peer::SenderTrackLease::new(self, track.clone()));
            }
            None => {
                self.peer.stream_inputs.borrow_mut().remove(&key);
            }
        }
        self.peer.refresh_stream_inputs();
        Ok(true)
    }
    fn validate_video_replacement(&self, track: &VideoTrack) -> Result<(), PeerError> {
        if self.peer._factory.controlled_media && !track.is_admitted_controlled_video() {
            return Err(PeerError {
                kind: PeerErrorKind::UnsupportedOperation,
                message: "controlled sender replacement requires admitted direct VP8/H264 input"
                    .into(),
            });
        }
        let encodings = self.parameters()?.encodings.len();
        if track.is_direct_encoded()
            && if track.is_direct_simulcast() {
                encodings != 3
            } else {
                encodings > 1
            }
        {
            return Err(PeerError {
                kind: PeerErrorKind::UnsupportedParameter,
                message: "encoded input does not match the sender's encoding layout".into(),
            });
        }
        if !track.is_local_to(&self.peer._factory) || track.state() != VideoTrackState::Live {
            return Err(PeerError {
                kind: PeerErrorKind::InvalidParameter,
                message: "sender replacement requires a live track from the same factory".into(),
            });
        }
        Ok(())
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
            self.validate_video_replacement(track)?;
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
                retained.insert(
                    self.resource_key(),
                    crate::peer::SenderTrackLease::new(self, track.clone()),
                );
            }
            None => {
                retained.remove(&self.resource_key());
            }
        }
        drop(retained);
        self.peer.refresh_stream_inputs();
        Ok(())
    }

    /// Invoke the actual SDK GenerateKeyFrame with full RID byte strings and
    /// owning native errors. Audio preserves native UnsupportedOperation;
    /// stopped/unconfigured video preserves the SDK's successful no-op before
    /// RID validation. Success is not proof that an encoder produced a frame.
    pub fn generate_keyframe(&self, rids: &[Vec<u8>]) -> Result<(), RtpSenderOperationError> {
        let rids: Vec<_> = rids
            .iter()
            .map(|rid| ffi::FfiRtpByteString { bytes: rid.clone() })
            .collect();
        let native = ffi::rtp_sender_generate_keyframe(self.native(), self.peer.native(), &rids);
        if native.is_null() {
            return Err(PeerError {
                kind: PeerErrorKind::InvalidState,
                message: "sender peer is unavailable".into(),
            }
            .into());
        }
        let error = crate::RtcError::from_native(native);
        if error.is_ok() {
            Ok(())
        } else {
            Err(RtpSenderOperationError::Native(error))
        }
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

    fn has_negotiated_dependency_descriptor(&self) -> bool {
        const URI: &str = "https://aomediacodec.github.io/av1-rtp-spec/#dependency-descriptor-rtp-header-extension";
        let list = ffi::peer_video_transceivers(self.peer.native());
        let Some(list) = list.as_ref() else {
            return false;
        };
        let sender_id = self.id();
        for index in 0..ffi::transceiver_list_len(list) {
            let transceiver = ffi::transceiver_list_at(list, index);
            let Some(transceiver) = transceiver.as_ref() else {
                continue;
            };
            let sender = ffi::rtp_transceiver_sender(transceiver);
            if sender
                .as_ref()
                .is_some_and(|sender| ffi::rtp_sender_id(sender) == sender_id)
            {
                return ffi::rtp_transceiver_header_extensions(transceiver, true)
                    .into_iter()
                    .any(|extension| {
                        extension.uri == URI
                            && (extension.direction
                                == RtpHeaderExtensionDirection::SendReceive as u8
                                || extension.direction
                                    == RtpHeaderExtensionDirection::SendOnly as u8)
                    });
            }
        }
        false
    }

    /// Complete owned SDK GetParameters snapshot, via the signaling proxy.
    /// Every call starts a new SDK transaction and invalidates previous snapshots
    /// for SetParameters. Children retain no sender/factory origin.
    pub fn get_parameters(&self) -> crate::RtpParameters {
        crate::RtpParameters::from_native(ffi::rtp_sender_parameters_value(self.native()))
    }

    /// Legacy partial overlay snapshot. This is not the full SDK value API.
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
        if self.track().is_some_and(|track| track.is_direct_encoded())
            && (if self
                .track()
                .is_some_and(|track| track.is_direct_simulcast())
            {
                parameters.encodings.len() != 3
            } else {
                parameters.encodings.len() > 1
            } || parameters.encodings.iter().any(|encoding| {
                (encoding.scalability_mode.is_some()
                    && !(self.track().is_some_and(|track| track.is_direct_l1t3())
                        && encoding.scalability_mode.as_deref() == Some("L1T3")))
                    || (!self
                        .track()
                        .is_some_and(|track| track.is_direct_simulcast())
                        && (encoding.scale_resolution_down_to.is_some()
                            || encoding
                                .scale_resolution_down_by
                                .is_some_and(|scale| scale != 1.0)))
            }))
        {
            return Err(PeerError {
                kind: PeerErrorKind::UnsupportedParameter,
                message: "direct encoded input does not support scaling, simulcast, or this scalability mode".into(),
            });
        }
        if self.track().is_some_and(|track| track.is_direct_l1t3())
            && parameters
                .encodings
                .iter()
                .any(|encoding| encoding.scalability_mode.as_deref() == Some("L1T3"))
            && !self.has_negotiated_dependency_descriptor()
        {
            return Err(PeerError {
                kind: PeerErrorKind::UnsupportedParameter,
                message: "direct H264 L1T3 requires a negotiated native dependency descriptor"
                    .into(),
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
                .is_some_and(|v| !v.is_finite() || v < 0.0 || v >= (i32::MAX as f64) + 1.0)
            {
                // SDK video encoder configuration converts this double to int.
                // Truncation is representable below the exclusive MAX + 1 bound.
                return Err(invalid(
                    "maximum framerate is outside safe SDK integer conversion",
                ));
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
                    || u64::from(r.width) * u64::from(r.height) > i32::MAX as u64
            }) {
                // Downstream layer selection calls SDK Resolution::PixelCount,
                // whose signed-int product must remain representable.
                return Err(invalid(
                    "target resolution is outside safe SDK PixelCount consumption",
                ));
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

    /// Install or clear the SDK's refcounted frame encryptor. This is additional
    /// frame encryption, not a replacement for native SRTP authentication.
    pub fn set_frame_encryptor(&self, encryptor: Option<&crate::FrameEncryptor>) {
        let value = encryptor
            .map(|value| value.native() as *const _)
            .unwrap_or(std::ptr::null());
        // SAFETY: the optional pointer is borrowed for this synchronous proxy
        // call; the SDK retains its own reference before the borrow ends.
        unsafe { ffi::rtp_sender_set_frame_encryptor(self.native(), value) };
    }
    pub fn frame_encryptor(&self) -> Option<crate::FrameEncryptor> {
        crate::FrameEncryptor::from_native(ffi::rtp_sender_frame_encryptor(self.native()))
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
    pub(crate) peer: Rc<PeerInner>,
}

impl fmt::Debug for RtpReceiver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RtpReceiver")
            .field("id", &self.id())
            .finish()
    }
}

/// A binding lifecycle failure or an unsafe receiver-delay consumption.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RtpReceiverOperationError {
    Binding(PeerError),
    /// Native jitter-delay conversion CHECKs when the cached delay is NaN.
    UnsafeDelayNaN,
}
impl fmt::Display for RtpReceiverOperationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Binding(error) => error.fmt(f),
            Self::UnsafeDelayNaN => {
                f.write_str("native jitter-buffer delay conversion cannot consume NaN")
            }
        }
    }
}
impl std::error::Error for RtpReceiverOperationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Binding(error) => Some(error),
            Self::UnsafeDelayNaN => None,
        }
    }
}
impl From<PeerError> for RtpReceiverOperationError {
    fn from(error: PeerError) -> Self {
        Self::Binding(error)
    }
}

impl RtpReceiver {
    pub fn track_reference(&self) -> Option<crate::MediaTrack> {
        let native = ffi::rtp_receiver_track_reference(self.native());
        (!native.is_null()).then(|| {
            crate::MediaTrack::from_native(
                native,
                crate::media_track::MediaTrackOwner::Factory {
                    _factory: self.peer._factory.clone(),
                },
            )
        })
    }
    pub fn same_resource(&self, other: &Self) -> bool {
        ffi::rtp_receiver_same_resource(self.native(), other.native())
    }

    /// Preserve native error and transceiver/negotiation side effects. SFrame
    /// key management is distinct from the legacy frame decryptor provider.
    pub fn create_sframe_decrypter(
        &self,
        suite: crate::SframeCipherSuite,
    ) -> Result<crate::SframeDecrypter, crate::SframeCreateError> {
        crate::SframeDecrypter::from_creation(
            ffi::rtp_receiver_create_sframe(self.native(), self.peer.native(), suite.raw()),
            self.peer.clone(),
        )
    }
    /// Construct a decryptor bound permanently to this native receiver.
    pub fn create_frame_decryptor(
        &self,
        provider: impl crate::FrameDecryptorProvider,
    ) -> crate::FrameDecryptor {
        crate::FrameDecryptor::new(self.native(), self.peer.clone(), provider)
    }
    pub fn set_frame_decryptor(
        &self,
        decryptor: Option<&crate::FrameDecryptor>,
    ) -> Result<(), crate::FrameDecryptorInstallError> {
        let value = decryptor
            .map(|value| value.native() as *const _)
            .unwrap_or(std::ptr::null());
        // SAFETY: the native wrapper checks exact receiver identity before
        // installation. The SDK retains its reference during the proxy call.
        unsafe { ffi::rtp_receiver_set_frame_decryptor(self.native(), value) }
            .then_some(())
            .ok_or(crate::FrameDecryptorInstallError::WrongReceiver)
    }
    pub fn frame_decryptor(&self) -> Option<crate::FrameDecryptor> {
        crate::FrameDecryptor::from_native(
            ffi::rtp_receiver_frame_decryptor(self.native()),
            self.peer.clone(),
        )
    }

    /// Copies the SDK's current SSRC and CSRC observations, including native
    /// timestamps and all available header extensions. No track/source join,
    /// received VLA, or passive keyframe notification is inferred.
    pub fn get_sources(&self) -> Vec<crate::RtpSource> {
        let sources = ffi::rtp_receiver_sources(self.native());
        let sources = sources.as_ref().expect("native RTP source snapshot");
        (0..ffi::rtp_source_list_len(sources))
            .map(|index| crate::RtpSource::from_native(ffi::rtp_source_list_get(sources, index)))
            .collect()
    }

    /// Copies the complete SDK receiver parameters through its worker proxy.
    /// The result owns its metadata independently of this receiver and peer.
    /// Unlike the sender getter, this does not start a sender transaction.
    pub fn get_parameters(&self) -> crate::RtpParameters {
        crate::RtpParameters::from_native(ffi::rtp_receiver_parameters_value(self.native()))
    }

    /// Calls the legacy SDK receiver setter. The pinned stock implementation
    /// does not support changes and returns false, including for an unchanged
    /// snapshot. No synthetic success or sender operation is substituted.
    pub fn set_parameters(&self, parameters: &crate::RtpParameters) -> bool {
        ffi::rtp_receiver_set_parameters_value(self.native(), parameters.native())
    }

    /// Sets the native minimum playout delay in seconds. None restores the
    /// native default; Some(0.0) remains an explicit zero. The SDK performs its
    /// own saturation and 0..=10000-ms clamping, including for infinities and
    /// negative values. Only NaN, which triggers a native CHECK, is rejected
    /// before it can be cached for later stream start.
    pub fn set_jitter_buffer_minimum_delay(
        &self,
        seconds: Option<f64>,
    ) -> Result<(), RtpReceiverOperationError> {
        ffi::rtp_receiver_set_jitter_buffer_minimum_delay(
            self.native(),
            seconds.is_some(),
            seconds.unwrap_or(0.0),
        )
        .then_some(())
        .ok_or(RtpReceiverOperationError::UnsafeDelayNaN)
    }

    /// Shared owning SDK first-packet/receptive-change observer slot.
    /// First-packet notification can fan out to all same-kind receivers and
    /// does not establish delivery to this receiver's SSRC.
    pub fn packet_observer(&self) -> crate::RtpReceiverObserver {
        crate::RtpReceiverObserver::from_native(
            ffi::rtp_receiver_observer(self.native()),
            self.peer.clone(),
        )
    }

    pub fn dtls_transport(&self) -> Option<crate::DtlsTransport> {
        crate::DtlsTransport::from_native(
            ffi::rtp_receiver_dtls(self.native(), self.peer.native()),
            self.peer.clone(),
        )
    }

    pub(crate) fn from_native(
        native: cxx::UniquePtr<ffi::NativeRtpReceiver>,
        peer: Rc<PeerInner>,
    ) -> Self {
        Self { native, peer }
    }

    pub fn id(&self) -> String {
        ffi::rtp_receiver_id(self.native())
    }
    pub fn id_bytes(&self) -> Vec<u8> {
        ffi::rtp_receiver_id_bytes(self.native())
    }

    pub fn media_type(&self) -> MediaType {
        MediaType::from_native(ffi::rtp_receiver_media_type(self.native()))
    }

    pub fn stream_ids(&self) -> Vec<String> {
        ffi::rtp_receiver_stream_ids(self.native())
    }
    pub fn stream_id_bytes(&self) -> Vec<Vec<u8>> {
        ffi::rtp_receiver_stream_id_bytes(self.native())
            .into_iter()
            .map(|id| id.bytes)
            .collect()
    }
    pub fn streams(&self) -> Vec<crate::MediaStream> {
        let list = ffi::rtp_receiver_streams(self.native());
        (0..ffi::media_streams_len(&list))
            .map(|i| {
                crate::MediaStream::from_native(
                    ffi::media_streams_get(&list, i),
                    self.peer._factory.clone(),
                )
            })
            .collect()
    }

    pub fn track(&self) -> Option<VideoTrack> {
        let native = ffi::rtp_receiver_track(self.native());
        (!native.is_null()).then(|| VideoTrack::remote(native, self.peer.clone()))
    }

    /// Owning native audio track, distinct from the video-only `track` view.
    pub fn audio_track(&self) -> Option<crate::AudioTrack> {
        let native = ffi::rtp_receiver_audio_track(self.peer.native(), self.native());
        (!native.is_null()).then(|| crate::AudioTrack::received(native, self.peer.clone()))
    }

    /// Attach a bounded decoded PCM sink to a receiver on this peer.
    /// Only one audio sink may be active; close it before selecting either mode
    /// again. Closing an encoded sink restores the native decoder first.
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
                message: "receiver is foreign, ended, not audio, or already has an audio sink"
                    .into(),
            });
        }
        Ok(crate::AudioSink::from_native(
            native,
            self.peer.clone(),
            self.id(),
        ))
    }

    /// Receive encoded Opus without decoding. Select this instead of a decoded
    /// audio sink before media arrives. Only one audio sink may be active.
    /// Closing permits reattachment or an explicit mode change, but restores
    /// native decoding in the interval. For opaque packets, stop reception
    /// before closing and resume only after attaching another encoded sink.
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
                    "receiver is foreign, not negotiated for Opus, or already has an audio sink"
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

    pub(crate) fn native(&self) -> &ffi::NativeRtpReceiver {
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
    /// Native infinity sentinels are preserved as i64::MIN/i64::MAX.
    pub capture_time_us: Option<i64>,
    /// Local receive clock microseconds when supplied by upstream.
    /// Native infinity sentinels are preserved as i64::MIN/i64::MAX.
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
            .field("direction", &self.direction_value())
            .field("current_direction", &self.current_direction_value())
            .field("stopped", &self.stopped())
            .finish()
    }
}

impl RtpTransceiver {
    pub fn same_resource(&self, other: &Self) -> bool {
        ffi::rtp_transceiver_same_resource(self.native(), other.native())
    }

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

    pub fn media_type(&self) -> MediaType {
        MediaType::from_native(ffi::rtp_transceiver_media_type(self.native()))
    }

    pub fn stopping(&self) -> bool {
        ffi::rtp_transceiver_stopping(self.native())
    }

    pub fn receptive(&self) -> bool {
        ffi::rtp_transceiver_receptive(self.native())
    }

    /// Public SDK Chromium-oriented fired-direction metadata, without
    /// conflating absence with any raw signed enum value.
    pub fn fired_direction(&self) -> Option<crate::RtpTransceiverDirectionValue> {
        let mut raw = 0;
        ffi::rtp_transceiver_fired_direction(self.native(), &mut raw)
            .then(|| crate::RtpTransceiverDirectionValue::from_raw(raw))
    }

    /// The SDK's optional SFrame state. None is not replaced with false.
    pub fn sframe_enabled(&self) -> Option<bool> {
        let mut enabled = false;
        ffi::rtp_transceiver_sframe_enabled(self.native(), &mut enabled).then_some(enabled)
    }

    /// Full signed SDK direction metadata, including Stopped and unknown values.
    /// This does not change the closed legacy set_direction consumer contract.
    pub fn direction_value(&self) -> crate::RtpTransceiverDirectionValue {
        crate::RtpTransceiverDirectionValue::from_raw(ffi::rtp_transceiver_direction_value(
            self.native(),
        ))
    }

    /// Exact SDK optional current direction. Native absence is separate from
    /// every raw enum value, including -1; no narrowed integer sentinel is used.
    pub fn current_direction_value(&self) -> Option<crate::RtpTransceiverDirectionValue> {
        let mut raw = 0;
        ffi::rtp_transceiver_current_direction_value(self.native(), &mut raw)
            .then(|| crate::RtpTransceiverDirectionValue::from_raw(raw))
    }

    /// Call SDK SetDirectionWithError with full known enum values and retain
    /// its owning error. Stopped is forwarded for native policy rejection.
    /// Unknown raw directions remain metadata, but are rejected here because
    /// later native offer codec selection CHECKs after the SDK accepts them.
    pub fn set_direction_value(
        &self,
        direction: crate::RtpTransceiverDirectionValue,
    ) -> Result<(), crate::RtpTransceiverOperationError> {
        self.check_capability_peer_open()
            .map_err(crate::RtpTransceiverOperationError::Binding)?;
        let mut safe = false;
        let native =
            ffi::rtp_transceiver_set_direction_value(self.native(), direction.raw(), &mut safe);
        if !safe {
            return Err(crate::RtpTransceiverOperationError::UnsafeDirection(
                direction.raw(),
            ));
        }
        Self::capability_install_result(native)
    }

    /// The public synchronous SDK stop operation, preserving native RTCError
    /// and idempotent already-stopping behavior. This is not StopInternal.
    pub fn stop_standard(&self) -> Result<(), crate::RtpTransceiverOperationError> {
        self.check_capability_peer_open()
            .map_err(crate::RtpTransceiverOperationError::Binding)?;
        Self::capability_install_result(ffi::rtp_transceiver_stop_standard(self.native()))
    }

    /// Legacy four-state convenience direction.
    pub fn direction(&self) -> RtpTransceiverDirection {
        direction(ffi::rtp_transceiver_direction(self.native()))
    }

    /// Legacy narrowed direction metadata. Use current_direction_value for the
    /// complete native enum and explicit optional presence.
    pub fn current_direction(&self) -> Option<RtpTransceiverDirection> {
        u8::try_from(ffi::rtp_transceiver_current_direction(self.native()))
            .ok()
            .map(direction)
    }

    pub fn stopped(&self) -> bool {
        ffi::rtp_transceiver_stopped(self.native())
    }

    /// Complete owned SDK codec preferences, not the legacy format DTOs.
    /// Empty means no override; this does not synthesize factory defaults.
    pub fn rtp_codec_preferences(&self) -> Result<Vec<crate::RtpCodecCapability>, PeerError> {
        self.check_capability_peer_open()?;
        Ok(
            crate::RtpCapabilities::from_native(ffi::rtp_transceiver_codec_preferences_value(
                self.native(),
            ))
            .codecs(),
        )
    }

    /// Complete configured SDK header-extension values for the next negotiation.
    /// Returned entries and their children are independent owned metadata.
    pub fn header_extension_capabilities_to_negotiate(
        &self,
    ) -> Result<Vec<crate::RtpHeaderExtensionCapabilityValue>, PeerError> {
        self.header_extension_capabilities(false)
    }

    /// Complete negotiated SDK snapshot. Unmatched entries are marked Stopped;
    /// pre-negotiation snapshots need not be empty. IDs and encryption are the
    /// actual SDK metadata, not sentinel-reconstructed or filtered DTO fields.
    pub fn negotiated_header_extension_capabilities(
        &self,
    ) -> Result<Vec<crate::RtpHeaderExtensionCapabilityValue>, PeerError> {
        self.header_extension_capabilities(true)
    }

    fn header_extension_capabilities(
        &self,
        negotiated: bool,
    ) -> Result<Vec<crate::RtpHeaderExtensionCapabilityValue>, PeerError> {
        self.check_capability_peer_open()?;
        Ok(
            crate::RtpCapabilities::from_native(ffi::rtp_transceiver_header_capabilities_value(
                self.native(),
                negotiated,
            ))
            .header_extensions(),
        )
    }

    /// Pass complete codec values to the SDK, including all inherited metadata.
    /// Empty clears the override. SDK validation and deduplication are retained;
    /// this does not substitute sender-factory capabilities by name/parameters.
    /// SDK failures retain their complete owning RTCError.
    pub fn set_rtp_codec_preferences(
        &self,
        codecs: &[crate::RtpCodecCapability],
    ) -> Result<(), crate::RtpCapabilityInstallError> {
        self.check_capability_peer_open()
            .map_err(crate::RtpCapabilityInstallError::Binding)?;
        let mut values = crate::RtpCapabilities::default();
        values.set_codecs(codecs);
        Self::capability_install_result(ffi::rtp_transceiver_set_codec_preferences_value(
            self.native(),
            values.native(),
        ))
    }

    /// Pass the complete ordered configured snapshot to the SDK. The native
    /// setter validates URI/order and mandatory extensions, then changes only
    /// direction. Supplied IDs/encryption are metadata, not installation requests.
    /// Unknown directions are retained where SDK semantics allow; this path
    /// neither formats them nor converts them through the legacy closed DTO.
    pub fn set_header_extension_capabilities_to_negotiate(
        &self,
        extensions: &[crate::RtpHeaderExtensionCapabilityValue],
    ) -> Result<(), crate::RtpCapabilityInstallError> {
        self.check_capability_peer_open()
            .map_err(crate::RtpCapabilityInstallError::Binding)?;
        let mut values = crate::RtpCapabilities::default();
        values.set_header_extensions(extensions);
        Self::capability_install_result(ffi::rtp_transceiver_set_header_capabilities_value(
            self.native(),
            values.native(),
        ))
    }

    fn capability_install_result(
        native: cxx::UniquePtr<ffi::NativeRtcError>,
    ) -> Result<(), crate::RtpCapabilityInstallError> {
        let error = crate::RtcError::from_native(native);
        if error.is_ok() {
            Ok(())
        } else {
            Err(crate::RtpCapabilityInstallError::Native(error))
        }
    }

    fn check_capability_peer_open(&self) -> Result<(), PeerError> {
        if self.peer.closed.get() {
            Err(PeerError {
                kind: PeerErrorKind::Closed,
                message: "peer is closed".into(),
            })
        } else {
            Ok(())
        }
    }

    /// Legacy configured extension DTO for the next SDP negotiation.
    pub fn header_extensions_to_negotiate(
        &self,
    ) -> Result<Vec<RtpHeaderExtensionCapability>, PeerError> {
        self.header_extensions(false)
    }

    /// Native negotiated extension snapshot. Entries marked `Stopped` are not
    /// negotiated. Before negotiation the native snapshot may contain stopped
    /// entries rather than an empty vector.
    pub fn negotiated_header_extensions(
        &self,
    ) -> Result<Vec<RtpHeaderExtensionCapability>, PeerError> {
        self.header_extensions(true)
    }

    fn header_extensions(
        &self,
        negotiated: bool,
    ) -> Result<Vec<RtpHeaderExtensionCapability>, PeerError> {
        if self.peer.closed.get() {
            return Err(PeerError {
                kind: PeerErrorKind::Closed,
                message: "peer is closed".into(),
            });
        }
        ffi::rtp_transceiver_header_extensions(self.native(), negotiated)
            .into_iter()
            .map(|value| {
                let direction = match value.direction {
                    0 => RtpHeaderExtensionDirection::SendReceive,
                    1 => RtpHeaderExtensionDirection::SendOnly,
                    2 => RtpHeaderExtensionDirection::ReceiveOnly,
                    3 => RtpHeaderExtensionDirection::Inactive,
                    4 => RtpHeaderExtensionDirection::Stopped,
                    _ => {
                        return Err(PeerError {
                            kind: PeerErrorKind::Internal,
                            message: "unknown native extension direction".into(),
                        });
                    }
                };
                Ok(RtpHeaderExtensionCapability {
                    uri: value.uri,
                    preferred_id: (value.preferred_id >= 0).then_some(value.preferred_id),
                    preferred_encrypt: value.preferred_encrypt,
                    direction,
                })
            })
            .collect()
    }

    /// Apply directions to the full ordered snapshot returned by
    /// [`Self::header_extensions_to_negotiate`]. Native validation checks the
    /// list and mandatory extensions. IDs and encryption remain native-owned.
    /// This changes the next negotiation, not already negotiated transport.
    pub fn set_header_extensions_to_negotiate(
        &self,
        extensions: &[RtpHeaderExtensionCapability],
    ) -> Result<(), PeerError> {
        if self.peer.closed.get() {
            return Err(PeerError {
                kind: PeerErrorKind::Closed,
                message: "peer is closed".into(),
            });
        }
        let extensions: Vec<_> = extensions
            .iter()
            .map(|value| ffi::FfiRtpHeaderExtension {
                uri: value.uri.clone(),
                preferred_id: value.preferred_id.unwrap_or(-1),
                preferred_encrypt: value.preferred_encrypt,
                direction: value.direction as u8,
            })
            .collect();
        let mut kind = 0;
        let mut error = String::new();
        if ffi::rtp_transceiver_set_header_extensions(
            self.native(),
            &extensions,
            &mut kind,
            &mut error,
        ) {
            Ok(())
        } else {
            Err(PeerError {
                kind: error_kind(kind),
                message: error,
            })
        }
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
