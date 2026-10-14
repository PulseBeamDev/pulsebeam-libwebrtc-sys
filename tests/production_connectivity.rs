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
    channels: Vec<(SessionPeerId, SessionChannelId, Vec<u8>)>,
    stats: HashMap<(SessionPeerId, OperationId), PeerStatsSnapshot>,
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
                    SessionEvent::Stats(value) => {
                        assert!(
                            self.stats
                                .insert((peer, value.operation_id), value)
                                .is_none()
                        );
                    }
                    SessionEvent::DataChannel { channel, label } => {
                        self.channels.push((peer, channel, label));
                    }
                    SessionEvent::IceGatheringStateChanged(IceGatheringState::Complete) => {
                        self.gathered.insert(peer);
                    }
                    _ => {}
                }
            }
        }
    }
    fn stats(
        &mut self,
        session: &mut ProductionSession,
        peers: [SessionPeerId; 2],
        peer: SessionPeerId,
        id: OperationId,
    ) -> PeerStatsSnapshot {
        loop {
            self.drain(session, peers);
            if let Some(value) = self.stats.remove(&(peer, id)) {
                return value;
            }
            wait(session);
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
fn migrated_actor_preserves_native_channel_configuration_metadata_and_text_bytes() {
    let mut session = ProductionSession::new(Default::default()).unwrap();
    let peers = [
        session.create_peer(Default::default()).unwrap(),
        session.create_peer(Default::default()).unwrap(),
    ];
    let label = b"actor\xff\0".to_vec();
    let protocol = b"protocol\x80\0".to_vec();
    let outgoing = session
        .create_channel_bytes(
            peers[0],
            &label,
            Some(DataChannelConfiguration {
                reliable: true,
                max_retransmit_time_ms: Some(-99),
                max_retransmits: Some(i32::MAX),
                protocol: protocol.clone(),
                ..Default::default()
            }),
        )
        .unwrap();
    let expected_label = label.clone();
    drop(label);
    let rejected = session
        .create_channel_bytes(
            peers[0],
            b"invalid",
            Some(DataChannelConfiguration {
                max_retransmit_time_ms: Some(1),
                max_retransmits: Some(1),
                ..Default::default()
            }),
        )
        .unwrap_err();
    assert_eq!(rejected.kind, PeerErrorKind::InvalidParameter);
    assert!(!rejected.message.is_empty());
    let mut events = Events::default();
    let id = session.create_offer(peers[0], false).unwrap();
    let offer = events.finish(&mut session, peers, peers[0], id).unwrap();
    let offer = events.gather(&mut session, peers, peers[0], offer);
    let (report, legacy) = thread::spawn(move || {
        let mut session = session;
        let id = session.set_remote_description(peers[1], offer).unwrap();
        events.finish(&mut session, peers, peers[1], id);
        let id = session.create_answer(peers[1]).unwrap();
        let answer = events.finish(&mut session, peers, peers[1], id).unwrap();
        let answer = events.gather(&mut session, peers, peers[1], answer);
        let id = session.set_remote_description(peers[0], answer).unwrap();
        events.finish(&mut session, peers, peers[0], id);
        let incoming = loop {
            events.drain(&mut session, peers);
            if let Some((_, incoming, _)) = events.channels.iter().find(|(peer, _, label)| *peer == peers[1] && label == &expected_label) {
                if session.channel_state(outgoing).unwrap() == DataChannelState::Open && session.channel_state(*incoming).unwrap() == DataChannelState::Open { break *incoming; }
            }
            wait(&mut session);
        };
        for channel in [outgoing, incoming] {
            assert_eq!(session.channel_label(channel).unwrap(), expected_label);
            let config = session.channel_configuration(channel).unwrap();
            assert_eq!(config.protocol, protocol);
            assert_eq!(config.max_retransmit_time_ms, None);
            assert_eq!(config.max_retransmits, Some(65535));
            assert!(!config.reliable);
        }
        let message = DataChannelMessage { kind: DataChannelMessageKind::Text, bytes: vec![0xff, 0, 0x80] };
        let send_id = session.send_async(outgoing, message.clone()).unwrap();
        let (mut completed, mut received) = (false, false);
        while !completed || !received {
            while let Some(event) = session.try_channel_event(outgoing).unwrap() {
                if let DataChannelEvent::SendComplete(value) = event {
                    assert_eq!(value.send_id, send_id); value.result.unwrap(); completed = true;
                }
            }
            while let Some(event) = session.try_channel_event(incoming).unwrap() {
                if let DataChannelEvent::Message(value) = event { assert_eq!(value, message); assert!(!received); received = true; }
            }
            if !completed || !received { wait(&mut session); }
        }
        let operation = session.request_stats(peers[1]).unwrap();
        let snapshot = loop {
            if let Some(SessionEvent::Stats(value)) = session.try_peer_event(peers[1]).unwrap() {
                if value.operation_id == operation { break value; }
            } else { wait(&mut session); }
        };
        assert!(snapshot.records.iter().any(|record| matches!(record, PeerStatsRecord::DataChannel(value)
            if value.label.as_deref() == Some(expected_label.as_slice()) && value.protocol.as_deref() == Some(protocol.as_slice()))));
        assert!(snapshot.report.len() > snapshot.records.len(), "full report includes the other native families");
        let native_channel = snapshot.report.of_class(RtcStatsClass::DataChannelStats).into_iter().find(|v| matches!(v.values(), RtcStatsValues::DataChannelStats(value) if value.label.as_deref() == Some(expected_label.as_slice()))).unwrap();
        let RtcStatsValues::DataChannelStats(values) = native_channel.values() else { panic!() };
        assert_eq!(values.protocol.as_deref(), Some(protocol.as_slice()));
        assert_eq!(values.bytes_received, Some(3));
        let operation = session.request_legacy_stats(peers[1], LegacyStatsOutputLevel::Debug).unwrap();
        assert!(session.request_stats(peers[1]).is_err());
        let legacy = loop {
            match session.try_peer_event(peers[1]).unwrap() {
                Some(SessionEvent::LegacyStats(value)) if value.operation_id == operation => break value,
                Some(SessionEvent::OperationComplete(value)) if value.operation_id == operation => panic!("legacy request failed: {value:?}"),
                _ => wait(&mut session),
            }
        };
        let channel = legacy.reports.iter().find(|r| r.id.kind() == LegacyStatsType::DataChannel).unwrap();
        assert_eq!(channel.find(LegacyStatsName::Label).unwrap().value, LegacyStatsValueData::String(expected_label.clone()));
        assert!(legacy.reports.iter().flat_map(|r| &r.values).any(|v| matches!(&v.value, LegacyStatsValueData::Id(_))), "negotiated native transport links use retained SDK IDs");
        session.close_peer(peers[0]).unwrap();
        session.close_peer(peers[1]).unwrap();
        assert_eq!(session.channel_label(outgoing).unwrap(), expected_label);
        session.shutdown().unwrap();
        (snapshot.report, legacy)
    }).join().unwrap();
    assert!(
        legacy
            .reports
            .iter()
            .all(|r| !r.id.to_native_string().is_empty())
    );
    assert!(!report.is_empty());
    assert!(!report.of_class(RtcStatsClass::DataChannelStats).is_empty());
}

#[test]
fn migrated_actor_keeps_reused_native_sender_ids_as_distinct_resources() {
    fn negotiate(session: &mut ProductionSession, peers: [SessionPeerId; 2], events: &mut Events) {
        let id = session.create_offer(peers[0], false).unwrap();
        let offer = events.finish(session, peers, peers[0], id).unwrap();
        let id = session
            .set_local_description(peers[0], offer.clone())
            .unwrap();
        events.finish(session, peers, peers[0], id);
        let id = session.set_remote_description(peers[1], offer).unwrap();
        events.finish(session, peers, peers[1], id);
        let id = session.create_answer(peers[1]).unwrap();
        let answer = events.finish(session, peers, peers[1], id).unwrap();
        let id = session
            .set_local_description(peers[1], answer.clone())
            .unwrap();
        events.finish(session, peers, peers[1], id);
        let id = session.set_remote_description(peers[0], answer).unwrap();
        events.finish(session, peers, peers[0], id);
    }
    let mut session = ProductionSession::new(Default::default()).unwrap();
    let peers = [
        session.create_peer(Default::default()).unwrap(),
        session.create_peer(Default::default()).unwrap(),
    ];
    let source = session.create_opus_source(1).unwrap();
    let retired = session
        .publish_opus(
            peers[0],
            source,
            "actor-reused-id",
            RtpTransceiverDirection::SendOnly,
        )
        .unwrap();
    let old_sender = session.sender(retired).unwrap();
    let old_receiver = session.receiver(retired).unwrap();
    session
        .register_sender_packet_observer(old_sender, 2)
        .unwrap();
    let native_id = session.sender_native_id(old_sender).unwrap();
    let session = thread::spawn(move || {
        let mut session = session;
        let mut events = Events::default();
        negotiate(&mut session, peers, &mut events);
        session.stop_standard(retired).unwrap();
        negotiate(&mut session, peers, &mut events);
        assert!(session.transceiver_stopped(retired).unwrap());
        let source = session.create_opus_source(1).unwrap();
        let replacement = session
            .publish_opus(
                peers[0],
                source,
                "actor-reused-id",
                RtpTransceiverDirection::SendOnly,
            )
            .unwrap();
        let new_sender = session.sender(replacement).unwrap();
        let new_receiver = session.receiver(replacement).unwrap();
        assert_eq!(session.sender_native_id(new_sender).unwrap(), native_id);
        assert_ne!(retired, replacement);
        assert_ne!(old_sender, new_sender);
        assert_ne!(old_receiver, new_receiver);
        assert_eq!(session.sender(replacement).unwrap(), new_sender);
        session
            .register_sender_packet_observer(new_sender, 2)
            .unwrap();
        session
            .unregister_sender_packet_observer(old_sender)
            .unwrap();
        assert!(
            session
                .register_sender_packet_observer(new_sender, 2)
                .is_err(),
            "old observer retirement must not unregister the new endpoint"
        );
        session.close_peer(peers[0]).unwrap();
        session.close_peer(peers[1]).unwrap();
        assert_eq!(session.sender_native_id(old_sender).unwrap(), native_id);
        session
    })
    .join()
    .unwrap();
    let mut session = session;
    session.shutdown().unwrap();
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
        session.clear_stats_cache(peers[0]).unwrap();
        let stats_id = session.request_sender_stats(sender).unwrap();
        assert!(
            session.request_sender_stats(sender).is_err(),
            "native report result retains admission"
        );
        let sender_stats = events.stats(&mut session, peers, peers[0], stats_id);
        assert!(
            sender_stats
                .report
                .of_class(RtcStatsClass::DataChannelStats)
                .is_empty(),
            "sender selection is not the complete peer report"
        );
        assert!(
            !sender_stats
                .report
                .of_class(RtcStatsClass::OutboundRtpStreamStats)
                .is_empty()
        );
        let stats_id = session.request_receiver_stats(receiver).unwrap();
        let receiver_stats = events.stats(&mut session, peers, peers[0], stats_id);
        assert!(
            receiver_stats
                .report
                .of_class(RtcStatsClass::DataChannelStats)
                .is_empty()
        );
        let id = session
            .request_stats_with_sender_selector(peers[0], None)
            .unwrap();
        assert!(
            events
                .stats(&mut session, peers, peers[0], id)
                .report
                .is_empty()
        );
        let id = session
            .request_stats_with_receiver_selector(peers[0], None)
            .unwrap();
        assert!(
            events
                .stats(&mut session, peers, peers[0], id)
                .report
                .is_empty()
        );
        let id = session
            .request_stats_with_sender_selector(peers[1], Some(sender))
            .unwrap();
        assert!(
            events
                .stats(&mut session, peers, peers[1], id)
                .report
                .is_empty()
        );
        let id = session
            .request_stats_with_receiver_selector(peers[1], Some(receiver))
            .unwrap();
        assert!(
            events
                .stats(&mut session, peers, peers[1], id)
                .report
                .is_empty()
        );
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
