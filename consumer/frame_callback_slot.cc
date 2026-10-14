#include <atomic>
#include <condition_variable>
#include <cstdlib>
#include <memory>
#include <mutex>
#include <thread>

#include "api/make_ref_counted.h"
#include "pulsebeam-webrtc-sys/native/frame_callback_slot.h"

namespace {
using pulsebeam::webrtc_sys::StockFrameCallbackSlot;

void Require(bool condition) {
  if (!condition) std::abort();
}

struct State {
  std::mutex mutex;
  std::condition_variable changed;
  std::weak_ptr<StockFrameCallbackSlot> slot;
  bool entered = false;
  bool release = false;
  bool returned = false;
  bool destroyed = false;
  std::thread::id destruction_role;
  std::atomic<unsigned> frames{0};
  std::atomic<unsigned> short_circuits{0};
  std::atomic<bool> unregistered{false};
};

// A native-only stand-in for the SDK callback, not an application binding.
// Blocking its call makes the unregister/late-completion ordering observable
// without sleeps, wall clocks, SDK pumping, or access to the slot's internals.
class Callback : public webrtc::TransformedFrameCallback {
 public:
  explicit Callback(std::shared_ptr<State> state) : state_(std::move(state)) {}
  void OnTransformedFrame(
      std::unique_ptr<webrtc::TransformableFrameInterface>) override {
    ++state_->frames;
    std::unique_lock lock(state_->mutex);
    state_->entered = true;
    state_->changed.notify_all();
    state_->changed.wait(lock, [this] { return state_->release; });
    state_->returned = true;
  }
  void StartShortCircuiting() override { ++state_->short_circuits; }

 protected:
  ~Callback() override {
    // Retirement must release the final native ref outside its mutex.
    if (auto slot = state_->slot.lock()) slot->StartShortCircuiting();
    std::lock_guard lock(state_->mutex);
    state_->destroyed = true;
    state_->destruction_role = std::this_thread::get_id();
  }

 private:
  std::shared_ptr<State> state_;
};
}  // namespace

void FrameCallbackSlotRegression() {
  auto state = std::make_shared<State>();
  auto slot = std::make_shared<StockFrameCallbackSlot>(
      webrtc::make_ref_counted<Callback>(state));
  state->slot = slot;
  std::thread completion([slot] { slot->OnTransformedFrame(nullptr); });
  {
    std::unique_lock lock(state->mutex);
    state->changed.wait(lock, [&] { return state->entered; });
  }
  std::thread unregister([slot, state] {
    const auto role = std::this_thread::get_id();
    slot->Retire();
    {
      std::lock_guard lock(state->mutex);
      Require(state->returned && state->destroyed);
      Require(state->destruction_role == role);
    }
    state->unregistered = true;
  });

  // Observe closed admission while the admitted frame is still blocked. The
  // fake short-circuit callback always increments, so rejection is observable.
  for (;;) {
    const auto before = state->short_circuits.load();
    slot->StartShortCircuiting();
    if (state->short_circuits.load() == before) break;
    std::this_thread::yield();
  }
  Require(!state->unregistered.load());
  {
    std::lock_guard lock(state->mutex);
    state->release = true;
    state->changed.notify_all();
  }
  completion.join();
  unregister.join();
  Require(state->unregistered.load());

  const auto shorts = state->short_circuits.load();
  slot->OnTransformedFrame(nullptr);
  slot->StartShortCircuiting();
  slot->Retire();  // Idempotent after the SDK reference has been released.
  Require(state->frames.load() == 1);
  Require(state->short_circuits.load() == shorts);
}
