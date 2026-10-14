use pulsebeam_webrtc_sys::*;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Default)]
struct Observed {
    created: AtomicUsize,
    starts: AtomicUsize,
    stops: AtomicUsize,
    info: AtomicUsize,
    drops: AtomicUsize,
    changes: Mutex<Option<NetworkChangeNotifier>>,
    threads: Mutex<Vec<std::thread::ThreadId>>,
}
struct Factory(Arc<Observed>);
struct Monitor(Arc<Observed>);
impl NetworkMonitorFactory for Factory {
    fn create(
        &self,
        trials: FieldTrialsSnapshot,
        changes: NetworkChangeNotifier,
    ) -> Option<Box<dyn NetworkMonitor>> {
        assert_eq!(trials.lookup("Monitor-Provider"), "EnabledOwned");
        assert!(
            !changes.notify(),
            "notification before Start must be rejected"
        );
        self.0.created.fetch_add(1, Ordering::SeqCst);
        self.0
            .threads
            .lock()
            .unwrap()
            .push(std::thread::current().id());
        *self.0.changes.lock().unwrap() = Some(changes);
        Some(Box::new(Monitor(self.0.clone())))
    }
}
impl NetworkMonitor for Monitor {
    fn start(&self) {
        self.0.starts.fetch_add(1, Ordering::SeqCst);
        self.0
            .threads
            .lock()
            .unwrap()
            .push(std::thread::current().id());
        assert!(self.0.changes.lock().unwrap().as_ref().unwrap().notify());
    }
    fn stop(&self) {
        self.0.stops.fetch_add(1, Ordering::SeqCst);
        self.0
            .threads
            .lock()
            .unwrap()
            .push(std::thread::current().id());
        assert!(!self.0.changes.lock().unwrap().as_ref().unwrap().notify());
    }
    fn interface_info(&self, _name: &[u8]) -> NetworkInterfaceInfo {
        self.0.info.fetch_add(1, Ordering::SeqCst);
        self.0
            .threads
            .lock()
            .unwrap()
            .push(std::thread::current().id());
        NetworkInterfaceInfo {
            adapter_type: NetworkAdapterType::Ethernet,
            underlying_type_for_vpn: NetworkAdapterType::Unknown,
            preference: NetworkPreference::NotPreferred,
            available: true,
            slice: NetworkSlice::UnifiedCommunications,
        }
    }
}
impl Drop for Monitor {
    fn drop(&mut self) {
        self.0
            .threads
            .lock()
            .unwrap()
            .push(std::thread::current().id());
        self.0.drops.fetch_add(1, Ordering::SeqCst);
    }
}
fn finish(
    session: &mut ProductionSession,
    peer: SessionPeerId,
    operation: OperationId,
) -> Option<SessionDescription> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        while let Some(event) = session.try_peer_event(peer).unwrap() {
            if let SessionEvent::OperationComplete(done) = event {
                if done.operation_id == operation {
                    return done.result.unwrap();
                }
            }
        }
        assert!(Instant::now() < deadline, "operation did not finish");
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn actor_monitor_owns_provider_and_retires_cross_thread_notifications() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<NetworkChangeNotifier>();
    let observed = Arc::new(Observed::default());
    let session = ProductionSession::builder(ProductionSessionConfig::default())
        .field_trials(FieldTrials::parse("Monitor-Provider/EnabledOwned/").unwrap())
        .network_monitor_factory(Factory(observed.clone()))
        .build()
        .unwrap();
    let observed_actor = observed.clone();
    let session = std::thread::spawn(move || {
        let mut session = session;
        let peer = session.create_peer(PeerConfiguration::default()).unwrap();
        let operation = session.create_offer(peer, false).unwrap();
        let offer = finish(&mut session, peer, operation).unwrap();
        let operation = session.set_local_description(peer, offer).unwrap();
        finish(&mut session, peer, operation);
        let deadline = Instant::now() + Duration::from_secs(15);
        while observed_actor.starts.load(Ordering::SeqCst) == 0 {
            assert!(
                Instant::now() < deadline,
                "native network monitor was not started"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        session
    })
    .join()
    .unwrap();
    assert_eq!(observed.created.load(Ordering::SeqCst), 1);
    let changes = observed.changes.lock().unwrap().as_ref().unwrap().clone();
    assert!(
        std::thread::spawn({
            let changes = changes.clone();
            move || changes.notify()
        })
        .join()
        .unwrap()
    );
    let deadline = Instant::now() + Duration::from_secs(15);
    while observed.info.load(Ordering::SeqCst) == 0 {
        assert!(
            Instant::now() < deadline,
            "interface metadata not requested"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    std::thread::spawn(move || {
        let mut session = session;
        session.shutdown().unwrap();
        drop(session);
    })
    .join()
    .unwrap();
    assert_eq!(observed.stops.load(Ordering::SeqCst), 1);
    assert_eq!(observed.drops.load(Ordering::SeqCst), 1);
    assert!(!changes.notify());
    std::thread::spawn(move || assert!(!changes.notify()))
        .join()
        .unwrap();
    let threads = observed.threads.lock().unwrap();
    assert!(threads.iter().all(|thread| *thread == threads[0]));
}
