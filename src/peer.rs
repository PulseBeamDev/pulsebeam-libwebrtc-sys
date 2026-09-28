use std::{cell::Cell, fmt, rc::Rc};

use crate::{
    AudioDecoderFactory, AudioEncoderFactory, Environment, NetworkManagerProvider, NetworkThread,
    PacketSocketFactoryProvider, SignalingThread, VideoDecoderFactoryHandle,
    VideoEncoderFactoryHandle, WorkerThread,
    data_channel::DataChannel,
    ffi,
    video::{
        RtpReceiver, RtpSender, RtpTransceiver, RtpTransceiverDirection, VideoSource, VideoTrack,
    },
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

/// An atomic snapshot of negotiated and in-progress local/remote SDP.
/// Descriptions are copied on the signaling thread and owned by the caller.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PeerDescriptions {
    pub current_local: Option<SessionDescription>,
    pub current_remote: Option<SessionDescription>,
    pub pending_local: Option<SessionDescription>,
    pub pending_remote: Option<SessionDescription>,
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

/// A bounded snapshot of upstream stats. Timestamps are microseconds since
/// the Unix epoch; a missing metric is `None`, never an implicit zero.
#[derive(Clone, Debug, PartialEq)]
pub struct PeerStatsSnapshot {
    pub operation_id: OperationId,
    pub timestamp_us: i64,
    pub records: Vec<PeerStatsRecord>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PeerStatsRecord {
    CandidatePair(CandidatePairStats),
    Transport(TransportStats),
    InboundRtp(InboundRtpStats),
    OutboundRtp(OutboundRtpStats),
    DataChannel(DataChannelStats),
}

#[derive(Clone, Debug, PartialEq)]
pub struct CandidatePairStats {
    pub id: String,
    pub timestamp_us: i64,
    pub transport_id: Option<String>,
    pub local_candidate_id: Option<String>,
    pub remote_candidate_id: Option<String>,
    pub state: Option<String>,
    pub nominated: Option<bool>,
    pub packets_sent: Option<u64>,
    pub packets_received: Option<u64>,
    pub bytes_sent: Option<u64>,
    pub bytes_received: Option<u64>,
    pub current_round_trip_time: Option<f64>,
    pub available_outgoing_bitrate: Option<f64>,
    pub available_incoming_bitrate: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TransportStats {
    pub id: String,
    pub timestamp_us: i64,
    pub selected_candidate_pair_id: Option<String>,
    pub ice_state: Option<String>,
    pub dtls_state: Option<String>,
    pub packets_sent: Option<u64>,
    pub packets_received: Option<u64>,
    pub bytes_sent: Option<u64>,
    pub bytes_received: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InboundRtpStats {
    pub id: String,
    pub timestamp_us: i64,
    pub kind: Option<String>,
    pub ssrc: Option<u64>,
    pub transport_id: Option<String>,
    pub mid: Option<String>,
    pub packets_received: Option<u64>,
    pub packets_lost: Option<i64>,
    pub bytes_received: Option<u64>,
    pub jitter: Option<f64>,
    pub frames_received: Option<u64>,
    pub frames_decoded: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OutboundRtpStats {
    pub id: String,
    pub timestamp_us: i64,
    pub kind: Option<String>,
    pub ssrc: Option<u64>,
    pub transport_id: Option<String>,
    pub mid: Option<String>,
    pub rid: Option<String>,
    pub packets_sent: Option<u64>,
    pub bytes_sent: Option<u64>,
    pub target_bitrate: Option<f64>,
    pub frames_encoded: Option<u64>,
    pub frames_sent: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DataChannelStats {
    pub id: String,
    pub timestamp_us: i64,
    pub label: Option<String>,
    pub protocol: Option<String>,
    pub state: Option<String>,
    pub data_channel_identifier: Option<i64>,
    pub messages_sent: Option<u64>,
    pub messages_received: Option<u64>,
    pub bytes_sent: Option<u64>,
    pub bytes_received: Option<u64>,
}

#[derive(Debug)]
pub enum PeerConnectionEvent {
    Stats(PeerStatsSnapshot),
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
    Track(RtpTransceiver),
    TrackRemoved(RtpReceiver),
    Closed,
}

/// ICE server URLs use `stun:`, `stuns:`, `turn:` or `turns:` URI schemes.
/// TURN UDP/TCP/TLS transport is selected by the URL (`?transport=udp` or
/// `?transport=tcp`); `turns:` requires a valid server TLS certificate.
#[derive(Clone, Eq, PartialEq)]
pub struct IceServer {
    pub urls: Vec<String>,
    pub username: String,
    pub password: String,
}

impl fmt::Debug for IceServer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IceServer")
            .field("urls", &self.urls)
            .field("username", &self.username)
            .field("password", &"[redacted]")
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum IceTransportPolicy {
    #[default]
    All,
    RelayOnly,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PeerConfiguration {
    pub ice_candidate_pool_size: u16,
    pub always_negotiate_data_channels: bool,
    pub ice_servers: Vec<IceServer>,
    pub ice_transport_policy: IceTransportPolicy,
    /// Additional PEM-encoded CA trusted for TURN/TLS on this peer. The pinned
    /// WebRTC roots remain trusted, and the server hostname is still checked.
    /// `None` uses only upstream trust anchors.
    pub turn_tls_ca_pem: Option<String>,
}

impl Default for PeerConfiguration {
    fn default() -> Self {
        Self {
            ice_candidate_pool_size: 0,
            always_negotiate_data_channels: true,
            ice_servers: Vec::new(),
            ice_transport_policy: IceTransportPolicy::All,
            turn_tls_ca_pem: None,
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

pub(crate) struct FactoryInner {
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
        if configuration.turn_tls_ca_pem.as_deref() == Some("") {
            return Err(PeerError {
                kind: PeerErrorKind::InvalidParameter,
                message: "TURN TLS CA certificate cannot be empty".into(),
            });
        }
        let mut ice_servers = Vec::new();
        for server in &configuration.ice_servers {
            if server.urls.is_empty() {
                return Err(invalid_ice_server("ICE server requires at least one URL"));
            }
            for url in &server.urls {
                let turn = url.starts_with("turn:") || url.starts_with("turns:");
                let scheme = if turn {
                    if url.starts_with("turns:") {
                        "turns:"
                    } else {
                        "turn:"
                    }
                } else if url.starts_with("stuns:") {
                    "stuns:"
                } else {
                    "stun:"
                };
                if !url.starts_with(scheme)
                    || url[scheme.len()..].is_empty()
                    || url.chars().any(char::is_whitespace)
                {
                    return Err(invalid_ice_server("invalid STUN/TURN server URL"));
                }
                if turn && (server.username.is_empty() || server.password.is_empty()) {
                    return Err(invalid_ice_server("TURN requires username and password"));
                }
                ice_servers.push(ffi::FfiIceServer {
                    url: url.clone(),
                    username: server.username.clone(),
                    password: server.password.clone(),
                });
            }
        }
        let mut message = String::new();
        let native = ffi::create_peer_connection(
            self.0.native.as_ref().expect("validated peer factory"),
            configuration.ice_candidate_pool_size,
            configuration.always_negotiate_data_channels,
            &ice_servers,
            configuration.ice_transport_policy == IceTransportPolicy::RelayOnly,
            configuration.turn_tls_ca_pem.as_deref().unwrap_or(""),
            &mut message,
        );
        if native.is_null() {
            Err(PeerError {
                kind: if message == "invalid TURN TLS CA certificate" {
                    PeerErrorKind::InvalidParameter
                } else {
                    PeerErrorKind::NativeConstruction
                },
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

    pub fn create_video_source(&self) -> Result<VideoSource, PeerError> {
        let native =
            ffi::create_video_source(self.0.native.as_ref().expect("validated peer factory"));
        if native.is_null() {
            Err(native_build_error("failed to create video source"))
        } else {
            Ok(VideoSource::from_native(native, self.0.clone()))
        }
    }

    pub fn create_video_track(
        &self,
        id: &str,
        source: &VideoSource,
    ) -> Result<VideoTrack, PeerError> {
        if id.is_empty() {
            return Err(PeerError {
                kind: PeerErrorKind::InvalidParameter,
                message: "video track id must not be empty".into(),
            });
        }
        let native = ffi::create_video_track(
            self.0.native.as_ref().expect("validated peer factory"),
            source.native(),
            id,
        );
        if native.is_null() {
            Err(native_build_error("failed to create video track"))
        } else {
            Ok(VideoTrack::local(native, self.0.clone(), source))
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
    pub fn add_video_transceiver(
        &self,
        track: &VideoTrack,
        direction: RtpTransceiverDirection,
    ) -> Result<RtpTransceiver, PeerError> {
        let mut error_type = 0;
        let mut message = String::new();
        let native = ffi::peer_add_video_transceiver(
            self.inner.native(),
            track.native(),
            direction as u8,
            &mut error_type,
            &mut message,
        );
        if native.is_null() {
            Err(PeerError {
                kind: error_kind(error_type),
                message,
            })
        } else {
            Ok(RtpTransceiver::from_native(native, self.inner.clone()))
        }
    }

    pub fn remove_track(&self, sender: &RtpSender) -> Result<(), PeerError> {
        let mut error_type = 0;
        let mut message = String::new();
        ffi::peer_remove_track(
            self.inner.native(),
            sender.native(),
            &mut error_type,
            &mut message,
        )
        .then_some(())
        .ok_or_else(|| PeerError {
            kind: error_kind(error_type),
            message,
        })
    }

    pub fn create_offer(&self) -> OperationId {
        self.create_offer_with_ice_restart(false)
    }

    /// Request new ICE credentials and candidate gathering in the next offer.
    /// The operation completes through `OperationComplete` like a normal offer.
    pub fn create_ice_restart_offer(&self) -> OperationId {
        self.create_offer_with_ice_restart(true)
    }

    fn create_offer_with_ice_restart(&self, ice_restart: bool) -> OperationId {
        let id = self.next_operation();
        ffi::peer_create_offer(self.native(), id.0, ice_restart);
        id
    }

    pub fn create_answer(&self) -> OperationId {
        let id = self.next_operation();
        ffi::peer_create_answer(self.native(), id.0);
        id
    }

    pub fn descriptions(&self) -> Result<PeerDescriptions, PeerError> {
        let mut native = Vec::new();
        match ffi::peer_descriptions(self.native(), &mut native) {
            0 => {
                let mut result = PeerDescriptions::default();
                for entry in native {
                    let description = Some(SessionDescription {
                        kind: sdp_type(entry.kind),
                        sdp: entry.sdp,
                    });
                    match entry.slot {
                        0 => result.current_local = description,
                        1 => result.current_remote = description,
                        2 => result.pending_local = description,
                        3 => result.pending_remote = description,
                        _ => unreachable!("native adapter returned an invalid SDP slot"),
                    }
                }
                Ok(result)
            }
            1 => Err(PeerError {
                kind: PeerErrorKind::Closed,
                message: "peer connection is closed".into(),
            }),
            _ => Err(PeerError {
                kind: PeerErrorKind::Internal,
                message: "failed to serialize peer descriptions".into(),
            }),
        }
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

    /// Request one typed stats snapshot. Only one request may be outstanding
    /// until its event is taken, keeping queued snapshots bounded. A request
    /// rejected here has no asynchronous completion.
    pub fn request_stats(&self) -> Result<OperationId, PeerError> {
        let id = self.next_operation();
        if ffi::peer_request_stats(self.native(), id.0) {
            Ok(id)
        } else {
            Err(PeerError {
                kind: PeerErrorKind::InvalidState,
                message: "peer closed or previous stats result not yet consumed".into(),
            })
        }
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

fn invalid_ice_server(message: &str) -> PeerError {
    PeerError {
        kind: PeerErrorKind::InvalidParameter,
        message: message.into(),
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

struct StatFields<'a>(&'a [ffi::FfiStatsField]);

impl StatFields<'_> {
    fn field(&self, name: &str, kind: u8) -> Option<&ffi::FfiStatsField> {
        self.0
            .iter()
            .find(|field| field.name == name && field.kind == kind)
    }

    fn text(&self, name: &str) -> Option<String> {
        self.field(name, 1).map(|field| field.text.clone())
    }

    fn unsigned(&self, name: &str) -> Option<u64> {
        self.field(name, 2).map(|field| field.unsigned_value)
    }

    fn signed(&self, name: &str) -> Option<i64> {
        self.field(name, 3).map(|field| field.signed_value)
    }

    fn decimal(&self, name: &str) -> Option<f64> {
        self.field(name, 4).map(|field| field.decimal)
    }

    fn flag(&self, name: &str) -> Option<bool> {
        self.field(name, 5).map(|field| field.flag)
    }
}

fn stats_record_from_ffi(record: ffi::FfiStatsRecord) -> PeerStatsRecord {
    let fields = StatFields(&record.fields);
    let id = record.id;
    let timestamp_us = record.timestamp_us;
    match record.kind {
        1 => PeerStatsRecord::CandidatePair(CandidatePairStats {
            id,
            timestamp_us,
            transport_id: fields.text("transport_id"),
            local_candidate_id: fields.text("local_candidate_id"),
            remote_candidate_id: fields.text("remote_candidate_id"),
            state: fields.text("state"),
            nominated: fields.flag("nominated"),
            packets_sent: fields.unsigned("packets_sent"),
            packets_received: fields.unsigned("packets_received"),
            bytes_sent: fields.unsigned("bytes_sent"),
            bytes_received: fields.unsigned("bytes_received"),
            current_round_trip_time: fields.decimal("current_round_trip_time"),
            available_outgoing_bitrate: fields.decimal("available_outgoing_bitrate"),
            available_incoming_bitrate: fields.decimal("available_incoming_bitrate"),
        }),
        2 => PeerStatsRecord::Transport(TransportStats {
            id,
            timestamp_us,
            selected_candidate_pair_id: fields.text("selected_candidate_pair_id"),
            ice_state: fields.text("ice_state"),
            dtls_state: fields.text("dtls_state"),
            packets_sent: fields.unsigned("packets_sent"),
            packets_received: fields.unsigned("packets_received"),
            bytes_sent: fields.unsigned("bytes_sent"),
            bytes_received: fields.unsigned("bytes_received"),
        }),
        3 => PeerStatsRecord::InboundRtp(InboundRtpStats {
            id,
            timestamp_us,
            kind: fields.text("kind"),
            ssrc: fields.unsigned("ssrc"),
            transport_id: fields.text("transport_id"),
            mid: fields.text("mid"),
            packets_received: fields.unsigned("packets_received"),
            packets_lost: fields.signed("packets_lost"),
            bytes_received: fields.unsigned("bytes_received"),
            jitter: fields.decimal("jitter"),
            frames_received: fields.unsigned("frames_received"),
            frames_decoded: fields.unsigned("frames_decoded"),
        }),
        4 => PeerStatsRecord::OutboundRtp(OutboundRtpStats {
            id,
            timestamp_us,
            kind: fields.text("kind"),
            ssrc: fields.unsigned("ssrc"),
            transport_id: fields.text("transport_id"),
            mid: fields.text("mid"),
            rid: fields.text("rid"),
            packets_sent: fields.unsigned("packets_sent"),
            bytes_sent: fields.unsigned("bytes_sent"),
            target_bitrate: fields.decimal("target_bitrate"),
            frames_encoded: fields.unsigned("frames_encoded"),
            frames_sent: fields.unsigned("frames_sent"),
        }),
        5 => PeerStatsRecord::DataChannel(DataChannelStats {
            id,
            timestamp_us,
            label: fields.text("label"),
            protocol: fields.text("protocol"),
            state: fields.text("state"),
            data_channel_identifier: fields.signed("data_channel_identifier"),
            messages_sent: fields.unsigned("messages_sent"),
            messages_received: fields.unsigned("messages_received"),
            bytes_sent: fields.unsigned("bytes_sent"),
            bytes_received: fields.unsigned("bytes_received"),
        }),
        _ => unreachable!("native stats record kind"),
    }
}

fn event_from_ffi(event: ffi::FfiPeerEvent, peer: &Rc<PeerInner>) -> Option<PeerConnectionEvent> {
    match event.kind {
        0 => None,
        12 => Some(PeerConnectionEvent::Stats(PeerStatsSnapshot {
            operation_id: OperationId(event.operation_id),
            timestamp_us: event.stats_timestamp_us,
            records: event
                .stats_records
                .into_iter()
                .map(stats_record_from_ffi)
                .collect(),
        })),
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
        10 => {
            let native = ffi::peer_take_transceiver(peer.native(), event.operation_id);
            assert!(
                !native.is_null(),
                "native adapter lost a remote transceiver"
            );
            Some(PeerConnectionEvent::Track(RtpTransceiver::from_native(
                native,
                peer.clone(),
            )))
        }
        11 => {
            let native = ffi::peer_take_receiver(peer.native(), event.operation_id);
            assert!(!native.is_null(), "native adapter lost a removed receiver");
            Some(PeerConnectionEvent::TrackRemoved(RtpReceiver::from_native(
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
