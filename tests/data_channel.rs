use std::{net::Ipv4Addr, thread, time::Duration};

#[path = "support/non_trickle.rs"]
mod non_trickle;

use pulsebeam_webrtc_sys::{
    DataChannel, DataChannelConfiguration, DataChannelEvent, DataChannelMessage,
    DataChannelMessageKind, DataChannelSendResult, DataChannelState, Environment, ManualClock,
    OperationId, PeerConfiguration, PeerConnection, PeerConnectionEvent, PeerConnectionFactory,
    RandomnessLease, SessionDescription, SimulatedNetwork,
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
        let offer =
            completion_description(&self.alice, self.alice.create_offer(), &mut alice_events);
        completion(
            &self.alice,
            self.alice.set_local_description(offer.clone()),
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
            self.bob.set_remote_description(gathered),
            &mut bob_events,
        );
        let answer = completion_description(&self.bob, self.bob.create_answer(), &mut bob_events);
        completion(
            &self.bob,
            self.bob.set_local_description(answer.clone()),
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
            self.alice.set_remote_description(gathered),
            &mut alice_events,
        );
        (alice_events, bob_events)
    }

    fn progress(&self) -> (Vec<PeerConnectionEvent>, Vec<PeerConnectionEvent>) {
        let alice_events = drain_peer(&self.alice);
        let bob_events = drain_peer(&self.bob);
        while let Some(packet) = self.network.next_packet() {
            self.network.deliver(packet.id).unwrap();
        }
        self.clock.advance(Duration::from_millis(1)).unwrap();
        thread::yield_now();
        (alice_events, bob_events)
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
    assert_eq!(
        drain_channel(&alice),
        [DataChannelEvent::StateChanged(DataChannelState::Open)]
    );
    assert_eq!(
        drain_channel(&bob),
        [DataChannelEvent::StateChanged(DataChannelState::Open)]
    );

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
        alice.send(DataChannelMessage::binary(vec![0; 16 * 1024 * 1024 + 1])),
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
    assert!(
        peer.create_data_channel(
            "invalid",
            DataChannelConfiguration {
                negotiated: true,
                ..Default::default()
            },
        )
        .is_err()
    );
}
