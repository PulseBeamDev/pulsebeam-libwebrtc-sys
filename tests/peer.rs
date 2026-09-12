use std::{net::Ipv4Addr, thread};

use pulsebeam_webrtc_sys::{
    ConnectionState, Environment, IceCandidate, ManualClock, OperationCompletion, OperationId,
    PeerConfiguration, PeerConnection, PeerConnectionEvent, PeerConnectionFactory, PeerErrorKind,
    SessionDescription, SimulatedNetwork,
};

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

fn drain_until_completion(
    peer: &PeerConnection,
    operation_id: OperationId,
    side_events: &mut Vec<PeerConnectionEvent>,
) -> OperationCompletion {
    for _ in 0..1_000_000 {
        while let Some(event) = peer.try_next_event() {
            if let PeerConnectionEvent::OperationComplete(completion) = &event
                && completion.operation_id == operation_id
            {
                return completion.clone();
            }
            side_events.push(event);
        }
        thread::yield_now();
    }
    panic!("operation {} did not complete", operation_id.get());
}

fn require_success(
    peer: &PeerConnection,
    operation_id: OperationId,
    events: &mut Vec<PeerConnectionEvent>,
) -> Option<SessionDescription> {
    drain_until_completion(peer, operation_id, events)
        .result
        .unwrap()
}

fn collect_candidate(event: PeerConnectionEvent, candidates: &mut Vec<IceCandidate>) -> bool {
    match event {
        PeerConnectionEvent::IceCandidate(candidate) => candidates.push(candidate),
        PeerConnectionEvent::ConnectionStateChanged(ConnectionState::Connected) => return true,
        _ => {}
    }
    false
}

#[test]
fn two_peers_negotiate_owned_sdp_and_ice_without_sleeps() {
    let clock = ManualClock::new(std::time::Duration::from_secs(1)).unwrap();
    let environment = Environment::builder().clock(&clock).build().unwrap();
    let network = SimulatedNetwork::new(&clock).unwrap();
    let alice_endpoint = network
        .register_endpoint(Ipv4Addr::new(10, 20, 0, 1).into())
        .unwrap();
    let bob_endpoint = network
        .register_endpoint(Ipv4Addr::new(10, 20, 0, 2).into())
        .unwrap();
    let alice_factory = factory(environment.clone(), &alice_endpoint);
    let bob_factory = factory(environment, &bob_endpoint);
    let mut alice = alice_factory
        .create_peer_connection(PeerConfiguration::default())
        .unwrap();
    let mut bob = bob_factory
        .create_peer_connection(PeerConfiguration::default())
        .unwrap();
    drop(alice_factory);
    drop(bob_factory);

    let mut alice_events = Vec::new();
    let mut bob_events = Vec::new();
    let offer_id = alice.create_offer();
    let offer = require_success(&alice, offer_id, &mut alice_events).unwrap();
    assert!(offer.sdp.contains("m=application"));
    let set_local_offer = alice.set_local_description(offer.clone());
    let set_remote_offer = bob.set_remote_description(offer);
    require_success(&alice, set_local_offer, &mut alice_events);
    require_success(&bob, set_remote_offer, &mut bob_events);

    let answer_id = bob.create_answer();
    let answer = require_success(&bob, answer_id, &mut bob_events).unwrap();
    let set_local_answer = bob.set_local_description(answer.clone());
    let set_remote_answer = alice.set_remote_description(answer);
    require_success(&bob, set_local_answer, &mut bob_events);
    require_success(&alice, set_remote_answer, &mut alice_events);

    let mut alice_candidates = Vec::new();
    let mut bob_candidates = Vec::new();
    let mut alice_connected = false;
    let mut bob_connected = false;
    let mut alice_observed = Vec::new();
    let mut bob_observed = Vec::new();
    let mut delivered_packets = 0;
    for event in alice_events.drain(..) {
        alice_connected |= collect_candidate(event, &mut alice_candidates);
    }
    for event in bob_events.drain(..) {
        bob_connected |= collect_candidate(event, &mut bob_candidates);
    }

    for _ in 0..2_000_000 {
        while let Some(event) = alice.try_next_event() {
            alice_observed.push(event.clone());
            alice_connected |= collect_candidate(event, &mut alice_candidates);
        }
        while let Some(event) = bob.try_next_event() {
            bob_observed.push(event.clone());
            bob_connected |= collect_candidate(event, &mut bob_candidates);
        }
        for candidate in alice_candidates.drain(..) {
            bob.add_ice_candidate(candidate);
        }
        for candidate in bob_candidates.drain(..) {
            alice.add_ice_candidate(candidate);
        }
        while let Some(packet) = network.next_packet() {
            network.deliver(packet.id).unwrap();
            delivered_packets += 1;
        }
        if alice_connected && bob_connected {
            break;
        }
        clock.advance(std::time::Duration::from_millis(1)).unwrap();
        thread::yield_now();
    }
    assert!(
        alice_connected && bob_connected,
        "delivered {delivered_packets} packets\nalice events: {alice_observed:#?}\nbob events: {bob_observed:#?}"
    );

    alice.close().unwrap();
    bob.close().unwrap();
    alice.close().unwrap();
}

#[test]
fn invalid_inputs_closed_operations_and_construction_failures_are_explicit() {
    let factory = PeerConnectionFactory::builder().build().unwrap();
    assert!(
        factory
            .create_peer_connection(PeerConfiguration {
                ice_candidate_pool_size: u16::MAX,
                ..Default::default()
            })
            .is_err()
    );

    let mut peer = factory
        .create_peer_connection(PeerConfiguration::default())
        .unwrap();
    let mut events = Vec::new();
    let invalid_sdp = peer.set_remote_description(SessionDescription {
        kind: pulsebeam_webrtc_sys::SessionDescriptionType::Offer,
        sdp: "not SDP".into(),
    });
    let error = drain_until_completion(&peer, invalid_sdp, &mut events)
        .result
        .unwrap_err();
    assert_eq!(error.kind, PeerErrorKind::Syntax);

    let invalid_candidate = peer.add_ice_candidate(IceCandidate {
        sdp_mid: "0".into(),
        sdp_mline_index: 0,
        candidate: "not a candidate".into(),
    });
    let error = drain_until_completion(&peer, invalid_candidate, &mut events)
        .result
        .unwrap_err();
    assert_eq!(error.kind, PeerErrorKind::Syntax);

    peer.close().unwrap();
    peer.close().unwrap();
    let after_close = peer.create_offer();
    let error = drain_until_completion(&peer, after_close, &mut events)
        .result
        .unwrap_err();
    assert_eq!(error.kind, PeerErrorKind::Closed);

    while peer.try_next_event().is_some() {}
    for _ in 0..10_000 {
        thread::yield_now();
    }
    assert!(peer.try_next_event().is_none());
}

#[test]
fn repeated_partial_lifecycles_quiesce_on_drop() {
    for _ in 0..32 {
        let factory = PeerConnectionFactory::builder().build().unwrap();
        let mut peer = factory
            .create_peer_connection(PeerConfiguration::default())
            .unwrap();
        peer.create_offer();
        peer.close().unwrap();
        drop(factory);
        drop(peer);
    }
}
