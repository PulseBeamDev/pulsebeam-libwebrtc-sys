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
    ConnectionState, ControlledPeerDriver, ControlledSimulatedNetwork, Environment,
    IceGatheringState, IceServer, ManualClock, OperationId, PeerConfiguration, PeerConnection,
    PeerConnectionEvent, PeerConnectionFactory, SessionDescription, TaskQueueFactory,
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
    drop(remote);
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
    let operation = peer.create_offer();
    let mut completed = false;
    for _ in 0..200 {
        driver.run_ready();
        queues.run_ready();
        while let Some(event) = peer.try_next_event() {
            if let PeerConnectionEvent::OperationComplete(result) = event
                && result.operation_id == operation
            {
                assert!(result.result.is_ok(), "offer failed: {result:?}");
                completed = true;
            }
        }
        if completed {
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
    assert!(completed, "the controlled peer did not produce an offer");
    peer.close().unwrap();
    drop(peer);
    let mut pending = factory
        .create_peer_connection(PeerConfiguration::default())
        .unwrap();
    let _operation = pending.create_offer();
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

#[test]
fn two_peers_exchange_gathered_sdp_with_virtual_time_only() {
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
    assert!(
        factory_a
            .create_peer_connection(PeerConfiguration {
                ice_servers: vec![IceServer {
                    urls: vec!["stun:127.0.0.1:3478".into()],
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
        alice.create_offer(),
        &mut alice_events,
        &driver,
        &queues,
        &clock,
        &network,
    )
    .unwrap();
    completed(
        &alice,
        alice.set_local_description(offer),
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
        bob.set_remote_description(offer),
        &mut bob_events,
        &driver,
        &queues,
        &clock,
        &network,
    );
    let answer = completed(
        &bob,
        bob.create_answer(),
        &mut bob_events,
        &driver,
        &queues,
        &clock,
        &network,
    )
    .unwrap();
    completed(
        &bob,
        bob.set_local_description(answer),
        &mut bob_events,
        &driver,
        &queues,
        &clock,
        &network,
    );
    let answer = gathered(&bob, &mut bob_events, &driver, &queues, &clock, &network);
    completed(
        &alice,
        alice.set_remote_description(answer),
        &mut alice_events,
        &driver,
        &queues,
        &clock,
        &network,
    );
    let mut alice_connected = alice_events.iter().any(|event| {
        matches!(
            event,
            PeerConnectionEvent::ConnectionStateChanged(ConnectionState::Connected)
        )
    });
    let mut bob_connected = bob_events.iter().any(|event| {
        matches!(
            event,
            PeerConnectionEvent::ConnectionStateChanged(ConnectionState::Connected)
        )
    });
    for _ in 0..10_000 {
        while let Some(event) = alice.try_next_event() {
            alice_connected |= matches!(
                event,
                PeerConnectionEvent::ConnectionStateChanged(ConnectionState::Connected)
            );
        }
        while let Some(event) = bob.try_next_event() {
            bob_connected |= matches!(
                event,
                PeerConnectionEvent::ConnectionStateChanged(ConnectionState::Connected)
            );
        }
        if alice_connected && bob_connected {
            break;
        }
        pump(&driver, &queues, &clock, &network);
    }
    assert!(
        alice_connected && bob_connected,
        "ICE did not connect under virtual time"
    );
    #[cfg(target_os = "linux")]
    assert!(native_thread_ids().is_subset(&threads_before));
    alice.close().unwrap();
    bob.close().unwrap();
}
