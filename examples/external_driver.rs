//! Common session operations with caller-owned progress and packet delivery.
//! This setup example does not qualify a complete deterministic media call.
use pulsebeam_webrtc_sys::{
    ControlledWorld, MediaType, OperationId, PeerConfiguration, QueuePriority,
    RtpTransceiverDirectionValue, RtpTransceiverInit, Session, SessionConfig, SessionEvent,
    SessionPeerId, SessionRuntime,
};
use std::{
    net::{IpAddr, Ipv4Addr},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

// The application procedure has no real/managed branch or encoded-source gate.
fn application<R: SessionRuntime>(
    runtime: R,
) -> Result<(Session<R>, SessionPeerId, OperationId), Box<dyn std::error::Error>> {
    let mut session = Session::with_runtime(runtime, SessionConfig::default()).build()?;
    let peer = session.create_peer(PeerConfiguration::default())?;
    let mut receive = RtpTransceiverInit::default();
    receive.set_direction(RtpTransceiverDirectionValue::ReceiveOnly);
    session.add_transceiver(peer, MediaType::Audio, &receive)?;
    session.add_transceiver(peer, MediaType::Video, &receive)?;
    let offer = session.create_offer(peer, false)?;
    Ok((session, peer, offer))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let world = ControlledWorld::acquire(17, Duration::from_secs(1))?;
    let mut queue = world.create_queue("example", QueuePriority::Normal)?;
    let fired = Arc::new(AtomicBool::new(false));
    let from_task = fired.clone();
    assert!(queue.post_delayed(Duration::from_millis(5), move || {
        from_task.store(true, Ordering::SeqCst);
    })?);
    assert_eq!(world.pump(1).dispatched, 0);
    world.advance(Duration::from_millis(5))?;
    assert_eq!(world.pump(1).dispatched, 1);
    assert!(fired.load(Ordering::SeqCst));

    let network = world.create_network()?;
    let sender = network.register_endpoint(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)))?;
    let receiver = network.register_endpoint(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 2)))?;
    let mut tx = sender.bind_udp(4000)?;
    let mut rx = receiver.bind_udp(5000)?;
    tx.send_to(rx.local_address(), b"packet".to_vec())?;
    let pending = network.next_packet().expect("outbound packet");
    assert!(rx.try_receive().is_none());
    network.deliver(pending.id)?;
    assert_eq!(rx.try_receive().unwrap().payload, b"packet");
    rx.close();
    tx.close();

    let runtime = world.runtime()?.with_endpoint(&sender)?;
    let (mut session, peer, operation) = application(runtime)?;
    let mut completed = false;
    for _ in 0..1000 {
        assert!(!world.pump(64).blocked);
        while let Some(event) = session.try_peer_event(peer)? {
            if let SessionEvent::OperationComplete(result) = event
                && result.operation_id == operation
            {
                let offer = result.result?.expect("native offer");
                assert!(offer.sdp.contains("m=audio "));
                assert!(offer.sdp.contains("m=video "));
                completed = true;
            }
        }
        if completed {
            break;
        }
        if let Some(deadline) = world.next_deadline()
            && deadline > world.now()
        {
            world.advance(deadline - world.now())?;
        }
    }
    assert!(
        completed,
        "native offer did not complete under caller progress"
    );
    session.close_peer(peer)?;
    session.shutdown()?;
    drop(session);
    queue.close();
    Ok(())
}
