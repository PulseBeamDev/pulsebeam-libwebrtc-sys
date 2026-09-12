use std::{
    fmt,
    marker::PhantomData,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use crate::ffi;

const NO_DEADLINE: i64 = i64::MIN;
static RANDOMNESS_ACTIVE: AtomicBool = AtomicBool::new(false);

pub(crate) struct RustTask(Option<Box<dyn FnOnce() + Send + 'static>>);

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
        if name.as_bytes().contains(&0) {
            return Err(BuildEnvironmentError::InvalidQueueName);
        }
        let native = ffi::create_task_queue(self.native(), name, priority as u8);
        if native.is_null() {
            Err(BuildEnvironmentError::NativeConstructionFailed)
        } else {
            Ok(TaskQueue {
                native,
                _creator_sequence: PhantomData,
            })
        }
    }

    /// Runs all cooperative work whose deadline is at or before the clock.
    /// Returns zero for a default threaded factory.
    pub fn run_ready(&self) -> usize {
        ffi::run_ready_tasks(self.native())
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
    _creator_sequence: PhantomData<Rc<()>>,
}

impl TaskQueue {
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

    pub fn close(&mut self) {
        drop(std::mem::replace(&mut self.native, cxx::UniquePtr::null()));
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
}

impl fmt::Display for BuildEnvironmentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidQueueName => "task queue names cannot contain a NUL byte",
            Self::MismatchedCooperativeClock => {
                "the environment clock differs from the cooperative factory clock"
            }
            Self::NativeConstructionFailed => "native WebRTC construction failed",
            Self::TimestampOutOfRange => "timestamp is outside WebRTC's signed microsecond range",
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
        RANDOMNESS_ACTIVE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| RandomnessLeaseError::AlreadyAcquired)?;
        let native = ffi::new_seeded_randomness(seed);
        if native.is_null() {
            RANDOMNESS_ACTIVE.store(false, Ordering::Release);
            return Err(RandomnessLeaseError::NativeConstructionFailed);
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
        RANDOMNESS_ACTIVE.store(false, Ordering::Release);
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
        formatter.write_str("failed to start WebRTC thread")
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
    fn rejects_out_of_range_time_without_native_linking() {
        assert_eq!(duration_micros(Duration::from_secs(u64::MAX)), None);
    }
}
