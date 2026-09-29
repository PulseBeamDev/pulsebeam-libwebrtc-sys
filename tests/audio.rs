use std::{net::Ipv4Addr, thread, time::Duration};

#[path = "support/non_trickle.rs"]
mod non_trickle;

use pulsebeam_webrtc_sys::{
    AudioFrameError, AudioPcmFrame, Environment, ManualClock, OperationId, PeerConfiguration,
    PeerConnection, PeerConnectionEvent, PeerConnectionFactory, PeerErrorKind,
    RtpTransceiverDirection, SessionDescription, SimulatedNetwork,
};

fn finish(
    peer: &PeerConnection,
    id: OperationId,
    events: &mut Vec<PeerConnectionEvent>,
) -> Option<SessionDescription> {
    for _ in 0..1_000_000 {
        while let Some(event) = peer.try_next_event() {
            match event {
                PeerConnectionEvent::OperationComplete(done) if done.operation_id == id => {
                    return done.result.unwrap();
                }
                event => events.push(event),
            }
        }
        thread::yield_now();
    }
    panic!("audio operation did not complete");
}

#[test]
fn caller_fed_audio_track_rejects_foreign_sources_and_survives_source_owner_drop() {
    let clock = ManualClock::new(Duration::from_secs(1)).unwrap();
    let environment = Environment::builder().clock(&clock).build().unwrap();
    let network = SimulatedNetwork::new(&clock).unwrap();
    let endpoint_a = network
        .register_endpoint(Ipv4Addr::new(10, 6, 0, 1).into())
        .unwrap();
    let endpoint_b = network
        .register_endpoint(Ipv4Addr::new(10, 6, 0, 2).into())
        .unwrap();
    let build = |endpoint: &pulsebeam_webrtc_sys::NetworkEndpoint| {
        PeerConnectionFactory::builder()
            .environment(environment.clone())
            .network_manager(endpoint.network_manager().unwrap())
            .packet_socket_factory(endpoint.packet_socket_factory().unwrap())
            .build()
            .unwrap()
    };
    let factory = build(&endpoint_a);
    let other = build(&endpoint_b);
    let mut source = factory.create_audio_source().unwrap();
    assert_eq!(
        other
            .create_audio_track("foreign", &source)
            .err()
            .unwrap()
            .kind,
        PeerErrorKind::InvalidParameter
    );
    assert_eq!(
        factory.create_audio_track("", &source).err().unwrap().kind,
        PeerErrorKind::InvalidParameter
    );
    let frame = AudioPcmFrame::i16_interleaved(48_000, 1, 1_000_000, vec![0; 480]).unwrap();
    source.push_frame(&frame).unwrap();
    let mut malformed = frame.clone();
    malformed.samples.pop();
    assert_eq!(
        source.push_frame(&malformed),
        Err(AudioFrameError::InvalidSampleCount)
    );
    let track = factory.create_audio_track("mic", &source).unwrap();
    assert_eq!(track.id(), "mic");
    assert!(track.enabled());
    track.set_enabled(false);
    assert!(!track.enabled());
    track.set_enabled(true);

    let mut peer = factory
        .create_peer_connection(PeerConfiguration::default())
        .unwrap();
    assert_eq!(
        other
            .create_peer_connection(PeerConfiguration::default())
            .unwrap()
            .add_audio_transceiver(&track, RtpTransceiverDirection::SendOnly)
            .err()
            .unwrap()
            .kind,
        PeerErrorKind::InvalidParameter
    );
    let transceiver = peer
        .add_audio_transceiver(&track, RtpTransceiverDirection::SendOnly)
        .unwrap();
    assert!(!transceiver.sender().id().is_empty());
    source.push_frame(&frame).unwrap();
    source.close().unwrap();
    assert_eq!(source.push_frame(&frame), Err(AudioFrameError::Released));
    drop(source);
    peer.remove_track(&transceiver.sender()).unwrap();
    peer.close().unwrap();
    drop(track);
    drop(peer);
    drop(factory);
}

#[test]
fn decoded_audio_sink_receives_headless_pcm_over_negotiated_peer() {
    let clock = ManualClock::new(Duration::from_secs(1)).unwrap();
    let environment = Environment::builder().clock(&clock).build().unwrap();
    let network = SimulatedNetwork::new(&clock).unwrap();
    let alice_endpoint = network
        .register_endpoint(Ipv4Addr::new(10, 7, 0, 1).into())
        .unwrap();
    let bob_endpoint = network
        .register_endpoint(Ipv4Addr::new(10, 7, 0, 2).into())
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
    assert_eq!(alice.audio_transceivers().unwrap().len(), 1);
    let mut alice_events = Vec::new();
    let mut bob_events = Vec::new();
    let offer = finish(&alice, alice.create_offer(), &mut alice_events).unwrap();
    assert!(offer.sdp.contains("m=audio"));
    finish(
        &alice,
        alice.set_local_description(offer),
        &mut alice_events,
    );
    let gathered =
        non_trickle::gathered_local_description(&alice, &clock, &network, &mut alice_events);
    finish(&bob, bob.set_remote_description(gathered), &mut bob_events);
    let answer = finish(&bob, bob.create_answer(), &mut bob_events).unwrap();
    finish(&bob, bob.set_local_description(answer), &mut bob_events);
    let gathered = non_trickle::gathered_local_description(&bob, &clock, &network, &mut bob_events);
    finish(
        &alice,
        alice.set_remote_description(gathered),
        &mut alice_events,
    );
    let receivers = bob.audio_receivers().unwrap();
    assert_eq!(receivers.len(), 1);
    let mut sink = receivers[0].attach_audio_sink().unwrap();
    let frame = AudioPcmFrame::i16_interleaved(48_000, 1, 1_000_000, vec![1000; 480]).unwrap();
    let mut received = None;
    let mut alice_connected = false;
    let mut bob_connected = false;
    for tick in 0..100_000 {
        if tick % 10 == 0 {
            source.push_frame(&frame).unwrap();
        }
        while let Some(packet) = network.next_packet() {
            network.deliver(packet.id).unwrap();
        }
        while let Some(event) = alice.try_next_event() {
            alice_connected |= matches!(
                event,
                PeerConnectionEvent::ConnectionStateChanged(
                    pulsebeam_webrtc_sys::ConnectionState::Connected
                )
            );
        }
        while let Some(event) = bob.try_next_event() {
            bob_connected |= matches!(
                event,
                PeerConnectionEvent::ConnectionStateChanged(
                    pulsebeam_webrtc_sys::ConnectionState::Connected
                )
            );
        }
        received = sink.try_next_frame();
        if received.is_some() {
            break;
        }
        clock.advance(Duration::from_millis(1)).unwrap();
        thread::yield_now();
    }
    let received = received.unwrap_or_else(|| {
        let snapshot = |peer: &PeerConnection| {
            let operation = peer.request_stats().unwrap();
            for _ in 0..1_000_000 {
                if let Some(PeerConnectionEvent::Stats(stats)) = peer.try_next_event()
                    && stats.operation_id == operation
                {
                    return Some(stats);
                }
                thread::yield_now();
            }
            None
        };
        panic!(
            "headless audio missing: alice_connected={alice_connected}, bob_connected={bob_connected}, dropped={}, alice={:?}, bob={:?}",
            sink.dropped_frames(), snapshot(&alice), snapshot(&bob)
        )
    });
    assert_eq!(received.channels, 1);
    assert_eq!(received.sample_rate_hz, 48_000);
    assert_eq!(
        received.samples.len(),
        received.samples_per_channel as usize
    );
    sink.close().unwrap();
    sink.close().unwrap();
    assert!(sink.try_next_frame().is_none());
    alice.close().unwrap();
    bob.close().unwrap();
}
