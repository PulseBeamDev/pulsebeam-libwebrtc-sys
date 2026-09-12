use std::{net::Ipv4Addr, thread, time::Duration};

use pulsebeam_webrtc_sys::{
    DataChannel, DataChannelConfiguration, DataChannelEvent, DataChannelMessage,
    DataChannelMessageKind, DataChannelSendResult, DataChannelState, Environment, IceCandidate,
    ManualClock, OperationId, PeerConfiguration, PeerConnection, PeerConnectionEvent,
    PeerConnectionFactory, RandomnessLease, SessionDescription, SimulatedNetwork,
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
        (alice_events, bob_events)
    }

    fn progress(&self) -> (Vec<PeerConnectionEvent>, Vec<PeerConnectionEvent>) {
        let mut alice_events = drain_peer(&self.alice);
        let mut bob_events = drain_peer(&self.bob);
        let alice_candidates = take_candidates(&mut alice_events);
        let bob_candidates = take_candidates(&mut bob_events);
        for candidate in alice_candidates {
            self.bob.add_ice_candidate(candidate);
        }
        for candidate in bob_candidates {
            self.alice.add_ice_candidate(candidate);
        }
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

fn route_initial_candidates(
    pair: &Pair,
    alice_events: &mut Vec<PeerConnectionEvent>,
    bob_events: &mut Vec<PeerConnectionEvent>,
) {
    for candidate in take_candidates(alice_events) {
        pair.bob.add_ice_candidate(candidate);
    }
    for candidate in take_candidates(bob_events) {
        pair.alice.add_ice_candidate(candidate);
    }
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
        let (mut alice_events, mut bob_events) = pair.negotiate();
        route_initial_candidates(&pair, &mut alice_events, &mut bob_events);
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
    let (mut alice_events, mut bob_events) = pair.negotiate();
    route_initial_candidates(&pair, &mut alice_events, &mut bob_events);
    wait_for_open(&pair, &alice, &bob);
    assert_eq!(alice.configuration().id, configuration.id);
    assert_eq!(bob.configuration().id, configuration.id);
    exchange_messages(&pair, &alice, &bob);
}

#[test]
fn close_and_drop_orders_quiesce_observers_and_reject_sends() {
    let pair = Pair::new(12);
    let mut alice = pair
        .alice
        .create_data_channel("teardown", DataChannelConfiguration::default())
        .unwrap();
    let (mut alice_events, mut bob_events) = pair.negotiate();
    route_initial_candidates(&pair, &mut alice_events, &mut bob_events);
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
