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
    AudioDecoderFactory, AudioEncoderFactory, CodecError, CodecSupport, DataChannel,
    DataChannelConfiguration, DataChannelError, DataChannelEvent, DataChannelMessage,
    DataChannelSendResult, EncodedAudioFrame, EncodedAudioSink, EncodedAudioSource,
    EncodedReceivedVideoFrame, EncodedVideoAccessUnit, EncodedVideoInput, EncodedVideoSink,
    EncodedVideoSource, IceCandidate, OperationCompletion, OperationId, PeerConfiguration,
    PeerConnection, PeerConnectionEvent, PeerConnectionFactory, PeerDescriptions, PeerError,
    PeerErrorKind, PeerStatsSnapshot, RtpReceiver, RtpSender, RtpSenderParameters, RtpTransceiver,
    RtpTransceiverDirection, SessionDescription, VideoCodecFormat, VideoDecoder,
    VideoDecoderFactory, VideoDecoderFactoryHandle, VideoRateControl, VideoResolution,
    readiness::Readiness,
};

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
    SessionReceiverId
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

// Advertise the wire codec for encoded-only reception without claiming that a
// decoder exists. An attempted decode explicitly fails rather than fabricating
// output. The receiver's pre-decode encoded sink is the supported receive path.
struct EncodedReceiveFactory(VideoCodecFormat);
impl VideoDecoderFactory for EncodedReceiveFactory {
    fn supported_formats(&self) -> Vec<VideoCodecFormat> {
        vec![self.0.clone()]
    }
    fn query_support(
        &self,
        _: &VideoCodecFormat,
        _: bool,
        _: Option<VideoResolution>,
    ) -> CodecSupport {
        CodecSupport {
            supported: false,
            power_efficient: false,
        }
    }
    fn create(&self, _: &VideoCodecFormat) -> Result<Box<dyn VideoDecoder>, CodecError> {
        Err(CodecError::UnsupportedFormat)
    }
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
    IceCandidate(IceCandidate),
    ConnectionStateChanged(crate::ConnectionState),
    SignalingStateChanged(crate::SignalingState),
    IceGatheringStateChanged(crate::IceGatheringState),
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
    DataChannel {
        channel: SessionChannelId,
        label: String,
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
// returns or imports a handle, callback, provider, Rc, or reference to that
// graph. Rc/Cell/RefCell mutations and destruction occur only under exclusive
// session ownership, even after migration; native callbacks never touch them.
// Native objects marshal affine operations and final releases to retained live
// engine role threads (including the bypass-proxy SDP initiation overloads).
// Only the independently synchronized Readiness and codec broker states cross
// callback threads. Drop tears down all dependents before the role threads.
// This rationale does NOT apply to any individual legacy/local handle.
unsafe impl Send for ProductionSession {}

impl ProductionSession {
    pub fn new(config: ProductionSessionConfig) -> Result<Self, PeerError> {
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
        let readiness = Arc::new(Readiness::default());
        let mut builder = PeerConnectionFactory::builder()
            .audio_encoder_factory(AudioEncoderFactory::with_opus_frames().map_err(codec_error)?)
            .audio_decoder_factory(AudioDecoderFactory::builtin_opus().map_err(codec_error)?)
            .readiness(readiness.clone());
        let input = config
            .video_format
            .map(|format| {
                let input =
                    EncodedVideoInput::new_for_format(format.clone()).map_err(codec_error)?;
                let decoder = VideoDecoderFactoryHandle::new(EncodedReceiveFactory(format))
                    .map_err(codec_error)?;
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
        let factory = builder.build()?;
        Ok(Self {
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

    pub fn create_peer(&mut self, config: PeerConfiguration) -> Result<SessionPeerId, PeerError> {
        self.check_open()?;
        let id = SessionPeerId(next_id()?);
        let value = self.factory.create_peer_connection(config)?;
        self.peers.insert(
            id,
            Peer {
                value,
                outstanding: HashSet::new(),
            },
        );
        Ok(id)
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
            Ok(if ice_restart {
                p.create_ice_restart_offer()
            } else {
                p.create_offer()
            })
        })
    }
    pub fn create_answer(&mut self, peer: SessionPeerId) -> Result<OperationId, PeerError> {
        self.operation(peer, |p| Ok(p.create_answer()))
    }
    pub fn set_local_description(
        &mut self,
        peer: SessionPeerId,
        description: SessionDescription,
    ) -> Result<OperationId, PeerError> {
        self.operation(peer, |p| Ok(p.set_local_description(description)))
    }
    pub fn set_remote_description(
        &mut self,
        peer: SessionPeerId,
        description: SessionDescription,
    ) -> Result<OperationId, PeerError> {
        self.operation(peer, |p| Ok(p.set_remote_description(description)))
    }
    pub fn add_ice_candidate(
        &mut self,
        peer: SessionPeerId,
        candidate: IceCandidate,
    ) -> Result<OperationId, PeerError> {
        self.operation(peer, |p| Ok(p.add_ice_candidate(candidate)))
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

    /// Activity notification only, not proof of a particular queued event.
    /// Drain owned events/frames, then wait. Signals coalesce and may be spurious.
    /// Polling never enters the engine, pumps work or advances a clock. A single
    /// actor waiter may register; its waker must schedule, not execute engine work.
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
            PeerConnectionEvent::IceCandidate(v) => SessionEvent::IceCandidate(v),
            PeerConnectionEvent::ConnectionStateChanged(v) => {
                SessionEvent::ConnectionStateChanged(v)
            }
            PeerConnectionEvent::SignalingStateChanged(v) => SessionEvent::SignalingStateChanged(v),
            PeerConnectionEvent::IceGatheringStateChanged(v) => {
                SessionEvent::IceGatheringStateChanged(v)
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
    ) -> Result<SessionChannelId, PeerError> {
        self.check_open()?;
        let id = SessionChannelId(next_id()?);
        let value = self
            .peers
            .get(&peer)
            .ok_or_else(missing)?
            .value
            .create_data_channel(label, config)?;
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
            .create_source(&self.factory)?;
        self.sources.insert(id, Source::Video(source));
        Ok(id)
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
    pub fn video_feedback(
        &mut self,
        source: SessionSourceId,
    ) -> Result<(bool, Option<VideoRateControl>), PeerError> {
        let Some(Source::Video(source)) = self.sources.get(&source) else {
            return Err(missing());
        };
        Ok((source.take_keyframe_request(), source.latest_rate_control()))
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
    fn insert_receiver(
        &mut self,
        peer: SessionPeerId,
        value: RtpReceiver,
    ) -> Result<SessionReceiverId, PeerError> {
        let native_id = value.id();
        if let Some((&id, _)) = self
            .receivers
            .iter()
            .find(|(_, existing)| existing.peer == peer && existing.value.id() == native_id)
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
        if let Some((&id, _)) = self
            .transceivers
            .iter()
            .find(|(_, existing)| existing.peer == peer && existing.value.receiver == receiver)
        {
            return Ok(id);
        }
        let sender = SessionSenderId(next_id()?);
        self.senders.insert(
            sender,
            Resource {
                peer,
                value: value.sender(),
            },
        );
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
