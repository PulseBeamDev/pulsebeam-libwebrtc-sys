use std::{thread, time::Duration};

use pulsebeam_webrtc_sys::{
    IceGatheringState, ManualClock, PeerConnection, PeerConnectionEvent, SessionDescription,
    SimulatedNetwork,
};

/// Wait for native ICE gathering, then send the *gathered* SDP rather than
/// forwarding per-candidate events. Preserve unrelated events for the caller.
pub fn gathered_local_description(
    peer: &PeerConnection,
    clock: &ManualClock,
    network: &SimulatedNetwork,
    events: &mut Vec<PeerConnectionEvent>,
) -> SessionDescription {
    let mut complete = events.iter().any(|event| {
        matches!(
            event,
            PeerConnectionEvent::IceGatheringStateChanged(IceGatheringState::Complete)
        )
    });
    for _ in 0..10_000 {
        while let Some(event) = peer.try_next_event() {
            complete |= matches!(
                event,
                PeerConnectionEvent::IceGatheringStateChanged(IceGatheringState::Complete)
            );
            if !matches!(event, PeerConnectionEvent::IceCandidate(_)) {
                events.push(event);
            }
        }
        while let Some(packet) = network.next_packet() {
            network.deliver(packet.id).unwrap();
        }
        if complete {
            let descriptions = peer.descriptions().unwrap();
            let description = descriptions
                .pending_local
                .or(descriptions.current_local)
                .expect("gathering completed without a local description");
            assert!(
                description.sdp.contains("a=candidate:"),
                "gathered SDP must carry the candidates used for non-trickle signaling"
            );
            return description;
        }
        clock.advance(Duration::from_millis(1)).unwrap();
        // ICE allocation also uses RTC worker-thread timers, not only the
        // simulated network clock. Give those threads real time to run.
        thread::sleep(Duration::from_millis(1));
    }
    panic!(
        "ICE gathering did not complete under the virtual clock: events={events:#?}, descriptions={:?}",
        peer.descriptions()
    );
}
