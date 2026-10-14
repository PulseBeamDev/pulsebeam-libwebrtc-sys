use pulsebeam_webrtc_sys::{
    Environment, EventLogEnvironment, EventLogOutput, EventLogOutputPeriod, FieldTrials,
    ManualClock, PeerConfiguration, PeerConnectionFactory, RandomnessLease, RandomnessLeaseError,
    RtcEvent, RtcEventLogFactory, RtcEventLogger,
};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

struct Factory(Arc<Mutex<Vec<EventLogEnvironment>>>);
struct Logger;
impl RtcEventLogFactory for Factory {
    fn create(&self, environment: EventLogEnvironment) -> Box<dyn RtcEventLogger> {
        self.0.lock().unwrap().push(environment);
        Box::new(Logger)
    }
}
impl RtcEventLogger for Logger {
    fn start(&self, _: EventLogOutput, _: EventLogOutputPeriod) -> bool {
        false
    }
    fn stop(&self) {}
    fn log(&self, _: RtcEvent) {}
}

#[test]
fn installed_creation_context_retains_rust_leases_after_native_peer_retirement() {
    let seed = 404;
    let lease = RandomnessLease::acquire(seed).unwrap();
    let clock = ManualClock::new(Duration::from_secs(10)).unwrap();
    let contexts = Arc::new(Mutex::new(Vec::new()));
    let environment = Environment::builder()
        .clock(&clock)
        .randomness(&lease)
        .field_trials(FieldTrials::parse("Provider-Context/Enabled/").unwrap())
        .build()
        .unwrap();
    let factory = PeerConnectionFactory::builder()
        .environment(environment)
        .event_log_factory(Factory(contexts.clone()))
        .build()
        .unwrap();
    let mut peer = factory
        .create_peer_connection(PeerConfiguration::default())
        .unwrap();
    assert_eq!(
        contexts.lock().unwrap().len(),
        1,
        "selected factory was not called"
    );
    peer.close().unwrap();
    drop(peer);
    drop(factory);
    drop(lease);

    assert_eq!(
        RandomnessLease::acquire(seed).err(),
        Some(RandomnessLeaseError::AlreadyAcquired)
    );
    let context = contexts.lock().unwrap().pop().unwrap();
    let alias = context.clone();
    let environment = context.environment().clone();
    drop(context);
    clock.advance(Duration::from_micros(7)).unwrap();
    std::thread::spawn(move || {
        assert_eq!(alias.timestamp().microseconds().unwrap(), 10_000_007);
        assert!(alias.field_trials().is_enabled("Provider-Context"));
        assert_eq!(
            alias.environment().clock_time_bounds(),
            environment.clock_time_bounds()
        );
        drop(alias);
        assert_eq!(
            RandomnessLease::acquire(seed).err(),
            Some(RandomnessLeaseError::AlreadyAcquired)
        );
        drop(environment);
    })
    .join()
    .unwrap();
    // Only the final context/Environment alias, not clock metadata or a hidden
    // global registry, keeps the exclusive lease installed.
    drop(RandomnessLease::acquire(seed).unwrap());
}
