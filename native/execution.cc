#include "pulsebeam-webrtc-sys/native/execution.h"

#include <algorithm>
#include <atomic>
#include <limits>
#include <map>
#include <mutex>
#include <string>
#include <thread>
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
#include "rtc_base/event.h"
#include "rtc_base/null_socket_server.h"
#include "rtc_base/synchronization/yield_policy.h"
#include "rtc_base/thread.h"
#include "rtc_base/time_utils.h"
#include "system_wrappers/include/clock.h"

namespace pulsebeam::webrtc_sys {
namespace {

constexpr std::int64_t kNoDeadline = std::numeric_limits<std::int64_t>::min();
std::atomic_bool driver_active{false};
std::mutex hooks_mutex;
bool randomness_active = false;
thread_local std::vector<webrtc::TaskQueueBase*> suspended_queues;
// Stay within NTP era zero as well as the signed nanosecond clock. Native
// NTP seconds otherwise wrap in 2036 even though the microsecond clock fits.
constexpr std::int64_t kMaxTimeUs =
    (static_cast<std::int64_t>(std::numeric_limits<std::uint32_t>::max()) -
     2208988800LL) * 1000000 + 999999;

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
                       std::string name,
                       webrtc::TaskQueueBase* identity = nullptr,
                       bool lifecycle = true) noexcept
      : state_(std::move(state)), name_(std::move(name)),
        identity_(identity ? identity : this), lifecycle_(lifecycle) {}

  webrtc::TaskQueueBase* identity() const { return identity_; }
  bool lifecycle() const { return lifecycle_; }

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
  webrtc::TaskQueueBase* identity_;
  const bool lifecycle_;
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
  std::mutex factories_mutex;
  std::vector<std::weak_ptr<NativeTaskQueueFactory::State>> factories;
  std::atomic<std::uint64_t> next_sequence{0};
};

struct NativeTaskQueueFactory::State {
  State() noexcept : default_factory(webrtc::CreateDefaultTaskQueueFactory()) {}
  explicit State(std::shared_ptr<NativeManualClock::State> clock_state) noexcept
      : clock(std::move(clock_state)) {}

  bool cooperative() const noexcept { return clock != nullptr; }

  std::shared_ptr<NativeManualClock::State> clock;
  std::unique_ptr<webrtc::TaskQueueFactory> default_factory;
  mutable std::mutex mutex;
  const std::thread::id creator = std::this_thread::get_id();
  std::map<TaskKey, ScheduledTask> tasks;
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

namespace {
std::shared_ptr<NativeTaskQueueFactory::State> MakeCooperativeState(
    const std::shared_ptr<NativeManualClock::State>& clock) {
  auto state = std::make_shared<NativeTaskQueueFactory::State>(clock);
  std::lock_guard lock(clock->factories_mutex);
  // A clock is one cooperative dispatch domain. Reject foreign creators
  // before registration, otherwise one inaccessible factory stalls PumpClock
  // and leaves its ready deadline permanently visible to the caller.
  for (const auto& weak : clock->factories) {
    if (auto existing = weak.lock(); existing && existing->creator != state->creator)
      return nullptr;
  }
  clock->factories.push_back(state);
  return state;
}

std::size_t PumpClock(const std::shared_ptr<NativeManualClock::State>& clock,
                      std::size_t budget,
                      webrtc::TaskQueueBase* excluded = nullptr,
                      bool lifecycle_only = false);
std::int64_t ClockDeadline(
    const std::shared_ptr<NativeManualClock::State>& clock);

// Intercept thread posts rather than using upstream's elapsed-time pump,
// which has no finite dispatch bound at frozen virtual time.
class ControlledThread final : public webrtc::Thread {
 public:
  explicit ControlledThread(
      std::shared_ptr<NativeTaskQueueFactory::State> state)
      : webrtc::Thread(std::make_unique<webrtc::NullSocketServer>()),
        queue_(new CooperativeTaskQueue(std::move(state), "peer", this)) {}
  ~ControlledThread() override {
    Stop();
    ClearPending();
  }
  void ClearPending() {
    if (auto* queue = std::exchange(queue_, nullptr)) queue->Delete();
  }
 protected:
  void PostTaskImpl(absl::AnyInvocable<void() &&> task,
                    const PostTaskTraits& traits,
                    const webrtc::Location& location) override {
    if (queue_ && !IsQuitting()) queue_->PostTask(std::move(task), location);
  }
  void PostDelayedTaskImpl(absl::AnyInvocable<void() &&> task,
                           webrtc::TimeDelta delay,
                           const PostDelayedTaskTraits& traits,
                           const webrtc::Location& location) override {
    if (queue_ && !IsQuitting()) queue_->PostDelayedTask(std::move(task), delay, location);
  }
 private:
  CooperativeTaskQueue* queue_;
};
}  // namespace

struct NativeDriverThread::State : webrtc::YieldInterface {
  struct Clock final : webrtc::ClockInterface {
    explicit Clock(std::shared_ptr<NativeManualClock::State> value)
        : state(std::move(value)) {}
    std::int64_t TimeNanos() const override {
      const auto micros = state->clock.TimeInMicroseconds();
      RTC_CHECK_LE(micros, kMaxTimeUs);
      return micros * 1000;
    }
    std::shared_ptr<NativeManualClock::State> state;
  };

  State(std::unique_ptr<webrtc::Thread> value,
        std::shared_ptr<NativeManualClock::State> clock_state,
        bool seeded = false)
      : thread(std::move(value)), clock(std::move(clock_state)), seeded(seeded) {
    webrtc::SetClockForTesting(&clock);
    yield_policy = std::make_unique<webrtc::ScopedYieldPolicy>(this);
  }
  void YieldExecution() override {
    // Supported Event waits have finite engine-owned ready dependencies.
    // Caller queues are never native lifecycle dependencies: pumping them
    // here could hang even after the event was signaled if they self-repost.
    // Never move time or reenter a suspended queue.
    suspended_queues.push_back(webrtc::TaskQueueBase::Current());
    PumpClock(clock.state, std::numeric_limits<std::size_t>::max(),
              webrtc::TaskQueueBase::Current(), true);
    suspended_queues.pop_back();
  }
  ~State() {
    RTC_CHECK(thread->IsCurrent());
    thread->Quit();
    // Pending captures can release native peers/codec queues. Destroy them
    // while the driver is still current and cooperative lifecycle yields work.
    static_cast<ControlledThread*>(thread.get())->ClearPending();
    thread->UnwrapCurrent();
    thread.reset();
    yield_policy.reset();
    std::lock_guard lock(hooks_mutex);
    webrtc::SetClockForTesting(nullptr);
    if (seeded) {
      webrtc::SetDefaultRandomGenerator();
      randomness_active = false;
    }
    driver_active.store(false);
  }

  std::unique_ptr<webrtc::Thread> thread;
  Clock clock;
  bool seeded;
  std::unique_ptr<webrtc::ScopedYieldPolicy> yield_policy;
};

struct NativeThread::State {
  explicit State(std::unique_ptr<webrtc::Thread> value) noexcept
      : owned(std::move(value)), thread(owned.get()) {}
  explicit State(webrtc::Thread* value) noexcept : thread(value) {}

  std::unique_ptr<webrtc::Thread> owned;
  webrtc::Thread* thread;
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
  state_->tasks.emplace(TaskKey{deadline, state_->clock->next_sequence++},
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
  if (delta > kMaxTimeUs - now) return;
  const std::int64_t deadline = now + delta;
  state_->tasks.emplace(TaskKey{deadline, state_->clock->next_sequence++},
                        ScheduledTask{this, true, std::move(task)});
}

void CooperativeTaskQueue::Run(absl::AnyInvocable<void() &&> task) noexcept {
  CurrentTaskQueueSetter current(identity_);
  std::move(task)();
  task = nullptr;
}

void CooperativeTaskQueue::Delete() {
  RTC_CHECK(state_->creator == std::this_thread::get_id());
  bool destroy = false;
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
    destroy = state_->running[this] == 0;
    if (destroy) {
      state_->running.erase(this);
      state_->deleted.erase(this);
    }
  }
  {
    CurrentTaskQueueSetter current(identity_);
    immediate.clear();
  }
  delayed.clear();
  // Retiring a queue from inside its dispatch never waits for that dispatch.
  if (destroy) delete this;
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
  std::lock_guard lock(hooks_mutex);
  webrtc::SetDefaultRandomGenerator();
  randomness_active = false;
}

NativeThread::NativeThread(std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeDriverThread::NativeDriverThread(std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeDriverThread::~NativeDriverThread() = default;
const std::unique_ptr<NativeDriverThread::State>& NativeDriverThread::state()
    const noexcept {
  return state_;
}
webrtc::Thread* NativeDriverThread::thread() const noexcept {
  return state_ ? state_->thread.get() : nullptr;
}
bool NativeDriverThread::uses_clock(const NativeManualClock& clock) const noexcept {
  return state_ && state_->clock.state == clock.state();
}
NativeThread::~NativeThread() = default;
const std::unique_ptr<NativeThread::State>& NativeThread::state()
    const noexcept {
  return state_;
}
webrtc::Thread* NativeThread::thread() const noexcept {
  return state_ ? state_->thread : nullptr;
}

std::unique_ptr<NativeManualClock> new_manual_clock(
    std::int64_t initial_time_us) noexcept {
  if (initial_time_us < 0 || initial_time_us > kMaxTimeUs) return nullptr;
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
      delta_us > kMaxTimeUs - now) {
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
  auto state = MakeCooperativeState(clock.state());
  if (!state) return nullptr;
  return std::make_unique<NativeTaskQueueFactory>(std::move(state));
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
  if (!task_queue_factory_is_current(factory)) return nullptr;
  const absl::string_view queue_name(name.data(), name.size());
  std::unique_ptr<webrtc::TaskQueueBase, webrtc::TaskQueueDeleter> queue;
  if (factory.state()->cooperative()) {
    // This entry point creates public caller queues, unlike the factory
    // installed into a native Environment for engine-owned media work.
    queue.reset(new CooperativeTaskQueue(
        factory.state(), std::string(queue_name), nullptr, false));
  } else {
    queue = factory.state()->default_factory->CreateTaskQueue(
        queue_name, ToPriority(priority));
  }
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

namespace {
using QueueState = NativeTaskQueueFactory::State;

std::vector<std::shared_ptr<QueueState>> ClockFactories(
    const std::shared_ptr<NativeManualClock::State>& clock) {
  std::vector<std::shared_ptr<QueueState>> result;
  std::lock_guard lock(clock->factories_mutex);
  for (auto it = clock->factories.begin(); it != clock->factories.end();) {
    if (auto state = it->lock()) {
      result.push_back(std::move(state));
      ++it;
    } else {
      it = clock->factories.erase(it);
    }
  }
  return result;
}

std::size_t PumpStates(const std::vector<std::shared_ptr<QueueState>>& states,
                       std::size_t budget,
                       webrtc::TaskQueueBase* excluded = nullptr,
                       bool lifecycle_only = false) {
  for (const auto& state : states) {
    if (state->creator != std::this_thread::get_id()) return 0;
  }
  std::size_t count = 0;
  while (count < budget) {
    std::shared_ptr<QueueState> selected;
    TaskKey key{std::numeric_limits<std::int64_t>::max(),
                std::numeric_limits<std::uint64_t>::max()};
    for (const auto& state : states) {
      std::lock_guard lock(state->mutex);
      const auto now = state->clock->clock.TimeInMicroseconds();
      for (auto it = state->tasks.begin();
           it != state->tasks.end() && it->first.first <= now; ++it) {
        auto* queue = it->second.queue;
        if ((lifecycle_only && !queue->lifecycle()) ||
            state->running[queue] != 0 || queue->identity() == excluded ||
            std::find(suspended_queues.begin(), suspended_queues.end(),
                      queue->identity()) != suspended_queues.end()) continue;
        if (it->first < key) {
          selected = state;
          key = it->first;
        }
        break;
      }
    }
    if (!selected) break;
    CooperativeTaskQueue* queue;
    absl::AnyInvocable<void() &&> task;
    {
      std::lock_guard lock(selected->mutex);
      auto it = selected->tasks.find(key);
      queue = it->second.queue;
      ++selected->running[queue];
      task = std::move(it->second.task);
      selected->tasks.erase(it);
    }
    queue->Run(std::move(task));
    bool destroy = false;
    {
      std::lock_guard lock(selected->mutex);
      if (--selected->running[queue] == 0 && selected->deleted[queue]) {
        selected->running.erase(queue);
        selected->deleted.erase(queue);
        destroy = true;
      }
    }
    if (destroy) delete queue;
    ++count;
  }
  return count;
}

std::size_t PumpClock(const std::shared_ptr<NativeManualClock::State>& clock,
                      std::size_t budget, webrtc::TaskQueueBase* excluded,
                      bool lifecycle_only) {
  return PumpStates(ClockFactories(clock), budget, excluded, lifecycle_only);
}

std::int64_t ClockDeadline(
    const std::shared_ptr<NativeManualClock::State>& clock) {
  std::int64_t deadline = kNoDeadline;
  for (const auto& state : ClockFactories(clock)) {
    std::lock_guard lock(state->mutex);
    if (!state->tasks.empty()) {
      const auto value = state->tasks.begin()->first.first;
      if (deadline == kNoDeadline || value < deadline) deadline = value;
    }
  }
  return deadline;
}
}  // namespace

bool task_queue_factory_is_current(
    const NativeTaskQueueFactory& factory) noexcept {
  return !factory.state()->cooperative() ||
         factory.state()->creator == std::this_thread::get_id();
}

std::size_t pump_ready_tasks(const NativeTaskQueueFactory& factory,
                            std::size_t budget) noexcept {
  if (!factory.state()->cooperative()) return 0;
  return PumpStates({factory.state()}, budget);
}

std::size_t run_ready_tasks(const NativeTaskQueueFactory& factory) noexcept {
  return pump_ready_tasks(factory, 1024);
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
  if (!task_queue_factory_is_current(factory)) return nullptr;
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
  std::lock_guard lock(hooks_mutex);
  if (randomness_active) return nullptr;
  randomness_active = true;
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

std::unique_ptr<NativeDriverThread> new_driver_thread(
    const NativeManualClock& clock) noexcept {
  std::lock_guard lock(hooks_mutex);
  if (driver_active.load() || webrtc::Thread::Current() ||
      webrtc::GetClockForTesting()) return nullptr;
  auto queues = MakeCooperativeState(clock.state());
  if (!queues) return nullptr;
  auto thread = std::make_unique<ControlledThread>(std::move(queues));
  if (!thread->WrapCurrent()) return nullptr;
  driver_active.store(true);
  return std::make_unique<NativeDriverThread>(
      std::make_unique<NativeDriverThread::State>(std::move(thread), clock.state()));
}

std::unique_ptr<NativeThread> borrow_driver_thread(
    const NativeDriverThread& driver) noexcept {
  if (!driver.thread() || !driver.thread()->IsCurrent()) return nullptr;
  return std::make_unique<NativeThread>(
      std::make_unique<NativeThread::State>(driver.thread()));
}

std::unique_ptr<NativeDriverThread> new_seeded_driver_thread(
    const NativeManualClock& clock, std::uint64_t seed) noexcept {
  std::lock_guard lock(hooks_mutex);
  if (driver_active.load() || randomness_active || webrtc::Thread::Current() ||
      webrtc::GetClockForTesting()) return nullptr;
  auto queues = MakeCooperativeState(clock.state());
  if (!queues) return nullptr;
  auto thread = std::make_unique<ControlledThread>(std::move(queues));
  if (!thread->WrapCurrent()) return nullptr;
  driver_active.store(true);
  randomness_active = true;
  webrtc::SetRandomGenerator(std::make_unique<SeededRandom>(seed));
  return std::make_unique<NativeDriverThread>(
      std::make_unique<NativeDriverThread::State>(std::move(thread), clock.state(), true));
}

std::size_t driver_pump(const NativeDriverThread& driver,
                        std::size_t budget) noexcept {
  if (!driver.thread() || !driver.thread()->IsCurrent()) return 0;
  return PumpClock(driver.state()->clock.state, budget);
}

bool driver_run_ready(const NativeDriverThread& driver) noexcept {
  if (!driver.thread() || !driver.thread()->IsCurrent()) return false;
  driver_pump(driver, 1024);
  return true;
}

bool test_driver_lifecycle_yield(
    const NativeDriverThread& driver,
    const NativeTaskQueueFactory& factory) noexcept {
  if (!driver_is_current(driver) ||
      factory.state()->clock != driver.state()->clock.state ||
      !task_queue_factory_is_current(factory)) return false;
  const auto before = driver.state()->clock.state->clock.TimeInMicroseconds();
  webrtc::Event event;
  CooperativeTaskQueueFactory implementation(factory.state());
  auto queue = implementation.CreateTaskQueue(
      "lifecycle-probe", webrtc::TaskQueueFactory::Priority::NORMAL);
  queue->PostTask([&event] { event.Set(); });
  return event.Wait(webrtc::TimeDelta::PlusInfinity()) &&
         driver.state()->clock.state->clock.TimeInMicroseconds() == before;
}

std::int64_t driver_next_deadline_us(const NativeDriverThread& driver) noexcept {
  if (!driver.thread() || !driver.thread()->IsCurrent()) return kNoDeadline;
  return ClockDeadline(driver.state()->clock.state);
}

bool driver_is_current(const NativeDriverThread& driver) noexcept {
  return driver.thread() && driver.thread()->IsCurrent();
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
