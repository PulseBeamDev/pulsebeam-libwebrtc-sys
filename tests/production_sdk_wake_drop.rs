use pulsebeam_webrtc_sys::*;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Wake, Waker},
    time::{Duration, Instant},
};
struct Marker(Arc<AtomicBool>);
impl Drop for Marker {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}
#[derive(Clone)]
struct Trials(Arc<Marker>);
impl FieldTrialsView for Trials {
    fn lookup(&self, _: &str) -> String {
        let _ = &self.0;
        String::new()
    }
    fn create_copy(&self) -> Arc<dyn FieldTrialsView> {
        Arc::new(self.clone())
    }
}
struct DropOwner {
    owner: Arc<Mutex<Option<ProductionSession>>>,
    caller: std::thread::ThreadId,
    sdk_wake: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
}
impl Wake for DropOwner {
    fn wake(self: Arc<Self>) {
        if let Some(owner) = self.owner.lock().unwrap().take() {
            self.sdk_wake
                .store(std::thread::current().id() != self.caller, Ordering::SeqCst);
            drop(owner);
            self.finished.store(true, Ordering::SeqCst);
        }
    }
}

// SDK roles cannot be synchronously retired by their own callback. Exercise
// an actual native signaling fence, not a caller-thread notification.
#[test]
fn final_session_owner_can_drop_from_the_actual_sdk_readiness_callback() {
    let released = Arc::new(AtomicBool::new(false));
    let marker = Arc::new(Marker(released.clone()));
    let weak = Arc::downgrade(&marker);
    let mut session = ProductionSession::builder(Default::default())
        .field_trials(Trials(marker))
        .build()
        .unwrap();
    let peer = session.create_peer(Default::default()).unwrap();
    // Retire setup tasks and consume a real SDK result before registering the
    // final-owner waker. The next legacy result is delivered by the SDK fence,
    // after its synchronous initiating proxy call has returned to the caller.
    let warmup = session
        .request_legacy_stats(peer, LegacyStatsOutputLevel::Standard)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if matches!(session.try_peer_event(peer).unwrap(),Some(SessionEvent::LegacyStats(value)) if value.operation_id == warmup)
        {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    while session.try_peer_event(peer).unwrap().is_some() {}
    let owner = Arc::new(Mutex::new(Some(session)));
    let finished = Arc::new(AtomicBool::new(false));
    let sdk_wake = Arc::new(AtomicBool::new(false));
    let waker = Waker::from(Arc::new(DropOwner {
        owner: owner.clone(),
        caller: std::thread::current().id(),
        sdk_wake: sdk_wake.clone(),
        finished: finished.clone(),
    }));
    {
        let mut guard = owner.lock().unwrap();
        let session = guard.as_mut().unwrap();
        while session
            .poll_ready(&mut Context::from_waker(&waker))
            .is_ready()
        {}
        session
            .request_legacy_stats(peer, LegacyStatsOutputLevel::Debug)
            .unwrap();
    }
    while !finished.load(Ordering::SeqCst) || !released.load(Ordering::SeqCst) {
        assert!(
            Instant::now() < deadline,
            "SDK-callback cleanup did not release its origin environment"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        sdk_wake.load(Ordering::SeqCst),
        "fixture did not enter the SDK callback thread"
    );
    assert!(owner.lock().unwrap().is_none());
    assert!(weak.upgrade().is_none());
    // The production lease drops after native factory, role threads and
    // providers. Reacquisition proves deferred teardown actually finished.
    loop {
        match ControlledWorld::acquire(944, Duration::from_secs(4)) {
            Ok(world) => {
                assert_eq!(world.now(), Duration::from_secs(4));
                break;
            }
            Err(_) => {
                assert!(
                    Instant::now() < deadline,
                    "owned SDK roles were retained after cleanup"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
}
