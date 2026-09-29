//! Demonstrates a caller-pumped peer, cooperative queue and externally routed
//! UDP packets. TCP and DNS outcomes are not available in controlled mode.
use pulsebeam_webrtc_sys::{
    ControlledPeerDriver, ControlledSimulatedNetwork, Environment, ManualClock, PeerConfiguration,
    PeerConnectionEvent, PeerConnectionFactory, QueuePriority, TaskQueueFactory,
};
use std::{
    net::{IpAddr, Ipv4Addr},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let clock = ManualClock::new(Duration::from_secs(1))?;
    let queues = TaskQueueFactory::cooperative(&clock)?;
    let mut queue = queues.create_queue("example", QueuePriority::Normal)?;
    let fired = Arc::new(AtomicBool::new(false));
    let from_task = fired.clone();
    assert!(queue.post_delayed(Duration::from_millis(5), move || {
        from_task.store(true, Ordering::SeqCst);
    })?);
    assert_eq!(queues.run_ready(), 0);
    clock.advance(Duration::from_millis(5))?;
    assert_eq!(queues.run_ready(), 1);
    assert!(fired.load(Ordering::SeqCst));

    let driver = ControlledPeerDriver::new(&clock)?;
    let network = ControlledSimulatedNetwork::new(&clock, &driver)?;
    let sender = network.register_endpoint(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)))?;
    let receiver = network.register_endpoint(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 2)))?;
    let mut tx = sender.bind_udp(4000)?;
    let mut rx = receiver.bind_udp(5000)?;
    tx.send_to(rx.local_address(), b"packet".to_vec())?;
    let pending = network
        .next_packet()
        .expect("send produces an outbound packet");
    assert!(rx.try_receive().is_none());
    network.deliver(pending.id)?;
    assert_eq!(rx.try_receive().unwrap().payload, b"packet");
    rx.close();
    tx.close();

    let environment = Environment::builder().task_queue_factory(&queues).build()?;
    let factory = PeerConnectionFactory::builder()
        .environment(environment)
        .controlled_driver(&driver)
        .network_manager(sender.network_manager()?)
        .packet_socket_factory(sender.packet_socket_factory()?)
        .build()?;
    let mut peer = factory.create_peer_connection(PeerConfiguration::default())?;
    let operation = peer.create_offer();
    let mut completed = false;
    for _ in 0..1000 {
        driver.run_ready();
        queues.run_ready();
        while let Some(event) = peer.try_next_event() {
            if let PeerConnectionEvent::OperationComplete(result) = event
                && result.operation_id == operation
            {
                result.result?;
                completed = true;
            }
        }
        if completed {
            break;
        }
        let deadline = [driver.next_deadline(), queues.next_deadline()]
            .into_iter()
            .flatten()
            .min();
        let step = deadline
            .and_then(|deadline| deadline.checked_sub(clock.now()))
            .filter(|delta| !delta.is_zero())
            .unwrap_or(Duration::from_millis(1));
        clock.advance(step)?;
    }
    assert!(
        completed,
        "offer did not complete under the controlled driver"
    );
    peer.close()?;
    queue.close();
    Ok(())
}
