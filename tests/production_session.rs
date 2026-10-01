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
                        codec: EncodedVideoCodec::H264,
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
