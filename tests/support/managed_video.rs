//! Shared high-resolution stock-software-video application and outer harness.
//! Codec preferences are explicit test inputs, not runtime admission rules.
use pulsebeam_webrtc_sys::*;
use serde::Serialize;
use std::{collections::HashMap, net::IpAddr, time::Duration};

struct Call<R: SessionRuntime> {
    sessions: [Session<R>; 2],
    peers: [SessionPeerId; 2],
    operations: HashMap<(usize, OperationId), Option<SessionDescription>>,
    gathered: [bool; 2],
    connected: [bool; 2],
    receiver: Option<SessionReceiverId>,
    events: Vec<String>,
}
impl<R: SessionRuntime> Call<R> {
    fn new(runtimes: [R; 2], resources: Option<ExecutionResources>) -> Self {
        let mut sessions = runtimes.map(|runtime| {
            let mut builder = Session::with_runtime(runtime, SessionConfig::default());
            if let Some(resources) = resources {
                builder = builder.execution_resources(resources);
            }
            builder.build().unwrap()
        });
        let peers = sessions
            .each_mut()
            .map(|session| session.create_peer(PeerConfiguration::default()).unwrap());
        Self {
            sessions,
            peers,
            operations: HashMap::new(),
            gathered: [false; 2],
            connected: [false; 2],
            receiver: None,
            events: Vec::new(),
        }
    }
    fn drain(&mut self) {
        for index in 0..2 {
            while let Some(event) = self.sessions[index]
                .try_peer_event(self.peers[index])
                .unwrap()
            {
                match event {
                    SessionEvent::OperationComplete(done) => {
                        assert!(
                            self.operations
                                .insert((index, done.operation_id), done.result.unwrap())
                                .is_none()
                        );
                    }
                    SessionEvent::IceGatheringStateChanged(state) => {
                        self.events.push(format!("{index}:gather:{state:?}"));
                        self.gathered[index] = state == IceGatheringState::Complete;
                    }
                    SessionEvent::ConnectionStateChanged(state) => {
                        self.events.push(format!("{index}:connection:{state:?}"));
                        self.connected[index] = state == ConnectionState::Connected;
                    }
                    SessionEvent::Track { receiver, .. } if index == 1 => {
                        assert_eq!(
                            self.sessions[index].receiver_media_type(receiver).unwrap(),
                            MediaType::Video
                        );
                        assert!(self.receiver.replace(receiver).is_none());
                        self.events.push("1:video-track".into());
                    }
                    _ => {}
                }
            }
        }
    }
    fn finish(
        &mut self,
        index: usize,
        operation: OperationId,
        progress: &mut impl FnMut(),
    ) -> Option<SessionDescription> {
        for _ in 0..10_000 {
            self.drain();
            if let Some(description) = self.operations.remove(&(index, operation)) {
                return description;
            }
            progress();
        }
        panic!("native operation stalled: peer {index}");
    }
    fn gather(
        &mut self,
        index: usize,
        description: SessionDescription,
        progress: &mut impl FnMut(),
    ) -> SessionDescription {
        let operation = self.sessions[index]
            .set_local_description(self.peers[index], description)
            .unwrap();
        self.finish(index, operation, progress);
        for _ in 0..10_000 {
            self.drain();
            if self.gathered[index] {
                let descriptions = self.sessions[index]
                    .descriptions(self.peers[index])
                    .unwrap();
                return descriptions
                    .current_local
                    .or(descriptions.pending_local)
                    .unwrap();
            }
            progress();
        }
        panic!("native gathering stalled: peer {index}");
    }
    fn negotiate(&mut self, codec_name: &str, progress: &mut impl FnMut()) {
        let operation = self.sessions[0].create_offer(self.peers[0], false).unwrap();
        let offer = self.finish(0, operation, progress).unwrap();
        assert!(offer.sdp.contains(&format!("{codec_name}/90000")));
        for other in ["VP8", "VP9", "H264", "AV1"] {
            if other != codec_name {
                assert!(!offer.sdp.contains(&format!("{other}/90000")));
            }
        }
        let offer = self.gather(0, offer, progress);
        let operation = self.sessions[1]
            .set_remote_description(self.peers[1], offer)
            .unwrap();
        self.finish(1, operation, progress);
        let operation = self.sessions[1].create_answer(self.peers[1]).unwrap();
        let answer = self.finish(1, operation, progress).unwrap();
        let answer = self.gather(1, answer, progress);
        let operation = self.sessions[0]
            .set_remote_description(self.peers[0], answer)
            .unwrap();
        self.finish(0, operation, progress);
        for _ in 0..10_000 {
            self.drain();
            if self.connected == [true; 2] && self.receiver.is_some() {
                return;
            }
            progress();
        }
        panic!("native peers did not connect: {:?}", self.events);
    }
}

#[derive(Debug, Eq, PartialEq, Serialize)]
struct FrameObservation {
    width: u32,
    height: u32,
    timestamp_us: i64,
    rtp_timestamp: u32,
    visible_i420: Vec<u8>,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
struct Observation {
    events: Vec<String>,
    frames: Vec<FrameObservation>,
    observed_at_us: u128,
    media_elapsed_us: u128,
}

fn application<R: SessionRuntime>(
    codec_name: &str,
    parameters: &[(&str, &str)],
    runtimes: [R; 2],
    resources: Option<ExecutionResources>,
    mut progress: impl FnMut(),
    mut now: impl FnMut() -> Duration,
) -> Observation {
    let mut call = Call::new(runtimes, resources);
    let source = call.sessions[0].create_raw_video_source().unwrap();
    let transceiver = call.sessions[0]
        .publish_video(
            call.peers[0],
            source,
            &format!("raw-{}-720p", codec_name.to_ascii_lowercase()),
            RtpTransceiverDirection::SendOnly,
            &[],
        )
        .unwrap();
    let codec = call.sessions[0]
        .get_rtp_sender_capabilities(MediaType::Video)
        .unwrap()
        .codecs()
        .into_iter()
        .find(|codec| {
            let base = codec.base_codec();
            base.name_bytes() == codec_name.as_bytes()
                && parameters.iter().all(|(key, value)| {
                    base.parameters().iter().any(|parameter| {
                        parameter.key == key.as_bytes() && parameter.value == value.as_bytes()
                    })
                })
        })
        .expect("stock sender capability must contain the requested codec/profile");
    call.sessions[0]
        .set_rtp_codec_preferences(transceiver, &[codec])
        .unwrap();
    call.sessions[0]
        .set_peer_bitrate(
            call.peers[0],
            Some(2_000_000),
            Some(3_000_000),
            Some(4_000_000),
        )
        .unwrap();
    call.negotiate(codec_name, &mut progress);
    let receiver = call.receiver.unwrap();
    call.sessions[1].attach_raw_video(receiver).unwrap();

    // Above VGA: host-derived cores can select native VPx worker pthreads.
    // Do not let a tiny source make an uncontrolled host default look safe.
    const WIDTH: u32 = 1280;
    const HEIGHT: u32 = 720;
    let y = (WIDTH * HEIGHT) as usize;
    let mut pixels = vec![128; y * 3 / 2];
    for row in 0..HEIGHT as usize {
        for col in 0..WIDTH as usize {
            pixels[row * WIDTH as usize + col] = if (row / 90 + col / 160) % 2 == 0 {
                40
            } else {
                180
            };
        }
    }
    let media_started_at = now();
    let mut next_capture = media_started_at;
    let mut submitted = 0;
    let mut received = Vec::new();
    for _ in 0..10_000 {
        let current = now();
        if submitted < 8 && current >= next_capture {
            let frame =
                VideoFrame::i420(WIDTH, HEIGHT, pixels.clone(), current.as_micros() as i64, 0)
                    .unwrap();
            call.sessions[0].push_raw_video(source, &frame).unwrap();
            submitted += 1;
            next_capture += Duration::from_millis(33);
        }
        progress();
        call.drain();
        if let Some(frame) = call.sessions[1].try_raw_video_frame(receiver).unwrap() {
            assert_eq!((frame.width, frame.height), (WIDTH, HEIGHT));
            let visible_i420 = frame.buffer.as_bytes().to_vec();
            assert_eq!(visible_i420.len(), y * 3 / 2);
            assert!(visible_i420[..y].iter().any(|&sample| sample < 80));
            assert!(visible_i420[..y].iter().any(|&sample| sample > 140));
            received.push(FrameObservation {
                width: frame.width,
                height: frame.height,
                timestamp_us: frame.timestamp_us,
                rtp_timestamp: frame.rtp_timestamp,
                visible_i420,
            });
            if received.len() == 2 {
                break;
            }
        }
    }
    assert_eq!(
        received.len(),
        2,
        "native sustained encode/transport/decode did not produce two distinct frames"
    );
    assert!(received[0].timestamp_us < received[1].timestamp_us);
    assert_ne!(received[0].rtp_timestamp, received[1].rtp_timestamp);
    let observed_at_us = now().as_micros();
    let media_elapsed_us = observed_at_us - media_started_at.as_micros();
    assert!(media_elapsed_us >= 33_000);
    for index in 0..2 {
        call.sessions[index].close_peer(call.peers[index]).unwrap();
        call.sessions[index].shutdown().unwrap();
    }
    Observation {
        events: call.events,
        frames: received,
        observed_at_us,
        media_elapsed_us,
    }
}

fn run(
    codec_name: &str,
    parameters: &[(&str, &str)],
    resources: Option<ExecutionResources>,
) -> Observation {
    eprintln!("CONTROLLED_MEDIA_BEGIN");
    let observation = {
        let world = ControlledWorld::acquire(0x7208, Duration::from_secs(7)).unwrap();
        let network = world.create_network().unwrap();
        let endpoints = ["192.0.2.1", "192.0.2.2"].map(|ip| {
            network
                .register_endpoint(ip.parse::<IpAddr>().unwrap())
                .unwrap()
        });
        let runtimes = endpoints
            .each_ref()
            .map(|endpoint| world.runtime().unwrap().with_endpoint(endpoint).unwrap());
        application(
            codec_name,
            parameters,
            runtimes,
            resources,
            || {
                assert!(!world.pump(256).blocked);
                while let Some(packet) = network.next_packet() {
                    network.deliver(packet.id).unwrap();
                }
                // The outer caller explicitly chooses every advance and delivery.
                world.advance(Duration::from_millis(1)).unwrap();
            },
            || world.now(),
        )
    };
    eprintln!("CONTROLLED_MEDIA_END");
    observation
}

pub fn check_replay(
    codec_name: &str,
    parameters: &[(&str, &str)],
    host_probe_variable: &str,
    trace_path_variable: &str,
) {
    // Optional diagnostic negative control deliberately selects native host
    // resources, whose worker behavior is NOT qualified by this default policy.
    let observation = if std::env::var_os(host_probe_variable).is_some() {
        run(codec_name, parameters, Some(ExecutionResources::default()))
    } else {
        let first = run(codec_name, parameters, None);
        let second = run(codec_name, parameters, None);
        assert!(
            first == second,
            "native {codec_name} replay differs: events_equal={}, frames_equal={}, capture_us={:?}, observed_us={:?}, rtp={:?}",
            first.events == second.events,
            first.frames == second.frames,
            [
                first
                    .frames
                    .iter()
                    .map(|frame| frame.timestamp_us)
                    .collect::<Vec<_>>(),
                second
                    .frames
                    .iter()
                    .map(|frame| frame.timestamp_us)
                    .collect::<Vec<_>>(),
            ],
            [
                (first.observed_at_us, first.media_elapsed_us),
                (second.observed_at_us, second.media_elapsed_us),
            ],
            [
                first
                    .frames
                    .iter()
                    .map(|frame| frame.rtp_timestamp)
                    .collect::<Vec<_>>(),
                second
                    .frames
                    .iter()
                    .map(|frame| frame.rtp_timestamp)
                    .collect::<Vec<_>>(),
            ],
        );
        first
    };
    if let Some(path) = std::env::var_os(trace_path_variable) {
        // File I/O is outside native lifecycle markers, never native progress.
        std::fs::write(path, serde_json::to_vec(&observation).unwrap()).unwrap();
    }
}
