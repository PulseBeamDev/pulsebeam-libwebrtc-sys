use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

use pulsebeam_webrtc_sys::{
    Environment, ManualClock, NetworkThread, RandomnessLease, RandomnessLeaseError,
    TaskQueueFactory, WorkerThread,
};

#[test]
fn cooperative_environment_runs_ready_work_in_deadline_order() {
    let clock = ManualClock::new(Duration::from_secs(10)).unwrap();
    let factory = TaskQueueFactory::cooperative(&clock).unwrap();
    let environment = Environment::builder()
        .task_queue_factory(&factory)
        .build()
        .unwrap();
    let first = factory.create_queue("first", Default::default()).unwrap();
    let second = factory.create_queue("second", Default::default()).unwrap();
    let observed = Arc::new(Mutex::new(Vec::new()));

    for (queue, delay, value) in [
        (&first, Duration::from_millis(5), 3),
        (&second, Duration::ZERO, 1),
        (&first, Duration::ZERO, 2),
        (&second, Duration::from_millis(2), 4),
    ] {
        let observed = observed.clone();
        queue
            .post_delayed(delay, move || observed.lock().unwrap().push(value))
            .unwrap();
    }

    assert_eq!(environment.now(), Duration::from_secs(10));
    assert_eq!(environment.clone().now(), Duration::from_secs(10));
    assert_eq!(factory.run_ready(), 2);
    assert_eq!(*observed.lock().unwrap(), [1, 2]);
    assert_eq!(factory.next_deadline(), Some(Duration::from_millis(10_002)));

    clock.advance(Duration::from_millis(2)).unwrap();
    assert_eq!(factory.run_ready(), 1);
    clock.advance(Duration::from_millis(3)).unwrap();
    assert_eq!(factory.run_ready(), 1);
    assert_eq!(*observed.lock().unwrap(), [1, 2, 4, 3]);
    assert_eq!(factory.next_deadline(), None);
}

#[test]
fn seeded_randomness_is_repeatable_exclusive_and_retained_by_roots() {
    fn sequence(seed: u64) -> Vec<u64> {
        let lease = RandomnessLease::acquire(seed).unwrap();
        let clock = ManualClock::new(Duration::ZERO).unwrap();
        let environment = Environment::builder()
            .clock(&clock)
            .randomness(&lease)
            .build()
            .unwrap();
        let values = (0..4).map(|_| lease.next_u64()).collect();
        assert_eq!(
            std::thread::spawn(move || RandomnessLease::acquire(seed).err())
                .join()
                .unwrap(),
            Some(RandomnessLeaseError::AlreadyAcquired)
        );
        drop(lease);
        assert_eq!(
            RandomnessLease::acquire(seed).err(),
            Some(RandomnessLeaseError::AlreadyAcquired)
        );
        drop(environment);
        values
    }

    let first = sequence(42);
    let second = sequence(42);
    assert_eq!(first, second);
    assert_ne!(first, sequence(43));
}

#[test]
fn early_queue_and_thread_drop_cancel_pending_work() {
    let clock = ManualClock::new(Duration::ZERO).unwrap();
    let factory = TaskQueueFactory::cooperative(&clock).unwrap();
    let ran = Arc::new(AtomicBool::new(false));
    {
        let mut queue = factory.create_queue("dropped", Default::default()).unwrap();
        let ran = ran.clone();
        queue
            .post_delayed(Duration::from_secs(1), move || {
                ran.store(true, Ordering::Release)
            })
            .unwrap();
        queue.close();
        queue.close();
        assert!(!queue.post(|| panic!("closed queue ran work")));
    }
    clock.advance(Duration::from_secs(1)).unwrap();
    assert_eq!(factory.run_ready(), 0);
    assert!(!ran.load(Ordering::Acquire));

    let ran = Arc::new(AtomicBool::new(false));
    {
        let mut thread = WorkerThread::start().unwrap();
        let ran = ran.clone();
        thread
            .post_delayed(Duration::from_secs(60), move || {
                ran.store(true, Ordering::Release)
            })
            .unwrap();
        thread.close();
        thread.close();
        assert!(!thread.post(|| panic!("closed thread ran work")));
    }
    assert!(!ran.load(Ordering::Acquire));

    // Socket-server construction and idempotent Rust cleanup are covered too.
    drop(NetworkThread::start().unwrap());
    drop(pulsebeam_webrtc_sys::SignalingThread::start().unwrap());
}

#[test]
fn mismatched_clock_fails_without_consuming_dependencies() {
    let first = ManualClock::new(Duration::ZERO).unwrap();
    let second = ManualClock::new(Duration::ZERO).unwrap();
    let factory = TaskQueueFactory::cooperative(&first).unwrap();
    assert!(
        Environment::builder()
            .clock(&second)
            .task_queue_factory(&factory)
            .build()
            .is_err()
    );

    let queue = factory
        .create_queue("still-live", Default::default())
        .unwrap();
    assert!(queue.post(|| {}));
    assert_eq!(factory.run_ready(), 1);
}

#[test]
fn factory_contract_serializes_a_queue_and_default_queues_execute() {
    let clock = ManualClock::new(Duration::ZERO).unwrap();
    let factory = TaskQueueFactory::cooperative(&clock).unwrap();
    let queue = factory
        .create_queue("serialized", Default::default())
        .unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    assert!(queue.post(move || {
        entered_tx.send(()).unwrap();
        release_rx.recv().unwrap();
    }));
    assert!(queue.post(|| {}));

    let first_factory = factory.clone();
    let first = std::thread::spawn(move || first_factory.run_ready());
    entered_rx.recv().unwrap();
    let second_factory = factory.clone();
    let second = std::thread::spawn(move || second_factory.run_ready());
    assert_eq!(second.join().unwrap(), 0);
    release_tx.send(()).unwrap();
    assert_eq!(first.join().unwrap(), 2);

    let default_factory = TaskQueueFactory::default_threaded().unwrap();
    let default_queue = default_factory
        .create_queue("default", Default::default())
        .unwrap();
    let (completed_tx, completed_rx) = mpsc::channel();
    assert!(default_queue.post(move || completed_tx.send(()).unwrap()));
    completed_rx.recv_timeout(Duration::from_secs(2)).unwrap();
}
