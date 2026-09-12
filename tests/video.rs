use std::{
    net::Ipv4Addr,
    sync::{
        Arc,
        atomic::{AtomicU32, AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use pulsebeam_webrtc_sys::{
    CodecError, CodecSupport, DecodedImageCallback, EncodedImageCallback, EncodedVideoFrame,
    Environment, IceCandidate, ManualClock, OperationId, PeerConfiguration, PeerConnection,
    PeerConnectionEvent, PeerConnectionFactory, RtpTransceiver, RtpTransceiverDirection,
    SessionDescription, SimulatedNetwork, VideoCodecFormat, VideoDecoder, VideoDecoderFactory,
    VideoDecoderFactoryHandle, VideoDecoderInfo, VideoDecoderSettings, VideoEncoder,
    VideoEncoderFactory, VideoEncoderFactoryHandle, VideoEncoderInfo, VideoEncoderSettings,
    VideoFrame, VideoFrameType, VideoRateControl, VideoResolution,
};

#[derive(Default)]
struct Counters {
    encoder_factory: AtomicUsize,
    encoder_create: AtomicUsize,
    encode: AtomicUsize,
    encoder_release: AtomicUsize,
    decoder_factory: AtomicUsize,
    decoder_create: AtomicUsize,
    decode: AtomicUsize,
    decoder_release: AtomicUsize,
    decoder_rtp_timestamp: AtomicU32,
}

fn h264() -> VideoCodecFormat {
    VideoCodecFormat::new("H264")
        .with_parameter("level-asymmetry-allowed", "1")
        .with_parameter("packetization-mode", "1")
        .with_parameter("profile-level-id", "42e01f")
}

struct EncoderFactory {
    counters: Arc<Counters>,
    fail: bool,
}

impl VideoEncoderFactory for EncoderFactory {
    fn supported_formats(&self) -> Vec<VideoCodecFormat> {
        self.counters.encoder_factory.fetch_add(1, Ordering::SeqCst);
        vec![h264()]
    }

    fn query_support(
        &self,
        format: &VideoCodecFormat,
        _: Option<&str>,
        _: Option<VideoResolution>,
    ) -> CodecSupport {
        CodecSupport {
            supported: format.name.eq_ignore_ascii_case("H264"),
            power_efficient: false,
        }
    }

    fn create(&self, format: &VideoCodecFormat) -> Result<Box<dyn VideoEncoder>, CodecError> {
        self.counters.encoder_create.fetch_add(1, Ordering::SeqCst);
        if self.fail || !format.name.eq_ignore_ascii_case("H264") {
            Err(CodecError::ConstructionFailed)
        } else {
            Ok(Box::new(TestEncoder(self.counters.clone())))
        }
    }
}

struct TestEncoder(Arc<Counters>);

impl VideoEncoder for TestEncoder {
    fn initialize(&mut self, _: VideoEncoderSettings) -> Result<(), CodecError> {
        Ok(())
    }

    fn encode(
        &mut self,
        frame: VideoFrame,
        _: &[VideoFrameType],
        callback: EncodedImageCallback,
    ) -> Result<(), CodecError> {
        self.0.encode.fetch_add(1, Ordering::SeqCst);
        callback.emit(&EncodedVideoFrame {
            // A minimal Annex-B SPS/PPS/IDR sample. The injected decoder owns
            // interpretation; the native H.264 RTP path needs valid framing.
            data: vec![
                0, 0, 0, 1, 0x67, 0x42, 0xc0, 0x0a, 0xd9, 0x1e, 0x84, 0, 0, 3, 0, 4, 0, 0, 3, 0,
                0xf0, 0x3c, 0x48, 0x99, 0x20, 0, 0, 0, 1, 0x68, 0xcb, 0x80, 0xc4, 0xb2, 0, 0, 1,
                0x65, 0x88, 0x84, 0xf1, 0x18, 0xa0, 0, 0x20, 0x5b, 0x1c, 0, 4, 7, 0xe3, 0x80, 0,
                0x80, 0xfe,
            ],
            width: frame.width,
            height: frame.height,
            rtp_timestamp: frame.rtp_timestamp,
            frame_type: VideoFrameType::Key,
            qp: Some(20),
        })
    }

    fn set_rates(&mut self, _: VideoRateControl) -> Result<(), CodecError> {
        Ok(())
    }

    fn release(&mut self) -> Result<(), CodecError> {
        self.0.encoder_release.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn info(&self) -> VideoEncoderInfo {
        VideoEncoderInfo {
            implementation_name: "test-only H264-shaped encoder".into(),
            hardware_accelerated: false,
            supports_native_handle: false,
        }
    }
}

struct DecoderFactory(Arc<Counters>);

impl VideoDecoderFactory for DecoderFactory {
    fn supported_formats(&self) -> Vec<VideoCodecFormat> {
        self.0.decoder_factory.fetch_add(1, Ordering::SeqCst);
        vec![h264()]
    }

    fn query_support(
        &self,
        format: &VideoCodecFormat,
        _: bool,
        _: Option<VideoResolution>,
    ) -> CodecSupport {
        CodecSupport {
            supported: format.name.eq_ignore_ascii_case("H264"),
            power_efficient: false,
        }
    }

    fn create(&self, format: &VideoCodecFormat) -> Result<Box<dyn VideoDecoder>, CodecError> {
        self.0.decoder_create.fetch_add(1, Ordering::SeqCst);
        if format.name.eq_ignore_ascii_case("H264") {
            Ok(Box::new(TestDecoder(self.0.clone())))
        } else {
            Err(CodecError::UnsupportedFormat)
        }
    }
}

struct TestDecoder(Arc<Counters>);

impl VideoDecoder for TestDecoder {
    fn configure(&mut self, _: VideoDecoderSettings) -> Result<(), CodecError> {
        Ok(())
    }

    fn decode(
        &mut self,
        frame: EncodedVideoFrame,
        callback: DecodedImageCallback,
    ) -> Result<(), CodecError> {
        self.0.decode.fetch_add(1, Ordering::SeqCst);
        self.0
            .decoder_rtp_timestamp
            .store(frame.rtp_timestamp, Ordering::SeqCst);
        callback.emit(&VideoFrame::i420(
            frame.width,
            frame.height,
            synthetic_i420(frame.width, frame.height),
            i64::from(frame.rtp_timestamp) * 1000,
            frame.rtp_timestamp,
        )?)
    }

    fn release(&mut self) -> Result<(), CodecError> {
        self.0.decoder_release.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn info(&self) -> VideoDecoderInfo {
        VideoDecoderInfo {
            implementation_name: "test-only H264-shaped decoder".into(),
            hardware_accelerated: false,
        }
    }
}

struct Pair {
    clock: ManualClock,
    network: SimulatedNetwork,
    alice_factory: PeerConnectionFactory,
    alice: PeerConnection,
    bob: PeerConnection,
    _alice_endpoint: pulsebeam_webrtc_sys::NetworkEndpoint,
    _bob_endpoint: pulsebeam_webrtc_sys::NetworkEndpoint,
}

impl Pair {
    fn new(counters: Arc<Counters>, fail_encoder: bool) -> Self {
        let clock = ManualClock::new(Duration::from_secs(1)).unwrap();
        let environment = Environment::builder().clock(&clock).build().unwrap();
        let network = SimulatedNetwork::new(&clock).unwrap();
        let alice_endpoint = network
            .register_endpoint(Ipv4Addr::new(10, 22, 0, 1).into())
            .unwrap();
        let bob_endpoint = network
            .register_endpoint(Ipv4Addr::new(10, 22, 0, 2).into())
            .unwrap();
        let alice_factory = factory(
            environment.clone(),
            &alice_endpoint,
            counters.clone(),
            fail_encoder,
        );
        let bob_factory = factory(environment, &bob_endpoint, counters, false);
        let alice = alice_factory
            .create_peer_connection(PeerConfiguration::default())
            .unwrap();
        let bob = bob_factory
            .create_peer_connection(PeerConfiguration::default())
            .unwrap();
        Self {
            clock,
            network,
            alice_factory,
            alice,
            bob,
            _alice_endpoint: alice_endpoint,
            _bob_endpoint: bob_endpoint,
        }
    }

    fn progress(&self) -> (Vec<PeerConnectionEvent>, Vec<PeerConnectionEvent>) {
        let mut alice_events = drain(&self.alice);
        let mut bob_events = drain(&self.bob);
        for candidate in take_candidates(&mut alice_events) {
            self.bob.add_ice_candidate(candidate);
        }
        for candidate in take_candidates(&mut bob_events) {
            self.alice.add_ice_candidate(candidate);
        }
        while let Some(packet) = self.network.next_packet() {
            self.network.deliver(packet.id).unwrap();
        }
        self.clock.advance(Duration::from_millis(1)).unwrap();
        thread::yield_now();
        (alice_events, bob_events)
    }

    fn negotiate(&self) -> Vec<PeerConnectionEvent> {
        let mut alice_events = Vec::new();
        let mut bob_events = Vec::new();
        let offer =
            completion_description(&self.alice, self.alice.create_offer(), &mut alice_events);
        completion(
            &self.alice,
            self.alice.set_local_description(offer.clone()),
            &mut alice_events,
        );
        completion(
            &self.bob,
            self.bob.set_remote_description(offer),
            &mut bob_events,
        );
        let answer = completion_description(&self.bob, self.bob.create_answer(), &mut bob_events);
        completion(
            &self.bob,
            self.bob.set_local_description(answer.clone()),
            &mut bob_events,
        );
        completion(
            &self.alice,
            self.alice.set_remote_description(answer),
            &mut alice_events,
        );
        for candidate in take_candidates(&mut alice_events) {
            self.bob.add_ice_candidate(candidate);
        }
        for candidate in take_candidates(&mut bob_events) {
            self.alice.add_ice_candidate(candidate);
        }
        bob_events
    }
}

fn factory(
    environment: Environment,
    endpoint: &pulsebeam_webrtc_sys::NetworkEndpoint,
    counters: Arc<Counters>,
    fail_encoder: bool,
) -> PeerConnectionFactory {
    PeerConnectionFactory::builder()
        .environment(environment)
        .network_manager(endpoint.network_manager().unwrap())
        .packet_socket_factory(endpoint.packet_socket_factory().unwrap())
        .video_encoder_factory(
            VideoEncoderFactoryHandle::new(EncoderFactory {
                counters: counters.clone(),
                fail: fail_encoder,
            })
            .unwrap(),
        )
        .video_decoder_factory(VideoDecoderFactoryHandle::new(DecoderFactory(counters)).unwrap())
        .build()
        .unwrap()
}

fn synthetic_i420(width: u32, height: u32) -> Vec<u8> {
    let len = (width * height + 2 * width.div_ceil(2) * height.div_ceil(2)) as usize;
    (0..len).map(|index| index as u8).collect()
}

fn drain(peer: &PeerConnection) -> Vec<PeerConnectionEvent> {
    std::iter::from_fn(|| peer.try_next_event()).collect()
}

fn take_candidates(events: &mut Vec<PeerConnectionEvent>) -> Vec<IceCandidate> {
    let mut candidates = Vec::new();
    events.retain_mut(|event| {
        if let PeerConnectionEvent::IceCandidate(_) = event {
            let PeerConnectionEvent::IceCandidate(candidate) =
                std::mem::replace(event, PeerConnectionEvent::Closed)
            else {
                unreachable!()
            };
            candidates.push(candidate);
            false
        } else {
            true
        }
    });
    candidates
}

fn completion(
    peer: &PeerConnection,
    id: OperationId,
    side_events: &mut Vec<PeerConnectionEvent>,
) -> Option<SessionDescription> {
    for _ in 0..1_000_000 {
        while let Some(event) = peer.try_next_event() {
            match event {
                PeerConnectionEvent::OperationComplete(done) if done.operation_id == id => {
                    return done.result.unwrap();
                }
                event => side_events.push(event),
            }
        }
        thread::yield_now();
    }
    panic!("operation {} did not complete", id.get());
}

fn completion_description(
    peer: &PeerConnection,
    id: OperationId,
    side_events: &mut Vec<PeerConnectionEvent>,
) -> SessionDescription {
    completion(peer, id, side_events).unwrap()
}

fn take_remote_transceiver(events: &mut Vec<PeerConnectionEvent>) -> Option<RtpTransceiver> {
    events
        .iter()
        .position(|event| matches!(event, PeerConnectionEvent::Track(_)))
        .map(|index| match events.swap_remove(index) {
            PeerConnectionEvent::Track(transceiver) => transceiver,
            _ => unreachable!(),
        })
}

#[test]
fn injected_h264_provider_carries_a_frame_between_peers() {
    let counters = Arc::new(Counters::default());
    let pair = Pair::new(counters.clone(), false);
    let source = pair.alice_factory.create_video_source().unwrap();
    let track = pair
        .alice_factory
        .create_video_track("synthetic", &source)
        .unwrap();
    let local_sink = track.attach_sink().unwrap();
    let transceiver = pair
        .alice
        .add_video_transceiver(&track, RtpTransceiverDirection::SendOnly)
        .unwrap();
    assert_eq!(transceiver.direction(), RtpTransceiverDirection::SendOnly);
    assert_eq!(transceiver.sender().track().unwrap().id(), "synthetic");

    let mut bob_events = pair.negotiate();
    let remote = (0..2_000_000)
        .find_map(|_| {
            if let Some(remote) = take_remote_transceiver(&mut bob_events) {
                return Some(remote);
            }
            bob_events.extend(pair.progress().1);
            None
        })
        .expect("remote video transceiver did not arrive");
    assert_eq!(
        remote.current_direction(),
        Some(RtpTransceiverDirection::ReceiveOnly)
    );
    let receiver = remote.receiver();
    let remote_track = receiver.track().unwrap();
    let sink = remote_track.attach_sink().unwrap();

    for _ in 0..100_000 {
        pair.progress();
        if transceiver.current_direction() == Some(RtpTransceiverDirection::SendOnly) {
            break;
        }
    }
    let frame = VideoFrame::i420(16, 16, synthetic_i420(16, 16), 2_000_000, 90_000).unwrap();
    source.push_frame(&frame).unwrap();
    assert_eq!(
        local_sink
            .try_next_frame()
            .expect("caller-fed source did not preserve the local frame"),
        frame
    );
    let received = (0..2_000_000)
        .find_map(|_| {
            pair.progress();
            sink.try_next_frame()
        })
        .unwrap_or_else(|| {
            panic!(
                "video frame did not arrive: encoders={} encoded={} decoders={} decoded={}",
                counters.encoder_create.load(Ordering::SeqCst),
                counters.encode.load(Ordering::SeqCst),
                counters.decoder_create.load(Ordering::SeqCst),
                counters.decode.load(Ordering::SeqCst),
            )
        });
    assert_eq!((received.width, received.height), (16, 16));
    assert_ne!(received.rtp_timestamp, 0);
    assert_eq!(
        counters.decoder_rtp_timestamp.load(Ordering::SeqCst),
        received.rtp_timestamp
    );
    assert_eq!(received.buffer.as_bytes(), synthetic_i420(16, 16));
    assert!(counters.encoder_factory.load(Ordering::SeqCst) > 0);
    assert!(counters.decoder_factory.load(Ordering::SeqCst) > 0);
    assert_eq!(counters.encoder_create.load(Ordering::SeqCst), 1);
    assert_eq!(counters.decoder_create.load(Ordering::SeqCst), 1);
    assert_eq!(counters.encode.load(Ordering::SeqCst), 1);
    assert_eq!(counters.decode.load(Ordering::SeqCst), 1);
    drop(sink);
    drop(remote_track);
    drop(receiver);
    drop(remote);
    drop(local_sink);
    drop(transceiver);
    drop(track);
    drop(source);
    drop(pair);
    assert_eq!(counters.encoder_release.load(Ordering::SeqCst), 1);
    assert_eq!(counters.decoder_release.load(Ordering::SeqCst), 1);
}

#[test]
fn media_removal_failure_and_repeated_teardown_are_safe() {
    for fail_encoder in [false, true] {
        let counters = Arc::new(Counters::default());
        let mut pair = Pair::new(counters.clone(), fail_encoder);
        let mut source = pair.alice_factory.create_video_source().unwrap();
        let track = pair
            .alice_factory
            .create_video_track("teardown", &source)
            .unwrap();
        let transceiver = pair
            .alice
            .add_video_transceiver(&track, RtpTransceiverDirection::SendReceive)
            .unwrap();
        transceiver
            .set_direction(RtpTransceiverDirection::SendOnly)
            .unwrap();
        assert_eq!(transceiver.direction(), RtpTransceiverDirection::SendOnly);
        let sender = transceiver.sender();
        let mut events = pair.negotiate();
        let remote = (0..2_000_000)
            .find_map(|_| {
                if let Some(remote) = take_remote_transceiver(&mut events) {
                    return Some(remote);
                }
                events.extend(pair.progress().1);
                None
            })
            .expect("remote teardown transceiver did not arrive");
        let mut sink = remote.receiver().track().unwrap().attach_sink().unwrap();
        source
            .push_frame(&VideoFrame::i420(4, 4, synthetic_i420(4, 4), 3_000_000, 180_000).unwrap())
            .unwrap();
        for _ in 0..50_000 {
            pair.progress();
            if counters.encoder_create.load(Ordering::SeqCst) > 0 {
                break;
            }
        }
        assert!(counters.encoder_create.load(Ordering::SeqCst) > 0);
        if fail_encoder {
            assert_eq!(counters.encode.load(Ordering::SeqCst), 0);
            assert!(sink.try_next_frame().is_none());
        }
        pair.alice.remove_track(&sender).unwrap();
        sink.close().unwrap();
        source.close().unwrap();
        assert_eq!(
            source.push_frame(&VideoFrame::i420(2, 2, vec![0; 6], 0, 0).unwrap()),
            Err(CodecError::Released)
        );
        pair.alice.close().unwrap();
        pair.bob.close().unwrap();
    }

    for index in 0..16 {
        let counters = Arc::new(Counters::default());
        let mut pair = Pair::new(counters, false);
        let mut source = pair.alice_factory.create_video_source().unwrap();
        let track = pair
            .alice_factory
            .create_video_track(&format!("repeat-{index}"), &source)
            .unwrap();
        let mut sink = track.attach_sink().unwrap();
        let frame = VideoFrame::i420(2, 2, vec![index as u8; 6], index as i64, index).unwrap();
        source.push_frame(&frame).unwrap();
        assert_eq!(sink.try_next_frame(), Some(frame));
        if index % 2 == 0 {
            sink.close().unwrap();
            source.close().unwrap();
            pair.alice.close().unwrap();
        } else {
            pair.alice.close().unwrap();
            source.close().unwrap();
            sink.close().unwrap();
        }
    }

    let counters = Arc::new(Counters::default());
    let mut pair = Pair::new(counters, false);
    let source = pair.alice_factory.create_video_source().unwrap();
    let track = pair
        .alice_factory
        .create_video_track("in-flight", &source)
        .unwrap();
    pair.alice
        .add_video_transceiver(&track, RtpTransceiverDirection::SendOnly)
        .unwrap();
    let _ = pair.negotiate();
    source
        .push_frame(&VideoFrame::i420(4, 4, synthetic_i420(4, 4), 0, 0).unwrap())
        .unwrap();
    pair.alice.close().unwrap();
}
