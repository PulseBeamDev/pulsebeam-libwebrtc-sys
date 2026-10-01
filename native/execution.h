#pragma once

#include <cstddef>
#include <cstdint>
#include <memory>

#include "api/environment/environment.h"
#include "rust/cxx.h"

namespace webrtc {
class Thread;
}

namespace pulsebeam::webrtc_sys {

struct RustTask;
struct RustReadiness;

// Contains only a thread-safe Rust notification state, never engine handles.
class ReadinessSignal final {
 public:
  explicit ReadinessSignal(rust::Box<RustReadiness> readiness) noexcept;
  ~ReadinessSignal();
  void Signal() const noexcept;

 private:
  rust::Box<RustReadiness> readiness_;
};

class NativeReadiness final {
 public:
  explicit NativeReadiness(rust::Box<RustReadiness> readiness) noexcept;
  ~NativeReadiness();
  std::shared_ptr<ReadinessSignal> signal() const noexcept;

 private:
  std::shared_ptr<ReadinessSignal> signal_;
};

std::unique_ptr<NativeReadiness> new_readiness(
    rust::Box<RustReadiness> readiness) noexcept;

inline void SignalReadiness(const std::shared_ptr<ReadinessSignal>& signal) {
  if (signal) signal->Signal();
}

struct ReadinessNotification {
  std::shared_ptr<ReadinessSignal> readiness;
  ~ReadinessNotification() { SignalReadiness(readiness); }
};

// Reserve production before starting any engine queues/threads. Controlled
// acquisition checks the same reservation under the process-hook lock.
class NativeProductionLease final {
 public:
  NativeProductionLease() noexcept = default;
  ~NativeProductionLease();
};
std::unique_ptr<NativeProductionLease> new_production_lease() noexcept;

class NativeManualClock final {
 public:
  struct State;

  explicit NativeManualClock(std::shared_ptr<State> state) noexcept;
  ~NativeManualClock();

  NativeManualClock(const NativeManualClock&) = delete;
  NativeManualClock& operator=(const NativeManualClock&) = delete;

  const std::shared_ptr<State>& state() const noexcept;

 private:
  std::shared_ptr<State> state_;
};

class NativeTaskQueueFactory final {
 public:
  struct State;

  explicit NativeTaskQueueFactory(std::shared_ptr<State> state) noexcept;
  ~NativeTaskQueueFactory();

  NativeTaskQueueFactory(const NativeTaskQueueFactory&) = delete;
  NativeTaskQueueFactory& operator=(const NativeTaskQueueFactory&) = delete;

  const std::shared_ptr<State>& state() const noexcept;

 private:
  std::shared_ptr<State> state_;
};

class NativeTaskQueue final {
 public:
  struct State;

  explicit NativeTaskQueue(std::unique_ptr<State> state) noexcept;
  ~NativeTaskQueue();

  NativeTaskQueue(const NativeTaskQueue&) = delete;
  NativeTaskQueue& operator=(const NativeTaskQueue&) = delete;

  const std::unique_ptr<State>& state() const noexcept;

 private:
  std::unique_ptr<State> state_;
};

class NativeEnvironment final {
 public:
  struct State;

  explicit NativeEnvironment(std::unique_ptr<State> state) noexcept;
  ~NativeEnvironment();

  NativeEnvironment(const NativeEnvironment&) = delete;
  NativeEnvironment& operator=(const NativeEnvironment&) = delete;

  const std::unique_ptr<State>& state() const noexcept;
  webrtc::Environment environment() const noexcept;

 private:
  std::unique_ptr<State> state_;
};

class NativeRandomnessLease final {
 public:
  NativeRandomnessLease() noexcept = default;
  ~NativeRandomnessLease();

  NativeRandomnessLease(const NativeRandomnessLease&) = delete;
  NativeRandomnessLease& operator=(const NativeRandomnessLease&) = delete;
};

class NativeDriverThread final {
 public:
  struct State;

  explicit NativeDriverThread(std::unique_ptr<State> state) noexcept;
  ~NativeDriverThread();

  NativeDriverThread(const NativeDriverThread&) = delete;
  NativeDriverThread& operator=(const NativeDriverThread&) = delete;

  const std::unique_ptr<State>& state() const noexcept;
  webrtc::Thread* thread() const noexcept;
  bool uses_clock(const NativeManualClock& clock) const noexcept;

 private:
  std::unique_ptr<State> state_;
};

class NativeThread final {
 public:
  struct State;

  explicit NativeThread(std::unique_ptr<State> state) noexcept;
  ~NativeThread();

  NativeThread(const NativeThread&) = delete;
  NativeThread& operator=(const NativeThread&) = delete;

  const std::unique_ptr<State>& state() const noexcept;
  webrtc::Thread* thread() const noexcept;

 private:
  std::unique_ptr<State> state_;
};

std::unique_ptr<NativeManualClock> new_manual_clock(
    std::int64_t initial_time_us) noexcept;
std::int64_t manual_clock_time_us(const NativeManualClock& clock) noexcept;
bool advance_manual_clock(const NativeManualClock& clock,
                          std::int64_t delta_us) noexcept;
std::int64_t system_clock_time_us() noexcept;

std::unique_ptr<NativeTaskQueueFactory>
new_default_task_queue_factory() noexcept;
std::unique_ptr<NativeTaskQueueFactory> new_cooperative_task_queue_factory(
    const NativeManualClock& clock) noexcept;
bool task_queue_factory_is_cooperative(
    const NativeTaskQueueFactory& factory) noexcept;
bool task_queue_factory_is_current(
    const NativeTaskQueueFactory& factory) noexcept;
bool task_queue_factory_uses_clock(const NativeTaskQueueFactory& factory,
                                   const NativeManualClock& clock) noexcept;
std::unique_ptr<NativeTaskQueue> create_task_queue(
    const NativeTaskQueueFactory& factory,
    rust::Str name,
    std::uint8_t priority) noexcept;
bool post_task(const NativeTaskQueue& queue, rust::Box<RustTask> task) noexcept;
bool post_delayed_task(const NativeTaskQueue& queue,
                       std::int64_t delay_us,
                       rust::Box<RustTask> task) noexcept;
std::size_t run_ready_tasks(const NativeTaskQueueFactory& factory) noexcept;
std::size_t pump_ready_tasks(const NativeTaskQueueFactory& factory,
                            std::size_t budget) noexcept;
std::int64_t next_task_deadline_us(
    const NativeTaskQueueFactory& factory) noexcept;

std::unique_ptr<NativeEnvironment> create_environment(
    const NativeManualClock* clock,
    const NativeTaskQueueFactory& factory) noexcept;
std::unique_ptr<NativeEnvironment> clone_environment(
    const NativeEnvironment& environment) noexcept;
std::int64_t environment_time_us(const NativeEnvironment& environment) noexcept;

std::unique_ptr<NativeRandomnessLease> new_seeded_randomness(
    std::uint64_t seed) noexcept;
std::uint64_t next_seeded_random_u64(
    const NativeRandomnessLease& lease) noexcept;

std::unique_ptr<NativeThread> new_thread(bool network) noexcept;
std::unique_ptr<NativeDriverThread> new_driver_thread(
    const NativeManualClock& clock) noexcept;
std::unique_ptr<NativeDriverThread> new_seeded_driver_thread(
    const NativeManualClock& clock, std::uint64_t seed) noexcept;
std::size_t driver_pump(const NativeDriverThread& driver,
                        std::size_t budget) noexcept;
std::unique_ptr<NativeThread> borrow_driver_thread(
    const NativeDriverThread& driver) noexcept;
bool driver_run_ready(const NativeDriverThread& driver) noexcept;
bool test_driver_lifecycle_yield(
    const NativeDriverThread& driver,
    const NativeTaskQueueFactory& factory) noexcept;
std::int64_t driver_next_deadline_us(const NativeDriverThread& driver) noexcept;
bool driver_is_current(const NativeDriverThread& driver) noexcept;
bool thread_post_task(const NativeThread& thread,
                      rust::Box<RustTask> task) noexcept;
bool thread_post_delayed_task(const NativeThread& thread,
                              std::int64_t delay_us,
                              rust::Box<RustTask> task) noexcept;

}  // namespace pulsebeam::webrtc_sys
