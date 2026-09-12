#include "pulsebeam-webrtc-sys/native/execution.h"

#include <algorithm>
#include <atomic>
#include <condition_variable>
#include <limits>
#include <map>
#include <mutex>
#include <string>
#include <utility>
#include <vector>

#include "api/environment/environment.h"
#include "api/environment/environment_factory.h"
#include "api/task_queue/default_task_queue_factory.h"
#include "api/task_queue/task_queue_base.h"
#include "api/task_queue/task_queue_factory.h"
#include "api/units/time_delta.h"
#include "api/units/timestamp.h"
#include "pulsebeam-webrtc-sys/src/lib.rs.h"
#include "rtc_base/crypto_random.h"
#include "rtc_base/thread.h"
#include "system_wrappers/include/clock.h"

namespace pulsebeam::webrtc_sys {
namespace {

constexpr std::int64_t kNoDeadline = std::numeric_limits<std::int64_t>::min();

class SharedClock final : public webrtc::Clock {
 public:
  explicit SharedClock(std::shared_ptr<NativeManualClock::State> state) noexcept
      : state_(std::move(state)) {}

  webrtc::Timestamp CurrentTime() override;
  webrtc::NtpTime ConvertTimestampToNtpTime(
      webrtc::Timestamp timestamp) override;

 private:
  std::shared_ptr<NativeManualClock::State> state_;
};

class CooperativeTaskQueue;

struct ScheduledTask {
  CooperativeTaskQueue* queue;
  bool delayed;
  absl::AnyInvocable<void() &&> task;
};

using TaskKey = std::pair<std::int64_t, std::uint64_t>;

class CooperativeTaskQueue final : public webrtc::TaskQueueBase {
 public:
  CooperativeTaskQueue(std::shared_ptr<NativeTaskQueueFactory::State> state,
                       std::string name) noexcept
      : state_(std::move(state)), name_(std::move(name)) {}

  absl::string_view queue_name() const override { return name_; }
  void Delete() override;
  void Run(absl::AnyInvocable<void() &&> task) noexcept;

 protected:
  void PostTaskImpl(absl::AnyInvocable<void() &&> task,
                    const PostTaskTraits&,
                    const webrtc::Location&) override;
  void PostDelayedTaskImpl(absl::AnyInvocable<void() &&> task,
                           webrtc::TimeDelta delay,
                           const PostDelayedTaskTraits&,
                           const webrtc::Location&) override;

 private:
  std::shared_ptr<NativeTaskQueueFactory::State> state_;
  std::string name_;
};

class CooperativeTaskQueueFactory final : public webrtc::TaskQueueFactory {
 public:
  explicit CooperativeTaskQueueFactory(
      std::shared_ptr<NativeTaskQueueFactory::State> state) noexcept
      : state_(std::move(state)) {}

  std::unique_ptr<webrtc::TaskQueueBase, webrtc::TaskQueueDeleter>
  CreateTaskQueue(absl::string_view name, Priority) const override {
    return std::unique_ptr<webrtc::TaskQueueBase, webrtc::TaskQueueDeleter>(
        new CooperativeTaskQueue(state_, std::string(name)));
  }

 private:
  std::shared_ptr<NativeTaskQueueFactory::State> state_;
};

class SeededRandom final : public webrtc::RandomGenerator {
 public:
  explicit SeededRandom(std::uint64_t seed) noexcept : state_(seed) {}

  bool Generate(void* output, std::size_t length) override {
    std::lock_guard lock(mutex_);
    auto* bytes = static_cast<std::uint8_t*>(output);
    while (length != 0) {
      std::uint64_t value = Next();
      const std::size_t count = std::min(length, sizeof(value));
      for (std::size_t index = 0; index < count; ++index) {
        bytes[index] = static_cast<std::uint8_t>(value >> (index * 8));
      }
      bytes += count;
      length -= count;
    }
    return true;
  }

 private:
  std::uint64_t Next() noexcept {
    std::uint64_t value = (state_ += 0x9e3779b97f4a7c15ULL);
    value = (value ^ (value >> 30)) * 0xbf58476d1ce4e5b9ULL;
    value = (value ^ (value >> 27)) * 0x94d049bb133111ebULL;
    return value ^ (value >> 31);
  }

  std::mutex mutex_;
  std::uint64_t state_;
};

webrtc::TaskQueueFactory::Priority ToPriority(std::uint8_t priority) noexcept {
  switch (priority) {
    case 1:
      return webrtc::TaskQueueFactory::Priority::kHigh;
    case 2:
      return webrtc::TaskQueueFactory::Priority::kVideo;
    case 3:
      return webrtc::TaskQueueFactory::Priority::kAudio;
    case 4:
      return webrtc::TaskQueueFactory::Priority::kLow;
    default:
      return webrtc::TaskQueueFactory::Priority::kNormal;
  }
}

}  // namespace

struct NativeManualClock::State {
  explicit State(std::int64_t initial_time_us) noexcept
      : clock(initial_time_us) {}

  webrtc::SimulatedClock clock;
  std::mutex advance_mutex;
};

struct NativeTaskQueueFactory::State {
  State() noexcept : default_factory(webrtc::CreateDefaultTaskQueueFactory()) {}
  explicit State(std::shared_ptr<NativeManualClock::State> clock_state) noexcept
      : clock(std::move(clock_state)) {}

  bool cooperative() const noexcept { return clock != nullptr; }

  std::shared_ptr<NativeManualClock::State> clock;
  std::unique_ptr<webrtc::TaskQueueFactory> default_factory;
  mutable std::mutex mutex;
  std::condition_variable idle;
  std::map<TaskKey, ScheduledTask> tasks;
  std::uint64_t next_sequence = 0;
  std::map<CooperativeTaskQueue*, std::size_t> running;
  std::map<CooperativeTaskQueue*, bool> deleted;
};

struct NativeTaskQueue::State {
  explicit State(std::unique_ptr<webrtc::TaskQueueBase,
                                 webrtc::TaskQueueDeleter> queue) noexcept
      : queue(std::move(queue)) {}

  std::unique_ptr<webrtc::TaskQueueBase, webrtc::TaskQueueDeleter> queue;
};

struct NativeEnvironment::State {
  explicit State(webrtc::Environment value) noexcept
      : environment(std::move(value)) {}

  webrtc::Environment environment;
};

struct NativeThread::State {
  explicit State(std::unique_ptr<webrtc::Thread> value) noexcept
      : thread(std::move(value)) {}

  std::unique_ptr<webrtc::Thread> thread;
};

webrtc::Timestamp SharedClock::CurrentTime() {
  return state_->clock.CurrentTime();
}

webrtc::NtpTime SharedClock::ConvertTimestampToNtpTime(
    webrtc::Timestamp timestamp) {
  return state_->clock.ConvertTimestampToNtpTime(timestamp);
}

void CooperativeTaskQueue::PostTaskImpl(absl::AnyInvocable<void() &&> task,
                                        const PostTaskTraits&,
                                        const webrtc::Location&) {
  std::lock_guard lock(state_->mutex);
  if (state_->deleted[this]) {
    return;
  }
  const std::int64_t deadline = state_->clock->clock.TimeInMicroseconds();
  state_->tasks.emplace(TaskKey{deadline, state_->next_sequence++},
                        ScheduledTask{this, false, std::move(task)});
}

void CooperativeTaskQueue::PostDelayedTaskImpl(
    absl::AnyInvocable<void() &&> task,
    webrtc::TimeDelta delay,
    const PostDelayedTaskTraits&,
    const webrtc::Location&) {
  std::lock_guard lock(state_->mutex);
  if (state_->deleted[this]) {
    return;
  }
  const std::int64_t now = state_->clock->clock.TimeInMicroseconds();
  const std::int64_t delta = std::max<std::int64_t>(0, delay.us());
  const std::int64_t deadline =
      delta > std::numeric_limits<std::int64_t>::max() - now
          ? std::numeric_limits<std::int64_t>::max()
          : now + delta;
  state_->tasks.emplace(TaskKey{deadline, state_->next_sequence++},
                        ScheduledTask{this, true, std::move(task)});
}

void CooperativeTaskQueue::Run(absl::AnyInvocable<void() &&> task) noexcept {
  CurrentTaskQueueSetter current(this);
  std::move(task)();
}

void CooperativeTaskQueue::Delete() {
  std::vector<absl::AnyInvocable<void() &&>> immediate;
  std::vector<absl::AnyInvocable<void() &&>> delayed;
  {
    std::unique_lock lock(state_->mutex);
    state_->deleted[this] = true;
    for (auto iterator = state_->tasks.begin();
         iterator != state_->tasks.end();) {
      if (iterator->second.queue != this) {
        ++iterator;
        continue;
      }
      auto& destination = iterator->second.delayed ? delayed : immediate;
      destination.push_back(std::move(iterator->second.task));
      iterator = state_->tasks.erase(iterator);
    }
    state_->idle.wait(lock, [&] { return state_->running[this] == 0; });
    state_->running.erase(this);
    state_->deleted.erase(this);
  }
  {
    CurrentTaskQueueSetter current(this);
    immediate.clear();
  }
  delayed.clear();
  delete this;
}

NativeManualClock::NativeManualClock(std::shared_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeManualClock::~NativeManualClock() = default;
const std::shared_ptr<NativeManualClock::State>& NativeManualClock::state()
    const noexcept {
  return state_;
}

NativeTaskQueueFactory::NativeTaskQueueFactory(
    std::shared_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeTaskQueueFactory::~NativeTaskQueueFactory() = default;
const std::shared_ptr<NativeTaskQueueFactory::State>&
NativeTaskQueueFactory::state() const noexcept {
  return state_;
}

NativeTaskQueue::NativeTaskQueue(std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeTaskQueue::~NativeTaskQueue() = default;
const std::unique_ptr<NativeTaskQueue::State>& NativeTaskQueue::state()
    const noexcept {
  return state_;
}

NativeEnvironment::NativeEnvironment(std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeEnvironment::~NativeEnvironment() = default;
const std::unique_ptr<NativeEnvironment::State>& NativeEnvironment::state()
    const noexcept {
  return state_;
}
webrtc::Environment NativeEnvironment::environment() const noexcept {
  return state_->environment;
}

NativeRandomnessLease::~NativeRandomnessLease() {
  webrtc::SetDefaultRandomGenerator();
}

NativeThread::NativeThread(std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeThread::~NativeThread() = default;
const std::unique_ptr<NativeThread::State>& NativeThread::state()
    const noexcept {
  return state_;
}
webrtc::Thread* NativeThread::thread() const noexcept {
  return state_ ? state_->thread.get() : nullptr;
}

std::unique_ptr<NativeManualClock> new_manual_clock(
    std::int64_t initial_time_us) noexcept {
  return std::make_unique<NativeManualClock>(
      std::make_shared<NativeManualClock::State>(initial_time_us));
}

std::int64_t manual_clock_time_us(const NativeManualClock& clock) noexcept {
  return clock.state()->clock.TimeInMicroseconds();
}

bool advance_manual_clock(const NativeManualClock& clock,
                          std::int64_t delta_us) noexcept {
  std::lock_guard lock(clock.state()->advance_mutex);
  const std::int64_t now = clock.state()->clock.TimeInMicroseconds();
  if (delta_us < 0 ||
      delta_us > std::numeric_limits<std::int64_t>::max() - now) {
    return false;
  }
  clock.state()->clock.AdvanceTimeMicroseconds(delta_us);
  return true;
}

std::int64_t system_clock_time_us() noexcept {
  return webrtc::Clock::GetRealTimeClock()->TimeInMicroseconds();
}

std::unique_ptr<NativeTaskQueueFactory>
new_default_task_queue_factory() noexcept {
  return std::make_unique<NativeTaskQueueFactory>(
      std::make_shared<NativeTaskQueueFactory::State>());
}

std::unique_ptr<NativeTaskQueueFactory> new_cooperative_task_queue_factory(
    const NativeManualClock& clock) noexcept {
  return std::make_unique<NativeTaskQueueFactory>(
      std::make_shared<NativeTaskQueueFactory::State>(clock.state()));
}

bool task_queue_factory_is_cooperative(
    const NativeTaskQueueFactory& factory) noexcept {
  return factory.state()->cooperative();
}

bool task_queue_factory_uses_clock(const NativeTaskQueueFactory& factory,
                                   const NativeManualClock& clock) noexcept {
  return factory.state()->clock == clock.state();
}

std::unique_ptr<NativeTaskQueue> create_task_queue(
    const NativeTaskQueueFactory& factory,
    rust::Str name,
    std::uint8_t priority) noexcept {
  const absl::string_view queue_name(name.data(), name.size());
  std::unique_ptr<webrtc::TaskQueueFactory> cooperative_factory;
  webrtc::TaskQueueFactory* implementation =
      factory.state()->default_factory.get();
  if (factory.state()->cooperative()) {
    cooperative_factory =
        std::make_unique<CooperativeTaskQueueFactory>(factory.state());
    implementation = cooperative_factory.get();
  }
  auto queue =
      implementation->CreateTaskQueue(queue_name, ToPriority(priority));
  if (!queue) {
    return nullptr;
  }
  return std::make_unique<NativeTaskQueue>(
      std::make_unique<NativeTaskQueue::State>(std::move(queue)));
}

bool post_task(const NativeTaskQueue& queue,
               rust::Box<RustTask> task) noexcept {
  if (!queue.state() || !queue.state()->queue) {
    return false;
  }
  queue.state()->queue->PostTask(
      [task = std::move(task)]() mutable { run_task(std::move(task)); });
  return true;
}

bool post_delayed_task(const NativeTaskQueue& queue,
                       std::int64_t delay_us,
                       rust::Box<RustTask> task) noexcept {
  if (!queue.state() || !queue.state()->queue || delay_us < 0) {
    return false;
  }
  queue.state()->queue->PostDelayedTask(
      [task = std::move(task)]() mutable { run_task(std::move(task)); },
      webrtc::TimeDelta::Micros(delay_us));
  return true;
}

std::size_t run_ready_tasks(const NativeTaskQueueFactory& factory) noexcept {
  const auto& state = factory.state();
  if (!state->cooperative()) {
    return 0;
  }
  std::size_t count = 0;
  for (;;) {
    CooperativeTaskQueue* queue;
    absl::AnyInvocable<void() &&> task;
    {
      std::lock_guard lock(state->mutex);
      const std::int64_t now = state->clock->clock.TimeInMicroseconds();
      auto iterator = state->tasks.begin();
      while (iterator != state->tasks.end() && iterator->first.first <= now &&
             state->running[iterator->second.queue] != 0) {
        ++iterator;
      }
      if (iterator == state->tasks.end() || iterator->first.first > now) {
        break;
      }
      queue = iterator->second.queue;
      if (state->deleted[queue]) {
        state->tasks.erase(iterator);
        continue;
      }
      ++state->running[queue];
      task = std::move(iterator->second.task);
      state->tasks.erase(iterator);
    }
    queue->Run(std::move(task));
    {
      std::lock_guard lock(state->mutex);
      --state->running[queue];
      state->idle.notify_all();
    }
    ++count;
  }
  return count;
}

std::int64_t next_task_deadline_us(
    const NativeTaskQueueFactory& factory) noexcept {
  const auto& state = factory.state();
  if (!state->cooperative()) {
    return kNoDeadline;
  }
  std::lock_guard lock(state->mutex);
  return state->tasks.empty() ? kNoDeadline : state->tasks.begin()->first.first;
}

std::unique_ptr<NativeEnvironment> create_environment(
    const NativeManualClock* clock,
    const NativeTaskQueueFactory& factory) noexcept {
  webrtc::EnvironmentFactory environment_factory;
  if (clock != nullptr) {
    environment_factory.Set(std::make_unique<SharedClock>(clock->state()));
  }
  if (factory.state()->cooperative()) {
    environment_factory.Set(
        std::make_unique<CooperativeTaskQueueFactory>(factory.state()));
  } else {
    environment_factory.Set(webrtc::CreateDefaultTaskQueueFactory());
  }
  return std::make_unique<NativeEnvironment>(
      std::make_unique<NativeEnvironment::State>(environment_factory.Create()));
}

std::unique_ptr<NativeEnvironment> clone_environment(
    const NativeEnvironment& environment) noexcept {
  return std::make_unique<NativeEnvironment>(
      std::make_unique<NativeEnvironment::State>(
          environment.state()->environment));
}

std::int64_t environment_time_us(
    const NativeEnvironment& environment) noexcept {
  return environment.state()->environment.clock().TimeInMicroseconds();
}

std::unique_ptr<NativeRandomnessLease> new_seeded_randomness(
    std::uint64_t seed) noexcept {
  webrtc::SetRandomGenerator(std::make_unique<SeededRandom>(seed));
  return std::make_unique<NativeRandomnessLease>();
}

std::uint64_t next_seeded_random_u64(const NativeRandomnessLease&) noexcept {
  return webrtc::CreateRandomId64();
}

std::unique_ptr<NativeThread> new_thread(bool network) noexcept {
  std::unique_ptr<webrtc::Thread> thread =
      network ? webrtc::Thread::CreateWithSocketServer()
              : webrtc::Thread::Create();
  if (!thread || !thread->Start()) {
    return nullptr;
  }
  return std::make_unique<NativeThread>(
      std::make_unique<NativeThread::State>(std::move(thread)));
}

bool thread_post_task(const NativeThread& thread,
                      rust::Box<RustTask> task) noexcept {
  if (!thread.state() || !thread.state()->thread ||
      thread.state()->thread->IsQuitting()) {
    return false;
  }
  thread.state()->thread->PostTask(
      [task = std::move(task)]() mutable { run_task(std::move(task)); });
  return true;
}

bool thread_post_delayed_task(const NativeThread& thread,
                              std::int64_t delay_us,
                              rust::Box<RustTask> task) noexcept {
  if (!thread.state() || !thread.state()->thread || delay_us < 0 ||
      thread.state()->thread->IsQuitting()) {
    return false;
  }
  thread.state()->thread->PostDelayedTask(
      [task = std::move(task)]() mutable { run_task(std::move(task)); },
      webrtc::TimeDelta::Micros(delay_us));
  return true;
}

}  // namespace pulsebeam::webrtc_sys
