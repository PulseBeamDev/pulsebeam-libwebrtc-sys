use pulsebeam_webrtc_sys::{
    Clock, ControlledWorld, Environment, NtpTime, ProductionSession, ProductionSessionConfig,
    RealTimeClockUnavailable, TimeValueError, Timestamp,
};
use std::time::Duration;

fn microseconds_modulo_era(ntp: NtpTime) -> i128 {
    let q = i128::from(NtpTime::FRACTIONS_PER_SECOND);
    let fraction = (i128::from(ntp.fractions()) * 1_000_000 + q - 1) / q;
    i128::from(ntp.seconds()) * 1_000_000 + fraction
}

#[test]
fn finite_modular_conversion_and_retained_native_leases() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Clock>();
    let clock = Clock::real_time().unwrap();
    let anchor = clock.current_time();
    let anchor_us = anchor.microseconds().unwrap();
    let anchor_ntp = clock.convert_timestamp_to_ntp_time(anchor).unwrap();
    const ERA: i128 = (1i128 << 32) * 1_000_000;
    for us in [i64::MIN + 1, -1, 0, 1, i64::MAX - 1] {
        let ntp = clock
            .convert_timestamp_to_ntp_time(Timestamp::from_microseconds(us).unwrap())
            .unwrap();
        let expected = (microseconds_modulo_era(anchor_ntp) + i128::from(us)
            - i128::from(anchor_us))
        .rem_euclid(ERA);
        assert_eq!(microseconds_modulo_era(ntp), expected);
    }
    for timestamp in [Timestamp::minus_infinity(), Timestamp::plus_infinity()] {
        assert_eq!(
            clock.convert_timestamp_to_ntp_time(timestamp),
            Err(TimeValueError::InfiniteValue)
        );
    }
    assert_eq!(
        clock.convert_timestamp_to_ntp_time_in_milliseconds(i64::MAX),
        Err(TimeValueError::OutOfRange)
    );
    assert!(clock.current_ntp_time().is_ok());
    assert!(clock.current_ntp_in_milliseconds().is_ok());

    let copy = clock.clone();
    std::thread::spawn(move || {
        assert_eq!(
            copy.convert_timestamp_to_ntp_time(anchor).unwrap(),
            anchor_ntp
        );
    })
    .join()
    .unwrap();

    let environment = Environment::builder().clock_handle(&clock).build().unwrap();
    let retained_environment = environment.clone();
    let mut actor = ProductionSession::builder(ProductionSessionConfig::default())
        .clock_handle(&clock)
        .build()
        .unwrap();
    actor = std::thread::spawn(move || {
        assert!(actor.clock_timestamp().unwrap().is_finite());
        actor
    })
    .join()
    .unwrap();
    drop(clock);
    drop(environment);
    actor.shutdown().unwrap();
    drop(actor);
    // Only the native environment's ClockReference now owns this clock.
    assert!(retained_environment.timestamp().is_finite());
    assert!(ControlledWorld::acquire(42, Duration::ZERO).is_err());
    drop(retained_environment);

    let world = ControlledWorld::acquire(42, Duration::ZERO).unwrap();
    assert!(matches!(Clock::real_time(), Err(RealTimeClockUnavailable)));
    assert_eq!(world.now(), Duration::ZERO);
    drop(world);
    assert!(Clock::real_time().is_ok());
    race_clock_and_hook_acquisition();
}

fn race_clock_and_hook_acquisition() {
    use std::sync::{
        Arc, Barrier,
        atomic::{AtomicBool, Ordering},
    };
    let phases = Arc::new(Barrier::new(2));
    let clock_acquired = Arc::new(AtomicBool::new(false));
    let worker_phases = phases.clone();
    let worker_acquired = clock_acquired.clone();
    let worker = std::thread::spawn(move || {
        for _ in 0..32 {
            worker_phases.wait();
            let clock = Clock::real_time();
            worker_acquired.store(clock.is_ok(), Ordering::Release);
            worker_phases.wait();
            // Keep the lease live until both acquisitions have been observed.
            worker_phases.wait();
            drop(clock);
            worker_phases.wait();
        }
    });
    let mut all_exclusive = true;
    for _ in 0..32 {
        phases.wait();
        let world = ControlledWorld::acquire(42, Duration::ZERO);
        phases.wait();
        let exactly_one_acquired = world.is_ok() != clock_acquired.load(Ordering::Acquire);
        phases.wait();
        drop(world);
        phases.wait();
        all_exclusive &= exactly_one_acquired;
    }
    worker.join().unwrap();
    // Do not panic while the worker could still be waiting at a barrier.
    assert!(all_exclusive);
}
