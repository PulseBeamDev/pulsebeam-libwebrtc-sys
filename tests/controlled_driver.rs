use std::{collections::BTreeSet, net::Ipv4Addr, sync::Mutex, time::Duration};

#[cfg(target_os = "linux")]
fn native_thread_ids() -> BTreeSet<std::ffi::OsString> {
    std::fs::read_dir("/proc/self/task")
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect()
}

static DRIVER_TEST_LOCK: Mutex<()> = Mutex::new(());

use pulsebeam_webrtc_sys::{
    ControlledPeerDriver, ControlledSimulatedNetwork, DataChannelConfiguration, DataChannelEvent,
    DataChannelMessage, DataChannelSendResult, DataChannelState, Environment, IceGatheringState,
    IceServer, ManualClock, OperationId, OutboundKind, PeerConfiguration, PeerConnection,
    PeerConnectionEvent, PeerConnectionFactory, PeerErrorKind, RandomnessLease,
    RtpTransceiverDirection, SessionDescription, SessionDescriptionType, TaskQueueFactory,
};

#[test]
fn peers_share_a_caller_pumped_thread_and_reject_threaded_configurations() {
    let _serial = DRIVER_TEST_LOCK
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    let clock = ManualClock::new(Duration::from_secs(1)).unwrap();
    let queues = TaskQueueFactory::cooperative(&clock).unwrap();
    let environment = Environment::builder()
        .task_queue_factory(&queues)
        .build()
        .unwrap();
    let driver = ControlledPeerDriver::new(&clock).unwrap();
    assert!(ControlledPeerDriver::new(&clock).is_err());
    assert!(driver.run_ready());
    assert_eq!(driver.next_deadline(), None);

    let mismatched = ManualClock::new(Duration::from_secs(1)).unwrap();
    let wrong_queues = TaskQueueFactory::cooperative(&mismatched).unwrap();
    let wrong_environment = Environment::builder()
        .task_queue_factory(&wrong_queues)
        .build()
        .unwrap();
    assert!(
        PeerConnectionFactory::builder()
            .environment(wrong_environment)
            .controlled_driver(&driver)
            .build()
            .is_err()
    );
    #[cfg(feature = "native")]
    assert!(
        PeerConnectionFactory::builder()
            .environment(environment.clone())
            .controlled_driver(&driver)
            .native_audio(true)
            .build()
            .is_err()
    );

    #[cfg(target_os = "linux")]
    let threads_before = native_thread_ids();
    assert!(ControlledSimulatedNetwork::new(&mismatched, &driver).is_err());
    let network = ControlledSimulatedNetwork::new(&clock, &driver).unwrap();
    let endpoint = network
        .register_endpoint(Ipv4Addr::new(10, 30, 0, 1).into())
        .unwrap();
    let remote = network
        .register_endpoint(Ipv4Addr::new(10, 30, 0, 2).into())
        .unwrap();
    let remote_ip = Ipv4Addr::new(10, 30, 0, 2).into();
    assert!(network.add_dns_record("", remote_ip).is_err());
    network.add_dns_record("relay.test", remote_ip).unwrap();
    endpoint
        .packet_socket_factory()
        .unwrap()
        .require_dns()
        .unwrap();
    let mut sender = endpoint.bind_udp(4100).unwrap();
    let mut receiver = remote.bind_udp(4200).unwrap();
    sender
        .send_to(receiver.local_address(), b"late".to_vec())
        .unwrap();
    let late = network.next_packet().unwrap();
    sender
        .send_to(receiver.local_address(), b"early".to_vec())
        .unwrap();
    let early = network.next_packet().unwrap();
    network.deliver(early.id).unwrap();
    assert_eq!(receiver.try_receive().unwrap().payload, b"early");
    clock.advance(Duration::from_millis(7)).unwrap();
    network.deliver_copy(late.id).unwrap();
    network.deliver(late.id).unwrap();
    assert_eq!(receiver.try_receive().unwrap().payload, b"late");
    assert_eq!(receiver.try_receive().unwrap().payload, b"late");
    sender
        .send_to(receiver.local_address(), b"lost".to_vec())
        .unwrap();
    network
        .drop_packet(network.next_packet().unwrap().id)
        .unwrap();
    assert!(receiver.try_receive().is_none());
    sender.close();
    receiver.close();
    drop(sender);
    drop(receiver);
    assert!(
        PeerConnectionFactory::builder()
            .environment(environment.clone())
            .network_manager(endpoint.network_manager().unwrap())
            .packet_socket_factory(endpoint.packet_socket_factory().unwrap())
            .build()
            .is_err()
    );
    let factory = PeerConnectionFactory::builder()
        .environment(environment)
        .controlled_driver(&driver)
        .network_manager(endpoint.network_manager().unwrap())
        .packet_socket_factory(endpoint.packet_socket_factory().unwrap())
        .build()
        .unwrap();
    let mut peer = factory
        .create_peer_connection(PeerConfiguration::default())
        .unwrap();
    #[cfg(target_os = "linux")]
    assert!(
        native_thread_ids().is_subset(&threads_before),
        "constructing a controlled peer started an OS thread"
    );
    let operation = peer.create_offer().unwrap();
    let mut offer_completed = false;
    for _ in 0..200 {
        driver.run_ready();
        queues.run_ready();
        while let Some(event) = peer.try_next_event() {
            if let PeerConnectionEvent::OperationComplete(result) = event
                && result.operation_id == operation
            {
                assert!(result.result.is_ok(), "offer failed: {result:?}");
                offer_completed = true;
            }
        }
        if offer_completed {
            break;
        }
        if let Some(deadline) = [driver.next_deadline(), queues.next_deadline()]
            .into_iter()
            .flatten()
            .min()
            && deadline > clock.now()
        {
            clock.advance(deadline - clock.now()).unwrap();
        }
    }
    assert!(
        offer_completed,
        "the controlled peer did not produce an offer"
    );
    // The pinned upstream video receiver synchronously waits for a task on
    // its decode queue while applying remote SDP. The cooperative queue
    // cannot execute that task until this call returns, so reject instead.
    let source = factory.create_video_source().unwrap();
    let track = factory
        .create_video_track("unsupported-video", &source)
        .unwrap();
    let error = peer
        .add_video_transceiver(&track, RtpTransceiverDirection::SendOnly)
        .unwrap_err();
    assert_eq!(error.kind, PeerErrorKind::UnsupportedOperation);
    let audio_source = factory.create_audio_source().unwrap();
    let audio_track = factory
        .create_audio_track("unsupported-audio", &audio_source)
        .unwrap();
    let error = peer
        .add_audio_transceiver(&audio_track, RtpTransceiverDirection::SendOnly)
        .unwrap_err();
    assert_eq!(error.kind, PeerErrorKind::UnsupportedOperation);
    for media in ["audio", "video"] {
        for local in [true, false] {
            let description = SessionDescription {
                kind: SessionDescriptionType::Offer,
                sdp: format!("v=0\r\nm={media} 9 UDP/TLS/RTP/SAVPF 96\r\n"),
            };
            let rejected = if local {
                peer.set_local_description(description).unwrap()
            } else {
                peer.set_remote_description(description).unwrap()
            };
            let PeerConnectionEvent::OperationComplete(result) = peer.try_next_event().unwrap()
            else {
                panic!("expected controlled {media} rejection");
            };
            assert_eq!(result.operation_id, rejected);
            assert_eq!(
                result.result.unwrap_err().kind,
                PeerErrorKind::UnsupportedOperation
            );
        }
    }
    drop(audio_track);
    drop(audio_source);
    drop(track);
    drop(source);
    peer.close().unwrap();
    drop(peer);

    let mut dns_peer = factory
        .create_peer_connection(PeerConfiguration {
            ice_servers: vec![IceServer {
                urls: vec!["stun:relay.test:3478".into()],
                username: String::new(),
                password: String::new(),
            }],
            ..PeerConfiguration::default()
        })
        .unwrap();
    let mut events = Vec::new();
    let offer = completed(
        &dns_peer,
        dns_peer.create_offer().unwrap(),
        &mut events,
        &driver,
        &queues,
        &clock,
        &network,
    )
    .unwrap();
    let _operation = dns_peer.set_local_description(offer).unwrap();
    let mut resolved_stun = false;
    for _ in 0..10_000 {
        driver.run_ready();
        queues.run_ready();
        while let Some(packet) = network.next_packet() {
            if packet.destination.ip() == remote_ip && packet.destination.port() == 3478 {
                resolved_stun = true;
            }
            network.drop_packet(packet.id).unwrap();
        }
        if resolved_stun {
            break;
        }
        clock.advance(Duration::from_millis(1)).unwrap();
    }
    assert!(
        resolved_stun,
        "DNS answer did not reach the UDP STUN socket"
    );
    dns_peer.close().unwrap();
    drop(dns_peer);

    let mut tcp_peer = factory
        .create_peer_connection(PeerConfiguration {
            ice_servers: vec![IceServer {
                urls: vec!["turn:relay.test:3478?transport=tcp".into()],
                username: "client".into(),
                password: "secret".into(),
            }],
            ..PeerConfiguration::default()
        })
        .unwrap();
    let offer = completed(
        &tcp_peer,
        tcp_peer.create_offer().unwrap(),
        &mut events,
        &driver,
        &queues,
        &clock,
        &network,
    )
    .unwrap();
    let _operation = tcp_peer.set_local_description(offer).unwrap();
    let mut connected = None;
    let mut sent_tcp_data = false;
    for _ in 0..10_000 {
        driver.run_ready();
        queues.run_ready();
        while let Some(packet) = network.next_packet() {
            match packet.kind {
                OutboundKind::TcpConnect => {
                    assert_eq!(packet.destination.ip(), remote_ip);
                    assert_eq!(packet.destination.port(), 3478);
                    connected = Some((packet.destination, packet.source));
                    network.deliver(packet.id).unwrap();
                }
                OutboundKind::TcpData => {
                    assert!(connected.is_some());
                    assert!(!packet.payload.is_empty());
                    sent_tcp_data = true;
                    network.drop_packet(packet.id).unwrap();
                }
                OutboundKind::Udp => network.drop_packet(packet.id).unwrap(),
            }
        }
        if sent_tcp_data {
            break;
        }
        clock.advance(Duration::from_millis(1)).unwrap();
    }
    let (source, destination) = connected.expect("TURN TCP never attempted to connect");
    assert!(sent_tcp_data, "TURN TCP sent no bytes after connect");
    // The driver controls the inbound byte stream independently of the
    // outbound TURN request. Malformed server bytes must not crash the peer.
    network
        .inject_tcp_data(source, destination, &[0, 0, 0, 0])
        .unwrap();
    driver.run_ready();
    tcp_peer.close().unwrap();
    drop(tcp_peer);
    assert!(network.inject_tcp_data(source, destination, &[]).is_err());

    // TLS must run through the same caller-driven TCP byte stream. The
    // externally delivered connect releases a genuine client handshake; no
    // server certificate or TURN relay success is asserted by this fixture.
    let mut tls_peer = factory
        .create_peer_connection(PeerConfiguration {
            ice_servers: vec![IceServer {
                urls: vec!["turns:relay.test:5349?transport=tcp".into()],
                username: "client".into(),
                password: "secret".into(),
            }],
            ..PeerConfiguration::default()
        })
        .unwrap();
    let offer = completed(
        &tls_peer,
        tls_peer.create_offer().unwrap(),
        &mut events,
        &driver,
        &queues,
        &clock,
        &network,
    )
    .unwrap();
    let _operation = tls_peer.set_local_description(offer).unwrap();
    let mut tls_connect = false;
    let mut client_hello = false;
    for _ in 0..10_000 {
        driver.run_ready();
        queues.run_ready();
        while let Some(packet) = network.next_packet() {
            match packet.kind {
                OutboundKind::TcpConnect => {
                    assert_eq!(packet.destination.ip(), remote_ip);
                    assert_eq!(packet.destination.port(), 5349);
                    tls_connect = true;
                    network.deliver(packet.id).unwrap();
                }
                OutboundKind::TcpData => {
                    assert!(tls_connect);
                    // TLS handshake record (not plaintext TURN STUN framing).
                    if packet.payload.starts_with(&[0x16, 0x03]) {
                        client_hello = true;
                    }
                    network.drop_packet(packet.id).unwrap();
                }
                OutboundKind::Udp => network.drop_packet(packet.id).unwrap(),
            }
        }
        if client_hello {
            break;
        }
        clock.advance(Duration::from_millis(1)).unwrap();
    }
    assert!(tls_connect, "TURN/TLS never attempted to connect");
    assert!(client_hello, "TURN/TLS never sent a client handshake");
    tls_peer.close().unwrap();
    drop(tls_peer);
    drop(remote);

    let mut pending = factory
        .create_peer_connection(PeerConfiguration::default())
        .unwrap();
    let _operation = pending.create_offer().unwrap();
    pending.close().unwrap();
    drop(pending);
    driver.run_ready();
    queues.run_ready();
    drop(factory);
    drop(endpoint);
    drop(network);
    drop(driver);
    assert!(ControlledPeerDriver::new(&clock).is_ok());
}

fn pump(
    driver: &ControlledPeerDriver,
    queues: &TaskQueueFactory,
    clock: &ManualClock,
    network: &ControlledSimulatedNetwork,
) {
    assert!(driver.run_ready());
    queues.run_ready();
    while let Some(packet) = network.next_packet() {
        network.deliver(packet.id).unwrap();
    }
    let next = [driver.next_deadline(), queues.next_deadline()]
        .into_iter()
        .flatten()
        .min();
    let delta = next
        .and_then(|deadline| deadline.checked_sub(clock.now()))
        .filter(|duration| !duration.is_zero())
        .unwrap_or(Duration::from_millis(1));
    clock.advance(delta).unwrap();
}

fn completed(
    peer: &PeerConnection,
    operation: OperationId,
    events: &mut Vec<PeerConnectionEvent>,
    driver: &ControlledPeerDriver,
    queues: &TaskQueueFactory,
    clock: &ManualClock,
    network: &ControlledSimulatedNetwork,
) -> Option<SessionDescription> {
    for _ in 0..10_000 {
        while let Some(event) = peer.try_next_event() {
            if let PeerConnectionEvent::OperationComplete(result) = &event
                && result.operation_id == operation
            {
                return result.result.clone().unwrap();
            }
            events.push(event);
        }
        pump(driver, queues, clock, network);
    }
    panic!(
        "operation {} did not complete on the driver",
        operation.get()
    );
}

fn gathered(
    peer: &PeerConnection,
    events: &mut Vec<PeerConnectionEvent>,
    driver: &ControlledPeerDriver,
    queues: &TaskQueueFactory,
    clock: &ManualClock,
    network: &ControlledSimulatedNetwork,
) -> SessionDescription {
    for _ in 0..10_000 {
        while let Some(event) = peer.try_next_event() {
            events.push(event);
        }
        if events.iter().any(|event| {
            matches!(
                event,
                PeerConnectionEvent::IceGatheringStateChanged(IceGatheringState::Complete)
            )
        }) {
            let descriptions = peer.descriptions().unwrap();
            let sdp = descriptions
                .pending_local
                .or(descriptions.current_local)
                .unwrap();
            assert!(
                sdp.sdp.contains("a=candidate:"),
                "ICE produced no candidate"
            );
            return sdp;
        }
        pump(driver, queues, clock, network);
    }
    panic!("ICE gathering did not complete on the driver");
}

fn connection_trace(event: &PeerConnectionEvent) -> Option<String> {
    match event {
        PeerConnectionEvent::ConnectionStateChanged(state) => Some(format!("{state:?}")),
        _ => None,
    }
}

#[test]
fn two_peers_exchange_gathered_sdp_with_virtual_time_only() {
    let _serial = DRIVER_TEST_LOCK
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    assert_eq!(connect_with_seed(), connect_with_seed());
}

fn connect_with_seed() -> (
    Vec<String>,
    Vec<String>,
    Vec<DataChannelMessage>,
    Vec<DataChannelMessage>,
) {
    let clock = ManualClock::new(Duration::from_secs(1)).unwrap();
    let queues = TaskQueueFactory::cooperative(&clock).unwrap();
    let randomness = RandomnessLease::acquire(0x53eed).unwrap();
    let environment = Environment::builder()
        .task_queue_factory(&queues)
        .randomness(&randomness)
        .build()
        .unwrap();
    let driver = ControlledPeerDriver::new(&clock).unwrap();
    let network = ControlledSimulatedNetwork::new(&clock, &driver).unwrap();
    #[cfg(target_os = "linux")]
    let threads_before = native_thread_ids();
    let endpoint_a = network
        .register_endpoint(Ipv4Addr::new(10, 31, 0, 1).into())
        .unwrap();
    let endpoint_b = network
        .register_endpoint(Ipv4Addr::new(10, 31, 0, 2).into())
        .unwrap();
    let factory = |endpoint: &pulsebeam_webrtc_sys::NetworkEndpoint| {
        PeerConnectionFactory::builder()
            .environment(environment.clone())
            .controlled_driver(&driver)
            .network_manager(endpoint.network_manager().unwrap())
            .packet_socket_factory(endpoint.packet_socket_factory().unwrap())
            .build()
            .unwrap()
    };
    let factory_a = factory(&endpoint_a);
    let factory_b = factory(&endpoint_b);
    let mut alice = factory_a
        .create_peer_connection(PeerConfiguration::default())
        .unwrap();
    let mut bob = factory_b
        .create_peer_connection(PeerConfiguration::default())
        .unwrap();
    let mut alice_channel = alice
        .create_data_channel("controlled", DataChannelConfiguration::default())
        .unwrap();
    assert!(
        factory_a
            .create_peer_connection(PeerConfiguration {
                ice_servers: vec![IceServer {
                    urls: vec!["turns:127.0.0.1:5349".into()],
                    username: String::new(),
                    password: String::new()
                }],
                ..PeerConfiguration::default()
            })
            .is_err()
    );
    let mut alice_events = Vec::new();
    let mut bob_events = Vec::new();
    let offer = completed(
        &alice,
        alice.create_offer().unwrap(),
        &mut alice_events,
        &driver,
        &queues,
        &clock,
        &network,
    )
    .unwrap();
    completed(
        &alice,
        alice.set_local_description(offer).unwrap(),
        &mut alice_events,
        &driver,
        &queues,
        &clock,
        &network,
    );
    let offer = gathered(
        &alice,
        &mut alice_events,
        &driver,
        &queues,
        &clock,
        &network,
    );
    completed(
        &bob,
        bob.set_remote_description(offer).unwrap(),
        &mut bob_events,
        &driver,
        &queues,
        &clock,
        &network,
    );
    let answer = completed(
        &bob,
        bob.create_answer().unwrap(),
        &mut bob_events,
        &driver,
        &queues,
        &clock,
        &network,
    )
    .unwrap();
    completed(
        &bob,
        bob.set_local_description(answer).unwrap(),
        &mut bob_events,
        &driver,
        &queues,
        &clock,
        &network,
    );
    let answer = gathered(&bob, &mut bob_events, &driver, &queues, &clock, &network);
    completed(
        &alice,
        alice.set_remote_description(answer).unwrap(),
        &mut alice_events,
        &driver,
        &queues,
        &clock,
        &network,
    );
    let mut alice_trace: Vec<_> = alice_events.iter().filter_map(connection_trace).collect();
    let mut bob_trace: Vec<_> = bob_events.iter().filter_map(connection_trace).collect();
    let mut bob_channel = bob_events
        .iter()
        .position(|event| matches!(event, PeerConnectionEvent::DataChannel(_)))
        .map(|index| match bob_events.remove(index) {
            PeerConnectionEvent::DataChannel(channel) => channel,
            _ => unreachable!(),
        });
    let mut alice_connected = alice_trace.iter().any(|state| state == "Connected");
    let mut bob_connected = bob_trace.iter().any(|state| state == "Connected");
    for _ in 0..10_000 {
        while let Some(event) = alice.try_next_event() {
            if let Some(state) = connection_trace(&event) {
                alice_connected |= state == "Connected";
                alice_trace.push(state);
            }
        }
        while let Some(event) = bob.try_next_event() {
            if let PeerConnectionEvent::DataChannel(channel) = event {
                assert!(bob_channel.replace(channel).is_none());
            } else if let Some(state) = connection_trace(&event) {
                bob_connected |= state == "Connected";
                bob_trace.push(state);
            }
        }
        if alice_connected && bob_connected && bob_channel.is_some() {
            break;
        }
        pump(&driver, &queues, &clock, &network);
    }
    assert!(
        alice_connected && bob_connected,
        "ICE did not connect under virtual time"
    );
    let mut bob_channel = bob_channel.expect("remote data channel was not announced");
    for _ in 0..10_000 {
        if alice_channel.state() == DataChannelState::Open
            && bob_channel.state() == DataChannelState::Open
        {
            break;
        }
        pump(&driver, &queues, &clock, &network);
    }
    assert_eq!(alice_channel.state(), DataChannelState::Open);
    assert_eq!(bob_channel.state(), DataChannelState::Open);
    assert_eq!(
        alice_channel.send(DataChannelMessage::binary([1, 2, 3, 0])),
        DataChannelSendResult::Sent
    );
    assert_eq!(
        bob_channel.send(DataChannelMessage::text("from bob")),
        DataChannelSendResult::Sent
    );
    let mut received_by_alice = Vec::new();
    let mut received_by_bob = Vec::new();
    for _ in 0..10_000 {
        pump(&driver, &queues, &clock, &network);
        for event in std::iter::from_fn(|| alice_channel.try_next_event()) {
            if let DataChannelEvent::Message(message) = event {
                received_by_alice.push(message);
            }
        }
        for event in std::iter::from_fn(|| bob_channel.try_next_event()) {
            if let DataChannelEvent::Message(message) = event {
                received_by_bob.push(message);
            }
        }
        if !received_by_alice.is_empty() && !received_by_bob.is_empty() {
            break;
        }
    }
    assert_eq!(received_by_alice, [DataChannelMessage::text("from bob")]);
    assert_eq!(received_by_bob, [DataChannelMessage::binary([1, 2, 3, 0])]);
    #[cfg(target_os = "linux")]
    assert!(native_thread_ids().is_subset(&threads_before));
    alice_channel.close().unwrap();
    bob_channel.close().unwrap();
    alice.close().unwrap();
    bob.close().unwrap();
    (alice_trace, bob_trace, received_by_alice, received_by_bob)
}
