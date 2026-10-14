use pulsebeam_webrtc_sys::*;
use std::{
    collections::{HashMap, HashSet},
    num::NonZeroUsize,
    sync::{Arc, Condvar, Mutex},
    task::{Context, Wake, Waker},
    thread,
    time::{Duration, Instant},
};

#[derive(Default)]
struct Notification(Mutex<bool>, Condvar);
impl Wake for Notification {
    fn wake(self: Arc<Self>) {
        *self.0.lock().unwrap() = true;
        self.1.notify_one();
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
    let mut notified = notification.0.lock().unwrap();
    while !*notified {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "native connectivity readiness did not wake the actor"
        );
        notified = notification.1.wait_timeout(notified, remaining).unwrap().0;
    }
}
#[derive(Default)]
struct Events {
    complete: HashMap<(SessionPeerId, OperationId), Option<SessionDescription>>,
    gathered: HashSet<SessionPeerId>,
}
impl Events {
    fn drain(&mut self, session: &mut ProductionSession, peers: [SessionPeerId; 2]) {
        for peer in peers {
            while let Some(event) = session.try_peer_event(peer).unwrap() {
                match event {
                    SessionEvent::OperationComplete(done) => {
                        assert!(
                            self.complete
                                .insert((peer, done.operation_id), done.result.unwrap())
                                .is_none()
                        );
                    }
                    SessionEvent::IceGatheringStateChanged(IceGatheringState::Complete) => {
                        self.gathered.insert(peer);
                    }
                    _ => {}
                }
            }
        }
    }
    fn finish(
        &mut self,
        session: &mut ProductionSession,
        peers: [SessionPeerId; 2],
        peer: SessionPeerId,
        id: OperationId,
    ) -> Option<SessionDescription> {
        loop {
            self.drain(session, peers);
            if let Some(result) = self.complete.remove(&(peer, id)) {
                return result;
            }
            wait(session);
        }
    }
    fn gather(
        &mut self,
        session: &mut ProductionSession,
        peers: [SessionPeerId; 2],
        peer: SessionPeerId,
        description: SessionDescription,
    ) -> SessionDescription {
        let id = session.set_local_description(peer, description).unwrap();
        self.finish(session, peers, peer, id);
        while !self.gathered.contains(&peer) {
            self.drain(session, peers);
            if !self.gathered.contains(&peer) {
                wait(session);
            }
        }
        session.descriptions(peer).unwrap().local().unwrap().clone()
    }
}

#[test]
fn migrated_actor_owns_dtmf_transport_aliases_and_revocable_ice() {
    fn assert_send<T: Send>() {}
    assert_send::<DtlsTransportInformation>();
    assert_send::<DtlsTransportEvent>();
    assert_send::<SessionSctpTransportInformation>();
    let mut session = ProductionSession::new(ProductionSessionConfig::default()).unwrap();
    let mut config = RtcConfiguration::default();
    config.options.bundle_policy = BundlePolicy::MaxBundle;
    let peers = [
        session
            .create_peer_with_configuration(config.clone())
            .unwrap(),
        session.create_peer_with_configuration(config).unwrap(),
    ];
    assert!(session.peer_sctp_transport(peers[0]).unwrap().is_none());
    assert!(
        session
            .peer_dtls_transport_by_mid(peers[0], b"absent")
            .unwrap()
            .is_none()
    );
    let source = session.create_opus_source(1).unwrap();
    let transceiver = session
        .publish_opus(
            peers[0],
            source,
            "tones",
            RtpTransceiverDirection::SendReceive,
        )
        .unwrap();
    let sender = session.sender(transceiver).unwrap();
    let receiver = session.receiver(transceiver).unwrap();
    assert!(session.sender_dtls_transport(sender).unwrap().is_none());
    let dtmf = session.sender_dtmf(sender).unwrap().unwrap();
    assert_eq!(session.sender_dtmf(sender).unwrap(), Some(dtmf));
    assert!(!session.dtmf_can_insert_tones(dtmf).unwrap());
    assert!(!session.insert_dtmf_tones(dtmf, "1", 100, 50).unwrap());
    assert_eq!(session.dtmf_duration_ms(dtmf).unwrap(), 100);
    assert_eq!(session.dtmf_inter_tone_gap_ms(dtmf).unwrap(), 50);
    assert_eq!(session.dtmf_comma_delay_ms(dtmf).unwrap(), 2000);
    assert!(matches!(
        session.insert_dtmf_tones(dtmf, "1", 40, i32::MAX),
        Err(SessionDtmfError::Input(DtmfInsertError::DelayOverflow))
    ));
    let channel_config = DataChannelConfiguration {
        negotiated: true,
        id: Some(0),
        ..Default::default()
    };
    let channels = [
        session
            .create_channel(peers[0], "transport", channel_config.clone())
            .unwrap(),
        session
            .create_channel(peers[1], "transport", channel_config)
            .unwrap(),
    ];
    let mut events = Events::default();
    let id = session.create_offer(peers[0], false).unwrap();
    let offer = events.finish(&mut session, peers, peers[0], id).unwrap();
    let offer = events.gather(&mut session, peers, peers[0], offer);
    assert!(offer.sdp.contains("telephone-event/48000"));
    let mid = offer
        .sdp
        .lines()
        .find_map(|line| line.strip_prefix("a=mid:"))
        .unwrap()
        .as_bytes()
        .to_vec();
    let sctp = session.peer_sctp_transport(peers[0]).unwrap().unwrap();
    assert_eq!(session.peer_sctp_transport(peers[0]).unwrap(), Some(sctp));
    let dtls = session.sctp_dtls_transport(sctp).unwrap().unwrap();
    // RTP endpoint transport association may still be absent before answer.
    if let Some(value) = session.sender_dtls_transport(sender).unwrap() {
        assert_eq!(value, dtls);
    }
    if let Some(value) = session.receiver_dtls_transport(receiver).unwrap() {
        assert_eq!(value, dtls);
    }
    assert_eq!(
        session.peer_dtls_transport_by_mid(peers[0], &mid).unwrap(),
        Some(dtls)
    );
    let ice = session.dtls_ice_transport(dtls).unwrap().unwrap();
    assert_eq!(session.dtls_ice_transport(dtls).unwrap(), Some(ice));
    assert_eq!(session.ice_component(ice).unwrap(), 1);
    session.register_dtls_observer(dtls, 1).unwrap();
    assert!(matches!(
        session.register_dtls_observer(dtls, 1),
        Err(SessionTransportError::Observer(
            TransportObserverError::AlreadyRegistered
        ))
    ));
    session.register_sctp_observer(sctp, 1).unwrap();
    assert!(matches!(
        session.register_sctp_observer(sctp, 0),
        Err(SessionTransportError::Observer(
            TransportObserverError::ZeroCapacity
        ))
    ));
    assert!(
        session
            .register_dtmf_observer(dtmf, NonZeroUsize::new(16).unwrap())
            .unwrap()
    );
    assert!(
        !session
            .register_dtmf_observer(dtmf, NonZeroUsize::new(16).unwrap())
            .unwrap()
    );

    let (session, info, connected_sctp) = thread::spawn(move || {
        let mut session = session;
        let id = session.set_remote_description(peers[1], offer).unwrap();
        events.finish(&mut session, peers, peers[1], id);
        let id = session.create_answer(peers[1]).unwrap();
        let answer = events.finish(&mut session, peers, peers[1], id).unwrap();
        assert!(answer.sdp.contains("telephone-event/48000"));
        let answer = events.gather(&mut session, peers, peers[1], answer);
        let id = session.set_remote_description(peers[0], answer).unwrap();
        events.finish(&mut session, peers, peers[0], id);
        let mut opened = HashSet::new();
        while opened.len() != channels.len() {
            events.drain(&mut session, peers);
            for channel in channels {
                while let Some(event) = session.try_channel_event(channel).unwrap() {
                    if event == DataChannelEvent::StateChanged(DataChannelState::Open) {
                        opened.insert(channel);
                    }
                }
            }
            if opened.len() != channels.len() {
                wait(&mut session);
            }
        }
        assert_eq!(session.sender_dtls_transport(sender).unwrap(), Some(dtls));
        assert_eq!(
            session.receiver_dtls_transport(receiver).unwrap(),
            Some(dtls)
        );
        let info = session.dtls_information(dtls).unwrap();
        assert_eq!(info.state(), DtlsTransportState::Connected);
        assert!(info.role().is_some());
        assert!(info.tls_version().is_some());
        assert!(info.ssl_cipher_suite().is_some());
        assert!(info.remote_ssl_certificates().unwrap().len() > 0);
        let connected_sctp = session.sctp_information(sctp).unwrap();
        assert_eq!(connected_sctp.state, SctpTransportState::Connected);
        assert_eq!(connected_sctp.dtls_transport, Some(dtls));
        assert!(connected_sctp.max_message_size.unwrap() > 0.0);
        assert!(connected_sctp.max_channels.unwrap() > 0);
        assert_eq!(
            session.ice_gathering_state(ice).unwrap(),
            IceTransportGatheringState::Complete
        );
        assert!(matches!(
            session.ice_role(ice).unwrap(),
            IceTransportRole::Controlling | IceTransportRole::Controlled
        ));
        assert!(matches!(
            session.ice_transport_state(ice).unwrap(),
            IceTransportState::Connected | IceTransportState::Completed
        ));
        let _native_optional_rtt = session.ice_rtt_estimate_ms(ice).unwrap();
        let _native_internal_state = session.ice_internal_state(ice).unwrap();
        assert!(session.dtmf_can_insert_tones(dtmf).unwrap());
        assert!(
            session
                .insert_dtmf_tones_with_comma_delay(dtmf, "a,x#", 40, 30, 30)
                .unwrap()
        );
        let mut tones = Vec::new();
        while tones.len() < 8 {
            while let Some(event) = session.take_dtmf_tone_change(dtmf).unwrap() {
                tones.push((event.tone, event.remaining_tones));
            }
            if tones.len() < 8 {
                wait(&mut session);
            }
        }
        assert_eq!(
            tones,
            vec![
                ("a".into(), Some(",x#".into())),
                ("a".into(), None),
                (",".into(), Some("x#".into())),
                (",".into(), None),
                ("#".into(), Some("".into())),
                ("#".into(), None),
                ("".into(), Some("".into())),
                ("".into(), None)
            ]
        );
        assert_eq!(session.dtmf_tones(dtmf).unwrap(), "");
        assert_eq!(session.dtmf_duration_ms(dtmf).unwrap(), 40);
        assert_eq!(session.dtmf_inter_tone_gap_ms(dtmf).unwrap(), 30);
        assert_eq!(session.dtmf_comma_delay_ms(dtmf).unwrap(), 30);
        assert_eq!(session.take_dropped_dtmf_tone_changes(dtmf).unwrap(), 0);
        session.unregister_dtmf_observer(dtmf).unwrap();
        assert!(
            session
                .register_dtmf_observer(dtmf, NonZeroUsize::new(1).unwrap())
                .unwrap()
        );
        assert!(session.insert_dtmf_tones(dtmf, "", 40, 30).unwrap());
        loop {
            let dropped = session.take_dropped_dtmf_tone_changes(dtmf).unwrap();
            if dropped != 0 {
                assert_eq!(dropped, 1);
                break;
            }
            wait(&mut session);
        }
        assert_eq!(session.dtmf_comma_delay_ms(dtmf).unwrap(), 2000);
        // Keep this native notification and transport transitions unconsumed
        // across migration and peer close.
        (session, info, connected_sctp)
    })
    .join()
    .unwrap();

    let (info, connected_sctp) = thread::spawn(move || {
        let mut session = session;
        let mut foreign = ProductionSession::new(ProductionSessionConfig::default()).unwrap();
        assert!(matches!(
            foreign.ice_component(ice),
            Err(SessionTransportError::Resource(PeerError {
                kind: PeerErrorKind::InvalidParameter,
                ..
            }))
        ));
        assert_eq!(
            foreign.dtls_information(dtls).err().unwrap().kind,
            PeerErrorKind::InvalidParameter
        );
        assert_eq!(
            foreign.sctp_information(sctp).unwrap_err().kind,
            PeerErrorKind::InvalidParameter
        );
        assert_eq!(
            foreign.dtmf_can_insert_tones(dtmf).unwrap_err().kind,
            PeerErrorKind::InvalidParameter
        );
        drop(foreign);
        session.close_peer(peers[0]).unwrap();
        assert_eq!(
            session.take_dtmf_tone_change(dtmf).unwrap().unwrap(),
            DtmfToneChange {
                tone: "".into(),
                remaining_tones: None
            }
        );
        assert!(!session.dtmf_can_insert_tones(dtmf).unwrap());
        assert!(!session.insert_dtmf_tones(dtmf, "1", 100, 50).unwrap());
        assert!(matches!(
            session.ice_component(ice),
            Err(SessionTransportError::InternalUnavailable(_))
        ));
        assert!(matches!(
            session.ice_internal_state(ice),
            Err(SessionTransportError::InternalUnavailable(_))
        ));
        assert_eq!(
            session.dtls_information(dtls).unwrap().state(),
            DtlsTransportState::Closed
        );
        let closed_sctp = session.sctp_information(sctp).unwrap();
        assert_eq!(closed_sctp.state, SctpTransportState::Closed);
        assert_eq!(closed_sctp.dtls_transport, None);
        // Pinned SCTP clears its DTLS reference but retains negotiated limits.
        assert_eq!(
            closed_sctp.max_message_size,
            connected_sctp.max_message_size
        );
        assert_eq!(closed_sctp.max_channels, connected_sctp.max_channels);
        assert_eq!(
            session.take_sctp_event(sctp).unwrap().unwrap().state,
            SctpTransportState::Closed
        );
        match session.take_dtls_event(dtls).unwrap().unwrap() {
            DtlsTransportEvent::StateChange(info) => {
                assert_eq!(info.state(), DtlsTransportState::Closed);
                assert!(info.remote_ssl_certificates().is_none());
                assert_eq!(info.role(), None);
            }
            DtlsTransportEvent::Error(error) => panic!("unexpected native DTLS failure: {error}"),
        }
        assert!(session.take_dtls_dropped_events(dtls).unwrap() > 0);
        assert!(session.take_sctp_dropped_events(sctp).unwrap() > 0);
        assert_eq!(session.take_dtls_dropped_events(dtls).unwrap(), 0);
        assert_eq!(session.take_sctp_dropped_events(sctp).unwrap(), 0);
        assert_eq!(session.dtls_ice_transport(dtls).unwrap(), Some(ice));
        session.unregister_dtls_observer(dtls).unwrap();
        session.unregister_sctp_observer(sctp).unwrap();
        session.release_dtmf_sender(dtmf).unwrap();
        session.release_ice_transport(ice).unwrap();
        session.release_sctp_transport(sctp).unwrap();
        session.release_dtls_transport(dtls).unwrap();
        assert!(matches!(
            session.ice_component(ice),
            Err(SessionTransportError::Resource(_))
        ));
        session.shutdown().unwrap();
        drop(session);
        (info, connected_sctp)
    })
    .join()
    .unwrap();
    // These are independent callback/snapshot/certificate values, not actor
    // handles. They remain usable after native role threads have been destroyed.
    assert_eq!(info.state(), DtlsTransportState::Connected);
    assert!(
        info.remote_certificate_data()
            .unwrap()
            .iter()
            .all(|certificate| !certificate.der.is_empty())
    );
    assert!(info.remote_ssl_certificates().unwrap().len() > 0);
    assert_eq!(connected_sctp.state, SctpTransportState::Connected);
    assert!(!connected_sctp.native_string().is_empty());
}
