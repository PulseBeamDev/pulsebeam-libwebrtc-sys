//! Move-only production ownership. Local handles never leave this module's
//! resource graph; IDs and owned values are the only application boundary.

use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    marker::PhantomData,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    task::{Context, Poll},
};

use crate::{
    AudioDecoderFactory, AudioEncoderFactory, CodecError, DataChannel, DataChannelConfiguration,
    DataChannelError, DataChannelEvent, DataChannelMessage, DataChannelSendResult,
    EncodedAudioFrame, EncodedAudioSink, EncodedAudioSource, EncodedReceivedVideoFrame,
    EncodedVideoAccessUnit, EncodedVideoInput, EncodedVideoSink, EncodedVideoSource, IceCandidate,
    IceCandidateDisposition, OfferAnswerOptions, OperationCompletion, OperationId, PeerConnection,
    PeerConnectionEvent, PeerConnectionFactory, PeerDescriptions, PeerError, PeerErrorKind,
    PeerStatsSnapshot, RtcConfiguration, RtpHeaderExtensionCapability, RtpReceiver, RtpSender,
    RtpSenderParameters, RtpTransceiver, RtpTransceiverDirection, SessionDescription,
    VideoCodecFormat, VideoRateControl, readiness::Readiness,
};

#[path = "production_connectivity.rs"]
mod connectivity;
pub use connectivity::{SessionDtmfError, SessionSctpTransportInformation, SessionTransportError};

macro_rules! resource_id {
    ($($name:ident),+ $(,)?) => {$ (
        /// An opaque session-owned identity. Copying it does not share access
        /// to an engine resource. IDs are never recycled across sessions.
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        pub struct $name(u64);
        impl $name { pub fn get(self) -> u64 { self.0 } }
    )+ };
}
resource_id!(
    SessionPeerId,
    SessionChannelId,
    SessionSourceId,
    SessionTransceiverId,
    SessionSenderId,
    SessionReceiverId,
    SessionFrameEncryptorId,
    SessionFrameDecryptorId,
    SessionSframeEncrypterId,
    SessionSframeDecrypterId,
    SessionDtmfSenderId,
    SessionDtlsTransportId,
    SessionSctpTransportId,
    SessionIceTransportId
);

static NEXT_RESOURCE: AtomicU64 = AtomicU64::new(1);
const MAX_OUTSTANDING_OPERATIONS: usize = 64;

fn next_id() -> Result<u64, PeerError> {
    NEXT_RESOURCE
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
            next.checked_add(1)
        })
        .map_err(|_| {
            error(
                PeerErrorKind::ResourceExhausted,
                "session resource identities exhausted",
            )
        })
}
fn error(kind: PeerErrorKind, message: &str) -> PeerError {
    PeerError {
        kind,
        message: message.into(),
    }
}
fn missing() -> PeerError {
    error(
        PeerErrorKind::InvalidParameter,
        "resource does not belong to this session",
    )
}
fn codec_error(value: CodecError) -> PeerError {
    PeerError {
        kind: PeerErrorKind::InvalidParameter,
        message: value.to_string(),
    }
}

/// Production uses system time, default engine role threads and headless
/// devices. No thread-bound caller provider, factory or handle may be injected.
#[derive(Clone, Debug, Default)]
pub struct ProductionSessionConfig {
    /// Optional direct encoded-video format. H.264 and VP8 are accepted.
    /// Reception is encoded-only; this does not advertise decoded H.264 output.
    pub video_format: Option<VideoCodecFormat>,
}

struct Peer {
    value: PeerConnection,
    outstanding: HashSet<OperationId>,
}
struct Resource<T> {
    peer: SessionPeerId,
    value: T,
}
struct Transceiver {
    value: RtpTransceiver,
    sender: SessionSenderId,
    receiver: SessionReceiverId,
}
enum Source {
    Opus(EncodedAudioSource),
    Video(EncodedVideoSource),
}

/// Owned peer notifications. Resource arrivals contain IDs, not local handles.
#[derive(Debug)]
pub enum SessionEvent {
    Stats(PeerStatsSnapshot),
    OperationComplete(OperationCompletion),
    OperationCompleteBytes {
        operation_id: OperationId,
        description: crate::SessionDescriptionBytes,
    },
    OperationFailedBytes {
        operation_id: OperationId,
        error: crate::PeerErrorBytes,
    },
    IceCandidate(IceCandidate),
    IceCandidateBytes(crate::IceCandidateBytes),
    IceCandidateRemoved(IceCandidate),
    IceCandidateRemovedBytes(crate::IceCandidateBytes),
    ConnectionStateChanged(crate::ConnectionState),
    SignalingStateChanged(crate::SignalingState),
    IceGatheringStateChanged(crate::IceGatheringState),
    IceConnectionStateChanged(crate::PeerIceConnectionState),
    StandardizedIceConnectionStateChanged(crate::PeerIceConnectionState),
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
    IceCandidateErrorBytes {
        address: String,
        port: i32,
        url: String,
        error_code: i32,
        message: Vec<u8>,
    },
    DataChannel {
        channel: SessionChannelId,
        label: Vec<u8>,
    },
    Track {
        transceiver: SessionTransceiverId,
        receiver: SessionReceiverId,
    },
    TrackRemoved {
        receiver: SessionReceiverId,
    },
    Closed,
}

/// A move-only, `Send`, non-`Sync` owner for a production engine resource graph.
/// Every engine operation requires exclusive actor access. Native sequencing
/// uses the engine's existing signaling/worker/network roles, not a second
/// application owner executor. Controlled worlds and legacy local handles stay
/// non-Send and cannot be imported into this graph.
///
/// ```
/// fn assert_send<T: Send>() {}
/// assert_send::<pulsebeam_webrtc_sys::ProductionSession>();
/// ```
/// ```compile_fail
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<pulsebeam_webrtc_sys::ProductionSession>();
/// ```
pub struct ProductionSession {
    // Observers/transports retire before endpoints, peers and role threads.
    connectivity: connectivity::ConnectivityResources,
    sframe_encrypters: HashMap<SessionSframeEncrypterId, Resource<crate::SframeEncrypter>>,
    sframe_decrypters: HashMap<SessionSframeDecrypterId, Resource<crate::SframeDecrypter>>,
    frame_encryptors: HashMap<SessionFrameEncryptorId, crate::FrameEncryptor>,
    frame_decryptors: HashMap<SessionFrameDecryptorId, Resource<crate::FrameDecryptor>>,
    sender_observers: HashMap<SessionSenderId, Resource<crate::RtpSenderObserver>>,
    receiver_observers: HashMap<SessionReceiverId, Resource<crate::RtpReceiverObserver>>,
    channels: HashMap<SessionChannelId, Resource<DataChannel>>,
    video_sinks: HashMap<SessionReceiverId, EncodedVideoSink>,
    audio_sinks: HashMap<SessionReceiverId, EncodedAudioSink>,
    transceivers: HashMap<SessionTransceiverId, Resource<Transceiver>>,
    senders: HashMap<SessionSenderId, Resource<RtpSender>>,
    receivers: HashMap<SessionReceiverId, Resource<RtpReceiver>>,
    sources: HashMap<SessionSourceId, Source>,
    peers: HashMap<SessionPeerId, Peer>,
    input: Option<EncodedVideoInput>,
    factory: PeerConnectionFactory,
    readiness: Arc<Readiness>,
    closed: bool,
    _exclusive: PhantomData<Cell<()>>,
}

// SAFETY: the *entire* Rc graph is captive here. Construction accepts only owned
// Send configuration and creates production resources internally. No method
// returns or imports a handle, Rc, or reference to that graph. Field-trial
// providers require Send + Sync; returned views are independent immutable
// native copies, not handles into the captive graph. Rc/Cell/RefCell mutations and destruction occur only under exclusive
// session ownership, even after migration; native callbacks never touch them.
// Native objects marshal affine operations and final releases to retained live
// engine role threads (including the bypass-proxy SDP initiation overloads).
// Only the independently synchronized Readiness and codec broker states cross
// callback threads. Drop tears down all dependents before the role threads.
// This rationale does NOT apply to any individual legacy/local handle.
unsafe impl Send for ProductionSession {}

/// Owned construction options for the production actor. Provider registrations
/// require thread-safe owned contracts, never local native handles. Existing
/// convenience constructors retain their defaults.
pub struct ProductionSessionBuilder {
    config: ProductionSessionConfig,
    audio_encoder_factory: Option<AudioEncoderFactory>,
    audio_decoder_factory: Option<AudioDecoderFactory>,
    video_encoder_factory: Option<crate::VideoEncoderFactoryHandle>,
    video_decoder_factory: Option<crate::VideoDecoderFactoryHandle>,
    temporal_fps: Option<[u8; 3]>,
    simulcast: bool,
    field_trials: Option<Arc<dyn crate::FieldTrialsView>>,
    event_logging: bool,
    event_log_factory: Option<Arc<dyn crate::RtcEventLogFactory>>,
    encode_metronome: Option<crate::Metronome>,
    decode_metronome: Option<crate::Metronome>,
    network_monitor_factory: Option<Arc<dyn crate::NetworkMonitorFactory>>,
    network_predictor_factory: Option<crate::NetworkStatePredictorFactory>,
    fec_factory: Option<crate::FecControllerFactory>,
    network_controller_factory: Option<crate::NetworkControllerFactory>,
    environment_event_log: Option<Box<dyn crate::RtcEventLogger>>,
    clock: Option<crate::Clock>,
    task_queue_factory: Option<crate::TaskQueueFactory>,
}

impl ProductionSessionBuilder {
    /// Explicit selection, distinct from this builder's absent carrier default.
    pub fn audio_encoder_factory(mut self, factory: &AudioEncoderFactory) -> Self {
        self.audio_encoder_factory = Some(factory.clone());
        self
    }
    pub fn audio_decoder_factory(mut self, factory: &AudioDecoderFactory) -> Self {
        self.audio_decoder_factory = Some(factory.clone());
        self
    }
    pub fn video_encoder_factory(mut self, factory: &crate::VideoEncoderFactoryHandle) -> Self {
        self.video_encoder_factory = Some(factory.clone());
        self
    }
    pub fn video_decoder_factory(mut self, factory: &crate::VideoDecoderFactoryHandle) -> Self {
        self.video_decoder_factory = Some(factory.clone());
        self
    }
    pub fn video_encoder_provider(
        self,
        provider: impl crate::VideoEncoderFactoryProvider,
    ) -> Result<Self, CodecError> {
        Ok(self.video_encoder_factory(&crate::VideoEncoderFactoryHandle::from_provider(provider)?))
    }
    pub fn video_decoder_provider(
        self,
        provider: impl crate::VideoDecoderFactoryProvider,
    ) -> Result<Self, CodecError> {
        Ok(self.video_decoder_factory(&crate::VideoDecoderFactoryHandle::from_provider(provider)?))
    }
    pub fn network_controller_factory(mut self, factory: &crate::NetworkControllerFactory) -> Self {
        self.network_controller_factory = Some(factory.clone());
        self
    }
    pub fn network_controller_provider(
        self,
        provider: impl crate::NetworkControllerFactoryProvider,
    ) -> Self {
        self.network_controller_factory(&crate::NetworkControllerFactory::from_provider(provider))
    }
    pub fn fec_controller_factory(mut self, factory: &crate::FecControllerFactory) -> Self {
        self.fec_factory = Some(factory.clone());
        self
    }
    pub fn fec_controller_provider(
        self,
        provider: impl crate::FecControllerFactoryProvider,
    ) -> Self {
        self.fec_controller_factory(&crate::FecControllerFactory::from_provider(provider))
    }
    pub fn network_state_predictor_factory(
        mut self,
        factory: &crate::NetworkStatePredictorFactory,
    ) -> Self {
        self.network_predictor_factory = Some(factory.clone());
        self
    }
    pub fn network_state_predictor_provider(
        self,
        provider: impl crate::NetworkStatePredictorFactoryProvider,
    ) -> Self {
        self.network_state_predictor_factory(&crate::NetworkStatePredictorFactory::from_provider(
            provider,
        ))
    }
    /// Register a worker-sequence encode metronome without importing a local
    /// native handle into the actor's captive resource graph.
    pub fn encode_metronome(mut self, metronome: &crate::Metronome) -> Self {
        self.encode_metronome = Some(metronome.clone());
        self
    }
    pub fn encode_metronome_provider(self, provider: impl crate::MetronomeProvider) -> Self {
        self.encode_metronome(&crate::Metronome::from_provider(provider))
    }
    pub fn decode_metronome(mut self, metronome: &crate::Metronome) -> Self {
        self.decode_metronome = Some(metronome.clone());
        self
    }
    pub fn decode_metronome_provider(self, provider: impl crate::MetronomeProvider) -> Self {
        self.decode_metronome(&crate::Metronome::from_provider(provider))
    }

    /// Select a thread-safe production scheduler. Controlled factories remain
    /// excluded from the actor resource graph.
    pub fn task_queue_factory(mut self, factory: &crate::TaskQueueFactory) -> Self {
        self.task_queue_factory = Some(factory.clone());
        self
    }

    pub fn task_queue_provider(
        self,
        provider: impl crate::TaskQueueFactoryProvider,
    ) -> Result<Self, crate::BuildEnvironmentError> {
        Ok(self.task_queue_factory(&crate::TaskQueueFactory::from_provider(provider)?))
    }

    /// Select an owned clock without replacing WebRTC's process time hooks.
    pub fn clock_handle(mut self, clock: &crate::Clock) -> Self {
        self.clock = Some(clock.clone());
        self
    }

    pub fn clock_provider(self, provider: impl crate::ClockProvider) -> Self {
        self.clock_handle(&crate::Clock::from_provider(provider))
    }

    /// Register an owned monitor factory with native network-thread callbacks.
    pub fn network_monitor_factory(mut self, factory: impl crate::NetworkMonitorFactory) -> Self {
        self.network_monitor_factory = Some(Arc::new(factory));
        self
    }

    pub fn environment_event_log(mut self, logger: impl crate::RtcEventLogger) -> Self {
        self.environment_event_log = Some(Box::new(logger));
        self
    }

    pub fn event_log_factory(mut self, factory: impl crate::RtcEventLogFactory) -> Self {
        self.event_log_factory = Some(Arc::new(factory));
        self
    }

    pub fn field_trials(mut self, view: impl crate::FieldTrialsView) -> Self {
        self.field_trials = Some(Arc::new(view));
        self
    }

    /// Make the bundled native event logger available to subsequently created
    /// peers. Does not start logging or create an output file.
    pub fn event_logging(mut self, enabled: bool) -> Self {
        self.event_logging = enabled;
        self
    }

    /// Direct single-encoding L1T3 convenience configuration.
    pub fn l1t3_input(mut self, fractions: [u8; 3]) -> Self {
        self.temporal_fps = Some(fractions);
        self.simulcast = false;
        self
    }

    /// Direct three-rung simulcast convenience configuration.
    pub fn simulcast_input(mut self) -> Self {
        self.temporal_fps = None;
        self.simulcast = true;
        self
    }

    pub fn build(self) -> Result<ProductionSession, PeerError> {
        if (self.temporal_fps.is_some() || self.simulcast) && self.config.video_format.is_none() {
            return Err(error(
                PeerErrorKind::UnsupportedParameter,
                "layered input requires a video format",
            ));
        }
        ProductionSession::build(self)
    }
}

impl ProductionSession {
    pub fn builder(config: ProductionSessionConfig) -> ProductionSessionBuilder {
        ProductionSessionBuilder {
            config,
            audio_encoder_factory: None,
            audio_decoder_factory: None,
            video_encoder_factory: None,
            video_decoder_factory: None,
            temporal_fps: None,
            simulcast: false,
            field_trials: None,
            event_logging: false,
            event_log_factory: None,
            encode_metronome: None,
            decode_metronome: None,
            network_monitor_factory: None,
            network_predictor_factory: None,
            fec_factory: None,
            network_controller_factory: None,
            environment_event_log: None,
            clock: None,
            task_queue_factory: None,
        }
    }

    pub fn new(config: ProductionSessionConfig) -> Result<Self, PeerError> {
        Self::builder(config).build()
    }

    /// Create the actor with direct, single-encoding H264 L1T3 input.
    /// Configuration must select the supported constrained-baseline H264 format.
    /// Native DD must be negotiated and L1T3 selected on the sender before input.
    /// `fps` passes cumulative native temporal frame-rate capabilities unchanged.
    pub fn new_l1t3(config: ProductionSessionConfig, fps: [u8; 3]) -> Result<Self, PeerError> {
        if config.video_format.is_none() {
            return Err(error(
                PeerErrorKind::UnsupportedParameter,
                "L1T3 requires an H264 video format",
            ));
        }
        Self::builder(config).l1t3_input(fps).build()
    }

    /// Create direct three-rung H264 input through native sender callbacks.
    /// Publication requires native three-stream initialization and three RIDs
    /// with matching rung geometry. Ambiguous single-encoder fallback rejects
    /// asynchronously rather than guessing an encoding. This does
    /// not extend stock libwebrtc's simultaneous simulcast receiving support.
    pub fn new_simulcast(config: ProductionSessionConfig) -> Result<Self, PeerError> {
        if config.video_format.is_none() {
            return Err(error(
                PeerErrorKind::UnsupportedParameter,
                "simulcast requires an H264 video format",
            ));
        }
        Self::builder(config).simulcast_input().build()
    }

    /// Install owned values or a caller-defined immutable provider before
    /// native factory construction. Provider callbacks may run on native threads.
    pub fn new_with_field_trials(
        config: ProductionSessionConfig,
        view: impl crate::FieldTrialsView,
    ) -> Result<Self, PeerError> {
        Self::builder(config).field_trials(view).build()
    }

    /// L1T3 constructor with an immutable field-trial provider.
    pub fn new_l1t3_with_field_trials(
        config: ProductionSessionConfig,
        fps: [u8; 3],
        view: impl crate::FieldTrialsView,
    ) -> Result<Self, PeerError> {
        if config.video_format.is_none() {
            return Err(error(
                PeerErrorKind::UnsupportedParameter,
                "L1T3 requires an H264 video format",
            ));
        }
        Self::builder(config)
            .l1t3_input(fps)
            .field_trials(view)
            .build()
    }

    /// Simulcast constructor with an immutable field-trial provider.
    pub fn new_simulcast_with_field_trials(
        config: ProductionSessionConfig,
        view: impl crate::FieldTrialsView,
    ) -> Result<Self, PeerError> {
        if config.video_format.is_none() {
            return Err(error(
                PeerErrorKind::UnsupportedParameter,
                "simulcast requires an H264 video format",
            ));
        }
        Self::builder(config)
            .simulcast_input()
            .field_trials(view)
            .build()
    }

    /// Full owning SDK capability metadata from this actor's selected factory.
    /// Safe after caller migration; no local engine handle escapes.
    pub fn get_rtp_sender_capabilities(
        &self,
        kind: crate::MediaType,
    ) -> Result<crate::RtpCapabilities, PeerError> {
        self.check_open()?;
        Ok(self.factory.get_rtp_sender_capabilities(kind))
    }

    /// Full owning SDK receiver capability metadata through the retained factory.
    pub fn get_rtp_receiver_capabilities(
        &self,
        kind: crate::MediaType,
    ) -> Result<crate::RtpCapabilities, PeerError> {
        self.check_open()?;
        Ok(self.factory.get_rtp_receiver_capabilities(kind))
    }

    /// Copy the actual factory environment's view, including after migration.
    pub fn field_trials(&self) -> Result<crate::FieldTrialsSnapshot, PeerError> {
        self.check_open()?;
        Ok(self.factory.field_trials())
    }

    /// Owned reading from the factory's effective native clock, including
    /// signed values. No local engine handle is returned to the actor caller.
    pub fn clock_timestamp(&self) -> Result<crate::Timestamp, PeerError> {
        self.check_open()?;
        Ok(self.factory.clock_timestamp())
    }

    fn build(options: ProductionSessionBuilder) -> Result<Self, PeerError> {
        let ProductionSessionBuilder {
            config,
            audio_encoder_factory,
            audio_decoder_factory,
            video_encoder_factory,
            video_decoder_factory,
            temporal_fps,
            simulcast,
            field_trials,
            event_logging,
            event_log_factory,
            encode_metronome,
            decode_metronome,
            network_monitor_factory,
            network_predictor_factory,
            fec_factory,
            network_controller_factory,
            environment_event_log,
            clock,
            task_queue_factory,
        } = options;
        if config
            .video_format
            .as_ref()
            .is_some_and(|f| !matches!(f.name.as_str(), "H264" | "VP8"))
        {
            return Err(error(
                PeerErrorKind::UnsupportedParameter,
                "production session supports encoded H264 or VP8",
            ));
        }
        if config.video_format.is_some() && video_encoder_factory.is_some() {
            return Err(error(
                PeerErrorKind::InvalidParameter,
                "the configured encoded input requires its own selected encoder factory; choose encoded input or an explicit encoder factory",
            ));
        }
        let readiness = Arc::new(Readiness::default());
        let selected_audio_encoder = match audio_encoder_factory {
            Some(factory) => factory,
            None => AudioEncoderFactory::with_opus_frames().map_err(codec_error)?,
        };
        let selected_audio_decoder = match audio_decoder_factory {
            Some(factory) => factory,
            None => AudioDecoderFactory::builtin_opus().map_err(codec_error)?,
        };
        let mut builder = PeerConnectionFactory::builder()
            .audio_encoder_factory(selected_audio_encoder)
            .audio_decoder_factory(selected_audio_decoder)
            .readiness(readiness.clone())
            .event_logging(event_logging);
        if let Some(metronome) = encode_metronome {
            builder = builder.encode_metronome(&metronome);
        }
        if let Some(metronome) = decode_metronome {
            builder = builder.decode_metronome(&metronome);
        }
        if let Some(factory) = event_log_factory {
            builder = builder.shared_event_log_factory(factory);
        }
        if let Some(factory) = network_monitor_factory {
            builder = builder.shared_network_monitor_factory(factory);
        }
        if let Some(factory) = network_predictor_factory {
            builder = builder.network_state_predictor_factory(&factory);
        }
        if let Some(factory) = fec_factory {
            builder = builder.fec_controller_factory(&factory);
        }
        if let Some(factory) = network_controller_factory {
            builder = builder.network_controller_factory(&factory);
        }
        if task_queue_factory
            .as_ref()
            .is_some_and(crate::TaskQueueFactory::is_cooperative)
        {
            return Err(error(
                PeerErrorKind::UnsupportedParameter,
                "controlled task queue factories cannot enter the production actor",
            ));
        }
        if field_trials.is_some()
            || environment_event_log.is_some()
            || clock.is_some()
            || task_queue_factory.is_some()
        {
            let mut environment = crate::Environment::builder();
            if let Some(factory) = task_queue_factory {
                environment = environment.task_queue_factory(&factory);
            }
            if let Some(clock) = clock {
                environment = environment.clock_handle(&clock);
            }
            if let Some(view) = field_trials {
                environment = environment.field_trials(view);
            }
            if let Some(logger) = environment_event_log {
                environment = environment.owned_event_log(logger);
            }
            let environment = environment
                .build()
                .map_err(|e| error(PeerErrorKind::NativeConstruction, &e.to_string()))?;
            builder = builder.environment(environment);
        }
        let input = config
            .video_format
            .map(|format| {
                let input = match temporal_fps {
                    None if simulcast => {
                        EncodedVideoInput::new_simulcast_for_format(format.clone())
                    }
                    Some(fps) => EncodedVideoInput::new_l1t3_for_format(format.clone(), fps),
                    None => EncodedVideoInput::new_for_format(format.clone()),
                }
                .map_err(codec_error)?;
                let decoder = input.encoded_receive_factory().map_err(codec_error)?;
                Ok::<_, PeerError>((input, decoder))
            })
            .transpose()?;
        let input = if let Some((input, decoder)) = input {
            builder = builder
                .video_encoder_factory(input.encoder_factory())
                .video_decoder_factory(decoder);
            Some(input)
        } else {
            None
        };
        if let Some(factory) = video_encoder_factory {
            builder = builder.video_encoder_factory(factory);
        }
        if let Some(factory) = video_decoder_factory {
            builder = builder.video_decoder_factory(factory);
        }
        let factory = builder.build()?;
        Ok(Self {
            connectivity: connectivity::ConnectivityResources::default(),
            sframe_encrypters: HashMap::new(),
            sframe_decrypters: HashMap::new(),
            frame_encryptors: HashMap::new(),
            frame_decryptors: HashMap::new(),
            sender_observers: HashMap::new(),
            receiver_observers: HashMap::new(),
            channels: HashMap::new(),
            video_sinks: HashMap::new(),
            audio_sinks: HashMap::new(),
            transceivers: HashMap::new(),
            senders: HashMap::new(),
            receivers: HashMap::new(),
            sources: HashMap::new(),
            peers: HashMap::new(),
            input,
            factory,
            readiness,
            closed: false,
            _exclusive: PhantomData,
        })
    }

    fn check_open(&self) -> Result<(), PeerError> {
        if self.closed {
            Err(error(PeerErrorKind::Closed, "session is closed"))
        } else {
            Ok(())
        }
    }

    /// Replace factory options for subsequently created peers. Existing peers
    /// are unchanged. This can be called before the first peer is created and
    /// after migrating the actor. Native `SetOptions` has no validation result.
    pub fn set_factory_options(&mut self, options: crate::FactoryOptions) -> Result<(), PeerError> {
        self.check_open()?;
        self.factory.set_options(options);
        Ok(())
    }

    pub fn create_peer(
        &mut self,
        config: crate::PeerConfiguration,
    ) -> Result<SessionPeerId, PeerError> {
        self.create_peer_with_configuration(config.into())
    }

    /// Actor-safe creation with complete owned native configuration values.
    pub fn create_peer_with_configuration(
        &mut self,
        config: RtcConfiguration,
    ) -> Result<SessionPeerId, PeerError> {
        self.create_peer_with_dependencies(config, crate::PeerConnectionDependencies::default())
    }

    /// Actor-safe creation with owned per-peer executable providers.
    pub fn create_peer_with_dependencies(
        &mut self,
        config: RtcConfiguration,
        dependencies: crate::PeerConnectionDependencies,
    ) -> Result<SessionPeerId, PeerError> {
        self.check_open()?;
        let id = SessionPeerId(next_id()?);
        let value = self
            .factory
            .create_peer_connection_with_dependencies(config, dependencies)?;
        self.peers.insert(
            id,
            Peer {
                value,
                outstanding: HashSet::new(),
            },
        );
        Ok(id)
    }

    /// Owned native snapshot. This can leave the actor, including across
    /// migrations; it contains no peer-affine references.
    pub fn peer_configuration(&self, peer: SessionPeerId) -> Result<RtcConfiguration, PeerError> {
        self.check_open()?;
        let peer = self.peers.get(&peer).ok_or_else(missing)?;
        if peer.value.inner.closed.get() {
            return Err(error(PeerErrorKind::Closed, "peer is closed"));
        }
        Ok(peer.value.configuration())
    }

    pub fn set_peer_configuration(
        &mut self,
        peer: SessionPeerId,
        config: RtcConfiguration,
    ) -> Result<(), PeerError> {
        self.check_open()?;
        self.peers
            .get(&peer)
            .ok_or_else(missing)?
            .value
            .set_configuration(config)
    }

    fn operation(
        &mut self,
        peer: SessionPeerId,
        submit: impl FnOnce(&PeerConnection) -> Result<OperationId, PeerError>,
    ) -> Result<OperationId, PeerError> {
        self.check_open()?;
        let peer = self.peers.get_mut(&peer).ok_or_else(missing)?;
        if peer.value.inner.closed.get() {
            return Err(error(PeerErrorKind::Closed, "peer is closed"));
        }
        if peer.outstanding.len() >= MAX_OUTSTANDING_OPERATIONS {
            return Err(error(
                PeerErrorKind::ResourceExhausted,
                "consume operation outcomes before submitting more work (limit 64)",
            ));
        }
        let id = submit(&peer.value)?;
        peer.outstanding.insert(id);
        Ok(id)
    }

    pub fn create_offer(
        &mut self,
        peer: SessionPeerId,
        ice_restart: bool,
    ) -> Result<OperationId, PeerError> {
        self.operation(peer, |p| {
            if ice_restart {
                p.create_ice_restart_offer()
            } else {
                p.create_offer()
            }
        })
    }
    pub fn create_offer_with_options(
        &mut self,
        peer: SessionPeerId,
        options: OfferAnswerOptions,
    ) -> Result<OperationId, PeerError> {
        self.operation(peer, |p| p.create_offer_with_options(options))
    }
    pub fn create_answer_with_options(
        &mut self,
        peer: SessionPeerId,
        options: OfferAnswerOptions,
    ) -> Result<OperationId, PeerError> {
        self.operation(peer, |p| p.create_answer_with_options(options))
    }
    pub fn restart_ice(&mut self, peer: SessionPeerId) -> Result<(), PeerError> {
        self.check_open()?;
        self.peers
            .get(&peer)
            .ok_or_else(missing)?
            .value
            .restart_ice()
    }
    pub fn should_fire_negotiation_needed(
        &mut self,
        peer: SessionPeerId,
        event_id: u32,
    ) -> Result<bool, PeerError> {
        self.check_open()?;
        self.peers
            .get(&peer)
            .ok_or_else(missing)?
            .value
            .should_fire_negotiation_needed(event_id)
    }
    pub fn set_local_description_auto(
        &mut self,
        peer: SessionPeerId,
    ) -> Result<OperationId, PeerError> {
        self.operation(peer, PeerConnection::set_local_description_auto)
    }
    pub fn add_ice_candidate_sync(
        &mut self,
        peer: SessionPeerId,
        candidate: IceCandidate,
    ) -> Result<IceCandidateDisposition, PeerError> {
        self.check_open()?;
        self.peers
            .get(&peer)
            .ok_or_else(missing)?
            .value
            .add_ice_candidate_sync(candidate)
    }
    pub fn add_ice_candidate_sync_bytes(
        &mut self,
        peer: SessionPeerId,
        candidate: crate::IceCandidateBytes,
    ) -> Result<IceCandidateDisposition, crate::PeerErrorBytes> {
        self.check_open().map_err(|error| crate::PeerErrorBytes {
            kind: error.kind,
            message: error.message.into_bytes(),
        })?;
        self.peers
            .get(&peer)
            .ok_or_else(|| crate::PeerErrorBytes {
                kind: PeerErrorKind::InvalidParameter,
                message: b"resource does not belong to this session".to_vec(),
            })?
            .value
            .add_ice_candidate_sync_bytes(candidate)
    }
    pub fn remove_ice_candidate_bytes(
        &mut self,
        peer: SessionPeerId,
        candidate: crate::IceCandidateBytes,
    ) -> Result<IceCandidateDisposition, crate::PeerErrorBytes> {
        self.check_open().map_err(|error| crate::PeerErrorBytes {
            kind: error.kind,
            message: error.message.into_bytes(),
        })?;
        self.peers
            .get(&peer)
            .ok_or_else(|| crate::PeerErrorBytes {
                kind: PeerErrorKind::InvalidParameter,
                message: b"resource does not belong to this session".to_vec(),
            })?
            .value
            .remove_ice_candidate_bytes(candidate)
    }
    pub fn remove_ice_candidate(
        &mut self,
        peer: SessionPeerId,
        candidate: IceCandidate,
    ) -> Result<IceCandidateDisposition, PeerError> {
        self.check_open()?;
        self.peers
            .get(&peer)
            .ok_or_else(missing)?
            .value
            .remove_ice_candidate(candidate)
    }
    pub fn add_ice_candidate_bytes(
        &mut self,
        peer: SessionPeerId,
        candidate: crate::IceCandidateBytes,
    ) -> Result<OperationId, PeerError> {
        self.operation(peer, |value| value.add_ice_candidate_bytes(candidate))
    }
    pub fn create_answer(&mut self, peer: SessionPeerId) -> Result<OperationId, PeerError> {
        self.operation(peer, PeerConnection::create_answer)
    }
    pub fn set_local_description(
        &mut self,
        peer: SessionPeerId,
        description: SessionDescription,
    ) -> Result<OperationId, PeerError> {
        self.operation(peer, |p| p.set_local_description(description))
    }
    pub fn set_remote_description(
        &mut self,
        peer: SessionPeerId,
        description: SessionDescription,
    ) -> Result<OperationId, PeerError> {
        self.operation(peer, |p| p.set_remote_description(description))
    }
    pub fn set_local_description_bytes(
        &mut self,
        peer: SessionPeerId,
        description: crate::SessionDescriptionBytes,
    ) -> Result<OperationId, PeerError> {
        self.operation(peer, |value| value.set_local_description_bytes(description))
    }
    pub fn set_remote_description_bytes(
        &mut self,
        peer: SessionPeerId,
        description: crate::SessionDescriptionBytes,
    ) -> Result<OperationId, PeerError> {
        self.operation(peer, |value| {
            value.set_remote_description_bytes(description)
        })
    }
    pub fn add_ice_candidate(
        &mut self,
        peer: SessionPeerId,
        candidate: IceCandidate,
    ) -> Result<OperationId, PeerError> {
        self.operation(peer, |p| p.add_ice_candidate(candidate))
    }
    pub fn request_stats(&mut self, peer: SessionPeerId) -> Result<OperationId, PeerError> {
        self.operation(peer, PeerConnection::request_stats)
    }
    pub fn descriptions(&mut self, peer: SessionPeerId) -> Result<PeerDescriptions, PeerError> {
        self.peers
            .get(&peer)
            .ok_or_else(missing)?
            .value
            .descriptions()
    }

    pub fn peer_ice_snapshot(
        &mut self,
        peer: SessionPeerId,
    ) -> Result<crate::PeerIceSnapshot, PeerError> {
        self.check_open()?;
        Ok(self
            .peers
            .get(&peer)
            .ok_or_else(missing)?
            .value
            .ice_snapshot())
    }
    pub fn peer_ice_connection_state(
        &mut self,
        peer: SessionPeerId,
    ) -> Result<crate::PeerIceConnectionState, PeerError> {
        Ok(self.peer_ice_snapshot(peer)?.legacy_state)
    }
    pub fn peer_standardized_ice_connection_state(
        &mut self,
        peer: SessionPeerId,
    ) -> Result<crate::PeerIceConnectionState, PeerError> {
        Ok(self.peer_ice_snapshot(peer)?.standardized_state)
    }
    pub fn peer_can_trickle_ice_candidates(
        &mut self,
        peer: SessionPeerId,
    ) -> Result<Option<bool>, PeerError> {
        Ok(self.peer_ice_snapshot(peer)?.can_trickle_ice_candidates)
    }

    /// Read actual native state without draining or interpreting peer events.
    /// Retained peer identities remain queryable after close and shutdown.
    pub fn peer_signaling_state(
        &mut self,
        peer: SessionPeerId,
    ) -> Result<crate::SignalingState, PeerError> {
        Ok(self
            .peers
            .get(&peer)
            .ok_or_else(missing)?
            .value
            .signaling_state())
    }

    pub fn peer_connection_state(
        &mut self,
        peer: SessionPeerId,
    ) -> Result<crate::ConnectionState, PeerError> {
        Ok(self
            .peers
            .get(&peer)
            .ok_or_else(missing)?
            .value
            .peer_connection_state())
    }

    pub fn peer_ice_gathering_state(
        &mut self,
        peer: SessionPeerId,
    ) -> Result<crate::IceGatheringState, PeerError> {
        Ok(self
            .peers
            .get(&peer)
            .ok_or_else(missing)?
            .value
            .ice_gathering_state())
    }

    pub fn descriptions_bytes(
        &mut self,
        peer: SessionPeerId,
    ) -> Result<crate::PeerDescriptionBytes, PeerError> {
        self.peers
            .get(&peer)
            .ok_or_else(missing)?
            .value
            .descriptions_bytes()
    }

    /// Activity notification only, not proof of a particular queued event.
    /// Drain owned events/frames, then wait. Signals coalesce and may be spurious.
    /// Polling never enters the engine, pumps work or advances a clock. A single
    /// actor waiter may register; its waker must schedule, not execute engine work.
    /// DTMF and registered transport observers share this readiness source.
    /// First-packet observer queues are explicit-poll-only and do not signal this
    /// readiness source; it is not a waiter for those observer records.
    pub fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        self.readiness.poll(cx)
    }
    pub async fn ready(&mut self) {
        std::future::poll_fn(|cx| self.poll_ready(cx)).await
    }

    pub fn try_peer_event(
        &mut self,
        peer: SessionPeerId,
    ) -> Result<Option<SessionEvent>, PeerError> {
        let value = self.peers.get_mut(&peer).ok_or_else(missing)?;
        let Some(event) = value.value.try_next_event() else {
            return Ok(None);
        };
        let event = match event {
            PeerConnectionEvent::Stats(stats) => {
                value.outstanding.remove(&stats.operation_id);
                SessionEvent::Stats(stats)
            }
            PeerConnectionEvent::OperationComplete(done) => {
                value.outstanding.remove(&done.operation_id);
                SessionEvent::OperationComplete(done)
            }
            PeerConnectionEvent::OperationCompleteBytes {
                operation_id,
                description,
            } => {
                value.outstanding.remove(&operation_id);
                SessionEvent::OperationCompleteBytes {
                    operation_id,
                    description,
                }
            }
            PeerConnectionEvent::OperationFailedBytes {
                operation_id,
                error,
            } => {
                value.outstanding.remove(&operation_id);
                SessionEvent::OperationFailedBytes {
                    operation_id,
                    error,
                }
            }
            PeerConnectionEvent::IceCandidate(v) => SessionEvent::IceCandidate(v),
            PeerConnectionEvent::IceCandidateBytes(value) => SessionEvent::IceCandidateBytes(value),
            PeerConnectionEvent::IceCandidateRemoved(value) => {
                SessionEvent::IceCandidateRemoved(value)
            }
            PeerConnectionEvent::IceCandidateRemovedBytes(value) => {
                SessionEvent::IceCandidateRemovedBytes(value)
            }
            PeerConnectionEvent::ConnectionStateChanged(v) => {
                SessionEvent::ConnectionStateChanged(v)
            }
            PeerConnectionEvent::SignalingStateChanged(v) => SessionEvent::SignalingStateChanged(v),
            PeerConnectionEvent::IceGatheringStateChanged(v) => {
                SessionEvent::IceGatheringStateChanged(v)
            }
            PeerConnectionEvent::IceConnectionStateChanged(value) => {
                SessionEvent::IceConnectionStateChanged(value)
            }
            PeerConnectionEvent::StandardizedIceConnectionStateChanged(value) => {
                SessionEvent::StandardizedIceConnectionStateChanged(value)
            }
            PeerConnectionEvent::NegotiationNeeded { event_id } => {
                SessionEvent::NegotiationNeeded { event_id }
            }
            PeerConnectionEvent::IceCandidateError {
                address,
                port,
                url,
                error_code,
                message,
            } => SessionEvent::IceCandidateError {
                address,
                port,
                url,
                error_code,
                message,
            },
            PeerConnectionEvent::IceCandidateErrorBytes {
                address,
                port,
                url,
                error_code,
                message,
            } => SessionEvent::IceCandidateErrorBytes {
                address,
                port,
                url,
                error_code,
                message,
            },
            PeerConnectionEvent::DataChannel(value) => {
                let channel = SessionChannelId(next_id()?);
                let label = value.label();
                self.channels.insert(channel, Resource { peer, value });
                SessionEvent::DataChannel { channel, label }
            }
            PeerConnectionEvent::Track(value) => {
                let transceiver = self.insert_transceiver(peer, value)?;
                let receiver = self.transceivers[&transceiver].value.receiver;
                SessionEvent::Track {
                    transceiver,
                    receiver,
                }
            }
            PeerConnectionEvent::TrackRemoved(value) => {
                let receiver = self.insert_receiver(peer, value)?;
                SessionEvent::TrackRemoved { receiver }
            }
            PeerConnectionEvent::Closed => SessionEvent::Closed,
        };
        Ok(Some(event))
    }

    pub fn create_channel(
        &mut self,
        peer: SessionPeerId,
        label: &str,
        config: DataChannelConfiguration,
    ) -> Result<SessionChannelId, crate::PeerErrorBytes> {
        self.create_channel_bytes(peer, label.as_bytes(), Some(config))
    }

    pub fn create_channel_bytes(
        &mut self,
        peer: SessionPeerId,
        label: &[u8],
        config: Option<DataChannelConfiguration>,
    ) -> Result<SessionChannelId, crate::PeerErrorBytes> {
        self.check_open()?;
        let id = SessionChannelId(next_id()?);
        let value = self
            .peers
            .get(&peer)
            .ok_or_else(missing)?
            .value
            .create_data_channel_bytes(label, config)?;
        self.channels.insert(id, Resource { peer, value });
        Ok(id)
    }
    pub fn send(
        &mut self,
        channel: SessionChannelId,
        message: DataChannelMessage,
    ) -> Result<DataChannelSendResult, PeerError> {
        self.check_open()?;
        Ok(self
            .channels
            .get(&channel)
            .ok_or_else(missing)?
            .value
            .send(message))
    }
    /// Admit a bounded native send without blocking on network-thread completion.
    /// Consume `SendComplete` from `try_channel_event` for the terminal result.
    pub fn send_async(
        &mut self,
        channel: SessionChannelId,
        message: DataChannelMessage,
    ) -> Result<crate::DataChannelSendId, PeerError> {
        self.check_open()?;
        self.channels
            .get(&channel)
            .ok_or_else(missing)?
            .value
            .send_async(message)
    }

    /// Independently sampled native cumulative transport counters.
    pub fn channel_counters(
        &mut self,
        channel: SessionChannelId,
    ) -> Result<crate::DataChannelCounters, PeerError> {
        Ok(self
            .channels
            .get(&channel)
            .ok_or_else(missing)?
            .value
            .counters())
    }

    pub fn channel_configuration(
        &mut self,
        channel: SessionChannelId,
    ) -> Result<DataChannelConfiguration, PeerError> {
        Ok(self
            .channels
            .get(&channel)
            .ok_or_else(missing)?
            .value
            .configuration())
    }

    pub fn channel_label(&mut self, channel: SessionChannelId) -> Result<Vec<u8>, PeerError> {
        Ok(self
            .channels
            .get(&channel)
            .ok_or_else(missing)?
            .value
            .label())
    }

    pub fn channel_state(
        &mut self,
        channel: SessionChannelId,
    ) -> Result<crate::DataChannelState, PeerError> {
        Ok(self
            .channels
            .get(&channel)
            .ok_or_else(missing)?
            .value
            .state())
    }

    pub fn channel_reliable(&mut self, channel: SessionChannelId) -> Result<bool, PeerError> {
        Ok(self
            .channels
            .get(&channel)
            .ok_or_else(missing)?
            .value
            .reliable())
    }

    pub fn channel_buffered_amount(&mut self, channel: SessionChannelId) -> Result<u64, PeerError> {
        Ok(self
            .channels
            .get(&channel)
            .ok_or_else(missing)?
            .value
            .buffered_amount())
    }

    pub fn try_channel_event(
        &mut self,
        channel: SessionChannelId,
    ) -> Result<Option<DataChannelEvent>, PeerError> {
        Ok(self
            .channels
            .get(&channel)
            .ok_or_else(missing)?
            .value
            .try_next_event())
    }
    /// Copy the engine's current data-channel error without protocol progress.
    pub fn channel_error(
        &mut self,
        channel: SessionChannelId,
    ) -> Result<Option<DataChannelError>, PeerError> {
        Ok(self
            .channels
            .get(&channel)
            .ok_or_else(missing)?
            .value
            .error())
    }

    /// Register/unregister native callbacks, retaining already-owned events.
    /// This does not pause transport or provide consumption-driven receive credit.
    pub fn set_channel_event_observation(
        &mut self,
        channel: SessionChannelId,
        enabled: bool,
    ) -> Result<(), PeerError> {
        self.channels
            .get_mut(&channel)
            .ok_or_else(missing)?
            .value
            .set_event_observation(enabled);
        Ok(())
    }

    /// Bounds on the owned channel inbox, not SCTP receive-window credit.
    pub fn channel_receive_queue_limits() -> crate::DataChannelReceiveQueueLimits {
        DataChannel::receive_queue_limits()
    }

    /// Engine send-queue capacity, not a negotiated message-size or receive limit.
    pub fn channel_send_queue_capacity() -> u64 {
        DataChannel::send_queue_capacity()
    }

    pub fn close_channel(&mut self, channel: SessionChannelId) -> Result<(), PeerError> {
        self.channels
            .get_mut(&channel)
            .ok_or_else(missing)?
            .value
            .close()
    }

    /// Start an owned native event-log destination on the peer's engine
    /// sequences. Output callbacks do not need reentrant actor access.
    pub fn start_peer_event_log(
        &mut self,
        peer: SessionPeerId,
        output: crate::EventLogOutput,
        period: Option<crate::EventLogOutputPeriod>,
    ) -> Result<(), crate::EventLogError> {
        if self.closed {
            return Err(crate::EventLogError::Closed);
        }
        self.peers
            .get(&peer)
            .ok_or(crate::EventLogError::Closed)?
            .value
            .start_event_log(output, period)
    }

    /// Wait for native output shutdown. Safe after moving the application actor.
    pub fn stop_peer_event_log(&mut self, peer: SessionPeerId) -> Result<(), crate::EventLogError> {
        if self.closed {
            return Err(crate::EventLogError::Closed);
        }
        self.peers
            .get(&peer)
            .ok_or(crate::EventLogError::Closed)?
            .value
            .stop_event_log()
    }

    /// Forward native bandwidth-estimation constraints without a binding allocator.
    pub fn set_peer_bitrate(
        &mut self,
        peer: SessionPeerId,
        minimum: Option<u32>,
        start: Option<u32>,
        maximum: Option<u32>,
    ) -> Result<(), PeerError> {
        self.check_open()?;
        self.peers
            .get(&peer)
            .ok_or_else(missing)?
            .value
            .set_bitrate(minimum, start, maximum)
    }

    /// Current binding observation backlog, separate from native transport
    /// buffering and already-returned owned outputs.
    pub fn peer_event_observation(
        &self,
        peer: SessionPeerId,
    ) -> Result<crate::PeerEventObservation, PeerError> {
        Ok(self
            .peers
            .get(&peer)
            .ok_or_else(missing)?
            .value
            .event_observation())
    }

    /// Idempotently retire one producer and release its actor-owned entry.
    /// Existing native tracks can retain their source until their sender is
    /// stopped/detached. Already transmitted input can reach an open receiver.
    /// Previously returned owned output is unaffected.
    pub fn close_source(&mut self, id: SessionSourceId) -> Result<(), PeerError> {
        if let Some(source) = self.sources.get_mut(&id) {
            match source {
                Source::Opus(source) => source
                    .close()
                    .map_err(|e| error(PeerErrorKind::Internal, &e.to_string()))?,
                Source::Video(source) => source.close().map_err(codec_error)?,
            }
            self.sources.remove(&id);
        }
        Ok(())
    }

    pub fn create_opus_source(&mut self, channels: u8) -> Result<SessionSourceId, PeerError> {
        self.check_open()?;
        let id = SessionSourceId(next_id()?);
        let source = self.factory.create_encoded_audio_source(channels)?;
        self.sources.insert(id, Source::Opus(source));
        Ok(id)
    }
    pub fn create_video_source(&mut self) -> Result<SessionSourceId, PeerError> {
        self.check_open()?;
        let id = SessionSourceId(next_id()?);
        let source = self
            .input
            .as_ref()
            .ok_or_else(|| {
                error(
                    PeerErrorKind::InvalidState,
                    "session has no encoded video format",
                )
            })?
            .create_source_with_readiness(&self.factory, Some(self.readiness.clone()))?;
        self.sources.insert(id, Source::Video(source));
        Ok(id)
    }
    /// Read one producer's native system-clock capture timeline. This does not
    /// claim that the current enqueue instant was the actual capture instant.
    pub fn opus_capture_time_now(
        &mut self,
        source: SessionSourceId,
    ) -> Result<std::time::Duration, PeerError> {
        self.check_open()?;
        let Some(Source::Opus(source)) = self.sources.get(&source) else {
            return Err(missing());
        };
        source
            .capture_time_now()
            .map_err(|e| error(PeerErrorKind::InvalidParameter, &e.to_string()))
    }

    /// Submit actual capture timing mapped into opus_capture_time_now's domain.
    /// Native packetization/negotiated extension serialization owns wire timing.
    pub fn push_opus_at(
        &mut self,
        source: SessionSourceId,
        frame: &crate::OpusInputFrame,
        capture_time: std::time::Duration,
    ) -> Result<(), PeerError> {
        self.check_open()?;
        let Some(Source::Opus(source)) = self.sources.get(&source) else {
            return Err(missing());
        };
        source
            .push_opus_at(frame, capture_time)
            .map_err(|e| error(PeerErrorKind::InvalidParameter, &e.to_string()))
    }

    /// Submit mapped capture timing with independent producer RFC6464 metadata.
    pub fn push_opus_at_with_audio_level(
        &mut self,
        source: SessionSourceId,
        frame: &crate::OpusInputFrame,
        capture_time: std::time::Duration,
        level: crate::OpusAudioLevel,
    ) -> Result<(), PeerError> {
        self.check_open()?;
        let Some(Source::Opus(source)) = self.sources.get(&source) else {
            return Err(missing());
        };
        source
            .push_opus_at_with_audio_level(frame, capture_time, level)
            .map_err(|e| error(PeerErrorKind::InvalidParameter, &e.to_string()))
    }

    pub fn push_opus(
        &mut self,
        source: SessionSourceId,
        frame: &crate::OpusInputFrame,
    ) -> Result<(), PeerError> {
        self.check_open()?;
        let Some(Source::Opus(source)) = self.sources.get(&source) else {
            return Err(missing());
        };
        source
            .push_opus(frame)
            .map_err(|e| error(PeerErrorKind::InvalidParameter, &e.to_string()))
    }
    /// Submit producer RFC6464 metadata without exposing sequence-bound handles.
    pub fn push_opus_with_audio_level(
        &mut self,
        source: SessionSourceId,
        frame: &crate::OpusInputFrame,
        level: crate::OpusAudioLevel,
    ) -> Result<(), PeerError> {
        self.check_open()?;
        let Some(Source::Opus(source)) = self.sources.get(&source) else {
            return Err(missing());
        };
        source
            .push_opus_with_audio_level(frame, level)
            .map_err(|e| error(PeerErrorKind::InvalidParameter, &e.to_string()))
    }
    pub fn push_video(
        &mut self,
        source: SessionSourceId,
        frame: EncodedVideoAccessUnit,
    ) -> Result<(), PeerError> {
        self.check_open()?;
        let Some(Source::Video(source)) = self.sources.get(&source) else {
            return Err(missing());
        };
        source.push_encoded(frame).map_err(codec_error)
    }
    /// Atomically admit a bounded three-unit capture batch, not atomic wire publication.
    pub fn push_video_simulcast(
        &mut self,
        source: SessionSourceId,
        frames: [EncodedVideoAccessUnit; 3],
    ) -> Result<(), PeerError> {
        self.check_open()?;
        let Some(Source::Video(source)) = self.sources.get(&source) else {
            return Err(missing());
        };
        source.push_simulcast(frames).map_err(codec_error)
    }

    pub fn video_encoding_keyframe_requests(
        &mut self,
        source: SessionSourceId,
    ) -> Result<[bool; 3], PeerError> {
        let Some(Source::Video(source)) = self.sources.get(&source) else {
            return Err(missing());
        };
        Ok(source.take_encoding_keyframe_requests())
    }

    pub fn video_feedback(
        &mut self,
        source: SessionSourceId,
    ) -> Result<(bool, Option<VideoRateControl>), PeerError> {
        let Some(Source::Video(source)) = self.sources.get(&source) else {
            return Err(missing());
        };
        Ok((source.take_keyframe_request(), source.latest_rate_control()))
    }

    /// Retrieve the latest source-keyed asynchronous video adapter failure.
    pub fn take_video_encoder_error(
        &mut self,
        source: SessionSourceId,
    ) -> Result<Option<CodecError>, PeerError> {
        let Some(Source::Video(source)) = self.sources.get(&source) else {
            return Err(missing());
        };
        Ok(source.take_encoder_error())
    }

    pub fn publish_opus(
        &mut self,
        peer: SessionPeerId,
        source: SessionSourceId,
        track_id: &str,
        direction: RtpTransceiverDirection,
    ) -> Result<SessionTransceiverId, PeerError> {
        self.check_open()?;
        let Some(Source::Opus(source)) = self.sources.get(&source) else {
            return Err(missing());
        };
        let track = self.factory.create_encoded_audio_track(track_id, source)?;
        let value = self
            .peers
            .get(&peer)
            .ok_or_else(missing)?
            .value
            .add_audio_transceiver(&track, direction)?;
        self.insert_transceiver(peer, value)
    }
    pub fn publish_video(
        &mut self,
        peer: SessionPeerId,
        source: SessionSourceId,
        track_id: &str,
        direction: RtpTransceiverDirection,
        rids: &[String],
    ) -> Result<SessionTransceiverId, PeerError> {
        self.check_open()?;
        let Some(Source::Video(source)) = self.sources.get(&source) else {
            return Err(missing());
        };
        let track = source.create_track(&self.factory, track_id)?;
        let value = self
            .peers
            .get(&peer)
            .ok_or_else(missing)?
            .value
            .add_video_transceiver_with_rids(&track, direction, rids)?;
        self.insert_transceiver(peer, value)
    }
    fn insert_sender(
        &mut self,
        peer: SessionPeerId,
        value: RtpSender,
    ) -> Result<SessionSenderId, PeerError> {
        if let Some((&id, _)) = self
            .senders
            .iter()
            .find(|(_, existing)| existing.peer == peer && existing.value.same_resource(&value))
        {
            return Ok(id);
        }
        let id = SessionSenderId(next_id()?);
        self.senders.insert(id, Resource { peer, value });
        Ok(id)
    }

    fn insert_receiver(
        &mut self,
        peer: SessionPeerId,
        value: RtpReceiver,
    ) -> Result<SessionReceiverId, PeerError> {
        if let Some((&id, _)) = self
            .receivers
            .iter()
            .find(|(_, existing)| existing.peer == peer && existing.value.same_resource(&value))
        {
            return Ok(id);
        }
        let id = SessionReceiverId(next_id()?);
        self.receivers.insert(id, Resource { peer, value });
        Ok(id)
    }
    fn insert_transceiver(
        &mut self,
        peer: SessionPeerId,
        value: RtpTransceiver,
    ) -> Result<SessionTransceiverId, PeerError> {
        let receiver = self.insert_receiver(peer, value.receiver())?;
        if let Some((&id, _)) = self.transceivers.iter().find(|(_, existing)| {
            existing.peer == peer && existing.value.value.same_resource(&value)
        }) {
            return Ok(id);
        }
        let sender = self.insert_sender(peer, value.sender())?;
        let id = SessionTransceiverId(next_id()?);
        self.transceivers.insert(
            id,
            Resource {
                peer,
                value: Transceiver {
                    value,
                    sender,
                    receiver,
                },
            },
        );
        Ok(id)
    }
    pub fn sender(
        &mut self,
        transceiver: SessionTransceiverId,
    ) -> Result<SessionSenderId, PeerError> {
        Ok(self
            .transceivers
            .get(&transceiver)
            .ok_or_else(missing)?
            .value
            .sender)
    }
    pub fn receiver(
        &mut self,
        transceiver: SessionTransceiverId,
    ) -> Result<SessionReceiverId, PeerError> {
        Ok(self
            .transceivers
            .get(&transceiver)
            .ok_or_else(missing)?
            .value
            .receiver)
    }
    /// Register the actual SDK sender slot with explicit bounded polling.
    pub fn register_sender_packet_observer(
        &mut self,
        sender: SessionSenderId,
        capacity: usize,
    ) -> Result<(), PeerError> {
        self.check_open()?;
        let resource = self.senders.get(&sender).ok_or_else(missing)?;
        let observer = self
            .sender_observers
            .entry(sender)
            .or_insert_with(|| Resource {
                peer: resource.peer,
                value: resource.value.packet_observer(),
            });
        observer
            .value
            .register_observer(capacity)
            .map_err(|e| error(PeerErrorKind::InvalidParameter, &e.to_string()))
    }
    pub fn unregister_sender_packet_observer(
        &mut self,
        sender: SessionSenderId,
    ) -> Result<(), PeerError> {
        self.senders.get(&sender).ok_or_else(missing)?;
        if let Some(observer) = self.sender_observers.get(&sender) {
            observer.value.unregister_observer();
        }
        Ok(())
    }
    pub fn take_sender_packet_event(
        &mut self,
        sender: SessionSenderId,
    ) -> Result<Option<crate::RtpPacketEvent>, PeerError> {
        self.senders.get(&sender).ok_or_else(missing)?;
        Ok(self
            .sender_observers
            .get(&sender)
            .and_then(|observer| observer.value.take_event()))
    }
    pub fn take_sender_dropped_packet_events(
        &mut self,
        sender: SessionSenderId,
    ) -> Result<u64, PeerError> {
        self.senders.get(&sender).ok_or_else(missing)?;
        Ok(self
            .sender_observers
            .get(&sender)
            .map_or(0, |observer| observer.value.take_dropped_events()))
    }

    /// Register the actual SDK receiver slot with explicit bounded polling.
    pub fn register_receiver_packet_observer(
        &mut self,
        receiver: SessionReceiverId,
        capacity: usize,
    ) -> Result<(), PeerError> {
        self.check_open()?;
        let resource = self.receivers.get(&receiver).ok_or_else(missing)?;
        let observer = self
            .receiver_observers
            .entry(receiver)
            .or_insert_with(|| Resource {
                peer: resource.peer,
                value: resource.value.packet_observer(),
            });
        observer
            .value
            .register_observer(capacity)
            .map_err(|e| error(PeerErrorKind::InvalidParameter, &e.to_string()))
    }
    pub fn unregister_receiver_packet_observer(
        &mut self,
        receiver: SessionReceiverId,
    ) -> Result<(), PeerError> {
        self.receivers.get(&receiver).ok_or_else(missing)?;
        if let Some(observer) = self.receiver_observers.get(&receiver) {
            observer.value.unregister_observer();
        }
        Ok(())
    }
    /// Actual SDK callback record for this slot. FirstPacketReceived may fan
    /// out to every same-kind receiver; it is not receiver-SSRC delivery proof.
    pub fn take_receiver_packet_event(
        &mut self,
        receiver: SessionReceiverId,
    ) -> Result<Option<crate::RtpPacketEvent>, PeerError> {
        self.receivers.get(&receiver).ok_or_else(missing)?;
        Ok(self
            .receiver_observers
            .get(&receiver)
            .and_then(|observer| observer.value.take_event()))
    }
    pub fn take_receiver_dropped_packet_events(
        &mut self,
        receiver: SessionReceiverId,
    ) -> Result<u64, PeerError> {
        self.receivers.get(&receiver).ok_or_else(missing)?;
        Ok(self
            .receiver_observers
            .get(&receiver)
            .map_or(0, |observer| observer.value.take_dropped_events()))
    }

    /// Preserve native creation failures and renegotiation side effects; no
    /// local handle is imported or exported, even after actor migration.
    pub fn create_sframe_encrypter(
        &mut self,
        sender: SessionSenderId,
        init: &crate::SframeEncrypterInit,
    ) -> Result<SessionSframeEncrypterId, crate::SframeCreateError> {
        self.check_open()?;
        let sender = self.senders.get(&sender).ok_or_else(missing)?;
        let id = SessionSframeEncrypterId(next_id()?);
        let value = sender.value.create_sframe_encrypter(init)?;
        self.sframe_encrypters.insert(
            id,
            Resource {
                peer: sender.peer,
                value,
            },
        );
        Ok(id)
    }
    pub fn create_sframe_decrypter(
        &mut self,
        receiver: SessionReceiverId,
        suite: crate::SframeCipherSuite,
    ) -> Result<SessionSframeDecrypterId, crate::SframeCreateError> {
        self.check_open()?;
        let receiver = self.receivers.get(&receiver).ok_or_else(missing)?;
        let id = SessionSframeDecrypterId(next_id()?);
        let value = receiver.value.create_sframe_decrypter(suite)?;
        self.sframe_decrypters.insert(
            id,
            Resource {
                peer: receiver.peer,
                value,
            },
        );
        Ok(id)
    }
    pub fn sframe_set_encryption_key(
        &mut self,
        value: SessionSframeEncrypterId,
        id: u64,
        key: &[u8],
    ) -> Result<crate::RtcError, PeerError> {
        self.check_open()?;
        Ok(self
            .sframe_encrypters
            .get(&value)
            .ok_or_else(missing)?
            .value
            .set_encryption_key(id, key))
    }
    pub fn sframe_add_decryption_key(
        &mut self,
        value: SessionSframeDecrypterId,
        id: u64,
        key: &[u8],
    ) -> Result<crate::RtcError, PeerError> {
        self.check_open()?;
        Ok(self
            .sframe_decrypters
            .get(&value)
            .ok_or_else(missing)?
            .value
            .add_decryption_key(id, key))
    }
    pub fn sframe_remove_decryption_key(
        &mut self,
        value: SessionSframeDecrypterId,
        id: u64,
    ) -> Result<crate::RtcError, PeerError> {
        self.check_open()?;
        Ok(self
            .sframe_decrypters
            .get(&value)
            .ok_or_else(missing)?
            .value
            .remove_decryption_key(id))
    }
    pub fn release_sframe_encrypter(
        &mut self,
        value: SessionSframeEncrypterId,
    ) -> Result<(), PeerError> {
        self.sframe_encrypters
            .remove(&value)
            .map(|_| ())
            .ok_or_else(missing)
    }
    pub fn release_sframe_decrypter(
        &mut self,
        value: SessionSframeDecrypterId,
    ) -> Result<(), PeerError> {
        self.sframe_decrypters
            .remove(&value)
            .map(|_| ())
            .ok_or_else(missing)
    }
    /// Construct inside the captive graph; no local native handle is imported.
    pub fn create_frame_encryptor(
        &mut self,
        provider: impl crate::FrameEncryptorProvider,
    ) -> Result<SessionFrameEncryptorId, PeerError> {
        self.check_open()?;
        let id = SessionFrameEncryptorId(next_id()?);
        self.frame_encryptors
            .insert(id, crate::FrameEncryptor::from_provider(provider));
        Ok(id)
    }
    pub fn create_frame_decryptor(
        &mut self,
        receiver: SessionReceiverId,
        provider: impl crate::FrameDecryptorProvider,
    ) -> Result<SessionFrameDecryptorId, PeerError> {
        self.check_open()?;
        let receiver = self.receivers.get(&receiver).ok_or_else(missing)?;
        let id = SessionFrameDecryptorId(next_id()?);
        let value = receiver.value.create_frame_decryptor(provider);
        self.frame_decryptors.insert(
            id,
            Resource {
                peer: receiver.peer,
                value,
            },
        );
        Ok(id)
    }
    pub fn set_sender_frame_encryptor(
        &mut self,
        sender: SessionSenderId,
        encryptor: Option<SessionFrameEncryptorId>,
    ) -> Result<(), PeerError> {
        self.check_open()?;
        let value = encryptor
            .map(|id| self.frame_encryptors.get(&id).ok_or_else(missing))
            .transpose()?;
        self.senders
            .get(&sender)
            .ok_or_else(missing)?
            .value
            .set_frame_encryptor(value);
        Ok(())
    }
    pub fn set_receiver_frame_decryptor(
        &mut self,
        receiver: SessionReceiverId,
        decryptor: Option<SessionFrameDecryptorId>,
    ) -> Result<(), crate::FrameDecryptorInstallError> {
        let binding = crate::FrameDecryptorInstallError::Binding;
        self.check_open().map_err(binding)?;
        let value = decryptor
            .map(|id| {
                self.frame_decryptors
                    .get(&id)
                    .map(|r| &r.value)
                    .ok_or_else(missing)
            })
            .transpose()
            .map_err(binding)?;
        self.receivers
            .get(&receiver)
            .ok_or_else(missing)
            .map_err(binding)?
            .value
            .set_frame_decryptor(value)
    }
    pub fn sender_frame_encryptor(
        &mut self,
        sender: SessionSenderId,
    ) -> Result<Option<SessionFrameEncryptorId>, PeerError> {
        self.check_open()?;
        let Some(value) = self
            .senders
            .get(&sender)
            .ok_or_else(missing)?
            .value
            .frame_encryptor()
        else {
            return Ok(None);
        };
        if let Some((&id, _)) = self
            .frame_encryptors
            .iter()
            .find(|(_, existing)| existing.same_resource(&value))
        {
            return Ok(Some(id));
        }
        let id = SessionFrameEncryptorId(next_id()?);
        self.frame_encryptors.insert(id, value);
        Ok(Some(id))
    }
    pub fn receiver_frame_decryptor(
        &mut self,
        receiver: SessionReceiverId,
    ) -> Result<Option<SessionFrameDecryptorId>, PeerError> {
        self.check_open()?;
        let receiver = self.receivers.get(&receiver).ok_or_else(missing)?;
        let Some(value) = receiver.value.frame_decryptor() else {
            return Ok(None);
        };
        if let Some((&id, _)) = self
            .frame_decryptors
            .iter()
            .find(|(_, existing)| existing.value.same_resource(&value))
        {
            return Ok(Some(id));
        }
        let id = SessionFrameDecryptorId(next_id()?);
        self.frame_decryptors.insert(
            id,
            Resource {
                peer: receiver.peer,
                value,
            },
        );
        Ok(Some(id))
    }
    pub fn frame_encryptor_failure(
        &mut self,
        encryptor: SessionFrameEncryptorId,
    ) -> Result<Option<crate::FrameCryptoFailure>, PeerError> {
        self.check_open()?;
        Ok(self
            .frame_encryptors
            .get(&encryptor)
            .ok_or_else(missing)?
            .failure())
    }
    pub fn frame_decryptor_failure(
        &mut self,
        decryptor: SessionFrameDecryptorId,
    ) -> Result<Option<crate::FrameCryptoFailure>, PeerError> {
        self.check_open()?;
        Ok(self
            .frame_decryptors
            .get(&decryptor)
            .ok_or_else(missing)?
            .value
            .failure())
    }
    pub fn frame_encryptor_max_ciphertext_byte_size(
        &mut self,
        encryptor: SessionFrameEncryptorId,
        media: crate::MediaType,
        frame_size: usize,
    ) -> Result<usize, PeerError> {
        self.check_open()?;
        Ok(self
            .frame_encryptors
            .get(&encryptor)
            .ok_or_else(missing)?
            .max_ciphertext_byte_size(media, frame_size))
    }
    pub fn frame_decryptor_max_plaintext_byte_size(
        &mut self,
        decryptor: SessionFrameDecryptorId,
        media: crate::MediaType,
        frame_size: usize,
    ) -> Result<usize, PeerError> {
        self.check_open()?;
        Ok(self
            .frame_decryptors
            .get(&decryptor)
            .ok_or_else(missing)?
            .value
            .max_plaintext_byte_size(media, frame_size))
    }
    pub fn encrypt_frame(
        &mut self,
        encryptor: SessionFrameEncryptorId,
        media: crate::MediaType,
        ssrc: u32,
        additional_data: &[u8],
        frame: &[u8],
        capacity: usize,
    ) -> Result<crate::EncryptedFrame, crate::FrameCryptoOperationError> {
        self.check_open()?;
        self.frame_encryptors
            .get(&encryptor)
            .ok_or_else(missing)?
            .encrypt(media, ssrc, additional_data, frame, capacity)
    }
    pub fn decrypt_frame(
        &mut self,
        decryptor: SessionFrameDecryptorId,
        media: crate::MediaType,
        csrcs: &[u32],
        additional_data: &[u8],
        frame: &[u8],
        capacity: usize,
    ) -> Result<crate::DecryptedFrame, crate::FrameCryptoOperationError> {
        self.check_open()?;
        self.frame_decryptors
            .get(&decryptor)
            .ok_or_else(missing)?
            .value
            .decrypt(media, csrcs, additional_data, frame, capacity)
    }
    /// Release this application identity. An installed SDK reference remains
    /// alive until replaced/cleared or its sender/receiver is destroyed.
    pub fn release_frame_encryptor(
        &mut self,
        encryptor: SessionFrameEncryptorId,
    ) -> Result<(), PeerError> {
        self.check_open()?;
        self.frame_encryptors
            .remove(&encryptor)
            .ok_or_else(missing)?;
        Ok(())
    }
    pub fn release_frame_decryptor(
        &mut self,
        decryptor: SessionFrameDecryptorId,
    ) -> Result<(), PeerError> {
        self.check_open()?;
        self.frame_decryptors
            .remove(&decryptor)
            .ok_or_else(missing)?;
        Ok(())
    }

    /// Actual native GenerateKeyFrame, including audio unsupported results and
    /// the stock stopped/unconfigured video's successful no-op. Native errors
    /// are independent owning values, not a narrowed message/type pair.
    pub fn generate_sender_keyframe(
        &mut self,
        sender: SessionSenderId,
        rids: &[Vec<u8>],
    ) -> Result<(), crate::RtpSenderOperationError> {
        self.check_open()?;
        self.senders
            .get(&sender)
            .ok_or_else(missing)?
            .value
            .generate_keyframe(rids)
    }

    /// Actual primary SDK SSRC; zero is the native not-yet-determined value.
    pub fn sender_ssrc(&mut self, sender: SessionSenderId) -> Result<u32, PeerError> {
        self.check_open()?;
        Ok(self.senders.get(&sender).ok_or_else(missing)?.value.ssrc())
    }

    /// SDK-generated sender identity, distinct from the actor resource ID.
    pub fn sender_native_id(&mut self, sender: SessionSenderId) -> Result<String, PeerError> {
        self.check_open()?;
        Ok(self.senders.get(&sender).ok_or_else(missing)?.value.id())
    }

    pub fn sender_media_type(
        &mut self,
        sender: SessionSenderId,
    ) -> Result<crate::MediaType, PeerError> {
        self.check_open()?;
        Ok(self
            .senders
            .get(&sender)
            .ok_or_else(missing)?
            .value
            .media_type())
    }

    /// A single native snapshot of stream IDs with their full byte domain.
    pub fn sender_stream_ids(
        &mut self,
        sender: SessionSenderId,
    ) -> Result<Vec<Vec<u8>>, PeerError> {
        self.check_open()?;
        Ok(self
            .senders
            .get(&sender)
            .ok_or_else(missing)?
            .value
            .stream_id_bytes())
    }

    /// UTF-8 convenience installation through SDK SetStreams. Native
    /// de-duplication applies. This void operation is not a negotiation ack.
    pub fn set_sender_streams(
        &mut self,
        sender: SessionSenderId,
        ids: &[String],
    ) -> Result<(), PeerError> {
        self.check_open()?;
        self.senders
            .get(&sender)
            .ok_or_else(missing)?
            .value
            .set_streams(ids);
        Ok(())
    }

    /// Initial encodings are independent complete SDK value copies, not a
    /// GetParameters transaction or a reliable seed for a reused sender.
    pub fn sender_initial_encodings(
        &mut self,
        sender: SessionSenderId,
    ) -> Result<Vec<crate::RtpEncodingParameters>, PeerError> {
        self.check_open()?;
        Ok(self
            .senders
            .get(&sender)
            .ok_or_else(missing)?
            .value
            .init_send_encodings())
    }

    /// Entire actual SDK snapshot; each successful SDK get starts a fresh
    /// transaction, invalidating older snapshots for a subsequent native set.
    pub fn get_sender_parameters(
        &mut self,
        sender: SessionSenderId,
    ) -> Result<crate::RtpParameters, PeerError> {
        self.check_open()?;
        Ok(self
            .senders
            .get(&sender)
            .ok_or_else(missing)?
            .value
            .get_parameters())
    }

    /// Native SSRC/CSRC observations copied through the receiver worker proxy.
    pub fn receiver_sources(
        &mut self,
        receiver: SessionReceiverId,
    ) -> Result<Vec<crate::RtpSource>, PeerError> {
        self.check_open()?;
        Ok(self
            .receivers
            .get(&receiver)
            .ok_or_else(missing)?
            .value
            .get_sources())
    }

    /// Entire independently owned SDK receiver snapshot.
    pub fn get_receiver_parameters(
        &mut self,
        receiver: SessionReceiverId,
    ) -> Result<crate::RtpParameters, PeerError> {
        self.check_open()?;
        Ok(self
            .receivers
            .get(&receiver)
            .ok_or_else(missing)?
            .value
            .get_parameters())
    }

    /// Preserves the legacy SDK receiver setter's unsupported false result.
    pub fn set_receiver_parameters(
        &mut self,
        receiver: SessionReceiverId,
        parameters: &crate::RtpParameters,
    ) -> Result<bool, PeerError> {
        self.check_open()?;
        Ok(self
            .receivers
            .get(&receiver)
            .ok_or_else(missing)?
            .value
            .set_parameters(parameters))
    }

    pub fn set_receiver_jitter_buffer_minimum_delay(
        &mut self,
        receiver: SessionReceiverId,
        seconds: Option<f64>,
    ) -> Result<(), crate::RtpReceiverOperationError> {
        self.check_open()?;
        self.receivers
            .get(&receiver)
            .ok_or_else(missing)?
            .value
            .set_jitter_buffer_minimum_delay(seconds)
    }

    pub fn sender_parameters(
        &mut self,
        sender: SessionSenderId,
    ) -> Result<RtpSenderParameters, PeerError> {
        self.senders
            .get(&sender)
            .ok_or_else(missing)?
            .value
            .parameters()
    }
    pub fn set_sender_parameters(
        &mut self,
        sender: SessionSenderId,
        parameters: RtpSenderParameters,
    ) -> Result<(), PeerError> {
        self.check_open()?;
        self.senders
            .get(&sender)
            .ok_or_else(missing)?
            .value
            .set_parameters(parameters)
    }
    pub fn header_extensions_to_negotiate(
        &mut self,
        transceiver: SessionTransceiverId,
    ) -> Result<Vec<RtpHeaderExtensionCapability>, PeerError> {
        self.transceivers
            .get(&transceiver)
            .ok_or_else(missing)?
            .value
            .value
            .header_extensions_to_negotiate()
    }

    /// Full owned SDK preferences without legacy format reconstruction.
    pub fn rtp_codec_preferences(
        &mut self,
        transceiver: SessionTransceiverId,
    ) -> Result<Vec<crate::RtpCodecCapability>, PeerError> {
        self.transceivers
            .get(&transceiver)
            .ok_or_else(missing)?
            .value
            .value
            .rtp_codec_preferences()
    }

    pub fn header_extension_capabilities_to_negotiate(
        &mut self,
        transceiver: SessionTransceiverId,
    ) -> Result<Vec<crate::RtpHeaderExtensionCapabilityValue>, PeerError> {
        self.transceivers
            .get(&transceiver)
            .ok_or_else(missing)?
            .value
            .value
            .header_extension_capabilities_to_negotiate()
    }

    pub fn negotiated_header_extension_capabilities(
        &mut self,
        transceiver: SessionTransceiverId,
    ) -> Result<Vec<crate::RtpHeaderExtensionCapabilityValue>, PeerError> {
        self.transceivers
            .get(&transceiver)
            .ok_or_else(missing)?
            .value
            .value
            .negotiated_header_extension_capabilities()
    }

    pub fn set_rtp_codec_preferences(
        &mut self,
        transceiver: SessionTransceiverId,
        codecs: &[crate::RtpCodecCapability],
    ) -> Result<(), crate::RtpCapabilityInstallError> {
        self.check_open()
            .map_err(crate::RtpCapabilityInstallError::Binding)?;
        self.transceivers
            .get(&transceiver)
            .ok_or_else(|| crate::RtpCapabilityInstallError::Binding(missing()))?
            .value
            .value
            .set_rtp_codec_preferences(codecs)
    }

    pub fn set_header_extension_capabilities_to_negotiate(
        &mut self,
        transceiver: SessionTransceiverId,
        extensions: &[crate::RtpHeaderExtensionCapabilityValue],
    ) -> Result<(), crate::RtpCapabilityInstallError> {
        self.check_open()
            .map_err(crate::RtpCapabilityInstallError::Binding)?;
        self.transceivers
            .get(&transceiver)
            .ok_or_else(|| crate::RtpCapabilityInstallError::Binding(missing()))?
            .value
            .value
            .set_header_extension_capabilities_to_negotiate(extensions)
    }

    pub fn negotiated_header_extensions(
        &mut self,
        transceiver: SessionTransceiverId,
    ) -> Result<Vec<RtpHeaderExtensionCapability>, PeerError> {
        self.transceivers
            .get(&transceiver)
            .ok_or_else(missing)?
            .value
            .value
            .negotiated_header_extensions()
    }

    pub fn set_header_extensions_to_negotiate(
        &mut self,
        transceiver: SessionTransceiverId,
        extensions: &[RtpHeaderExtensionCapability],
    ) -> Result<(), PeerError> {
        self.check_open()?;
        self.transceivers
            .get(&transceiver)
            .ok_or_else(missing)?
            .value
            .value
            .set_header_extensions_to_negotiate(extensions)
    }

    pub fn transceiver_media_type(
        &mut self,
        transceiver: SessionTransceiverId,
    ) -> Result<crate::MediaType, PeerError> {
        Ok(self
            .transceivers
            .get(&transceiver)
            .ok_or_else(missing)?
            .value
            .value
            .media_type())
    }

    pub fn transceiver_stopping(
        &mut self,
        transceiver: SessionTransceiverId,
    ) -> Result<bool, PeerError> {
        Ok(self
            .transceivers
            .get(&transceiver)
            .ok_or_else(missing)?
            .value
            .value
            .stopping())
    }

    pub fn transceiver_receptive(
        &mut self,
        transceiver: SessionTransceiverId,
    ) -> Result<bool, PeerError> {
        Ok(self
            .transceivers
            .get(&transceiver)
            .ok_or_else(missing)?
            .value
            .value
            .receptive())
    }

    pub fn fired_direction(
        &mut self,
        transceiver: SessionTransceiverId,
    ) -> Result<Option<crate::RtpTransceiverDirectionValue>, PeerError> {
        Ok(self
            .transceivers
            .get(&transceiver)
            .ok_or_else(missing)?
            .value
            .value
            .fired_direction())
    }

    pub fn sframe_enabled(
        &mut self,
        transceiver: SessionTransceiverId,
    ) -> Result<Option<bool>, PeerError> {
        Ok(self
            .transceivers
            .get(&transceiver)
            .ok_or_else(missing)?
            .value
            .value
            .sframe_enabled())
    }

    pub fn direction_value(
        &mut self,
        transceiver: SessionTransceiverId,
    ) -> Result<crate::RtpTransceiverDirectionValue, PeerError> {
        Ok(self
            .transceivers
            .get(&transceiver)
            .ok_or_else(missing)?
            .value
            .value
            .direction_value())
    }

    pub fn current_direction_value(
        &mut self,
        transceiver: SessionTransceiverId,
    ) -> Result<Option<crate::RtpTransceiverDirectionValue>, PeerError> {
        Ok(self
            .transceivers
            .get(&transceiver)
            .ok_or_else(missing)?
            .value
            .value
            .current_direction_value())
    }

    pub fn set_direction_value(
        &mut self,
        transceiver: SessionTransceiverId,
        direction: crate::RtpTransceiverDirectionValue,
    ) -> Result<(), crate::RtpTransceiverOperationError> {
        self.check_open()
            .map_err(crate::RtpTransceiverOperationError::Binding)?;
        self.transceivers
            .get(&transceiver)
            .ok_or_else(missing)
            .map_err(crate::RtpTransceiverOperationError::Binding)?
            .value
            .value
            .set_direction_value(direction)
    }

    pub fn stop_standard(
        &mut self,
        transceiver: SessionTransceiverId,
    ) -> Result<(), crate::RtpTransceiverOperationError> {
        self.check_open()
            .map_err(crate::RtpTransceiverOperationError::Binding)?;
        self.transceivers
            .get(&transceiver)
            .ok_or_else(missing)
            .map_err(crate::RtpTransceiverOperationError::Binding)?
            .value
            .value
            .stop_standard()
    }

    pub fn transceiver_stopped(
        &mut self,
        transceiver: SessionTransceiverId,
    ) -> Result<bool, PeerError> {
        Ok(self
            .transceivers
            .get(&transceiver)
            .ok_or_else(missing)?
            .value
            .value
            .stopped())
    }

    pub fn set_direction(
        &mut self,
        transceiver: SessionTransceiverId,
        direction: RtpTransceiverDirection,
    ) -> Result<(), PeerError> {
        self.check_open()?;
        self.transceivers
            .get(&transceiver)
            .ok_or_else(missing)?
            .value
            .value
            .set_direction(direction)
    }
    pub fn replace_video_source(
        &mut self,
        sender: SessionSenderId,
        source: Option<SessionSourceId>,
        track_id: &str,
    ) -> Result<(), PeerError> {
        self.check_open()?;
        let track = source
            .map(|id| {
                let Some(Source::Video(source)) = self.sources.get(&id) else {
                    return Err(missing());
                };
                source.create_track(&self.factory, track_id)
            })
            .transpose()?;
        self.senders
            .get(&sender)
            .ok_or_else(missing)?
            .value
            .set_track(track.as_ref())
    }
    pub fn request_keyframe(&mut self, receiver: SessionReceiverId) -> Result<(), PeerError> {
        self.check_open()?;
        self.receivers
            .get(&receiver)
            .ok_or_else(missing)?
            .value
            .request_keyframe()
    }
    pub fn attach_encoded_audio(&mut self, receiver: SessionReceiverId) -> Result<(), PeerError> {
        self.check_open()?;
        if self.audio_sinks.contains_key(&receiver) || self.video_sinks.contains_key(&receiver) {
            return Err(error(
                PeerErrorKind::InvalidState,
                "receiver already has a sink",
            ));
        }
        let sink = self
            .receivers
            .get(&receiver)
            .ok_or_else(missing)?
            .value
            .attach_encoded_audio_sink()?;
        self.audio_sinks.insert(receiver, sink);
        Ok(())
    }
    pub fn attach_encoded_video(&mut self, receiver: SessionReceiverId) -> Result<(), PeerError> {
        self.check_open()?;
        if self.video_sinks.contains_key(&receiver) || self.audio_sinks.contains_key(&receiver) {
            return Err(error(
                PeerErrorKind::InvalidState,
                "receiver already has a sink",
            ));
        }
        let sink = self
            .receivers
            .get(&receiver)
            .ok_or_else(missing)?
            .value
            .attach_encoded_sink()?;
        self.video_sinks.insert(receiver, sink);
        Ok(())
    }
    pub fn try_audio_frame(
        &mut self,
        receiver: SessionReceiverId,
    ) -> Result<Option<EncodedAudioFrame>, PeerError> {
        Ok(self
            .audio_sinks
            .get(&receiver)
            .ok_or_else(missing)?
            .try_next_frame())
    }
    pub fn try_video_frame(
        &mut self,
        receiver: SessionReceiverId,
    ) -> Result<Option<EncodedReceivedVideoFrame>, PeerError> {
        Ok(self
            .video_sinks
            .get(&receiver)
            .ok_or_else(missing)?
            .try_next_frame())
    }
    pub fn dropped_frames(&mut self, receiver: SessionReceiverId) -> Result<u64, PeerError> {
        if let Some(sink) = self.video_sinks.get(&receiver) {
            return Ok(sink.dropped_frames());
        }
        if let Some(sink) = self.audio_sinks.get(&receiver) {
            return Ok(sink.dropped_frames());
        }
        Err(missing())
    }

    /// Close without removing the peer's observation identity. Accepted pending
    /// operations still yield exactly one outcome through try_peer_event.
    pub fn close_peer(&mut self, peer: SessionPeerId) -> Result<(), PeerError> {
        self.peers.get(&peer).ok_or_else(missing)?;
        self.connectivity.retire_dtmf_observers(peer);
        // Retire raw observer slots before stopping the peer's resource graph.
        // Already queued records remain available, including after close.
        for observer in self
            .sender_observers
            .values()
            .filter(|value| value.peer == peer)
        {
            observer.value.unregister_observer();
        }
        for observer in self
            .receiver_observers
            .values()
            .filter(|value| value.peer == peer)
        {
            observer.value.unregister_observer();
        }
        self.peers
            .get_mut(&peer)
            .ok_or_else(missing)?
            .value
            .close()?;
        for resource in self.channels.values_mut().filter(|v| v.peer == peer) {
            resource.value.close()?;
        }
        for (&id, resource) in &self.receivers {
            if resource.peer == peer {
                if let Some(sink) = self.audio_sinks.get_mut(&id) {
                    sink.close()?;
                }
                if let Some(sink) = self.video_sinks.get_mut(&id) {
                    sink.close();
                }
            }
        }
        Ok(())
    }
    pub fn shutdown(&mut self) -> Result<(), PeerError> {
        if self.closed {
            return Ok(());
        }
        for peer in self.peers.keys().copied().collect::<Vec<_>>() {
            self.close_peer(peer)?;
        }
        for source in self.sources.values_mut() {
            match source {
                Source::Opus(source) => source
                    .close()
                    .map_err(|e| error(PeerErrorKind::Internal, &e.to_string()))?,
                Source::Video(source) => source.close().map_err(codec_error)?,
            }
        }
        self.sources.clear();
        self.closed = true;
        self.readiness.notify();
        Ok(())
    }
}

impl Drop for ProductionSession {
    fn drop(&mut self) {
        let _ = self.shutdown();
        // Field order releases observers/sinks, RTP handles, sources and peers
        // before the factory releases its existing engine role threads.
    }
}
