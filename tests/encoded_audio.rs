use std::{net::Ipv4Addr, thread, time::Duration};

#[path = "support/non_trickle.rs"]
mod non_trickle;

use pulsebeam_webrtc_sys::{
    AudioPcmFrame, Environment, ManualClock, OperationId, PeerConfiguration, PeerConnection,
    PeerConnectionEvent, PeerConnectionFactory, PeerErrorKind, RtpTransceiverDirection,
    SessionDescription, SimulatedNetwork,
};

fn finish(peer: &PeerConnection, id: OperationId) -> Option<SessionDescription> {
    for _ in 0..1_000_000 {
        while let Some(event) = peer.try_next_event() {
            if let PeerConnectionEvent::OperationComplete(done) = event
                && done.operation_id == id
            {
                return done.result.unwrap();
            }
        }
        thread::yield_now();
    }
    panic!("encoded audio operation did not complete");
}

#[test]
fn encoded_opus_receiver_gets_packets_without_decoded_sink() {
    let clock = ManualClock::new(Duration::from_secs(1)).unwrap();
    let environment = Environment::builder().clock(&clock).build().unwrap();
    let network = SimulatedNetwork::new(&clock).unwrap();
    let alice_endpoint = network
        .register_endpoint(Ipv4Addr::new(10, 8, 0, 1).into())
        .unwrap();
    let bob_endpoint = network
        .register_endpoint(Ipv4Addr::new(10, 8, 0, 2).into())
        .unwrap();
    let factory = |endpoint: &pulsebeam_webrtc_sys::NetworkEndpoint| {
        PeerConnectionFactory::builder()
            .environment(environment.clone())
            .network_manager(endpoint.network_manager().unwrap())
            .packet_socket_factory(endpoint.packet_socket_factory().unwrap())
            .build()
            .unwrap()
    };
    let alice_factory = factory(&alice_endpoint);
    let bob_factory = factory(&bob_endpoint);
    let mut alice = alice_factory
        .create_peer_connection(PeerConfiguration::default())
        .unwrap();
    let mut bob = bob_factory
        .create_peer_connection(PeerConfiguration::default())
        .unwrap();
    let source = alice_factory.create_audio_source().unwrap();
    let track = alice_factory.create_audio_track("voice", &source).unwrap();
    alice
        .add_audio_transceiver(&track, RtpTransceiverDirection::SendOnly)
        .unwrap();

    let offer = finish(&alice, alice.create_offer()).unwrap();
    finish(&alice, alice.set_local_description(offer));
    let gathered =
        non_trickle::gathered_local_description(&alice, &clock, &network, &mut Vec::new());
    finish(&bob, bob.set_remote_description(gathered));
    let answer = finish(&bob, bob.create_answer()).unwrap();
    finish(&bob, bob.set_local_description(answer));
    let gathered = non_trickle::gathered_local_description(&bob, &clock, &network, &mut Vec::new());
    finish(&alice, alice.set_remote_description(gathered));

    let receivers = bob.audio_receivers().unwrap();
    let mut sink = receivers[0].attach_encoded_audio_sink().unwrap();
    assert_eq!(
        receivers[0].attach_audio_sink().err().unwrap().kind,
        PeerErrorKind::InvalidState
    );
    assert_eq!(
        receivers[0].attach_encoded_audio_sink().err().unwrap().kind,
        PeerErrorKind::InvalidState
    );
    let frame = AudioPcmFrame::i16_interleaved(48_000, 1, 1_000_000, vec![1000; 480]).unwrap();
    let mut received = None;
    for tick in 0..100_000 {
        if tick % 10 == 0 {
            source.push_frame(&frame).unwrap();
        }
        while let Some(packet) = network.next_packet() {
            network.deliver(packet.id).unwrap();
        }
        while alice.try_next_event().is_some() {}
        while bob.try_next_event().is_some() {}
        received = sink.try_next_frame();
        if received.is_some() {
            break;
        }
        clock.advance(Duration::from_millis(1)).unwrap();
        thread::yield_now();
    }
    let received = received
        .unwrap_or_else(|| panic!("missing Opus packets, dropped={}", sink.dropped_frames()));
    assert!(!received.data.is_empty());
    assert!(received.samples_per_channel > 0);
    assert!(received.samples_per_channel <= 5760);
    assert_ne!(received.ssrc, 0);
    sink.close().unwrap();
    sink.close().unwrap();
    assert!(sink.try_next_frame().is_none());
    alice.close().unwrap();
    bob.close().unwrap();
}
