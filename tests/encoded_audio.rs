use std::{net::Ipv4Addr, thread, time::Duration};

#[path = "support/non_trickle.rs"]
mod non_trickle;

use pulsebeam_webrtc_sys::{
    AudioEncoderFactory, AudioPcmFrame, ConnectionState, Environment, ManualClock, OperationId,
    OpusInputError, OpusInputFrame, PeerConfiguration, PeerConnection, PeerConnectionEvent,
    PeerConnectionFactory, PeerErrorKind, RtpTransceiverDirection, SessionDescription,
    SimulatedNetwork,
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
    assert!(received.sequence_number.is_some());
    assert!(received.audio_level_dbov.is_some_and(|level| level <= 127));
    sink.close().unwrap();
    sink.close().unwrap();
    assert!(sink.try_next_frame().is_none());
    alice.close().unwrap();
    bob.close().unwrap();
}

#[test]
fn mono_and_stereo_opus_sources_preserve_distinct_payloads_without_encoding() {
    #[cfg(feature = "native")]
    assert_eq!(
        PeerConnectionFactory::builder()
            .audio_encoder_factory(AudioEncoderFactory::with_opus_frames().unwrap())
            .native_audio(true)
            .build()
            .err()
            .unwrap()
            .kind,
        PeerErrorKind::InvalidParameter
    );
    let clock = ManualClock::new(Duration::from_secs(1)).unwrap();
    let environment = Environment::builder().clock(&clock).build().unwrap();
    let network = SimulatedNetwork::new(&clock).unwrap();
    let a = network
        .register_endpoint(Ipv4Addr::new(10, 9, 0, 1).into())
        .unwrap();
    let b = network
        .register_endpoint(Ipv4Addr::new(10, 9, 0, 2).into())
        .unwrap();
    let alice_factory = PeerConnectionFactory::builder()
        .environment(environment.clone())
        .network_manager(a.network_manager().unwrap())
        .packet_socket_factory(a.packet_socket_factory().unwrap())
        .audio_encoder_factory(AudioEncoderFactory::with_opus_frames().unwrap())
        .build()
        .unwrap();
    let bob_factory = PeerConnectionFactory::builder()
        .environment(environment)
        .network_manager(b.network_manager().unwrap())
        .packet_socket_factory(b.packet_socket_factory().unwrap())
        .build()
        .unwrap();
    assert_eq!(
        bob_factory
            .create_encoded_audio_source(1)
            .err()
            .unwrap()
            .kind,
        PeerErrorKind::UnsupportedOperation
    );
    let mut alice = alice_factory
        .create_peer_connection(PeerConfiguration::default())
        .unwrap();
    let mut bob = bob_factory
        .create_peer_connection(PeerConfiguration::default())
        .unwrap();
    assert_eq!(
        alice_factory.create_audio_source().err().unwrap().kind,
        PeerErrorKind::UnsupportedOperation
    );
    let first = alice_factory.create_encoded_audio_source(1).unwrap();
    let second = alice_factory.create_encoded_audio_source(2).unwrap();
    assert_eq!((first.channels(), second.channels()), (1, 2));
    assert_eq!(
        alice_factory
            .create_encoded_audio_source(3)
            .err()
            .unwrap()
            .kind,
        PeerErrorKind::InvalidParameter
    );
    for (data, duration, expected) in [
        (
            vec![0xfc, 0x01],
            960,
            pulsebeam_webrtc_sys::OpusInputError::InvalidPacket,
        ),
        (
            vec![0xf8, 0x01],
            480,
            pulsebeam_webrtc_sys::OpusInputError::InvalidDuration,
        ),
        (
            vec![0xf8; 1201],
            960,
            pulsebeam_webrtc_sys::OpusInputError::InvalidPacket,
        ),
    ] {
        assert_eq!(
            first.push_opus(&OpusInputFrame {
                data,
                rtp_timestamp: 0,
                samples_per_channel: duration,
            }),
            Err(expected)
        );
    }
    assert_eq!(
        first.push_opus(&OpusInputFrame {
            data: vec![0xf8, 0xff, 0xfe],
            rtp_timestamp: 0,
            samples_per_channel: 960,
        }),
        Err(pulsebeam_webrtc_sys::OpusInputError::Backpressure)
    );
    let track_a = alice_factory
        .create_encoded_audio_track("opus-a", &first)
        .unwrap();
    let track_b = alice_factory
        .create_encoded_audio_track("opus-b", &second)
        .unwrap();
    let capabilities = alice.audio_sender_capabilities().unwrap();
    // Mono/stereo share one RFC 7587 RTP codec identity. Stereo is a
    // receiver preference, not a second codec/payload type advertisement.
    let opus: Vec<_> = capabilities
        .iter()
        .filter(|codec| codec.name().eq_ignore_ascii_case("opus"))
        .cloned()
        .collect();
    assert_eq!(opus.len(), 1, "duplicate Opus RTP advertisements");
    let opus = &opus[0];
    alice
        .add_audio_transceiver(&track_a, RtpTransceiverDirection::SendOnly)
        .unwrap()
        .set_audio_codec_preferences(&[opus.clone()])
        .unwrap();
    alice
        .add_audio_transceiver(&track_b, RtpTransceiverDirection::SendOnly)
        .unwrap()
        .set_audio_codec_preferences(&[opus.clone()])
        .unwrap();

    let offer = finish(&alice, alice.create_offer()).unwrap();
    finish(&alice, alice.set_local_description(offer));
    let gathered =
        non_trickle::gathered_local_description(&alice, &clock, &network, &mut Vec::new());
    finish(&bob, bob.set_remote_description(gathered));
    let mut answer = finish(&bob, bob.create_answer()).unwrap();
    // The receiver explicitly requests stereo on the second m-line. An
    // ordinary built-in answer defaults to mono even when it can decode stereo.
    let sections: Vec<_> = answer.sdp.split("m=audio").collect();
    assert_eq!(sections.len(), 3);
    answer.sdp = format!(
        "{}m=audio{}m=audio{}",
        sections[0],
        sections[1],
        sections[2].replacen("useinbandfec=1", "useinbandfec=1;stereo=1", 1)
    );
    finish(&bob, bob.set_local_description(answer));
    let gathered = non_trickle::gathered_local_description(&bob, &clock, &network, &mut Vec::new());
    finish(&alice, alice.set_remote_description(gathered));

    let receivers = bob.audio_receivers().unwrap();
    assert_eq!(receivers.len(), 2);
    let sinks: Vec<_> = receivers
        .iter()
        .map(|r| r.attach_encoded_audio_sink().unwrap())
        .collect();
    let mut connected = [false; 2];
    for _ in 0..100_000 {
        while let Some(packet) = network.next_packet() {
            network.deliver(packet.id).unwrap();
        }
        for (index, peer) in [&alice, &bob].into_iter().enumerate() {
            while let Some(event) = peer.try_next_event() {
                if let PeerConnectionEvent::ConnectionStateChanged(ConnectionState::Connected) =
                    event
                {
                    connected[index] = true;
                }
            }
        }
        if connected == [true; 2] {
            break;
        }
        clock.advance(Duration::from_millis(1)).unwrap();
        thread::yield_now();
    }
    assert_eq!(
        connected, [true; 2],
        "peers must connect before pushing Opus"
    );
    let packets = [vec![0xf8, 0xff, 0xfe], vec![0xfc, 0x12, 0x34, 0x56]];
    let mut seen = [false; 2];
    let mut sink_stream = [None; 2];
    let mut last_push_timestamp = [0; 2];
    let mut pending: [Option<OpusInputFrame>; 2] = [None, None];
    for tick in 0..100_000u32 {
        for (index, source) in [&first, &second].into_iter().enumerate() {
            if tick % 20 == 0 && pending[index].is_none() {
                pending[index] = Some(OpusInputFrame {
                    data: packets[index].clone(),
                    rtp_timestamp: tick * 48,
                    samples_per_channel: 960,
                });
            }
            if let Some(frame) = &pending[index] {
                match source.push_opus(frame) {
                    Ok(()) => {
                        last_push_timestamp[index] = frame.rtp_timestamp;
                        pending[index] = None;
                    }
                    // A threaded encoder may lag virtual capture time. Retry
                    // the same unadmitted packet after transport/clock progress,
                    // without duplicating successful sends or expanding credits.
                    Err(OpusInputError::Backpressure) => {}
                    Err(error) => panic!(
                        "Opus push failed at tick={tick} channels={} error={error:?}",
                        source.channels()
                    ),
                }
            }
        }
        while let Some(packet) = network.next_packet() {
            network.deliver(packet.id).unwrap();
        }
        while alice.try_next_event().is_some() {}
        while bob.try_next_event().is_some() {}
        for (sink_index, sink) in sinks.iter().enumerate() {
            while let Some(frame) = sink.try_next_frame() {
                let stream = packets
                    .iter()
                    .position(|payload| frame.data == *payload)
                    .expect("foreign or re-encoded payload");
                if let Some((previous, ssrc)) = sink_stream[sink_index] {
                    assert_eq!((stream, frame.ssrc), (previous, ssrc));
                } else {
                    sink_stream[sink_index] = Some((stream, frame.ssrc));
                }
                seen[stream] = true;
                assert_eq!(frame.samples_per_channel, 960);
            }
        }
        if seen == [true; 2] {
            break;
        }
        clock.advance(Duration::from_millis(1)).unwrap();
        thread::yield_now();
    }
    assert_eq!(seen, [true; 2], "both sources must arrive byte-identically");
    assert_ne!(sink_stream[0], sink_stream[1], "tracks must stay isolated");

    let variants = [
        (vec![0xf0, 0x11], 480),
        (vec![0xff, 3, 0x11, 0x12, 0x13], 2880),
    ];
    for (index, source) in [&first, &second].into_iter().enumerate() {
        let (data, samples_per_channel) = &variants[index];
        let frame = OpusInputFrame {
            data: data.clone(),
            rtp_timestamp: last_push_timestamp[index] + 960,
            samples_per_channel: *samples_per_channel,
        };
        let mut admitted = false;
        for _ in 0..100_000 {
            match source.push_opus(&frame) {
                Ok(()) => {
                    admitted = true;
                    break;
                }
                Err(OpusInputError::Backpressure) => {}
                Err(error) => panic!("Opus variant admission failed: {error:?}"),
            }
            while let Some(packet) = network.next_packet() {
                network.deliver(packet.id).unwrap();
            }
            while alice.try_next_event().is_some() {}
            while bob.try_next_event().is_some() {}
            clock.advance(Duration::from_millis(1)).unwrap();
            thread::yield_now();
        }
        assert!(admitted, "Opus variant never admitted");
    }
    let mut variant_seen = [false; 2];
    for _ in 0..20_000 {
        while let Some(packet) = network.next_packet() {
            network.deliver(packet.id).unwrap();
        }
        while alice.try_next_event().is_some() {}
        while bob.try_next_event().is_some() {}
        for (sink_index, sink) in sinks.iter().enumerate() {
            while let Some(frame) = sink.try_next_frame() {
                if frame.data == variants[0].0 {
                    assert_eq!(frame.samples_per_channel, 480);
                    assert_eq!(sink_stream[sink_index].unwrap().0, 0);
                    variant_seen[0] = true;
                } else if frame.data == variants[1].0 {
                    assert_eq!(frame.samples_per_channel, 2880);
                    assert_eq!(sink_stream[sink_index].unwrap().0, 1);
                    variant_seen[1] = true;
                }
            }
        }
        if variant_seen == [true; 2] {
            break;
        }
        clock.advance(Duration::from_millis(1)).unwrap();
        thread::yield_now();
    }
    assert_eq!(
        variant_seen, [true; 2],
        "10 ms and 60 ms Opus must arrive intact"
    );
    alice.close().unwrap();
    bob.close().unwrap();
}
