use std::{net::Ipv4Addr, thread, time::Duration};

#[path = "support/non_trickle.rs"]
mod non_trickle;

use pulsebeam_webrtc_sys::{
    DataChannel, DataChannelConfiguration, DataChannelCounters, DataChannelEvent,
    DataChannelMessage, DataChannelMessageKind, DataChannelSendError, DataChannelSendResult,
    DataChannelState, Environment, ManualClock, OperationId, PeerConfiguration, PeerConnection,
    PeerConnectionEvent, PeerConnectionFactory, PeerErrorKind, RandomnessLease, SessionDescription,
    SimulatedNetwork,
};

struct Pair {
    clock: ManualClock,
    network: SimulatedNetwork,
    alice: PeerConnection,
    bob: PeerConnection,
    _alice_endpoint: pulsebeam_webrtc_sys::NetworkEndpoint,
    _bob_endpoint: pulsebeam_webrtc_sys::NetworkEndpoint,
}

impl Pair {
    fn new(seed: u64) -> Self {
        let randomness = RandomnessLease::acquire(seed).unwrap();
        let clock = ManualClock::new(Duration::from_secs(1)).unwrap();
        let environment = Environment::builder()
            .clock(&clock)
            .randomness(&randomness)
            .build()
            .unwrap();
        let network = SimulatedNetwork::new(&clock).unwrap();
        let alice_endpoint = network
            .register_endpoint(Ipv4Addr::new(10, 21, 0, 1).into())
            .unwrap();
        let bob_endpoint = network
            .register_endpoint(Ipv4Addr::new(10, 21, 0, 2).into())
            .unwrap();
        let alice_factory = factory(environment.clone(), &alice_endpoint);
        let bob_factory = factory(environment, &bob_endpoint);
        let alice = alice_factory
            .create_peer_connection(PeerConfiguration::default())
            .unwrap();
        let bob = bob_factory
            .create_peer_connection(PeerConfiguration::default())
            .unwrap();
        Self {
            clock,
            network,
            alice,
            bob,
            _alice_endpoint: alice_endpoint,
            _bob_endpoint: bob_endpoint,
        }
    }

    fn negotiate(&self) -> (Vec<PeerConnectionEvent>, Vec<PeerConnectionEvent>) {
        let mut alice_events = Vec::new();
        let mut bob_events = Vec::new();
        let offer = completion_description(
            &self.alice,
            self.alice.create_offer().unwrap(),
            &mut alice_events,
        );
        completion(
            &self.alice,
            self.alice.set_local_description(offer.clone()).unwrap(),
            &mut alice_events,
        );
        let gathered = non_trickle::gathered_local_description(
            &self.alice,
            &self.clock,
            &self.network,
            &mut alice_events,
        );
        assert_eq!(gathered.kind, offer.kind);
        completion(
            &self.bob,
            self.bob.set_remote_description(gathered).unwrap(),
            &mut bob_events,
        );
        let answer = completion_description(
            &self.bob,
            self.bob.create_answer().unwrap(),
            &mut bob_events,
        );
        completion(
            &self.bob,
            self.bob.set_local_description(answer.clone()).unwrap(),
            &mut bob_events,
        );
        let gathered = non_trickle::gathered_local_description(
            &self.bob,
            &self.clock,
            &self.network,
            &mut bob_events,
        );
        assert_eq!(gathered.kind, answer.kind);
        completion(
            &self.alice,
            self.alice.set_remote_description(gathered).unwrap(),
            &mut alice_events,
        );
        (alice_events, bob_events)
    }

    fn progress(&self) -> (Vec<PeerConnectionEvent>, Vec<PeerConnectionEvent>) {
        let alice_events = drain_peer(&self.alice);
        let bob_events = drain_peer(&self.bob);
        self.progress_transport_only();
        (alice_events, bob_events)
    }

    fn progress_transport_only(&self) {
        while let Some(packet) = self.network.next_packet() {
            self.network.deliver(packet.id).unwrap();
        }
        self.clock.advance(Duration::from_millis(1)).unwrap();
        thread::yield_now();
    }
}

fn factory(
    environment: Environment,
    endpoint: &pulsebeam_webrtc_sys::NetworkEndpoint,
) -> PeerConnectionFactory {
    PeerConnectionFactory::builder()
        .environment(environment)
        .network_manager(endpoint.network_manager().unwrap())
        .packet_socket_factory(endpoint.packet_socket_factory().unwrap())
        .build()
        .unwrap()
}

fn drain_peer(peer: &PeerConnection) -> Vec<PeerConnectionEvent> {
    std::iter::from_fn(|| peer.try_next_event()).collect()
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

fn drain_channel(channel: &DataChannel) -> Vec<DataChannelEvent> {
    std::iter::from_fn(|| channel.try_next_event()).collect()
}

fn wait_for_open(pair: &Pair, alice: &DataChannel, bob: &DataChannel) {
    let mut alice_open = false;
    let mut bob_open = false;
    for _ in 0..2_000_000 {
        pair.progress();
        for event in drain_channel(alice) {
            alice_open |= matches!(
                event,
                DataChannelEvent::StateChanged(DataChannelState::Open)
            );
        }
        for event in drain_channel(bob) {
            bob_open |= matches!(
                event,
                DataChannelEvent::StateChanged(DataChannelState::Open)
            );
        }
        if alice_open && bob_open {
            return;
        }
    }
    panic!("data channels did not open");
}

fn wait_for_remote_channel(
    pair: &Pair,
    mut initial_events: Vec<PeerConnectionEvent>,
) -> DataChannel {
    for _ in 0..2_000_000 {
        for event in initial_events.drain(..) {
            if let PeerConnectionEvent::DataChannel(channel) = event {
                return channel;
            }
        }
        initial_events = pair.progress().1;
    }
    panic!("remote data channel did not arrive");
}

fn exchange_messages(pair: &Pair, alice: &DataChannel, bob: &DataChannel) {
    let alice_text = DataChannelMessage::text("alice text");
    let alice_binary = DataChannelMessage::binary([0, 1, 2, 255]);
    let bob_text = DataChannelMessage::text("bob text");
    let bob_binary = DataChannelMessage::binary([9, 8, 0, 7]);
    assert_eq!(alice.send(alice_text.clone()), DataChannelSendResult::Sent);
    assert_eq!(
        alice.send(alice_binary.clone()),
        DataChannelSendResult::Sent
    );
    assert_eq!(bob.send(bob_text.clone()), DataChannelSendResult::Sent);
    assert_eq!(bob.send(bob_binary.clone()), DataChannelSendResult::Sent);

    let mut at_alice = Vec::new();
    let mut at_bob = Vec::new();
    for _ in 0..2_000_000 {
        pair.progress();
        for event in drain_channel(alice) {
            if let DataChannelEvent::Message(message) = event {
                at_alice.push(message);
            }
        }
        for event in drain_channel(bob) {
            if let DataChannelEvent::Message(message) = event {
                at_bob.push(message);
            }
        }
        if at_alice.len() == 2 && at_bob.len() == 2 {
            break;
        }
    }
    assert_eq!(at_alice, [bob_text, bob_binary]);
    assert_eq!(at_bob, [alice_text, alice_binary]);
    assert_eq!(at_alice[0].kind, DataChannelMessageKind::Text);
    assert_eq!(at_alice[1].kind, DataChannelMessageKind::Binary);
}

#[test]
fn remote_and_negotiated_channels_exchange_owned_bytes_deterministically() {
    {
        let pair = Pair::new(11);
        let mut alice = pair
            .alice
            .create_data_channel("announced", DataChannelConfiguration::default())
            .unwrap();
        assert_eq!(
            alice.send(DataChannelMessage::text("too early")),
            DataChannelSendResult::NotOpen
        );
        let (_alice_events, bob_events) = pair.negotiate();
        let mut bob = wait_for_remote_channel(&pair, bob_events);
        assert_eq!(bob.label(), "announced");
        let remote_configuration = bob.configuration();
        assert!(remote_configuration.ordered);
        assert!(!remote_configuration.negotiated);
        assert!(remote_configuration.id.is_some());
        wait_for_open(&pair, &alice, &bob);
        exchange_messages(&pair, &alice, &bob);
        alice.close().unwrap();
        bob.close().unwrap();
    }

    let pair = Pair::new(11);
    let configuration = DataChannelConfiguration {
        negotiated: true,
        id: Some(7),
        ..Default::default()
    };
    let alice = pair
        .alice
        .create_data_channel("negotiated", configuration.clone())
        .unwrap();
    let bob = pair
        .bob
        .create_data_channel("negotiated", configuration.clone())
        .unwrap();
    let (_alice_events, _bob_events) = pair.negotiate();
    wait_for_open(&pair, &alice, &bob);
    assert_eq!(alice.configuration().id, configuration.id);
    assert_eq!(bob.configuration().id, configuration.id);
    exchange_messages(&pair, &alice, &bob);
}

#[test]
fn advisory_notifications_coalesce_while_messages_remain_ordered() {
    let pair = Pair::new(13);
    let configuration = DataChannelConfiguration {
        negotiated: true,
        id: Some(9),
        ..Default::default()
    };
    let alice = pair
        .alice
        .create_data_channel("advisory", configuration.clone())
        .unwrap();
    let bob = pair
        .bob
        .create_data_channel("advisory", configuration)
        .unwrap();
    pair.negotiate();
    // Do not consume channel events during connection establishment.
    for _ in 0..2_000_000 {
        pair.progress();
        if alice.state() == DataChannelState::Open && bob.state() == DataChannelState::Open {
            break;
        }
    }
    assert_eq!(alice.state(), DataChannelState::Open);
    assert_eq!(bob.state(), DataChannelState::Open);
    for channel in [&alice, &bob] {
        let events = drain_channel(channel);
        assert!(events.contains(&DataChannelEvent::StateChanged(DataChannelState::Open)));
        assert!(events.iter().any(|event| matches!(
            event, DataChannelEvent::MaxMessageSizeChanged { max_message_size } if *max_message_size > 0
        )));
        assert_eq!(events.len(), 2);
    }

    let mut messages = Vec::new();
    for sequence in 0..64u8 {
        assert_eq!(
            alice.send(DataChannelMessage::binary(vec![sequence; 128])),
            DataChannelSendResult::Sent
        );
        // Await each receive and local queue drain, keeping native progress
        // active without taking Alice's advisory events between sends.
        for _ in 0..2_000_000 {
            pair.progress();
            for event in drain_channel(&bob) {
                if let DataChannelEvent::Message(message) = event {
                    messages.push(message);
                }
            }
            if messages.len() == usize::from(sequence) + 1 && alice.buffered_amount() == 0 {
                break;
            }
        }
        assert_eq!(messages.len(), usize::from(sequence) + 1);
        assert_eq!(alice.buffered_amount(), 0);
    }
    assert_eq!(messages.len(), 64);
    for (sequence, message) in messages.iter().enumerate() {
        assert_eq!(
            message,
            &DataChannelMessage::binary(vec![sequence as u8; 128])
        );
    }
    assert_eq!(alice.buffered_amount(), 0);
    assert_eq!(
        drain_channel(&alice),
        [DataChannelEvent::BufferedAmountChanged {
            sent_data_size: 64 * 128
        }]
    );
}

#[test]
fn native_stream_ids_can_be_reused_after_reset() {
    let pair = Pair::new(14);
    let mut first = pair
        .alice
        .create_data_channel("first", Default::default())
        .unwrap();
    let (_, events) = pair.negotiate();
    let first_remote = wait_for_remote_channel(&pair, events);
    wait_for_open(&pair, &first, &first_remote);
    let sid = first.configuration().id;
    assert!(sid.is_some());
    assert_eq!(first.error(), None);
    first.close().unwrap();
    first.close().unwrap();
    for _ in 0..2_000_000 {
        pair.progress();
        if first.state() == DataChannelState::Closed
            && first_remote.state() == DataChannelState::Closed
        {
            break;
        }
    }
    assert_eq!(first.state(), DataChannelState::Closed);
    assert_eq!(first_remote.state(), DataChannelState::Closed);
    assert_eq!(first.error(), None);
    assert_eq!(first_remote.error(), None);

    let next = pair
        .alice
        .create_data_channel("next", Default::default())
        .unwrap();
    let next_remote = wait_for_remote_channel(&pair, Vec::new());
    wait_for_open(&pair, &next, &next_remote);
    assert_eq!(next.configuration().id, sid);
    assert_eq!(next_remote.configuration().id, sid);
    assert_eq!(next.label(), "next");
    exchange_messages(&pair, &next, &next_remote);
}

#[test]
fn native_observer_registration_preserves_ordered_delivery() {
    let pair = Pair::new(16);
    let alice = pair
        .alice
        .create_data_channel("observation", Default::default())
        .unwrap();
    let (_, events) = pair.negotiate();
    let mut bob = wait_for_remote_channel(&pair, events);
    wait_for_open(&pair, &alice, &bob);
    bob.set_event_observation(false);
    let expected: Vec<_> = (0..3)
        .map(|index| DataChannelMessage::binary([index]))
        .collect();
    for message in &expected {
        assert_eq!(alice.send(message.clone()), DataChannelSendResult::Sent);
    }
    for _ in 0..1_000 {
        pair.progress_transport_only();
    }
    assert!(drain_channel(&bob).is_empty());
    assert_eq!(bob.state(), DataChannelState::Open);
    assert_eq!(bob.error(), None);
    bob.set_event_observation(true);
    bob.set_event_observation(true);
    let mut received = Vec::new();
    for _ in 0..2_000_000 {
        pair.progress_transport_only();
        for event in drain_channel(&bob) {
            if let DataChannelEvent::Message(message) = event {
                received.push(message);
            }
        }
        if received.len() == expected.len() {
            break;
        }
    }
    assert_eq!(received, expected);
}

#[test]
fn native_pre_observer_overflow_is_an_owned_resource_error() {
    let pair = Pair::new(15);
    let alice = pair
        .alice
        .create_data_channel("unobserved", Default::default())
        .unwrap();
    let (_, events) = pair.negotiate();
    let mut bob = wait_for_remote_channel(&pair, events);
    wait_for_open(&pair, &alice, &bob);
    assert_eq!(alice.error(), None);
    bob.set_event_observation(false);
    bob.set_event_observation(false);

    // Unregister Bob's native observer without creating another receive policy.
    // Exercise the pinned engine's own 16 MiB pre-observer queue, not a Rust
    // inbox cap or a consumer-credit implementation.
    let payload = vec![0xa5; 64 * 1024];
    let mut accepted = 0u64;
    while accepted <= DataChannel::send_queue_capacity() + payload.len() as u64 {
        if bob.state() == DataChannelState::Closed {
            break;
        }
        assert_eq!(
            alice.send(DataChannelMessage::binary(payload.clone())),
            DataChannelSendResult::Sent
        );
        accepted += payload.len() as u64;
        for _ in 0..2_000_000 {
            pair.progress_transport_only();
            if alice.buffered_amount() == 0 || alice.state() == DataChannelState::Closed {
                break;
            }
        }
        assert!(alice.buffered_amount() == 0 || alice.state() == DataChannelState::Closed);
    }
    assert!(accepted > DataChannel::send_queue_capacity());
    for _ in 0..2_000_000 {
        pair.progress_transport_only();
        if bob.state() == DataChannelState::Closed {
            break;
        }
    }
    assert_eq!(bob.state(), DataChannelState::Closed);
    let error = bob
        .error()
        .expect("native pre-observer overflow must report an error");
    assert_eq!(error.kind, PeerErrorKind::ResourceExhausted);
    assert_eq!(error.detail, None);
    assert_eq!(error.sctp_cause_code, None);
    assert!(!error.message.is_empty());
    bob.set_event_observation(true);
    assert_eq!(
        drain_channel(&bob),
        [DataChannelEvent::StateChanged(DataChannelState::Closed)]
    );
    assert_eq!(bob.error(), Some(error.clone()));
    drop(bob);
    drop(alice);
    drop(pair);
    assert_eq!(error.kind, PeerErrorKind::ResourceExhausted);
    assert!(!error.to_string().is_empty());
}

#[test]
fn close_and_drop_orders_quiesce_observers_and_reject_sends() {
    let pair = Pair::new(12);
    let mut alice = pair
        .alice
        .create_data_channel("teardown", DataChannelConfiguration::default())
        .unwrap();
    let (_alice_events, bob_events) = pair.negotiate();
    let bob = wait_for_remote_channel(&pair, bob_events);
    wait_for_open(&pair, &alice, &bob);

    assert_eq!(
        alice.send(DataChannelMessage::binary(vec![
            0;
            DataChannel::send_queue_capacity()
                as usize
                + 1
        ])),
        DataChannelSendResult::Backpressure
    );
    assert_eq!(
        alice.send(DataChannelMessage::binary(vec![42; 64 * 1024])),
        DataChannelSendResult::Sent
    );
    alice.close().unwrap();
    assert_eq!(
        alice.send(DataChannelMessage::text("during close")),
        DataChannelSendResult::NotOpen
    );

    let mut saw_terminal = false;
    for _ in 0..2_000_000 {
        pair.progress();
        for event in drain_channel(&alice) {
            assert!(
                !saw_terminal,
                "event arrived after terminal close: {event:?}"
            );
            saw_terminal = matches!(
                event,
                DataChannelEvent::StateChanged(DataChannelState::Closed)
            );
        }
        if saw_terminal {
            break;
        }
    }
    assert!(saw_terminal);
    for _ in 0..10_000 {
        pair.progress();
    }
    assert!(alice.try_next_event().is_none());

    drop(pair.alice);
    assert_eq!(alice.state(), DataChannelState::Closed);
    drop(bob);
    drop(alice);

    for _ in 0..16 {
        let factory = PeerConnectionFactory::builder().build().unwrap();
        let peer = factory
            .create_peer_connection(PeerConfiguration::default())
            .unwrap();
        let channel = peer
            .create_data_channel("partial", DataChannelConfiguration::default())
            .unwrap();
        drop(channel);
        drop(peer);
        drop(factory);

        let factory = PeerConnectionFactory::builder().build().unwrap();
        let peer = factory
            .create_peer_connection(PeerConfiguration::default())
            .unwrap();
        let mut channel = peer
            .create_data_channel("peer-first", DataChannelConfiguration::default())
            .unwrap();
        drop(peer);
        channel.close().unwrap();
        drop(channel);
        drop(factory);
    }
}

#[test]
fn owning_transport_aliases_observers_and_revocable_ice_internal() {
    use pulsebeam_webrtc_sys::{DtlsTransportEvent, DtlsTransportState,
        SctpTransportState, TransportObserverError, IceInternalUnavailable};
    let mut pair = Pair::new(32);
    assert!(pair.alice.sctp_transport().is_none());
    let alice_channel = pair.alice.create_data_channel("transports", DataChannelConfiguration::default()).unwrap();
    let (_, events) = pair.negotiate();
    let bob_channel = wait_for_remote_channel(&pair, events);
    wait_for_open(&pair, &alice_channel, &bob_channel);

    let sctp = pair.alice.sctp_transport().unwrap();
    let sctp_alias = pair.alice.sctp_transport().unwrap();
    assert!(sctp.same_resource(&sctp_alias));
    let sctp_info = sctp.information();
    assert_eq!(sctp_info.state(), SctpTransportState::Connected);
    assert!(sctp_info.max_message_size().is_some());
    let dtls = sctp.dtls_transport().unwrap();
    assert!(dtls.same_resource(&sctp_info.dtls_transport().unwrap()));
    assert!(dtls.same_resource(&sctp_alias.dtls_transport().unwrap()));
    let dtls_info = dtls.information();
    assert_eq!(dtls_info.state(), DtlsTransportState::Connected);
    assert!(dtls_info.role().is_some());
    assert!(dtls_info.remote_ssl_certificates().is_some());
    assert_eq!(dtls_info.clone().remote_certificate_data(), dtls_info.remote_certificate_data());
    assert_eq!(dtls_info.remote_ssl_certificates().unwrap().len(), dtls_info.remote_certificate_data().unwrap().len());
    let constructed = pulsebeam_webrtc_sys::SctpTransportInformation::from_parts(SctpTransportState::Connecting,
        Some(&dtls), Some(0.0), Some(0));
    assert!(dtls.same_resource(&constructed.dtls_transport().unwrap()));
    assert_eq!(constructed.max_message_size(), Some(0.0));
    assert_eq!(constructed.max_channels(), Some(0));
    drop(constructed);
    let ice = dtls.ice_transport().unwrap();
    let internal = ice.internal().unwrap();
    assert_eq!(internal.component(), Ok(1));

    assert_eq!(sctp.register_observer(0), Err(TransportObserverError::ZeroCapacity));
    assert_eq!(sctp.register_observer(1), Ok(()));
    assert_eq!(sctp_alias.register_observer(1), Err(TransportObserverError::AlreadyRegistered));
    let dtls_alias = sctp_info.dtls_transport().unwrap();
    assert_eq!(dtls.register_observer(1), Ok(()));
    assert_eq!(dtls_alias.register_observer(1), Err(TransportObserverError::AlreadyRegistered));
    // Explicit peer teardown leaves owned interfaces/snapshots alive, but
    // revokes the borrowed internal pointer. No pump or synthetic close event.
    pair.alice.close().unwrap();
    assert_eq!(dtls.information().state(), DtlsTransportState::Closed);
    assert_eq!(sctp.information().state(), SctpTransportState::Closed);
    assert!(sctp.dtls_transport().is_none());
    assert!(ice.internal().is_none());
    assert_eq!(internal.component(), Err(IceInternalUnavailable));
    assert_eq!(dtls_info.state(), DtlsTransportState::Connected);
    assert_eq!(sctp_info.state(), SctpTransportState::Connected);
    assert!(dtls.same_resource(&sctp_info.dtls_transport().unwrap()));
    assert_eq!(sctp_alias.take_event().unwrap().state(), SctpTransportState::Closed);
    match dtls_alias.take_event().unwrap() {
        DtlsTransportEvent::StateChange(info) => assert_eq!(info.state(), DtlsTransportState::Closed),
        DtlsTransportEvent::Error(error) => panic!("unexpected DTLS error: {error}"),
    }
    assert_eq!(dtls.take_dropped_events(), 0);
    assert_eq!(sctp.take_dropped_events(), 0);
    sctp_alias.unregister_observer();
    dtls_alias.unregister_observer();
    assert_eq!(dtls.register_observer(1), Ok(()));
    // Drop while registered: native unregistration occurs before observer
    // destruction, and retained peer context keeps the network thread alive.
    drop(dtls_alias);
    drop(sctp_info);
    drop(sctp_alias);
    drop(sctp);
    drop(dtls);
    pair.bob.close().unwrap();
}

#[test]
fn invalid_data_channel_configuration_is_explicit() {
    let factory = PeerConnectionFactory::builder().build().unwrap();
    let peer = factory
        .create_peer_connection(PeerConfiguration::default())
        .unwrap();
    assert!(
        peer.create_data_channel(
            "invalid",
            DataChannelConfiguration {
                max_retransmit_time_ms: Some(1),
                max_retransmits: Some(1),
                ..Default::default()
            },
        )
        .is_err()
    );
    // SCTP reserves 65535. Keep the owned native validation outcome rather
    // than treating the entire Rust u16 representation as an admitted range.
    assert_eq!(
        peer.create_data_channel(
            "reserved-id",
            DataChannelConfiguration {
                negotiated: true,
                id: Some(u16::MAX),
                ..Default::default()
            },
        )
        .unwrap_err()
        .kind,
        PeerErrorKind::InvalidRange
    );
    // The native engine admits a negotiated channel without an assigned id.
    // It will allocate the stream id when the DTLS role becomes known.
    let channel = peer
        .create_data_channel(
            "native-assigned",
            DataChannelConfiguration {
                negotiated: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(channel.configuration().id, None);
    let inband = peer
        .create_data_channel(
            "native-inband",
            DataChannelConfiguration {
                id: Some(42),
                ..Default::default()
            },
        )
        .unwrap();
    // Upstream intentionally ignores an id for in-band negotiation.
    assert_eq!(inband.configuration().id, None);
    for (id, priority) in [(13, 17), (15, u16::MAX)] {
        let config = DataChannelConfiguration {
            ordered: false,
            max_retransmit_time_ms: Some(0),
            protocol: "opaque/application-protocol".into(),
            negotiated: true,
            id: Some(id),
            priority: Some(pulsebeam_webrtc_sys::DataChannelPriority::Custom(priority)),
            ..Default::default()
        };
        let channel = peer.create_data_channel("custom", config.clone()).unwrap();
        assert_eq!(channel.configuration(), config);
        assert!(!channel.reliable());
    }
}

#[test]
fn async_native_completion_size_and_counters() {
    let pair = Pair::new(20);
    let mut alice = pair
        .alice
        .create_data_channel("async", Default::default())
        .unwrap();
    let (_, events) = pair.negotiate();
    let bob = wait_for_remote_channel(&pair, events);
    // Collect negotiation advisories while waiting for open. The shared
    // wait_for_open helper drains these events as well as state changes.
    let mut limits = Vec::new();
    for _ in 0..2_000_000 {
        pair.progress();
        for event in drain_channel(&alice) {
            if let DataChannelEvent::MaxMessageSizeChanged { max_message_size } = event {
                limits.push(max_message_size);
            }
        }
        if alice.state() == DataChannelState::Open && bob.state() == DataChannelState::Open {
            break;
        }
    }
    assert_eq!(alice.state(), DataChannelState::Open);
    assert_eq!(bob.state(), DataChannelState::Open);
    assert_eq!(limits.len(), 1);
    let limit = usize::try_from(limits[0]).unwrap();
    assert!(limit > 0);
    assert!(alice.reliable());
    assert_eq!(alice.counters(), DataChannelCounters::default());
    alice.set_event_observation(false);
    let first = alice.send_async(DataChannelMessage::text("hello")).unwrap();
    let second = alice
        .send_async(DataChannelMessage::binary([1, 2, 3]))
        .unwrap();
    let mut completed = std::collections::HashMap::new();
    let mut received = Vec::new();
    for _ in 0..2_000_000 {
        pair.progress();
        for event in drain_channel(&alice) {
            if let DataChannelEvent::SendComplete(done) = event {
                assert!(completed.insert(done.send_id, done.result).is_none());
            }
        }
        for event in drain_channel(&bob) {
            if let DataChannelEvent::Message(message) = event {
                received.push(message);
            }
        }
        if completed.len() == 2 && received.len() == 2 {
            break;
        }
    }
    assert_eq!(completed.remove(&first), Some(Ok(())));
    assert_eq!(completed.remove(&second), Some(Ok(())));
    assert_eq!(
        received,
        [
            DataChannelMessage::text("hello"),
            DataChannelMessage::binary([1, 2, 3])
        ]
    );
    assert_eq!(alice.counters().messages_sent, 2);
    assert_eq!(alice.counters().bytes_sent, 8);
    assert_eq!(bob.counters().messages_received, 2);
    assert_eq!(bob.counters().bytes_received, 8);

    // The pinned SCTP engine reports oversized messages as a network failure
    // and closes the channel. Observe that native result, not a fabricated
    // InvalidRange error, after the valid sends have reached the receiver.
    let oversized = alice
        .send_async(DataChannelMessage::binary(vec![0; limit + 1]))
        .unwrap();
    let mut rejection = None;
    for _ in 0..2_000_000 {
        pair.progress();
        for event in drain_channel(&alice) {
            if let DataChannelEvent::SendComplete(done) = event {
                assert_eq!(done.send_id, oversized);
                assert!(rejection.replace(done.result).is_none());
            }
        }
        if rejection.is_some() {
            break;
        }
    }
    match rejection.unwrap().unwrap_err() {
        DataChannelSendError::Native(error) => {
            assert_eq!(error.kind, PeerErrorKind::Network);
            assert!(!error.message.is_empty());
        }
        error => panic!("expected native size rejection, got {error:?}"),
    }
    assert_eq!(alice.counters().messages_sent, 2);
    assert_eq!(alice.error().unwrap().kind, PeerErrorKind::Network);
    alice.close().unwrap();
    for _ in 0..2_000_000 {
        pair.progress();
        if alice.state() == DataChannelState::Closed {
            break;
        }
    }
    assert_eq!(alice.state(), DataChannelState::Closed);
    let closed = alice
        .send_async(DataChannelMessage::text("closed"))
        .unwrap();
    for _ in 0..2_000_000 {
        pair.progress();
        if let Some(DataChannelEvent::SendComplete(done)) = alice.try_next_event() {
            assert_eq!(done.send_id, closed);
            assert!(done.result.is_err());
            return;
        }
    }
    panic!("closed send never settled");
}

#[test]
fn async_admission_and_cancellation_survive_peer_drop() {
    let factory = PeerConnectionFactory::builder().build().unwrap();
    let peer = factory.create_peer_connection(Default::default()).unwrap();
    let channel = peer
        .create_data_channel("detached", Default::default())
        .unwrap();
    drop(peer);
    let mut admitted = std::collections::HashSet::new();
    for _ in 0..64 {
        assert!(admitted.insert(channel.send_async(DataChannelMessage::binary([1])).unwrap()));
    }
    assert_eq!(
        channel
            .send_async(DataChannelMessage::binary([2]))
            .unwrap_err()
            .kind,
        PeerErrorKind::ResourceExhausted
    );
    assert_eq!(
        channel
            .send_async(DataChannelMessage::binary(vec![
                0;
                DataChannel::send_queue_capacity()
                    as usize
                    + 1
            ]))
            .unwrap_err()
            .kind,
        PeerErrorKind::ResourceExhausted
    );
    for _ in 0..2_000_000 {
        if let Some(DataChannelEvent::SendComplete(done)) = channel.try_next_event() {
            assert!(
                admitted.remove(&done.send_id),
                "duplicate or unknown result"
            );
            assert_eq!(done.result, Err(DataChannelSendError::Canceled));
        }
        if admitted.is_empty() {
            break;
        }
        std::thread::yield_now();
    }
    assert!(admitted.is_empty());
    let next = channel.send_async(DataChannelMessage::binary([3])).unwrap();
    for _ in 0..2_000_000 {
        if let Some(DataChannelEvent::SendComplete(done)) = channel.try_next_event() {
            assert_eq!(done.send_id, next);
            assert_eq!(done.result, Err(DataChannelSendError::Canceled));
            return;
        }
        std::thread::yield_now();
    }
    panic!("detached send never settled");
}
