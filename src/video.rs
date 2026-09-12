use std::{fmt, rc::Rc};

use crate::{
    CodecError, VideoFrame, ffi,
    peer::{FactoryInner, PeerError, PeerInner, error_kind},
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

impl RtpSender {
    pub fn id(&self) -> String {
        ffi::rtp_sender_id(self.native())
    }

    pub fn track(&self) -> Option<VideoTrack> {
        let native = ffi::rtp_sender_track(self.native());
        (!native.is_null()).then(|| VideoTrack::remote(native, self.peer.clone()))
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
