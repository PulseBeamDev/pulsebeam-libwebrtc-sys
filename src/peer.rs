use std::{cell::Cell, fmt, rc::Rc};

use crate::{
    AudioDecoderFactory, AudioEncoderFactory, Environment, NetworkManagerProvider, NetworkThread,
    PacketSocketFactoryProvider, SignalingThread, VideoDecoderFactoryHandle,
    VideoEncoderFactoryHandle, WorkerThread, data_channel::DataChannel, ffi,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OperationId(u64);

impl OperationId {
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum SessionDescriptionType {
    Offer = 0,
    ProvisionalAnswer = 1,
    Answer = 2,
    Rollback = 3,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionDescription {
    pub kind: SessionDescriptionType,
    pub sdp: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IceCandidate {
    pub sdp_mid: String,
    pub sdp_mline_index: i32,
    pub candidate: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignalingState {
    Stable,
    HaveLocalOffer,
    HaveLocalProvisionalAnswer,
    HaveRemoteOffer,
    HaveRemoteProvisionalAnswer,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionState {
    New,
    Connecting,
    Connected,
    Disconnected,
    Failed,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IceGatheringState {
    New,
    Gathering,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeerErrorKind {
    UnsupportedOperation,
    UnsupportedParameter,
    InvalidParameter,
    InvalidRange,
    Syntax,
    InvalidState,
    InvalidModification,
    Network,
    ResourceExhausted,
    Internal,
    Operation,
    Closed,
    NativeConstruction,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PeerError {
    pub kind: PeerErrorKind,
    pub message: String,
}

impl fmt::Display for PeerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl std::error::Error for PeerError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationCompletion {
    pub operation_id: OperationId,
    pub result: Result<Option<SessionDescription>, PeerError>,
}

#[derive(Debug)]
pub enum PeerConnectionEvent {
    OperationComplete(OperationCompletion),
    IceCandidate(IceCandidate),
    ConnectionStateChanged(ConnectionState),
    SignalingStateChanged(SignalingState),
    IceGatheringStateChanged(IceGatheringState),
    NegotiationNeeded {
        event_id: u32,
    },
    IceCandidateError {
        address: String,
        port: i32,
        url: String,
        error_code: i32,
        message: String,
    },
    DataChannel(DataChannel),
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PeerConfiguration {
    pub ice_candidate_pool_size: u16,
    pub always_negotiate_data_channels: bool,
}

impl Default for PeerConfiguration {
    fn default() -> Self {
        Self {
            ice_candidate_pool_size: 0,
            always_negotiate_data_channels: true,
        }
    }
}

pub struct PeerConnectionFactoryBuilder {
    environment: Option<Environment>,
    network_thread: Option<NetworkThread>,
    worker_thread: Option<WorkerThread>,
    signaling_thread: Option<SignalingThread>,
    network_manager: Option<NetworkManagerProvider>,
    packet_socket_factory: Option<PacketSocketFactoryProvider>,
    audio_encoder: Option<AudioEncoderFactory>,
    audio_decoder: Option<AudioDecoderFactory>,
    video_encoder: Option<VideoEncoderFactoryHandle>,
    video_decoder: Option<VideoDecoderFactoryHandle>,
}

impl PeerConnectionFactoryBuilder {
    pub fn environment(mut self, environment: Environment) -> Self {
        self.environment = Some(environment);
        self
    }

    pub fn network_thread(mut self, thread: NetworkThread) -> Self {
        self.network_thread = Some(thread);
        self
    }

    pub fn worker_thread(mut self, thread: WorkerThread) -> Self {
        self.worker_thread = Some(thread);
        self
    }

    pub fn signaling_thread(mut self, thread: SignalingThread) -> Self {
        self.signaling_thread = Some(thread);
        self
    }

    pub fn network_manager(mut self, provider: NetworkManagerProvider) -> Self {
        self.network_manager = Some(provider);
        self
    }

    pub fn packet_socket_factory(mut self, provider: PacketSocketFactoryProvider) -> Self {
        self.packet_socket_factory = Some(provider);
        self
    }

    pub fn audio_encoder_factory(mut self, factory: AudioEncoderFactory) -> Self {
        self.audio_encoder = Some(factory);
        self
    }

    pub fn audio_decoder_factory(mut self, factory: AudioDecoderFactory) -> Self {
        self.audio_decoder = Some(factory);
        self
    }

    pub fn video_encoder_factory(mut self, factory: VideoEncoderFactoryHandle) -> Self {
        self.video_encoder = Some(factory);
        self
    }

    pub fn video_decoder_factory(mut self, factory: VideoDecoderFactoryHandle) -> Self {
        self.video_decoder = Some(factory);
        self
    }

    pub fn build(self) -> Result<PeerConnectionFactory, PeerError> {
        let environment = match self.environment {
            Some(environment) => environment,
            None => Environment::builder().build().map_err(native_build_error)?,
        };
        let network_thread = match self.network_thread {
            Some(thread) => thread,
            None => NetworkThread::start().map_err(native_build_error)?,
        };
        let worker_thread = match self.worker_thread {
            Some(thread) => thread,
            None => WorkerThread::start().map_err(native_build_error)?,
        };
        let signaling_thread = match self.signaling_thread {
            Some(thread) => thread,
            None => SignalingThread::start().map_err(native_build_error)?,
        };
        let mut message = String::new();
        // SAFETY: every non-null pointer is retained in FactoryInner and its
        // native factory is destroyed before those dependencies are dropped.
        let native = unsafe {
            ffi::new_peer_connection_factory(
                environment.native(),
                network_thread.native(),
                worker_thread.native(),
                signaling_thread.native(),
                optional_ptr(self.network_manager.as_ref().map(|value| value.native())),
                optional_ptr(
                    self.packet_socket_factory
                        .as_ref()
                        .map(|value| value.native()),
                ),
                optional_ptr(self.audio_encoder.as_ref().map(|value| value.native())),
                optional_ptr(self.audio_decoder.as_ref().map(|value| value.native())),
                optional_ptr(self.video_encoder.as_ref().map(|value| value.native())),
                optional_ptr(self.video_decoder.as_ref().map(|value| value.native())),
                &mut message,
            )
        };
        if native.is_null() {
            return Err(PeerError {
                kind: PeerErrorKind::NativeConstruction,
                message,
            });
        }
        Ok(PeerConnectionFactory(Rc::new(FactoryInner {
            native,
            _environment: environment,
            _network_thread: network_thread,
            _worker_thread: worker_thread,
            _signaling_thread: signaling_thread,
            _network_manager: self.network_manager,
            _packet_socket_factory: self.packet_socket_factory,
            _audio_encoder: self.audio_encoder,
            _audio_decoder: self.audio_decoder,
            _video_encoder: self.video_encoder,
            _video_decoder: self.video_decoder,
        })))
    }
}

impl Default for PeerConnectionFactoryBuilder {
    fn default() -> Self {
        Self {
            environment: None,
            network_thread: None,
            worker_thread: None,
            signaling_thread: None,
            network_manager: None,
            packet_socket_factory: None,
            audio_encoder: None,
            audio_decoder: None,
            video_encoder: None,
            video_decoder: None,
        }
    }
}

struct FactoryInner {
    native: cxx::UniquePtr<ffi::NativePeerConnectionFactory>,
    _environment: Environment,
    _network_thread: NetworkThread,
    _worker_thread: WorkerThread,
    _signaling_thread: SignalingThread,
    _network_manager: Option<NetworkManagerProvider>,
    _packet_socket_factory: Option<PacketSocketFactoryProvider>,
    _audio_encoder: Option<AudioEncoderFactory>,
    _audio_decoder: Option<AudioDecoderFactory>,
    _video_encoder: Option<VideoEncoderFactoryHandle>,
    _video_decoder: Option<VideoDecoderFactoryHandle>,
}

/// A sequence-bound owner for modular WebRTC peer-connection construction.
///
/// ```compile_fail
/// fn assert_send<T: Send>() {}
/// assert_send::<pulsebeam_webrtc_sys::PeerConnectionFactory>();
/// ```
///
/// ```compile_fail
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<pulsebeam_webrtc_sys::PeerConnectionFactory>();
/// ```
pub struct PeerConnectionFactory(Rc<FactoryInner>);

impl PeerConnectionFactory {
    pub fn builder() -> PeerConnectionFactoryBuilder {
        PeerConnectionFactoryBuilder::default()
    }

    pub fn create_peer_connection(
        &self,
        configuration: PeerConfiguration,
    ) -> Result<PeerConnection, PeerError> {
        if configuration.ice_candidate_pool_size > u8::MAX.into() {
            return Err(PeerError {
                kind: PeerErrorKind::InvalidRange,
                message: "ICE candidate pool size must be at most 255".into(),
            });
        }
        let mut message = String::new();
        let native = ffi::create_peer_connection(
            self.0.native.as_ref().expect("validated peer factory"),
            configuration.ice_candidate_pool_size,
            configuration.always_negotiate_data_channels,
            &mut message,
        );
        if native.is_null() {
            Err(PeerError {
                kind: PeerErrorKind::NativeConstruction,
                message,
            })
        } else {
            Ok(PeerConnection {
                inner: Rc::new(PeerInner {
                    native,
                    _factory: self.0.clone(),
                }),
                next_operation_id: Cell::new(1),
            })
        }
    }
}

/// A sequence-bound peer connection with a caller-polled owned event queue.
///
/// ```compile_fail
/// fn assert_send<T: Send>() {}
/// assert_send::<pulsebeam_webrtc_sys::PeerConnection>();
/// ```
///
/// ```compile_fail
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<pulsebeam_webrtc_sys::PeerConnection>();
/// ```
pub struct PeerConnection {
    pub(crate) inner: Rc<PeerInner>,
    next_operation_id: Cell<u64>,
}

pub(crate) struct PeerInner {
    native: cxx::UniquePtr<ffi::NativePeerConnection>,
    _factory: Rc<FactoryInner>,
}

impl PeerConnection {
    pub fn create_offer(&self) -> OperationId {
        let id = self.next_operation();
        ffi::peer_create_offer(self.native(), id.0);
        id
    }

    pub fn create_answer(&self) -> OperationId {
        let id = self.next_operation();
        ffi::peer_create_answer(self.native(), id.0);
        id
    }

    pub fn set_local_description(&self, description: SessionDescription) -> OperationId {
        let id = self.next_operation();
        ffi::peer_set_local_description(
            self.native(),
            id.0,
            description.kind as u8,
            &description.sdp,
        );
        id
    }

    pub fn set_remote_description(&self, description: SessionDescription) -> OperationId {
        let id = self.next_operation();
        ffi::peer_set_remote_description(
            self.native(),
            id.0,
            description.kind as u8,
            &description.sdp,
        );
        id
    }

    pub fn add_ice_candidate(&self, candidate: IceCandidate) -> OperationId {
        let id = self.next_operation();
        ffi::peer_add_ice_candidate(
            self.native(),
            id.0,
            &candidate.sdp_mid,
            candidate.sdp_mline_index,
            &candidate.candidate,
        );
        id
    }

    pub fn try_next_event(&self) -> Option<PeerConnectionEvent> {
        event_from_ffi(ffi::peer_take_event(self.native()), &self.inner)
    }

    pub fn close(&mut self) -> Result<(), PeerError> {
        ffi::close_peer_connection(self.native())
            .then_some(())
            .ok_or_else(|| PeerError {
                kind: PeerErrorKind::Internal,
                message: "failed to close peer connection".into(),
            })
    }

    fn next_operation(&self) -> OperationId {
        let current = self.next_operation_id.get();
        self.next_operation_id
            .set(current.checked_add(1).unwrap_or(1));
        OperationId(current)
    }

    fn native(&self) -> &ffi::NativePeerConnection {
        self.inner.native()
    }
}

impl PeerInner {
    pub(crate) fn native(&self) -> &ffi::NativePeerConnection {
        self.native.as_ref().expect("validated peer connection")
    }
}

fn native_build_error(error: impl fmt::Display) -> PeerError {
    PeerError {
        kind: PeerErrorKind::NativeConstruction,
        message: error.to_string(),
    }
}

fn optional_ptr<T>(value: Option<&T>) -> *const T {
    value.map_or(std::ptr::null(), |value| value as *const T)
}

fn event_from_ffi(event: ffi::FfiPeerEvent, peer: &Rc<PeerInner>) -> Option<PeerConnectionEvent> {
    match event.kind {
        0 => None,
        1 => Some(PeerConnectionEvent::OperationComplete(
            OperationCompletion {
                operation_id: OperationId(event.operation_id),
                result: if event.error_type == 0 {
                    Ok(event.has_description.then(|| SessionDescription {
                        kind: sdp_type(event.sdp_type),
                        sdp: event.sdp,
                    }))
                } else {
                    Err(PeerError {
                        kind: error_kind(event.error_type),
                        message: event.message,
                    })
                },
            },
        )),
        2 => Some(PeerConnectionEvent::IceCandidate(IceCandidate {
            sdp_mid: event.sdp_mid,
            sdp_mline_index: event.sdp_mline_index,
            candidate: event.candidate,
        })),
        3 => Some(PeerConnectionEvent::ConnectionStateChanged(
            connection_state(event.state),
        )),
        4 => Some(PeerConnectionEvent::SignalingStateChanged(signaling_state(
            event.state,
        ))),
        5 => Some(PeerConnectionEvent::IceGatheringStateChanged(
            gathering_state(event.state),
        )),
        6 => Some(PeerConnectionEvent::NegotiationNeeded {
            event_id: event.event_id,
        }),
        7 => Some(PeerConnectionEvent::IceCandidateError {
            address: event.address,
            port: event.port,
            url: event.url,
            error_code: event.error_code,
            message: event.message,
        }),
        8 => Some(PeerConnectionEvent::Closed),
        9 => {
            let native = ffi::peer_take_data_channel(peer.native(), event.operation_id);
            assert!(
                !native.is_null(),
                "native adapter lost a remote data channel"
            );
            Some(PeerConnectionEvent::DataChannel(DataChannel::from_native(
                native,
                peer.clone(),
            )))
        }
        _ => unreachable!("native adapter returned an invalid peer event"),
    }
}

fn sdp_type(value: u8) -> SessionDescriptionType {
    match value {
        0 => SessionDescriptionType::Offer,
        1 => SessionDescriptionType::ProvisionalAnswer,
        2 => SessionDescriptionType::Answer,
        3 => SessionDescriptionType::Rollback,
        _ => unreachable!("native adapter returned an invalid SDP type"),
    }
}

fn connection_state(value: u8) -> ConnectionState {
    match value {
        0 => ConnectionState::New,
        1 => ConnectionState::Connecting,
        2 => ConnectionState::Connected,
        3 => ConnectionState::Disconnected,
        4 => ConnectionState::Failed,
        5 => ConnectionState::Closed,
        _ => unreachable!("native adapter returned an invalid connection state"),
    }
}

fn signaling_state(value: u8) -> SignalingState {
    match value {
        0 => SignalingState::Stable,
        1 => SignalingState::HaveLocalOffer,
        2 => SignalingState::HaveLocalProvisionalAnswer,
        3 => SignalingState::HaveRemoteOffer,
        4 => SignalingState::HaveRemoteProvisionalAnswer,
        5 => SignalingState::Closed,
        _ => unreachable!("native adapter returned an invalid signaling state"),
    }
}

fn gathering_state(value: u8) -> IceGatheringState {
    match value {
        0 => IceGatheringState::New,
        1 => IceGatheringState::Gathering,
        2 => IceGatheringState::Complete,
        _ => unreachable!("native adapter returned an invalid ICE gathering state"),
    }
}

pub(crate) fn error_kind(value: u8) -> PeerErrorKind {
    match value {
        1 => PeerErrorKind::UnsupportedOperation,
        2 => PeerErrorKind::UnsupportedParameter,
        3 => PeerErrorKind::InvalidParameter,
        4 => PeerErrorKind::InvalidRange,
        5 => PeerErrorKind::Syntax,
        6 => PeerErrorKind::InvalidState,
        7 => PeerErrorKind::InvalidModification,
        8 => PeerErrorKind::Network,
        9 => PeerErrorKind::ResourceExhausted,
        10 => PeerErrorKind::Internal,
        11 => PeerErrorKind::Operation,
        255 => PeerErrorKind::Closed,
        _ => PeerErrorKind::Internal,
    }
}
