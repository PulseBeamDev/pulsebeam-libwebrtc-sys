//! Low-level Rust integration with PulseBeam's pinned libwebrtc artifact.

mod execution;

pub(crate) use execution::{RustTask, run_task};

pub use execution::{
    BuildEnvironmentError, Environment, EnvironmentBuilder, ManualClock, NetworkThread,
    QueuePriority, RandomnessLease, RandomnessLeaseError, SignalingThread, SystemClock, TaskQueue,
    TaskQueueFactory, ThreadStartError, WorkerThread,
};

#[cxx::bridge(namespace = "pulsebeam::webrtc_sys")]
mod ffi {
    extern "Rust" {
        type RustTask;

        fn run_task(task: Box<RustTask>);
    }

    unsafe extern "C++" {
        include!("pulsebeam-webrtc-sys/native/probe.h");
        include!("pulsebeam-webrtc-sys/native/execution.h");

        type NativeEnvironment;
        type NativeManualClock;
        type NativeRandomnessLease;
        type NativeTaskQueue;
        type NativeTaskQueueFactory;
        type NativeThread;

        fn bridge_identity() -> &'static str;

        fn new_manual_clock(initial_time_us: i64) -> UniquePtr<NativeManualClock>;
        fn manual_clock_time_us(clock: &NativeManualClock) -> i64;
        fn advance_manual_clock(clock: &NativeManualClock, delta_us: i64) -> bool;
        fn system_clock_time_us() -> i64;

        fn new_default_task_queue_factory() -> UniquePtr<NativeTaskQueueFactory>;
        fn new_cooperative_task_queue_factory(
            clock: &NativeManualClock,
        ) -> UniquePtr<NativeTaskQueueFactory>;
        fn task_queue_factory_is_cooperative(factory: &NativeTaskQueueFactory) -> bool;
        fn task_queue_factory_uses_clock(
            factory: &NativeTaskQueueFactory,
            clock: &NativeManualClock,
        ) -> bool;
        fn create_task_queue(
            factory: &NativeTaskQueueFactory,
            name: &str,
            priority: u8,
        ) -> UniquePtr<NativeTaskQueue>;
        fn post_task(queue: &NativeTaskQueue, task: Box<RustTask>) -> bool;
        fn post_delayed_task(queue: &NativeTaskQueue, delay_us: i64, task: Box<RustTask>) -> bool;
        fn run_ready_tasks(factory: &NativeTaskQueueFactory) -> usize;
        fn next_task_deadline_us(factory: &NativeTaskQueueFactory) -> i64;

        unsafe fn create_environment(
            clock: *const NativeManualClock,
            factory: &NativeTaskQueueFactory,
        ) -> UniquePtr<NativeEnvironment>;
        fn clone_environment(environment: &NativeEnvironment) -> UniquePtr<NativeEnvironment>;
        fn environment_time_us(environment: &NativeEnvironment) -> i64;

        fn new_seeded_randomness(seed: u64) -> UniquePtr<NativeRandomnessLease>;
        fn next_seeded_random_u64(lease: &NativeRandomnessLease) -> u64;

        fn new_thread(network: bool) -> UniquePtr<NativeThread>;
        fn thread_post_task(thread: &NativeThread, task: Box<RustTask>) -> bool;
        fn thread_post_delayed_task(
            thread: &NativeThread,
            delay_us: i64,
            task: Box<RustTask>,
        ) -> bool;
    }
}

/// Returns the identity shared by this Rust bridge and its precompiled native half.
///
/// The underlying CXX module remains private:
///
/// ```compile_fail
/// use pulsebeam_webrtc_sys::ffi;
/// ```
pub fn bridge_identity() -> &'static str {
    ffi::bridge_identity()
}

#[cfg(test)]
#[path = "../build_support/manifest.rs"]
#[allow(dead_code)]
mod manifest;

#[cfg(test)]
#[path = "../build_support/artifact.rs"]
#[allow(dead_code)]
mod artifact;
