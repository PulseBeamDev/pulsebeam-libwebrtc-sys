use std::{
    collections::HashSet,
    sync::{Arc, Condvar, Mutex},
    task::{Context, Wake, Waker},
    thread,
    time::{Duration, Instant},
};

use pulsebeam_webrtc_sys::*;

#[derive(Default)]
struct Notification {
    ready: Mutex<bool>,
    changed: Condvar,
}
impl Wake for Notification {
    fn wake(self: Arc<Self>) {
        *self.ready.lock().unwrap() = true;
        self.changed.notify_one();
    }
}

fn wait(session: &mut ProductionSession) {
    let notification = Arc::new(Notification::default());
    let waker = Waker::from(notification.clone());
    if session
        .poll_ready(&mut Context::from_waker(&waker))
        .is_ready()
    {
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut notified = notification.ready.lock().unwrap();
    while !*notified {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "native readiness did not wake the actor"
        );
        notified = notification
            .changed
            .wait_timeout(notified, remaining)
            .unwrap()
            .0;
    }
}

fn finish(
    session: &mut ProductionSession,
    peer: SessionPeerId,
    operation: OperationId,
) -> Option<SessionDescription> {
    loop {
        while let Some(event) = session.try_peer_event(peer).unwrap() {
            if let SessionEvent::OperationComplete(done) = event
                && done.operation_id == operation
            {
                return done.result.unwrap();
            }
        }
        wait(session);
    }
}

#[test]
fn session_owns_live_media_graph_across_caller_migration_and_final_drop() {
    fn assert_send<T: Send>() {}
    assert_send::<ProductionSession>();
    fn assert_send_future(_: impl std::future::Future + Send) {}
    assert_send_future(async {
        let mut session = ProductionSession::new(ProductionSessionConfig::default()).unwrap();
        session.ready().await;
        session.shutdown().unwrap();
    });

    let h264 = VideoCodecFormat::new("H264")
        .with_parameter("level-asymmetry-allowed", "1")
        .with_parameter("packetization-mode", "1")
        .with_parameter("profile-level-id", "42e01f");
    let mut session = ProductionSession::new(ProductionSessionConfig {
        video_format: Some(h264),
    })
    .unwrap();
    let peer = session.create_peer(PeerConfiguration::default()).unwrap();
    let channel = session
        .create_channel(peer, "migrating", DataChannelConfiguration::default())
        .unwrap();
    let opus = session.create_opus_source(1).unwrap();
    session
        .publish_opus(peer, opus, "voice", RtpTransceiverDirection::SendOnly)
        .unwrap();
    let video = session.create_video_source().unwrap();
    let transceiver = session
        .publish_video(
            peer,
            video,
            "camera",
            RtpTransceiverDirection::SendOnly,
            &[],
        )
        .unwrap();
    let sender = session.sender(transceiver).unwrap();
    assert!(ControlledWorld::acquire(9, Duration::from_secs(1)).is_err());

    let (session, offer) = thread::spawn(move || {
        assert_eq!(
            session.sender_parameters(sender).unwrap().encodings.len(),
            1
        );
        let offer_id = session.create_offer(peer, false).unwrap();
        let offer = finish(&mut session, peer, offer_id).unwrap();
        assert!(offer.sdp.contains("H264/90000"));
        assert!(offer.sdp.contains("opus/48000"));
        (session, offer)
    })
    .join()
    .unwrap();

    let session = thread::spawn(move || {
        let mut session = session;
        let set_id = session.set_local_description(peer, offer).unwrap();
        finish(&mut session, peer, set_id);
        assert!(session.descriptions(peer).unwrap().pending_local.is_some());
        session.replace_video_source(sender, None, "").unwrap();
        session
            .replace_video_source(sender, Some(video), "replacement")
            .unwrap();
        session
            .push_video(
                video,
                EncodedVideoAccessUnit {
                    data: vec![
                        0, 0, 1, 0x67, 0x42, 0xc0, 0x1f, 0, 0, 1, 0x68, 1, 0, 0, 1, 0x65, 0x88,
                    ],
                    width: 16,
                    height: 16,
                    timestamp_us: 1_000_000,
                    key_frame: true,
                    qp: None,
                    metadata: EncodedVideoMetadata {
                        codec: EncodedVideoCodec::H264 {
                            base_layer_sync: false,
                        },
                        simulcast_index: None,
                        spatial_index: None,
                        temporal_index: None,
                        end_of_picture: true,
                    },
                },
            )
            .unwrap();
        assert_eq!(
            session
                .send(
                    channel,
                    DataChannelMessage {
                        kind: DataChannelMessageKind::Binary,
                        bytes: vec![1, 2, 3],
                    }
                )
                .unwrap(),
            DataChannelSendResult::NotOpen
        );
        session
    })
    .join()
    .unwrap();

    thread::spawn(move || {
        let mut session = session;
        session.close_channel(channel).unwrap();
        session.close_peer(peer).unwrap();
        session.close_peer(peer).unwrap();
        session.shutdown().unwrap();
        session.shutdown().unwrap();
        drop(session);
        let world = ControlledWorld::acquire(10, Duration::from_secs(1)).unwrap();
        assert_eq!(world.pump(0).dispatched, 0);
        drop(world);
    })
    .join()
    .unwrap();
}

#[test]
fn temporal_actor_keeps_native_extension_control_owned_across_migration() {
    const DD: &str =
        "https://aomediacodec.github.io/av1-rtp-spec/#dependency-descriptor-rtp-header-extension";
    const VLA: &str = "http://www.webrtc.org/experiments/rtp-hdrext/video-layers-allocation00";
    use pulsebeam_webrtc_sys::RtpHeaderExtensionDirection;
    assert!(
        ProductionSession::new_l1t3(ProductionSessionConfig::default(), [64, 128, 255]).is_err()
    );
    let h264 = VideoCodecFormat::new("H264")
        .with_parameter("level-asymmetry-allowed", "1")
        .with_parameter("packetization-mode", "1")
        .with_parameter("profile-level-id", "42e01f");
    let mut session = ProductionSession::new_l1t3(
        ProductionSessionConfig {
            video_format: Some(h264),
        },
        [64, 128, 255],
    )
    .unwrap();
    let peer = session.create_peer(PeerConfiguration::default()).unwrap();
    let source = session.create_video_source().unwrap();
    let transceiver = session
        .publish_video(
            peer,
            source,
            "temporal",
            RtpTransceiverDirection::SendOnly,
            &[],
        )
        .unwrap();
    let sender = session.sender(transceiver).unwrap();
    let defaults = session.header_extensions_to_negotiate(transceiver).unwrap();
    let (mut session, configured) = thread::spawn(move || {
        let mut configured = defaults.clone();
        for uri in [DD, VLA] {
            let extension = configured
                .iter_mut()
                .find(|extension| extension.uri() == uri)
                .unwrap();
            assert_eq!(extension.direction, RtpHeaderExtensionDirection::Stopped);
            extension.direction = RtpHeaderExtensionDirection::SendReceive;
        }
        session
            .set_header_extensions_to_negotiate(transceiver, &configured)
            .unwrap();
        assert_eq!(
            session.header_extensions_to_negotiate(transceiver).unwrap(),
            configured
        );
        assert!(
            session
                .negotiated_header_extensions(transceiver)
                .unwrap()
                .iter()
                .all(|extension| extension.direction == RtpHeaderExtensionDirection::Stopped)
        );
        let mut parameters = session.sender_parameters(sender).unwrap();
        parameters.encodings[0].scalability_mode = Some("L1T3".into());
        assert!(session.set_sender_parameters(sender, parameters).is_err());
        assert_eq!(session.take_video_encoder_error(source).unwrap(), None);
        assert_eq!(session.video_feedback(source).unwrap(), (false, None));
        let operation = session.create_offer(peer, false).unwrap();
        let offer = finish(&mut session, peer, operation).unwrap();
        assert!(offer.sdp.contains("H264/90000"));
        assert!(offer.sdp.contains(DD));
        assert!(offer.sdp.contains(VLA));
        (session, configured)
    })
    .join()
    .unwrap();
    thread::spawn(move || {
        assert_eq!(
            session.header_extensions_to_negotiate(transceiver).unwrap(),
            configured
        );
        session.close_peer(peer).unwrap();
        assert!(session.header_extensions_to_negotiate(transceiver).is_err());
        session.shutdown().unwrap();
        drop(session);
        // Snapshots are independently owned, not handles into the native graph.
        assert!(configured.iter().any(|extension| extension.uri() == DD));
    })
    .join()
    .unwrap();
}

#[test]
fn migrated_temporal_actor_receives_native_dd_and_wakes_for_rejection() {
    const DD: &str =
        "https://aomediacodec.github.io/av1-rtp-spec/#dependency-descriptor-rtp-header-extension";
    const KEY: &[u8] = &[
        0, 0, 0, 1, 0x67, 0x42, 0xc0, 0x0a, 0xd9, 0x1e, 0x84, 0, 0, 3, 0, 4, 0, 0, 3, 0, 0xf0,
        0x3c, 0x48, 0x99, 0x20, 0, 0, 0, 1, 0x68, 0xcb, 0x80, 0xc4, 0xb2, 0, 0, 0, 1, 0x65, 0x88,
        0x84, 0xf1, 0x18, 0xa0, 0, 0x20, 0x5b, 0x1c, 0, 4, 7, 0xe3, 0x80, 0, 0x80, 0xfe,
    ];
    fn outcome(
        session: &mut ProductionSession,
        peers: [SessionPeerId; 2],
        source: SessionSourceId,
        receiver: SessionReceiverId,
    ) -> Result<EncodedReceivedVideoFrame, CodecError> {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            for peer in peers {
                while session.try_peer_event(peer).unwrap().is_some() {}
            }
            if let Some(error) = session.take_video_encoder_error(source).unwrap() {
                return Err(error);
            }
            if let Some(frame) = session.try_video_frame(receiver).unwrap() {
                return Ok(frame);
            }
            assert!(
                Instant::now() < deadline,
                "temporal media or rejection did not arrive"
            );
            wait(session);
        }
    }

    for selected in [false, true] {
        let format = VideoCodecFormat::new("H264")
            .with_parameter("level-asymmetry-allowed", "1")
            .with_parameter("packetization-mode", "1")
            .with_parameter("profile-level-id", "42e01f");
        let mut session = ProductionSession::new_l1t3(
            ProductionSessionConfig {
                video_format: Some(format),
            },
            [64, 128, 255],
        )
        .unwrap();
        let peers = [
            session.create_peer(PeerConfiguration::default()).unwrap(),
            session.create_peer(PeerConfiguration::default()).unwrap(),
        ];
        let source = session.create_video_source().unwrap();
        let outgoing = session
            .publish_video(
                peers[0],
                source,
                "temporal",
                RtpTransceiverDirection::SendOnly,
                &[],
            )
            .unwrap();
        for transceiver in [outgoing] {
            let mut extensions = session.header_extensions_to_negotiate(transceiver).unwrap();
            extensions
                .iter_mut()
                .find(|extension| extension.uri() == DD)
                .unwrap()
                .direction = RtpHeaderExtensionDirection::SendReceive;
            session
                .set_header_extensions_to_negotiate(transceiver, &extensions)
                .unwrap();
        }
        let session = thread::spawn(move || {
            let operation = session.create_offer(peers[0], false).unwrap();
            let offer = finish(&mut session, peers[0], operation).unwrap();
            let offer = set_local_and_gather(&mut session, peers[0], offer);
            let operation = session.set_remote_description(peers[1], offer).unwrap();
            let mut completed = false;
            let mut arrival = None;
            while !completed || arrival.is_none() {
                while let Some(event) = session.try_peer_event(peers[1]).unwrap() {
                    match event {
                        SessionEvent::OperationComplete(done) if done.operation_id == operation => {
                            done.result.unwrap();
                            completed = true;
                        }
                        SessionEvent::Track {
                            transceiver,
                            receiver,
                        } => {
                            arrival = Some((transceiver, receiver));
                        }
                        _ => {}
                    }
                }
                if !completed || arrival.is_none() {
                    wait(&mut session);
                }
            }
            let (incoming, receiver) = arrival.unwrap();
            // Native JSEP creates the offered receiver instead of reusing a
            // locally addTransceiver-created resource. Configure the actual
            // owned arrival after the offer, before creating the answer.
            let mut extensions = session.header_extensions_to_negotiate(incoming).unwrap();
            extensions
                .iter_mut()
                .find(|extension| extension.uri() == DD)
                .unwrap()
                .direction = RtpHeaderExtensionDirection::SendReceive;
            session
                .set_header_extensions_to_negotiate(incoming, &extensions)
                .unwrap();
            let operation = session.create_answer(peers[1]).unwrap();
            let answer = finish(&mut session, peers[1], operation).unwrap();
            let answer = set_local_and_gather(&mut session, peers[1], answer);
            let operation = session.set_remote_description(peers[0], answer).unwrap();
            finish(&mut session, peers[0], operation);
            for transceiver in [outgoing, incoming] {
                assert_eq!(
                    session
                        .negotiated_header_extensions(transceiver)
                        .unwrap()
                        .iter()
                        .find(|extension| extension.uri() == DD)
                        .unwrap()
                        .direction,
                    RtpHeaderExtensionDirection::SendReceive
                );
            }
            if selected {
                let sender = session.sender(outgoing).unwrap();
                let mut parameters = session.sender_parameters(sender).unwrap();
                parameters.encodings[0].scalability_mode = Some("L1T3".into());
                session.set_sender_parameters(sender, parameters).unwrap();
            }
            assert_eq!(session.receiver(incoming).unwrap(), receiver);
            session.attach_encoded_video(receiver).unwrap();
            (session, receiver)
        })
        .join()
        .unwrap();
        let (session, receiver) = session;
        let (session, owned) = thread::spawn(move || {
            let mut session = session;
            let mut unit = EncodedVideoAccessUnit {
                data: KEY.to_vec(),
                width: 16,
                height: 16,
                timestamp_us: i64::try_from(SystemClock.now().as_micros()).unwrap(),
                key_frame: true,
                qp: None,
                metadata: EncodedVideoMetadata {
                    codec: EncodedVideoCodec::H264 {
                        base_layer_sync: false,
                    },
                    simulcast_index: None,
                    spatial_index: None,
                    temporal_index: Some(0),
                    end_of_picture: true,
                },
            };
            session.push_video(source, unit.clone()).unwrap();
            let first = outcome(&mut session, peers, source, receiver);
            if !selected {
                assert_eq!(first.unwrap_err(), CodecError::InvalidConfiguration);
                assert!(session.try_video_frame(receiver).unwrap().is_none());
                return (session, None);
            }
            let first = first.unwrap();
            assert_eq!(first.data, KEY);
            assert!(first.key_frame);
            assert_eq!(first.temporal_index, Some(0));
            assert!(first.dependencies.is_empty());
            let mut base_id = first.frame_id.unwrap();
            assert_eq!(
                first.decode_target_indications,
                vec![DecodeTargetIndication::Switch; 4]
            );
            assert!(
                session
                    .video_feedback(source)
                    .unwrap()
                    .1
                    .unwrap()
                    .layer_bitrates_bps[0][2]
                    .is_some()
            );
            let mut delivered = None;
            for _ in 0..4 {
                unit.timestamp_us += 33_333;
                unit.data = vec![0, 0, 0, 1, 0x41, 0x88, 0x84, 0xf1, 7];
                unit.key_frame = false;
                unit.metadata.temporal_index = Some(2);
                unit.metadata.codec = EncodedVideoCodec::H264 {
                    base_layer_sync: true,
                };
                session.push_video(source, unit.clone()).unwrap();
                match outcome(&mut session, peers, source, receiver) {
                    Ok(frame) => {
                        delivered = Some(frame);
                        break;
                    }
                    Err(error) => {
                        assert_eq!(error, CodecError::InvalidFrame);
                        assert!(session.video_feedback(source).unwrap().0);
                        let mut key = unit.clone();
                        key.timestamp_us += 33_333;
                        key.data = KEY.to_vec();
                        key.key_frame = true;
                        key.metadata.temporal_index = Some(0);
                        key.metadata.codec = EncodedVideoCodec::H264 {
                            base_layer_sync: false,
                        };
                        session.push_video(source, key.clone()).unwrap();
                        let frame = outcome(&mut session, peers, source, receiver).unwrap();
                        assert_eq!(frame.data, KEY);
                        base_id = frame.frame_id.unwrap();
                        session.video_feedback(source).unwrap();
                        unit.timestamp_us = key.timestamp_us;
                    }
                }
            }
            let delivered = delivered.expect("producer delta after native keyframe feedback");
            assert_eq!(delivered.data, unit.data);
            assert_eq!(delivered.temporal_index, Some(2));
            assert_eq!(delivered.ssrc, first.ssrc);
            assert!(delivered.frame_id.unwrap() > base_id);
            assert_eq!(delivered.dependencies, vec![base_id]);
            assert_eq!(
                delivered.decode_target_indications,
                vec![
                    DecodeTargetIndication::NotPresent,
                    DecodeTargetIndication::NotPresent,
                    DecodeTargetIndication::Switch,
                    DecodeTargetIndication::Switch
                ]
            );
            assert_eq!(session.dropped_frames(receiver).unwrap(), 0);
            (session, Some(delivered))
        })
        .join()
        .unwrap();
        thread::spawn(move || {
            let mut session = session;
            session.shutdown().unwrap();
            drop(session);
            if let Some(frame) = owned {
                assert_eq!(frame.data, vec![0, 0, 0, 1, 0x41, 0x88, 0x84, 0xf1, 7]);
            }
            let world = ControlledWorld::acquire(741, Duration::from_secs(1)).unwrap();
            assert_eq!(world.pump(0).dispatched, 0);
            drop(world);
        })
        .join()
        .unwrap();
    }
}

fn set_local_and_gather(
    session: &mut ProductionSession,
    peer: SessionPeerId,
    description: SessionDescription,
) -> SessionDescription {
    let operation = session.set_local_description(peer, description).unwrap();
    let mut complete = false;
    let mut gathered = false;
    while !complete || !gathered {
        while let Some(event) = session.try_peer_event(peer).unwrap() {
            match event {
                SessionEvent::OperationComplete(done) if done.operation_id == operation => {
                    done.result.unwrap();
                    complete = true;
                }
                SessionEvent::IceGatheringStateChanged(IceGatheringState::Complete) => {
                    gathered = true;
                }
                _ => {}
            }
        }
        if !complete || !gathered {
            wait(session);
        }
    }
    let descriptions = session.descriptions(peer).unwrap();
    descriptions
        .current_local
        .or(descriptions.pending_local)
        .unwrap()
}

#[test]
fn migrated_actor_is_woken_for_real_reliable_channel_delivery() {
    let mut session = ProductionSession::new(ProductionSessionConfig::default()).unwrap();
    let alice = session.create_peer(PeerConfiguration::default()).unwrap();
    let bob = session.create_peer(PeerConfiguration::default()).unwrap();
    let config = DataChannelConfiguration {
        negotiated: true,
        id: Some(0),
        ..DataChannelConfiguration::default()
    };
    let outgoing = session
        .create_channel(alice, "actor", config.clone())
        .unwrap();
    let incoming = session.create_channel(bob, "actor", config).unwrap();
    let session = thread::spawn(move || {
        let mut session = session;
        let operation = session.create_offer(alice, false).unwrap();
        let offer = finish(&mut session, alice, operation).unwrap();
        let offer = set_local_and_gather(&mut session, alice, offer);
        let operation = session.set_remote_description(bob, offer).unwrap();
        finish(&mut session, bob, operation);
        let operation = session.create_answer(bob).unwrap();
        let answer = finish(&mut session, bob, operation).unwrap();
        let answer = set_local_and_gather(&mut session, bob, answer);
        let operation = session.set_remote_description(alice, answer).unwrap();
        finish(&mut session, alice, operation);
        session
    })
    .join()
    .unwrap();
    thread::spawn(move || {
        let mut session = session;
        assert!(ProductionSession::channel_send_queue_capacity() > 0);
        assert_eq!(session.channel_error(outgoing).unwrap(), None);
        assert_eq!(session.channel_error(incoming).unwrap(), None);
        let mut opened = HashSet::new();
        while opened.len() != 2 {
            for channel in [outgoing, incoming] {
                while let Some(event) = session.try_channel_event(channel).unwrap() {
                    if let DataChannelEvent::StateChanged(DataChannelState::Open) = event {
                        opened.insert(channel);
                    }
                }
            }
            if opened.len() != 2 {
                wait(&mut session);
            }
        }
        for index in 0..64u32 {
            assert_eq!(
                session
                    .send(
                        outgoing,
                        DataChannelMessage {
                            kind: DataChannelMessageKind::Binary,
                            bytes: index.to_be_bytes().to_vec(),
                        }
                    )
                    .unwrap(),
                DataChannelSendResult::Sent
            );
        }
        let mut received = Vec::new();
        while received.len() < 64 {
            while let Some(event) = session.try_channel_event(incoming).unwrap() {
                if let DataChannelEvent::Message(message) = event {
                    received.push(message.bytes);
                }
            }
            if received.len() < 64 {
                wait(&mut session);
            }
        }
        assert_eq!(
            received,
            (0..64u32)
                .map(|n| n.to_be_bytes().to_vec())
                .collect::<Vec<_>>()
        );
        session.shutdown().unwrap();
        drop(session);
    })
    .join()
    .unwrap();
}

#[test]
fn accepted_operations_are_bounded_until_outcomes_are_consumed_and_survive_close() {
    let mut session = ProductionSession::new(ProductionSessionConfig::default()).unwrap();
    let peer = session.create_peer(PeerConfiguration::default()).unwrap();
    let mut accepted = HashSet::new();
    for _ in 0..64 {
        // Parse rejection still reserves and delivers exactly one terminal result.
        accepted.insert(
            session
                .add_ice_candidate(
                    peer,
                    IceCandidate {
                        sdp_mid: String::new(),
                        sdp_mline_index: 0,
                        candidate: "invalid".into(),
                    },
                )
                .unwrap(),
        );
    }
    assert_eq!(
        session.create_offer(peer, false).unwrap_err().kind,
        PeerErrorKind::ResourceExhausted
    );
    let session = thread::spawn(move || {
        let mut session = session;
        session.close_peer(peer).unwrap();
        let mut observed = HashSet::new();
        while let Some(event) = session.try_peer_event(peer).unwrap() {
            if let SessionEvent::OperationComplete(done) = event {
                assert!(done.result.is_err());
                assert!(
                    observed.insert(done.operation_id),
                    "duplicate terminal outcome"
                );
            }
        }
        assert_eq!(observed, accepted);
        assert_eq!(
            session.create_offer(peer, false).unwrap_err().kind,
            PeerErrorKind::Closed
        );
        session
    })
    .join()
    .unwrap();
    drop(session);
}

#[test]
fn foreign_ids_cannot_import_local_ownership_and_controlled_acquisition_is_exclusive() {
    let mut first = ProductionSession::new(ProductionSessionConfig::default()).unwrap();
    let peer = first.create_peer(PeerConfiguration::default()).unwrap();
    let mut second = ProductionSession::new(ProductionSessionConfig::default()).unwrap();
    assert_eq!(
        second.create_offer(peer, false).unwrap_err().kind,
        PeerErrorKind::InvalidParameter
    );
    drop(second);
    drop(first);
    let world = ControlledWorld::acquire(11, Duration::from_secs(1)).unwrap();
    assert_eq!(
        ProductionSession::new(ProductionSessionConfig::default())
            .err()
            .unwrap()
            .kind,
        PeerErrorKind::InvalidState
    );
    drop(world);
}
