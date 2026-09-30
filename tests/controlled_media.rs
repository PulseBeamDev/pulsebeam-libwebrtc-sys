//! Safe-public-API qualification of the controlled encoded-input media path.
use pulsebeam_webrtc_sys::*;
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    sync::Mutex,
    time::Duration,
};

static CONTROLLED_TEST: Mutex<()> = Mutex::new(());

struct UnavailableVp8;
impl VideoDecoderFactory for UnavailableVp8 {
    fn supported_formats(&self) -> Vec<VideoCodecFormat> {
        vec![VideoCodecFormat::new("VP8")]
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

const OPUS: [&[u8]; 2] = [
    include_bytes!("fixtures/simulation-tone-440.opus"),
    include_bytes!("fixtures/simulation-tone-880.opus"),
];
const VP8: [&[u8]; 2] = [
    include_bytes!("fixtures/simulation-red.vp8"),
    include_bytes!("fixtures/simulation-blue.vp8"),
];

#[derive(Debug, Default, PartialEq, Eq)]
struct Trace {
    events: Vec<String>,
    messages: Vec<(usize, DataChannelMessage)>,
    audio: Vec<ReceivedAudioFrame>,
    video: Vec<ReceivedVideoFrame>,
}

#[derive(Default)]
struct Observations {
    impaired: bool,
    link_sequence: u64,
    pending_packets: Vec<(Duration, u64, bool)>,
    retired: bool,
    gathered: [bool; 2],
    operations: Vec<(usize, OperationCompletion)>,
    terminal_operations: Vec<(usize, OperationId)>,
    audio_ids: Vec<(usize, String)>,
    video_ids: Vec<(usize, String)>,
    audio: Vec<(usize, AudioSink)>,
    video: Vec<(usize, VideoSink)>,
    channels: Vec<(usize, DataChannel)>,
    trace: Trace,
}

impl Observations {
    fn step(
        &mut self,
        world: &ControlledWorld,
        network: &ControlledSimulatedNetwork,
        peers: &[PeerConnection; 2],
        media: bool,
    ) {
        let now = world.now();
        let pump = world.pump(512);
        assert!(pump.dispatched <= 512);
        assert_eq!(world.now(), now, "pumping advanced virtual time");
        for _ in 0..256 {
            let Some(packet) = network.next_packet() else {
                break;
            };
            if self.impaired && !self.retired {
                self.link_sequence += 1;
                let sequence = self.link_sequence;
                // Fixed-seed external fixture policy, not a library policy.
                let random = sequence.wrapping_mul(6364136223846793005).wrapping_add(159);
                if sequence % 23 == 0 {
                    network.drop_packet(packet.id).unwrap();
                    self.trace
                        .events
                        .push(format!("{}:link:{sequence}:drop", now.as_micros()));
                } else {
                    let deadline = now + Duration::from_millis(5 + (random >> 32) % 26);
                    self.pending_packets
                        .push((deadline, packet.id, sequence % 13 == 0));
                    self.trace.events.push(format!(
                        "{}:link:{sequence}:schedule:{}",
                        now.as_micros(),
                        deadline.as_micros()
                    ));
                }
                continue;
            }
            match network.deliver(packet.id) {
                Ok(()) => {}
                Err(NetworkError::DestinationUnavailable) if self.retired => {
                    // Close may emit RTCP BYE/transport shutdown traffic before
                    // tearing down its destination socket. This disposition
                    // is observable, but must not resurrect a retired decoder.
                    self.trace
                        .events
                        .push(format!("{}:network:retired-destination", now.as_micros()));
                }
                Err(error) => panic!("live network delivery failed: {error:?}"),
            }
        }
        self.pending_packets
            .sort_by_key(|(deadline, id, _)| (*deadline, std::cmp::Reverse(*id)));
        while self
            .pending_packets
            .first()
            .is_some_and(|(deadline, _, _)| *deadline <= now)
        {
            let (_, id, duplicate) = self.pending_packets.remove(0);
            if self.retired {
                network.drop_packet(id).unwrap();
            } else {
                if duplicate {
                    network.deliver_copy(id).unwrap();
                    self.trace
                        .events
                        .push(format!("{}:link:duplicate", now.as_micros()));
                }
                network.deliver(id).unwrap();
            }
        }
        for (who, peer) in peers.iter().enumerate() {
            while let Some(event) = peer.try_next_event() {
                let description = match event {
                    PeerConnectionEvent::OperationComplete(result) => {
                        let status = match &result.result {
                            Ok(_) => "ok".to_owned(),
                            Err(error) => format!("error:{:?}", error.kind),
                        };
                        let description = format!("operation:{:?}:{status}", result.operation_id);
                        assert!(
                            !self
                                .terminal_operations
                                .contains(&(who, result.operation_id)),
                            "duplicate terminal operation"
                        );
                        self.terminal_operations.push((who, result.operation_id));
                        self.operations.push((who, result));
                        description
                    }
                    PeerConnectionEvent::IceGatheringStateChanged(state) => {
                        self.gathered[who] = state == IceGatheringState::Complete;
                        format!("gathering:{state:?}")
                    }
                    PeerConnectionEvent::ConnectionStateChanged(state) => {
                        format!("connection:{state:?}")
                    }
                    PeerConnectionEvent::SignalingStateChanged(state) => {
                        format!("signaling:{state:?}")
                    }
                    PeerConnectionEvent::Track(transceiver) => {
                        let receiver = transceiver.receiver();
                        let id = receiver.id();
                        if let Some(track) = receiver.track() {
                            self.video_ids.push((who, track.id()));
                            self.video.push((who, track.attach_sink().unwrap()));
                        } else {
                            self.audio_ids.push((who, id.clone()));
                            self.audio
                                .push((who, receiver.attach_audio_sink().unwrap()));
                        }
                        format!("track:{id}")
                    }
                    PeerConnectionEvent::TrackRemoved(receiver) => {
                        format!("removed:{}", receiver.id())
                    }
                    PeerConnectionEvent::DataChannel(channel) => {
                        self.channels.push((who, channel));
                        "channel".into()
                    }
                    PeerConnectionEvent::Closed => "closed".into(),
                    // Gathering candidates are represented by the gathering
                    // outcome. SDP/ICE credentials and certificate fingerprints
                    // are not part of the logical trace.
                    PeerConnectionEvent::IceCandidate(_)
                    | PeerConnectionEvent::NegotiationNeeded { .. } => continue,
                    other => panic!("unexpected peer event: {other:?}"),
                };
                self.trace
                    .events
                    .push(format!("{}:{who}:{description}", now.as_micros()));
            }
        }
        for (who, channel) in &self.channels {
            while let Some(event) = channel.try_next_event() {
                if let DataChannelEvent::Message(message) = &event {
                    self.trace.messages.push((*who, message.clone()));
                }
                self.trace
                    .events
                    .push(format!("{}:{who}:data:{event:?}", now.as_micros()));
            }
        }
        if media {
            for (_, sink) in &self.audio {
                if let Some(frame) = sink.try_next_received_frame() {
                    self.trace.audio.push(frame);
                }
            }
            for (_, sink) in &self.video {
                for _ in 0..4 {
                    let Some(frame) = sink.try_next_received_frame() else {
                        break;
                    };
                    self.trace.video.push(frame);
                }
            }
        }
    }

    fn completed(
        &mut self,
        world: &ControlledWorld,
        network: &ControlledSimulatedNetwork,
        peers: &[PeerConnection; 2],
        who: usize,
        operation: OperationId,
    ) -> Option<SessionDescription> {
        for _ in 0..2_000 {
            self.step(world, network, peers, false);
            if let Some(at) = self
                .operations
                .iter()
                .position(|(owner, result)| *owner == who && result.operation_id == operation)
            {
                return self.operations.remove(at).1.result.unwrap();
            }
            world.advance(Duration::from_millis(1)).unwrap();
        }
        panic!(
            "operation {operation:?} did not complete; trace={:?}",
            self.trace.events
        );
    }

    fn gather(
        &mut self,
        world: &ControlledWorld,
        network: &ControlledSimulatedNetwork,
        peers: &[PeerConnection; 2],
        who: usize,
    ) -> SessionDescription {
        for _ in 0..2_000 {
            self.step(world, network, peers, false);
            if self.gathered[who] {
                let descriptions = peers[who].descriptions().unwrap();
                return descriptions
                    .pending_local
                    .or(descriptions.current_local)
                    .unwrap();
            }
            world.advance(Duration::from_millis(1)).unwrap();
        }
        panic!("gathering did not complete");
    }

    fn negotiate(
        &mut self,
        world: &ControlledWorld,
        network: &ControlledSimulatedNetwork,
        peers: &[PeerConnection; 2],
        offerer: usize,
    ) {
        let answerer = 1 - offerer;
        let offer = self
            .completed(
                world,
                network,
                peers,
                offerer,
                peers[offerer].create_offer(),
            )
            .unwrap();
        self.completed(
            world,
            network,
            peers,
            offerer,
            peers[offerer].set_local_description(offer),
        );
        let offer = self.gather(world, network, peers, offerer);
        self.completed(
            world,
            network,
            peers,
            answerer,
            peers[answerer].set_remote_description(offer),
        );
        let answer = self
            .completed(
                world,
                network,
                peers,
                answerer,
                peers[answerer].create_answer(),
            )
            .unwrap();
        self.completed(
            world,
            network,
            peers,
            answerer,
            peers[answerer].set_local_description(answer),
        );
        let answer = self.gather(world, network, peers, answerer);
        self.completed(
            world,
            network,
            peers,
            offerer,
            peers[offerer].set_remote_description(answer),
        );
    }
}

fn vp8_frame(world: &ControlledWorld, bytes: &[u8]) -> EncodedVideoAccessUnit {
    EncodedVideoAccessUnit {
        data: bytes.to_vec(),
        width: 32,
        height: 32,
        timestamp_us: world.now().as_micros() as i64,
        key_frame: true,
        qp: None,
        metadata: EncodedVideoMetadata {
            codec: EncodedVideoCodec::Vp8 {
                non_reference: false,
                layer_sync: false,
                key_index: None,
            },
            temporal_index: None,
            simulcast_index: None,
            spatial_index: None,
            end_of_picture: true,
        },
    }
}

fn input_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(14695981039346656037, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(1099511628211)
    })
}

fn run_media(addresses: [IpAddr; 2], impaired: bool) -> Trace {
    eprintln!("CONTROLLED_MEDIA_BEGIN");
    let trace = {
        let world = ControlledWorld::acquire(731, Duration::from_secs(10)).unwrap();
        let network = world.create_network().unwrap();
        let input = EncodedVideoInput::new_for_format(VideoCodecFormat::new("VP8")).unwrap();
        let audio_decoders = [
            AudioDecoderFactory::builtin_opus().unwrap(),
            AudioDecoderFactory::builtin_opus().unwrap(),
        ];
        let video_decoders = [
            VideoDecoderFactoryHandle::builtin_vp8().unwrap(),
            VideoDecoderFactoryHandle::builtin_vp8().unwrap(),
        ];
        // Endpoint drop is an explicit link shutdown. Keep the endpoint
        // owners alive, not just the provider recipes built from them.
        let endpoints: Vec<_> = addresses
            .into_iter()
            .map(|address| network.register_endpoint(address).unwrap())
            .collect();
        let factories: Vec<_> = (0..2)
            .map(|who| {
                let endpoint = &endpoints[who];
                world
                    .peer_factory_builder()
                    .unwrap()
                    .network_manager(endpoint.network_manager().unwrap())
                    .packet_socket_factory(endpoint.packet_socket_factory().unwrap())
                    .audio_encoder_factory(AudioEncoderFactory::with_opus_frames().unwrap())
                    .audio_decoder_factory(audio_decoders[who].clone())
                    .video_encoder_factory(input.encoder_factory())
                    .video_decoder_factory(video_decoders[who].clone())
                    .controlled_media()
                    .build()
                    .unwrap()
            })
            .collect();
        let mut peers = [
            factories[0]
                .create_peer_connection(PeerConfiguration::default())
                .unwrap(),
            factories[1]
                .create_peer_connection(PeerConfiguration::default())
                .unwrap(),
        ];
        let audio_sources: Vec<_> = factories
            .iter()
            .map(|factory| factory.create_encoded_audio_source(1).unwrap())
            .collect();
        let mut video_sources: Vec<_> = factories
            .iter()
            .map(|factory| input.create_source(factory).unwrap())
            .collect();
        let mut tracks = Vec::new();
        let mut audio_tracks = Vec::new();
        let mut transceivers = Vec::new();
        for who in 0..2 {
            let track = video_sources[who]
                .create_track(&factories[who], &format!("video-{who}"))
                .unwrap();
            let audio_track = factories[who]
                .create_encoded_audio_track(&format!("audio-{who}"), &audio_sources[who])
                .unwrap();
            transceivers.push(
                peers[who]
                    .add_video_transceiver(&track, RtpTransceiverDirection::SendReceive)
                    .unwrap(),
            );
            let audio_transceiver = peers[who]
                .add_audio_transceiver(&audio_track, RtpTransceiverDirection::SendReceive)
                .unwrap();
            let mono = peers[who]
                .audio_sender_capabilities()
                .unwrap()
                .into_iter()
                .find(|codec| {
                    codec.name().eq_ignore_ascii_case("opus")
                        && codec
                            .parameters()
                            .iter()
                            .any(|parameter| parameter.key == "stereo" && parameter.value == "0")
                })
                .expect("mono Opus capability");
            audio_transceiver
                .set_audio_codec_preferences(&[mono])
                .unwrap();
            transceivers.push(audio_transceiver);
            tracks.push(track);
            audio_tracks.push(audio_track);
        }
        let outgoing = peers[0]
            .create_data_channel("media-control", DataChannelConfiguration::default())
            .unwrap();
        let mut observations = Observations {
            impaired,
            ..Observations::default()
        };
        let offer = observations
            .completed(&world, &network, &peers, 0, peers[0].create_offer())
            .unwrap();
        observations.completed(
            &world,
            &network,
            &peers,
            0,
            peers[0].set_local_description(offer),
        );
        let offer = observations.gather(&world, &network, &peers, 0);
        observations.completed(
            &world,
            &network,
            &peers,
            1,
            peers[1].set_remote_description(offer),
        );
        let answer = observations
            .completed(&world, &network, &peers, 1, peers[1].create_answer())
            .unwrap();
        observations.completed(
            &world,
            &network,
            &peers,
            1,
            peers[1].set_local_description(answer),
        );
        let answer = observations.gather(&world, &network, &peers, 1);
        observations.completed(
            &world,
            &network,
            &peers,
            0,
            peers[0].set_remote_description(answer),
        );
        // Explicit AddTransceiver entries on the answerer are not implicitly
        // reused for remote offer m-lines. Negotiate its two senders in a second
        // gathered-SDP exchange rather than feeding unnegotiated sources.
        let reverse_offer = observations
            .completed(&world, &network, &peers, 1, peers[1].create_offer())
            .unwrap();
        observations.completed(
            &world,
            &network,
            &peers,
            1,
            peers[1].set_local_description(reverse_offer),
        );
        let reverse_offer = observations.gather(&world, &network, &peers, 1);
        observations.completed(
            &world,
            &network,
            &peers,
            0,
            peers[0].set_remote_description(reverse_offer),
        );
        let reverse_answer = observations
            .completed(&world, &network, &peers, 0, peers[0].create_answer())
            .unwrap();
        observations.completed(
            &world,
            &network,
            &peers,
            0,
            peers[0].set_local_description(reverse_answer),
        );
        let reverse_answer = observations.gather(&world, &network, &peers, 0);
        observations.completed(
            &world,
            &network,
            &peers,
            1,
            peers[1].set_remote_description(reverse_answer),
        );
        for _ in 0..2_000 {
            observations.step(&world, &network, &peers, false);
            if outgoing.state() == DataChannelState::Open
                && observations
                    .channels
                    .iter()
                    .any(|(_, channel)| channel.state() == DataChannelState::Open)
            {
                break;
            }
            world.advance(Duration::from_millis(1)).unwrap();
        }
        assert_eq!(
            outgoing.state(),
            DataChannelState::Open,
            "connection trace: {:?}",
            observations.trace.events
        );
        assert_eq!(
            outgoing.send(DataChannelMessage::text("forward")),
            DataChannelSendResult::Sent
        );
        let remote = &observations.channels[0].1;
        assert_eq!(
            remote.send(DataChannelMessage::text("reverse")),
            DataChannelSendResult::Sent
        );
        observations.channels.push((0, outgoing));
        let before_invalid = audio_decoders[0].decoder_statistics();
        assert_eq!(
            audio_sources[0].push_opus_at(
                &OpusInputFrame {
                    data: Vec::new(),
                    rtp_timestamp: 0,
                    samples_per_channel: 960
                },
                world.now()
            ),
            Err(OpusInputError::InvalidPacket)
        );
        assert_eq!(
            audio_sources[0].push_opus_at(
                &OpusInputFrame {
                    data: OPUS[0].to_vec(),
                    rtp_timestamp: 0,
                    samples_per_channel: 480
                },
                world.now()
            ),
            Err(OpusInputError::InvalidDuration)
        );
        assert_eq!(
            video_sources[0].push_encoded(vp8_frame(&world, &[0, 0, 0])),
            Err(CodecError::InvalidFrame)
        );
        assert_eq!(audio_decoders[0].decoder_statistics(), before_invalid);
        assert_eq!(world.pump(0).dispatched, 0);
        // Merely advancing the clock does not execute queued engine work.
        world.advance(Duration::from_millis(1)).unwrap();
        assert_eq!(audio_decoders[0].decoder_statistics(), before_invalid);

        // A third, unnegotiated sender exercises bounded Opus reservations and
        // drop with pending offer/media without consuming the observed pair's
        // media. No pump is needed to complete its destruction.
        {
            let abandoned = factories[0]
                .create_peer_connection(PeerConfiguration::default())
                .unwrap();
            let mut buffered_audio = factories[0].create_encoded_audio_source(1).unwrap();
            let buffered_track = factories[0]
                .create_encoded_audio_track("buffered-audio", &buffered_audio)
                .unwrap();
            let buffered_transceiver = abandoned
                .add_audio_transceiver(&buffered_track, RtpTransceiverDirection::SendOnly)
                .unwrap();
            for packet in 0..12 {
                buffered_audio
                    .push_opus_at(
                        &OpusInputFrame {
                            data: OPUS[0].to_vec(),
                            rtp_timestamp: packet * 960,
                            samples_per_channel: 960,
                        },
                        world.now(),
                    )
                    .unwrap();
            }
            assert_eq!(
                buffered_audio.push_opus_at(
                    &OpusInputFrame {
                        data: OPUS[0].to_vec(),
                        rtp_timestamp: 12 * 960,
                        samples_per_channel: 960
                    },
                    world.now()
                ),
                Err(OpusInputError::Backpressure)
            );
            abandoned.create_offer();
            buffered_audio.close().unwrap();
            drop(buffered_transceiver);
            drop(abandoned);
        }
        // Video overflow uses an isolated unsent source; counted eviction must
        // not affect either active producer sharing the same input broker.
        {
            let mut buffered_video = input.create_source(&factories[0]).unwrap();
            for offset in 0..18 {
                let mut frame = vp8_frame(&world, VP8[0]);
                frame.timestamp_us += offset;
                buffered_video.push_encoded(frame).unwrap();
            }
            assert_eq!(buffered_video.pending_frames(), 16);
            assert_eq!(buffered_video.dropped_frames(), 2);
            buffered_video.close().unwrap();
            assert_eq!(buffered_video.pending_frames(), 0);
        }
        for tick in 0..80 {
            for who in 0..2 {
                if tick % 2 == 0 {
                    let packet = OpusInputFrame {
                        data: OPUS[who].to_vec(),
                        rtp_timestamp: 480_000 + tick * 480,
                        samples_per_channel: 960,
                    };
                    audio_sources[who]
                        .push_opus_at(&packet, world.now())
                        .unwrap_or_else(|error| {
                            panic!(
                                "Opus push {who} tick {tick}: {error:?}; decoder={:?}; events={:?}",
                                audio_decoders[1 - who].decoder_statistics(),
                                observations.trace.events
                            )
                        });
                }
                if tick % 10 == 0 {
                    video_sources[who]
                        .push_encoded(EncodedVideoAccessUnit {
                            data: VP8[who].to_vec(),
                            width: 32,
                            height: 32,
                            timestamp_us: world.now().as_micros() as i64,
                            key_frame: true,
                            qp: None,
                            metadata: EncodedVideoMetadata {
                                codec: EncodedVideoCodec::Vp8 {
                                    non_reference: false,
                                    layer_sync: false,
                                    key_index: None,
                                },
                                temporal_index: None,
                                simulcast_index: None,
                                spatial_index: None,
                                end_of_picture: true,
                            },
                        })
                        .unwrap();
                }
            }
            observations.step(&world, &network, &peers, true);
            world.advance(Duration::from_millis(10)).unwrap();
        }
        let caller = DecoderStatistics::current_thread_token();
        for who in 0..2 {
            for statistics in [
                audio_decoders[who].decoder_statistics().unwrap(),
                video_decoders[who].decoder_statistics().unwrap(),
            ] {
                assert!(
                    statistics.decoder_creations > 0
                        && statistics.decode_calls > 0
                        && statistics.decoded_outputs > 0,
                    "no real decoding: {statistics:?}"
                );
                assert_eq!(
                    statistics.thread_token, caller,
                    "native decoder/output ran on another thread"
                );
                assert_eq!(statistics.thread_mismatches, 0);
            }
            // Native decoder inputs must equal the prerecorded compressed
            // bytes, independently proving that compression was bypassed.
            assert_eq!(
                audio_decoders[who]
                    .decoder_statistics()
                    .unwrap()
                    .last_input_hash,
                input_hash(OPUS[1 - who])
            );
            assert_eq!(
                video_decoders[who]
                    .decoder_statistics()
                    .unwrap()
                    .last_input_hash,
                input_hash(VP8[1 - who])
            );
            let recipient = peers[who].controlled_id();
            assert!(
                observations
                    .trace
                    .audio
                    .iter()
                    .any(|frame| frame.peer_id == recipient
                        && observations
                            .audio_ids
                            .contains(&(who, frame.receiver_id.clone()))
                        && frame
                            .frame
                            .samples
                            .iter()
                            .any(|sample| sample.unsigned_abs() > 100)),
                "missing recognizable PCM for peer {who}"
            );
            let blocks: Vec<_> = observations
                .trace
                .audio
                .iter()
                .filter(|frame| frame.peer_id == recipient)
                .collect();
            let samples: Vec<_> = blocks
                .iter()
                .flat_map(|block| {
                    block
                        .frame
                        .samples
                        .iter()
                        .step_by(usize::from(block.frame.channels))
                })
                .copied()
                .collect();
            let crossings = samples
                .windows(2)
                .filter(|pair| pair[0] <= 0 && pair[1] > 0)
                .count();
            let frequency =
                crossings as f64 * f64::from(blocks[0].frame.sample_rate_hz) / samples.len() as f64;
            let band = if who == 0 {
                700.0..1200.0
            } else {
                300.0..650.0
            };
            assert!(
                band.contains(&frequency),
                "unexpected received tone band for peer {who}: {frequency}"
            );
            assert!(
                observations.trace.video.iter().any(|frame| {
                    if frame.peer_id != recipient
                        || !observations
                            .video_ids
                            .contains(&(who, frame.track_id.clone()))
                    {
                        return false;
                    }
                    let pixels = frame.frame.buffer.as_bytes();
                    frame.frame.width == 32
                        && frame.frame.height == 32
                        && pixels.len() == 1536
                        && if who == 0 {
                            pixels[1024] > 200 && pixels[1280] < 150
                        } else {
                            pixels[1024] < 150 && pixels[1280] > 200
                        }
                }),
                "missing recognizable pixels for peer {who}"
            );
        }
        assert!(
            observations
                .trace
                .messages
                .iter()
                .any(|(who, message)| *who == 1
                    && message.kind == DataChannelMessageKind::Text
                    && message.bytes == b"forward")
        );
        assert!(
            observations
                .trace
                .messages
                .iter()
                .any(|(who, message)| *who == 0
                    && message.kind == DataChannelMessageKind::Text
                    && message.bytes == b"reverse")
        );
        // Replace only the first video producer on the negotiated sender.
        // Its receiver identity remains stable until subsequent renegotiation.
        let saved_video = observations.trace.video[0].clone();
        let saved_audio = observations.trace.audio[0].clone();
        let mut replacement = input.create_source(&factories[0]).unwrap();
        let replacement_track = replacement
            .create_track(&factories[0], "video-0-replacement")
            .unwrap();
        let sender = transceivers[0].sender();
        let sender_id = sender.id();
        sender.set_track(Some(&replacement_track)).unwrap();
        assert_eq!(sender.id(), sender_id);
        video_sources[0].close().unwrap();
        assert_eq!(
            video_sources[0].push_encoded(vp8_frame(&world, VP8[0])),
            Err(CodecError::Released)
        );
        sender.request_keyframe(&[]).unwrap();
        let replacement_begin = observations.trace.video.len();
        let audio_begin = observations.trace.audio.len();
        let mut keyframe_seen = false;
        for tick in 0..50 {
            if tick % 2 == 0 {
                for who in 0..2 {
                    audio_sources[who]
                        .push_opus_at(
                            &OpusInputFrame {
                                data: OPUS[who].to_vec(),
                                rtp_timestamp: (world.now().as_micros() * 48 / 1000) as u32,
                                samples_per_channel: 960,
                            },
                            world.now(),
                        )
                        .unwrap();
                }
            }
            if tick % 10 == 0 {
                replacement.push_encoded(vp8_frame(&world, VP8[1])).unwrap();
                video_sources[1]
                    .push_encoded(vp8_frame(&world, VP8[1]))
                    .unwrap();
            }
            observations.step(&world, &network, &peers, true);
            if replacement.take_keyframe_request() {
                keyframe_seen = true;
                observations.trace.events.push(format!(
                    "{}:keyframe:stream:{}",
                    world.now().as_micros(),
                    replacement.stream_id()
                ));
            }
            world.advance(Duration::from_millis(10)).unwrap();
        }
        assert!(
            keyframe_seen,
            "keyframe feedback missing on replacement source"
        );
        assert!(
            observations.trace.video[replacement_begin..]
                .iter()
                .any(|frame| frame.peer_id == peers[1].controlled_id()
                    && frame.frame.buffer.as_bytes()[1024] > 200)
        );
        for who in 0..2 {
            assert!(
                observations.trace.audio[audio_begin..]
                    .iter()
                    .any(|frame| frame.peer_id == peers[who].controlled_id()),
                "unrelated audio interrupted"
            );
        }
        assert!(
            observations.trace.video[replacement_begin..]
                .iter()
                .any(|frame| frame.peer_id == peers[0].controlled_id()),
            "unrelated video interrupted"
        );

        // Stop and negotiate away that receiver, then create a fresh receiver
        // for a new producer. Old owned output stays readable throughout.
        transceivers[0].stop().unwrap();
        replacement.close().unwrap();
        observations.negotiate(&world, &network, &peers, 0);
        let retired_at = world.now();
        let reborn = input.create_source(&factories[0]).unwrap();
        let reborn_track = reborn
            .create_track(&factories[0], "video-0-reborn")
            .unwrap();
        let reborn_transceiver = peers[0]
            .add_video_transceiver(&reborn_track, RtpTransceiverDirection::SendOnly)
            .unwrap();
        observations.negotiate(&world, &network, &peers, 0);
        let recreation_begin = observations.trace.video.len();
        for tick in 0..50 {
            if tick % 2 == 0 {
                for who in 0..2 {
                    audio_sources[who]
                        .push_opus_at(
                            &OpusInputFrame {
                                data: OPUS[who].to_vec(),
                                rtp_timestamp: (world.now().as_micros() * 48 / 1000) as u32,
                                samples_per_channel: 960,
                            },
                            world.now(),
                        )
                        .unwrap();
                }
            }
            if tick % 10 == 0 {
                reborn.push_encoded(vp8_frame(&world, VP8[0])).unwrap();
                video_sources[1]
                    .push_encoded(vp8_frame(&world, VP8[1]))
                    .unwrap();
            }
            observations.step(&world, &network, &peers, true);
            world.advance(Duration::from_millis(10)).unwrap();
        }
        assert!(
            observations.trace.video[recreation_begin..]
                .iter()
                .any(|frame| frame.peer_id == peers[1].controlled_id()
                    && frame.track_id == "video-0-reborn"
                    && frame.frame.buffer.as_bytes()[1280] > 200)
        );
        assert!(
            observations
                .trace
                .video
                .iter()
                .filter(|frame| frame.peer_id == peers[1].controlled_id()
                    && frame.track_id == "video-0")
                .all(|frame| frame.observed_at.unwrap() <= retired_at)
        );
        assert_eq!(saved_video, observations.trace.video[0]);
        assert_eq!(saved_audio, observations.trace.audio[0]);
        assert!(
            video_decoders[1]
                .decoder_statistics()
                .unwrap()
                .decoder_creations
                >= 2
        );
        assert!(!reborn_transceiver.stopped());

        // Close with accepted signaling, encoded input and network work still
        // queued. Every observed operation must terminate exactly once, without
        // decoding or emitting media after close.
        reborn.push_encoded(vp8_frame(&world, VP8[0])).unwrap();
        let pending = [
            (0, peers[0].create_offer()),
            (
                0,
                peers[0]
                    .set_local_description(peers[0].descriptions().unwrap().current_local.unwrap()),
            ),
            (1, peers[1].create_offer()),
        ];
        peers[0].close().unwrap();
        peers[0].close().unwrap();
        peers[1].close().unwrap();
        observations.retired = true;
        for (_, id, _) in observations.pending_packets.drain(..) {
            network.drop_packet(id).unwrap();
        }
        let before = [
            audio_decoders[0].decoder_statistics(),
            audio_decoders[1].decoder_statistics(),
            video_decoders[0].decoder_statistics(),
            video_decoders[1].decoder_statistics(),
        ];
        for _ in 0..8 {
            observations.step(&world, &network, &peers, false);
        }
        assert_eq!(
            before,
            [
                audio_decoders[0].decoder_statistics(),
                audio_decoders[1].decoder_statistics(),
                video_decoders[0].decoder_statistics(),
                video_decoders[1].decoder_statistics()
            ]
        );
        for (who, operation) in pending {
            assert_eq!(
                observations
                    .terminal_operations
                    .iter()
                    .filter(|entry| **entry == (who, operation))
                    .count(),
                1
            );
        }
        assert!(
            observations
                .audio
                .iter()
                .all(|(_, sink)| sink.try_next_received_frame().is_none())
        );
        assert!(
            observations
                .video
                .iter()
                .all(|(_, sink)| sink.try_next_received_frame().is_none())
        );
        observations.trace
    };
    // All native roots have gone away; only passive owned outputs survive.
    eprintln!("CONTROLLED_MEDIA_END");
    trace
}

#[test]
fn builtin_opus_vp8_links_have_real_owned_output_and_replay() {
    let _guard = CONTROLLED_TEST
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let mut scenarios = Vec::new();
    for addresses in [
        [
            Ipv4Addr::new(10, 77, 0, 1).into(),
            Ipv4Addr::new(10, 77, 0, 2).into(),
        ],
        [
            Ipv6Addr::new(0xfd00, 77, 0, 0, 0, 0, 0, 1).into(),
            Ipv6Addr::new(0xfd00, 77, 0, 0, 0, 0, 0, 2).into(),
        ],
    ] {
        for impaired in [false, true] {
            let first = run_media(addresses, impaired);
            let second = run_media(addresses, impaired);
            assert_eq!(
                first, second,
                "logical/media replay diverged: {addresses:?} impaired={impaired}"
            );
            if impaired {
                assert!(first.events.iter().any(|event| event.ends_with(":drop")));
                assert!(
                    first
                        .events
                        .iter()
                        .any(|event| event.ends_with(":duplicate"))
                );
            }
            scenarios.push((addresses, impaired, first));
        }
    }
    // Outside all controlled lifecycles, prove owned outputs are passive and
    // safely readable on another thread without any native root or dispatch.
    let passive = (
        scenarios[0].2.audio[0].clone(),
        scenarios[0].2.video[0].clone(),
    );
    let expected = passive.clone();
    let returned = std::thread::spawn(move || {
        assert!(!passive.0.frame.samples.is_empty());
        assert!(!passive.1.frame.buffer.as_bytes().is_empty());
        passive
    })
    .join()
    .unwrap();
    assert_eq!(returned, expected);
    if let Some(path) = std::env::var_os("PULSEBEAM_CONTROLLED_TRACE_PATH") {
        let traces: Vec<_> = scenarios.into_iter().map(|(addresses, impaired, first)| {
        let trace = serde_json::json!({
            "addresses": addresses.map(|ip| ip.to_string()), "impaired": impaired,
            "events": first.events,
            "messages": first.messages.iter().map(|(peer, message)| serde_json::json!({ "peer": peer, "kind": format!("{:?}", message.kind), "bytes": message.bytes })).collect::<Vec<_>>(),
            "audio": first.audio.iter().map(|frame| serde_json::json!({
                "peer": frame.peer_id, "receiver": frame.receiver_id,
                "observed_us": frame.observed_at.map(|time| time.as_micros()),
                "capture_us": frame.frame.capture_time_us,
                "rate": frame.frame.sample_rate_hz, "channels": frame.frame.channels,
                "samples_per_channel": frame.frame.samples_per_channel,
                "samples": frame.frame.samples,
            })).collect::<Vec<_>>(),
            "video": first.video.iter().map(|frame| serde_json::json!({
                "peer": frame.peer_id, "track": frame.track_id,
                "observed_us": frame.observed_at.map(|time| time.as_micros()),
                "capture_us": frame.frame.timestamp_us, "rtp_timestamp": frame.frame.rtp_timestamp,
                "width": frame.frame.width, "height": frame.frame.height,
                "rotation": frame.frame.rotation as u16,
                "i420": frame.frame.buffer.as_bytes(),
            })).collect::<Vec<_>>(),
        });
        trace
        }).collect();
        // Serialization/file I/O is outside every controlled lifecycle marker.
        std::fs::write(path, serde_json::to_vec(&traces).unwrap()).unwrap();
    }
}

#[test]
fn controlled_configuration_rejects_mismatched_endpoints_before_work() {
    let _guard = CONTROLLED_TEST
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    eprintln!("CONTROLLED_MEDIA_BEGIN");
    {
        let world = ControlledWorld::acquire(731, Duration::from_secs(10)).unwrap();
        let network = world.create_network().unwrap();
        let first = network
            .register_endpoint(Ipv4Addr::new(10, 78, 0, 1).into())
            .unwrap();
        let second = network
            .register_endpoint(Ipv4Addr::new(10, 78, 0, 2).into())
            .unwrap();
        let another_network = world.create_network().unwrap();
        let foreign = another_network
            .register_endpoint(Ipv4Addr::new(10, 78, 0, 1).into())
            .unwrap();
        for endpoint in [&second, &foreign] {
            let error = world
                .peer_factory_builder()
                .unwrap()
                .network_manager(first.network_manager().unwrap())
                .packet_socket_factory(endpoint.packet_socket_factory().unwrap())
                .build()
                .err()
                .expect("mismatched endpoint accepted");
            assert_eq!(error.kind, PeerErrorKind::InvalidParameter);
            assert_eq!(world.now(), Duration::from_secs(10));
            assert_eq!(world.next_deadline(), None);
        }
        let error = world
            .peer_factory_builder()
            .unwrap()
            .network_manager(first.network_manager().unwrap())
            .packet_socket_factory(first.packet_socket_factory().unwrap())
            .controlled_media()
            .build()
            .err()
            .expect("unselected codecs accepted");
        assert_eq!(error.kind, PeerErrorKind::InvalidParameter);
        assert_eq!(world.next_deadline(), None);
        let input = EncodedVideoInput::new_for_format(VideoCodecFormat::new("VP8")).unwrap();
        let audio_encoder = AudioEncoderFactory::with_opus_frames().unwrap();
        let audio_decoder = AudioDecoderFactory::builtin_opus().unwrap();
        let video_decoder = VideoDecoderFactoryHandle::builtin_vp8().unwrap();
        let builder = || {
            world
                .peer_factory_builder()
                .unwrap()
                .network_manager(first.network_manager().unwrap())
                .packet_socket_factory(first.packet_socket_factory().unwrap())
                .audio_encoder_factory(audio_encoder.clone())
                .audio_decoder_factory(audio_decoder.clone())
                .video_encoder_factory(input.encoder_factory())
                .video_decoder_factory(video_decoder.clone())
                .controlled_media()
        };
        #[cfg(feature = "native")]
        {
            let error = builder()
                .native_audio(true)
                .build()
                .err()
                .expect("physical devices accepted");
            assert_eq!(error.kind, PeerErrorKind::InvalidParameter);
            assert_eq!(world.next_deadline(), None);
        }
        for invalid in [
            builder().audio_encoder_factory(AudioEncoderFactory::builtin().unwrap()),
            builder().audio_decoder_factory(AudioDecoderFactory::builtin().unwrap()),
            builder().video_encoder_factory(EncodedVideoInput::new().unwrap().encoder_factory()),
            builder()
                .video_decoder_factory(VideoDecoderFactoryHandle::new(UnavailableVp8).unwrap()),
            builder().audio_processing(AudioProcessingConfig::default()),
        ] {
            let error = invalid
                .build()
                .err()
                .expect("incompatible controlled profile accepted");
            assert_eq!(error.kind, PeerErrorKind::InvalidParameter);
            assert_eq!(world.now(), Duration::from_secs(10));
            assert_eq!(
                world.next_deadline(),
                None,
                "rejected configuration started native work"
            );
        }
    }
    eprintln!("CONTROLLED_MEDIA_END");
}
