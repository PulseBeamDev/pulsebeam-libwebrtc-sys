use std::{fmt, marker::PhantomData, rc::Rc, sync::Arc, time::Duration};

use crate::ffi;

const NO_DEADLINE: i64 = i64::MIN;
/// Largest timestamp within both WebRTC's nanosecond clock and NTP era zero.
pub const MAX_CONTROLLED_TIME: Duration =
    Duration::from_micros((u32::MAX as u64 - 2_208_988_800) * 1_000_000 + 999_999);

// Non-Send closures enter native code only through creator-thread cooperative
// queues. Threaded posting methods continue to require Send closures.
pub(crate) struct RustTask(Option<Box<dyn FnOnce() + 'static>>);

pub(crate) fn run_task(mut task: Box<RustTask>) {
    if let Some(task) = task.0.take() {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(task));
    }
}

fn boxed_task(task: impl FnOnce() + Send + 'static) -> Box<RustTask> {
    Box::new(RustTask(Some(Box::new(task))))
}

fn duration_micros(duration: Duration) -> Option<i64> {
    i64::try_from(duration.as_micros()).ok()
}

/// A handle to WebRTC's process system clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl SystemClock {
    pub fn now(self) -> Duration {
        Duration::from_micros(ffi::system_clock_time_us() as u64)
    }
}

/// A caller-advanced clock shared by environments and cooperative queues.
#[derive(Clone)]
pub struct ManualClock(Arc<ManualClockInner>);

struct ManualClockInner {
    native: cxx::UniquePtr<ffi::NativeManualClock>,
}

// SAFETY: pinned `webrtc::SimulatedClock` stores time atomically, and the
// repository adapter exposes only its documented concurrent read/advance API.
unsafe impl Send for ManualClockInner {}
// SAFETY: see the `Send` rationale; all exposed native operations are atomic.
unsafe impl Sync for ManualClockInner {}

impl ManualClock {
    pub fn new(initial_time: Duration) -> Result<Self, BuildEnvironmentError> {
        if initial_time > MAX_CONTROLLED_TIME {
            return Err(BuildEnvironmentError::TimestampOutOfRange);
        }
        let initial_time_us =
            duration_micros(initial_time).ok_or(BuildEnvironmentError::TimestampOutOfRange)?;
        let native = ffi::new_manual_clock(initial_time_us);
        if native.is_null() {
            return Err(BuildEnvironmentError::NativeConstructionFailed);
        }
        Ok(Self(Arc::new(ManualClockInner { native })))
    }

    pub fn now(&self) -> Duration {
        Duration::from_micros(ffi::manual_clock_time_us(self.native()) as u64)
    }

    pub fn advance(&self, delta: Duration) -> Result<(), BuildEnvironmentError> {
        let delta_us = duration_micros(delta).ok_or(BuildEnvironmentError::TimestampOutOfRange)?;
        if ffi::advance_manual_clock(self.native(), delta_us) {
            Ok(())
        } else {
            Err(BuildEnvironmentError::TimestampOutOfRange)
        }
    }

    pub(crate) fn native(&self) -> &ffi::NativeManualClock {
        self.0.native.as_ref().expect("validated manual clock")
    }
}

/// WebRTC task queue priority.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum QueuePriority {
    #[default]
    Normal = 0,
    High = 1,
    Video = 2,
    Audio = 3,
    Low = 4,
}

/// Thread-safe factory for either WebRTC's default queues or caller-driven queues.
#[derive(Clone)]
pub struct TaskQueueFactory(Arc<TaskQueueFactoryInner>);

struct TaskQueueFactoryInner {
    native: cxx::UniquePtr<ffi::NativeTaskQueueFactory>,
    clock: Option<ManualClock>,
}

// SAFETY: `TaskQueueFactory` requires thread-safe implementations. The default
// upstream factory satisfies that contract and the cooperative adapter guards
// all shared scheduler state with a mutex.
unsafe impl Send for TaskQueueFactoryInner {}
// SAFETY: see the `Send` rationale; factory methods take shared references.
unsafe impl Sync for TaskQueueFactoryInner {}

impl TaskQueueFactory {
    pub fn default_threaded() -> Result<Self, BuildEnvironmentError> {
        Self::from_native(ffi::new_default_task_queue_factory(), None)
    }

    pub fn cooperative(clock: &ManualClock) -> Result<Self, BuildEnvironmentError> {
        Self::from_native(
            ffi::new_cooperative_task_queue_factory(clock.native()),
            Some(clock.clone()),
        )
    }

    fn from_native(
        native: cxx::UniquePtr<ffi::NativeTaskQueueFactory>,
        clock: Option<ManualClock>,
    ) -> Result<Self, BuildEnvironmentError> {
        if native.is_null() {
            Err(BuildEnvironmentError::NativeConstructionFailed)
        } else {
            Ok(Self(Arc::new(TaskQueueFactoryInner { native, clock })))
        }
    }

    pub fn create_queue(
        &self,
        name: &str,
        priority: QueuePriority,
    ) -> Result<TaskQueue, BuildEnvironmentError> {
        self.check_current()?;
        if name.as_bytes().contains(&0) {
            return Err(BuildEnvironmentError::InvalidQueueName);
        }
        let native = ffi::create_task_queue(self.native(), name, priority as u8);
        if native.is_null() {
            Err(BuildEnvironmentError::NativeConstructionFailed)
        } else {
            Ok(TaskQueue {
                native,
                factory: self.clone(),
                driver: None,
                _creator_sequence: PhantomData,
            })
        }
    }

    /// Dispatch up to 1,024 ready cooperative tasks without moving time.
    /// Returns zero for a default threaded factory or off the cooperative
    /// factory's creator thread. Work may remain ready; use `pump` for a
    /// caller-selected budget and a remaining-work snapshot.
    pub fn run_ready(&self) -> usize {
        self.pump(1_024).map_or(0, |result| result.dispatched)
    }

    /// Dispatch at most `budget` top-level tasks. Nested lifecycle yields do
    /// not count against it. A zero budget never dispatches work.
    pub fn pump(&self, budget: usize) -> Result<PumpResult, BuildEnvironmentError> {
        self.check_current()?;
        let dispatched = ffi::pump_ready_tasks(self.native(), budget);
        let next_deadline = self.next_deadline();
        let ready = self
            .0
            .clock
            .as_ref()
            .is_some_and(|clock| next_deadline.is_some_and(|deadline| deadline <= clock.now()));
        Ok(PumpResult {
            dispatched,
            ready,
            next_deadline,
        })
    }

    fn check_current(&self) -> Result<(), BuildEnvironmentError> {
        if ffi::task_queue_factory_is_current(self.native()) {
            Ok(())
        } else {
            Err(BuildEnvironmentError::WrongThread)
        }
    }

    /// Returns the next cooperative deadline, or `None` when no work is queued.
    pub fn next_deadline(&self) -> Option<Duration> {
        let deadline = ffi::next_task_deadline_us(self.native());
        (deadline != NO_DEADLINE).then(|| Duration::from_micros(deadline as u64))
    }

    pub fn is_cooperative(&self) -> bool {
        ffi::task_queue_factory_is_cooperative(self.native())
    }

    fn native(&self) -> &ffi::NativeTaskQueueFactory {
        self.0
            .native
            .as_ref()
            .expect("validated task queue factory")
    }
}

/// A sequence-bound WebRTC task queue.
///
/// ```compile_fail
/// fn assert_send<T: Send>() {}
/// assert_send::<pulsebeam_webrtc_sys::TaskQueue>();
/// ```
///
/// ```compile_fail
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<pulsebeam_webrtc_sys::TaskQueue>();
/// ```
pub struct TaskQueue {
    native: cxx::UniquePtr<ffi::NativeTaskQueue>,
    factory: TaskQueueFactory,
    driver: Option<ControlledPeerDriver>,
    _creator_sequence: PhantomData<Rc<()>>,
}

impl TaskQueue {
    /// Post caller-thread work. Reject threaded factories before retaining the
    /// closure; the native cooperative queue only runs/destroys it on this
    /// queue's creator thread. This permits cooperative self-reposting work.
    pub fn post_local(&self, task: impl FnOnce() + 'static) -> Result<bool, BuildEnvironmentError> {
        if !self.factory.is_cooperative() {
            return Err(BuildEnvironmentError::NotCooperative);
        }
        self.factory.check_current()?;
        Ok(self
            .native
            .as_ref()
            .is_some_and(|native| ffi::post_task(native, Box::new(RustTask(Some(Box::new(task)))))))
    }

    pub fn post(&self, task: impl FnOnce() + Send + 'static) -> bool {
        self.native
            .as_ref()
            .is_some_and(|native| ffi::post_task(native, boxed_task(task)))
    }

    pub fn post_delayed(
        &self,
        delay: Duration,
        task: impl FnOnce() + Send + 'static,
    ) -> Result<bool, BuildEnvironmentError> {
        let delay_us = duration_micros(delay).ok_or(BuildEnvironmentError::TimestampOutOfRange)?;
        if self.factory.0.clock.as_ref().is_some_and(|clock| {
            clock
                .now()
                .checked_add(delay)
                .is_none_or(|deadline| deadline > MAX_CONTROLLED_TIME)
        }) {
            return Err(BuildEnvironmentError::TimestampOutOfRange);
        }
        Ok(self
            .native
            .as_ref()
            .is_some_and(|native| ffi::post_delayed_task(native, delay_us, boxed_task(task))))
    }

    pub fn close(&mut self) {
        drop(std::mem::replace(&mut self.native, cxx::UniquePtr::null()));
    }
}

/// A thread-safe, copyable WebRTC environment.
pub struct Environment {
    native: cxx::UniquePtr<ffi::NativeEnvironment>,
    _clock: Option<ManualClock>,
    _task_queue_factory: TaskQueueFactory,
    _randomness: Option<Arc<RandomnessState>>,
}

impl Clone for Environment {
    fn clone(&self) -> Self {
        let native = self
            .native
            .as_ref()
            .map_or_else(cxx::UniquePtr::null, |native| {
                ffi::clone_environment(native)
            });
        Self {
            native,
            _clock: self._clock.clone(),
            _task_queue_factory: self._task_queue_factory.clone(),
            _randomness: self._randomness.clone(),
        }
    }
}

// SAFETY: the pinned `webrtc::Environment` contract explicitly declares the
// value thread-safe; retained dependencies also satisfy their thread contracts.
unsafe impl Send for Environment {}
// SAFETY: see the `Send` rationale; the adapter exposes shared reads only.
unsafe impl Sync for Environment {}

impl Environment {
    pub fn builder() -> EnvironmentBuilder {
        EnvironmentBuilder::default()
    }

    pub fn now(&self) -> Duration {
        Duration::from_micros(ffi::environment_time_us(self.native()) as u64)
    }

    pub(crate) fn uses_system_clock(&self) -> bool {
        self._clock.is_none()
    }

    pub fn close(&mut self) {
        drop(std::mem::replace(&mut self.native, cxx::UniquePtr::null()));
    }

    pub(crate) fn uses_controlled_clock(&self, clock: &ManualClock) -> bool {
        self._task_queue_factory.is_cooperative()
            && self._task_queue_factory.check_current().is_ok()
            && ffi::task_queue_factory_uses_clock(self._task_queue_factory.native(), clock.native())
    }

    pub(crate) fn native(&self) -> &ffi::NativeEnvironment {
        self.native.as_ref().expect("validated environment")
    }
}

#[derive(Default)]
pub struct EnvironmentBuilder {
    clock: Option<ManualClock>,
    task_queue_factory: Option<TaskQueueFactory>,
    randomness: Option<Arc<RandomnessState>>,
}

impl EnvironmentBuilder {
    pub fn clock(mut self, clock: &ManualClock) -> Self {
        self.clock = Some(clock.clone());
        self
    }

    pub fn task_queue_factory(mut self, factory: &TaskQueueFactory) -> Self {
        self.task_queue_factory = Some(factory.clone());
        self
    }

    pub fn randomness(mut self, lease: &RandomnessLease) -> Self {
        self.randomness = Some(lease.0.clone());
        self
    }

    pub fn build(self) -> Result<Environment, BuildEnvironmentError> {
        let factory = match self.task_queue_factory {
            Some(factory) => factory,
            None => TaskQueueFactory::default_threaded()?,
        };
        factory.check_current()?;
        let clock = match (&self.clock, &factory.0.clock) {
            (None, Some(factory_clock)) => Some(factory_clock.clone()),
            (Some(clock), Some(_))
                if !ffi::task_queue_factory_uses_clock(factory.native(), clock.native()) =>
            {
                return Err(BuildEnvironmentError::MismatchedCooperativeClock);
            }
            (clock, _) => clock.clone(),
        };
        let clock_ptr = clock
            .as_ref()
            .map_or(std::ptr::null(), |clock| clock.native() as *const _);
        // SAFETY: a null pointer selects upstream system time; otherwise the
        // pointed-to clock is retained in the returned Environment.
        let native = unsafe { ffi::create_environment(clock_ptr, factory.native()) };
        if native.is_null() {
            return Err(BuildEnvironmentError::NativeConstructionFailed);
        }
        Ok(Environment {
            native,
            _clock: clock,
            _task_queue_factory: factory,
            _randomness: self.randomness,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildEnvironmentError {
    InvalidQueueName,
    MismatchedCooperativeClock,
    NativeConstructionFailed,
    TimestampOutOfRange,
    WrongThread,
    NotCooperative,
}

impl fmt::Display for BuildEnvironmentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidQueueName => "task queue names cannot contain a NUL byte",
            Self::MismatchedCooperativeClock => {
                "the environment clock differs from the cooperative factory clock"
            }
            Self::NativeConstructionFailed => "native WebRTC construction failed",
            Self::TimestampOutOfRange => {
                "timestamp is outside the controlled clock's NTP-era-zero range"
            }
            Self::WrongThread => "cooperative execution requires the factory's creator thread",
            Self::NotCooperative => "local tasks require a cooperative queue",
        })
    }
}

impl std::error::Error for BuildEnvironmentError {}

/// Exclusive process-global installation of deterministic WebRTC randomness.
pub struct RandomnessLease(Arc<RandomnessState>);

struct RandomnessState {
    native: Option<cxx::UniquePtr<ffi::NativeRandomnessLease>>,
}

// SAFETY: the native generator serializes its mutable PRNG state and WebRTC's
// installation seam is process-global. Exclusivity is enforced before install.
unsafe impl Send for RandomnessState {}
// SAFETY: see the `Send` rationale; calls are serialized by the native mutex.
unsafe impl Sync for RandomnessState {}

impl RandomnessLease {
    pub fn acquire(seed: u64) -> Result<Self, RandomnessLeaseError> {
        let native = ffi::new_seeded_randomness(seed);
        if native.is_null() {
            return Err(RandomnessLeaseError::AlreadyAcquired);
        }
        Ok(Self(Arc::new(RandomnessState {
            native: Some(native),
        })))
    }

    pub fn next_u64(&self) -> u64 {
        ffi::next_seeded_random_u64(self.0.native.as_ref().expect("active randomness lease"))
    }
}

impl Drop for RandomnessState {
    fn drop(&mut self) {
        drop(self.native.take());
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RandomnessLeaseError {
    AlreadyAcquired,
    NativeConstructionFailed,
}

impl fmt::Display for RandomnessLeaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::AlreadyAcquired => "WebRTC randomness is already leased",
            Self::NativeConstructionFailed => "failed to install WebRTC randomness",
        })
    }
}

impl std::error::Error for RandomnessLeaseError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThreadStartError;

impl fmt::Display for ThreadStartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("failed to initialize WebRTC thread")
    }
}

impl std::error::Error for ThreadStartError {}

macro_rules! thread_handle {
    ($name:ident, $network:literal) => {
        /// An owning, sequence-bound WebRTC execution thread.
        ///
        /// ```compile_fail
        /// fn assert_send<T: Send>() {}
        #[doc = concat!("assert_send::<pulsebeam_webrtc_sys::", stringify!($name), ">();")]
        /// ```
        ///
        /// ```compile_fail
        /// fn assert_sync<T: Sync>() {}
        #[doc = concat!("assert_sync::<pulsebeam_webrtc_sys::", stringify!($name), ">();")]
        /// ```
        pub struct $name {
            native: cxx::UniquePtr<ffi::NativeThread>,
            _creator_sequence: PhantomData<Rc<()>>,
        }

        impl $name {
            pub(crate) fn from_native(native: cxx::UniquePtr<ffi::NativeThread>) -> Self {
                Self {
                    native,
                    _creator_sequence: PhantomData,
                }
            }

            pub fn start() -> Result<Self, ThreadStartError> {
                let native = ffi::new_thread($network);
                if native.is_null() {
                    Err(ThreadStartError)
                } else {
                    Ok(Self {
                        native,
                        _creator_sequence: PhantomData,
                    })
                }
            }

            pub fn post(&self, task: impl FnOnce() + Send + 'static) -> bool {
                self.native
                    .as_ref()
                    .is_some_and(|native| ffi::thread_post_task(native, boxed_task(task)))
            }

            pub fn post_delayed(
                &self,
                delay: Duration,
                task: impl FnOnce() + Send + 'static,
            ) -> Result<bool, BuildEnvironmentError> {
                let delay_us =
                    duration_micros(delay).ok_or(BuildEnvironmentError::TimestampOutOfRange)?;
                Ok(self.native.as_ref().is_some_and(|native| {
                    ffi::thread_post_delayed_task(native, delay_us, boxed_task(task))
                }))
            }

            pub fn close(&mut self) {
                drop(std::mem::replace(&mut self.native, cxx::UniquePtr::null()));
            }

            pub(crate) fn native(&self) -> &ffi::NativeThread {
                self.native.as_ref().expect("validated WebRTC thread")
            }
        }
    };
}

thread_handle!(NetworkThread, true);
thread_handle!(WorkerThread, false);
thread_handle!(SignalingThread, false);

/// One caller-owned, sequence-bound WebRTC thread for peer network, worker,
/// and signaling roles. It never starts an OS thread. Its budgeted pump also
/// dispatches cooperative media queues sharing the manual clock. Only one such
/// driver may be active per process because WebRTC's thread clock is global.
/// Native devices are not supported with this driver.
///
/// ```compile_fail
/// fn assert_send<T: Send>() {}
/// assert_send::<pulsebeam_webrtc_sys::ControlledPeerDriver>();
/// ```
#[derive(Clone)]
pub struct ControlledPeerDriver(Rc<DriverInner>);

struct DriverInner {
    native: cxx::UniquePtr<ffi::NativeDriverThread>,
    clock: ManualClock,
    seeded: bool,
    next_peer_id: std::cell::Cell<u64>,
    _creator_sequence: PhantomData<Rc<()>>,
}

impl ControlledPeerDriver {
    pub fn new(clock: &ManualClock) -> Result<Self, ThreadStartError> {
        let native = ffi::new_driver_thread(clock.native());
        if native.is_null() {
            Err(ThreadStartError)
        } else {
            Ok(Self(Rc::new(DriverInner {
                native,
                clock: clock.clone(),
                seeded: false,
                next_peer_id: std::cell::Cell::new(1),
                _creator_sequence: PhantomData,
            })))
        }
    }

    /// Dispatch up to 1,024 ready tasks without moving time or waiting.
    /// Returns true on the creator thread (including when idle), false
    /// off-thread. Success does not imply a drain; use `pump` for a
    /// caller-selected budget and a remaining-work snapshot.
    pub fn run_ready(&self) -> bool {
        if !self.is_current() {
            return false;
        }
        self.pump(1_024);
        true
    }

    /// Dispatch at most `budget` tasks across this driver's peer and media
    /// queues. All queues sharing its clock use deadline then posting order.
    pub fn pump(&self, budget: usize) -> PumpResult {
        let dispatched = ffi::driver_pump(self.native(), budget);
        let next_deadline = self.next_deadline();
        PumpResult {
            dispatched,
            ready: next_deadline.is_some_and(|deadline| deadline <= self.0.clock.now()),
            next_deadline,
        }
    }

    /// Earliest pending deadline across native peer and media task queues,
    /// with microsecond precision. Pending external packets are caller-owned.
    pub fn next_deadline(&self) -> Option<Duration> {
        let deadline = ffi::driver_next_deadline_us(self.native());
        (deadline != NO_DEADLINE).then(|| Duration::from_micros(deadline as u64))
    }

    pub(crate) fn is_current(&self) -> bool {
        ffi::driver_is_current(self.native())
    }

    pub(crate) fn is_seeded(&self) -> bool {
        self.0.seeded
    }

    pub(crate) fn allocate_peer_id(&self) -> Option<u64> {
        let id = self.0.next_peer_id.get();
        self.0.next_peer_id.set(id.checked_add(1)?);
        Some(id)
    }

    pub(crate) fn clock(&self) -> &ManualClock {
        &self.0.clock
    }

    pub(crate) fn borrow_thread(&self) -> cxx::UniquePtr<ffi::NativeThread> {
        ffi::borrow_driver_thread(self.native())
    }

    pub(crate) fn same_identity(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    pub(crate) fn native(&self) -> &ffi::NativeDriverThread {
        self.0.native.as_ref().expect("validated controlled driver")
    }
}

/// Snapshot after a caller-selected finite dispatch budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PumpResult {
    pub dispatched: usize,
    pub ready: bool,
    pub next_deadline: Option<Duration>,
}

/// Exclusive seeded, caller-thread-controlled execution domain.
///
/// Acquisition installs the process clock and non-cryptographic random hook
/// atomically, or leaves existing hooks untouched. Peer factories, networks
/// and queues retain the driver until their caller-thread teardown finishes.
/// Production peers and independent parallel worlds must not coexist with it.
///
/// ```compile_fail
/// fn assert_send<T: Send>() {}
/// assert_send::<pulsebeam_webrtc_sys::ControlledWorld>();
/// ```
///
/// ```compile_fail
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<pulsebeam_webrtc_sys::ControlledWorld>();
/// ```
pub struct ControlledWorld {
    factory: TaskQueueFactory,
    clock: ManualClock,
    driver: ControlledPeerDriver,
}

impl ControlledWorld {
    pub fn acquire(seed: u64, initial_time: Duration) -> Result<Self, WorldAcquireError> {
        let clock = ManualClock::new(initial_time).map_err(WorldAcquireError::Environment)?;
        let factory =
            TaskQueueFactory::cooperative(&clock).map_err(WorldAcquireError::Environment)?;
        let native = ffi::new_seeded_driver_thread(clock.native(), seed);
        if native.is_null() {
            return Err(WorldAcquireError::HooksUnavailable);
        }
        let driver = ControlledPeerDriver(Rc::new(DriverInner {
            native,
            clock: clock.clone(),
            seeded: true,
            next_peer_id: std::cell::Cell::new(1),
            _creator_sequence: PhantomData,
        }));
        Ok(Self {
            factory,
            clock,
            driver,
        })
    }

    pub fn now(&self) -> Duration {
        self.clock.now()
    }

    /// Move virtual time without executing any queued work. Fractional
    /// microseconds are truncated; time is never advanced by pumping/yielding.
    pub fn advance(&self, delta: Duration) -> Result<(), BuildEnvironmentError> {
        self.clock.advance(delta)
    }

    pub fn pump(&self, budget: usize) -> PumpResult {
        self.driver.pump(budget)
    }

    pub fn next_deadline(&self) -> Option<Duration> {
        self.driver.next_deadline()
    }

    pub fn driver(&self) -> &ControlledPeerDriver {
        &self.driver
    }

    pub fn create_queue(
        &self,
        name: &str,
        priority: QueuePriority,
    ) -> Result<TaskQueue, BuildEnvironmentError> {
        let mut queue = self.factory.create_queue(name, priority)?;
        queue.driver = Some(self.driver.clone());
        Ok(queue)
    }

    /// Create a packet network on this world's timeline without exposing a
    /// separately sendable clock handle. Delivery and shaping remain external.
    pub fn create_network(&self) -> Result<crate::ControlledSimulatedNetwork, crate::NetworkError> {
        crate::ControlledSimulatedNetwork::new(&self.clock, &self.driver)
    }

    /// Build a factory with matching clock, cooperative queues and driver.
    /// Supply controlled packet providers before `build`.
    pub fn peer_factory_builder(
        &self,
    ) -> Result<crate::PeerConnectionFactoryBuilder, BuildEnvironmentError> {
        let environment = Environment::builder()
            .task_queue_factory(&self.factory)
            .build()?;
        Ok(crate::PeerConnectionFactory::builder()
            .environment(environment)
            .controlled_driver(&self.driver))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldAcquireError {
    Environment(BuildEnvironmentError),
    HooksUnavailable,
}
impl fmt::Display for WorldAcquireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Environment(error) => error.fmt(f),
            Self::HooksUnavailable => {
                f.write_str("controlled clock/randomness hooks or caller thread are already owned")
            }
        }
    }
}
impl std::error::Error for WorldAcquireError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn thread_safe_handles_have_positive_traits() {
        assert_send_sync::<ManualClock>();
        assert_send_sync::<TaskQueueFactory>();
        assert_send_sync::<Environment>();
        assert_send_sync::<RandomnessLease>();
    }

    #[test]
    fn lifecycle_yield_leaves_self_reposting_caller_work_for_the_pump() {
        use std::cell::{Cell, RefCell};

        fn repost(queue: Rc<RefCell<TaskQueue>>, count: Rc<Cell<usize>>) {
            let next_queue = queue.clone();
            queue
                .borrow()
                .post_local(move || {
                    count.set(count.get() + 1);
                    repost(next_queue, count);
                })
                .unwrap();
        }

        let world = ControlledWorld::acquire(81, Duration::from_secs(99)).unwrap();
        let queue = Rc::new(RefCell::new(
            world
                .create_queue("caller-reposting", QueuePriority::Normal)
                .unwrap(),
        ));
        let count = Rc::new(Cell::new(0));
        repost(queue.clone(), count.clone());
        assert!(ffi::test_driver_lifecycle_yield(
            world.driver.native(),
            world.factory.native(),
        ));
        assert_eq!(count.get(), 0);
        assert_eq!(world.now(), Duration::from_secs(99));
        assert!(world.pump(0).ready);
        assert_eq!(world.pump(3).dispatched, 3);
        assert_eq!(count.get(), 3);
        queue.borrow_mut().close();
    }

    #[test]
    fn rejects_out_of_range_time_without_native_linking() {
        assert_eq!(duration_micros(Duration::from_secs(u64::MAX)), None);
    }
}
