#include "pulsebeam-webrtc-sys/native/data_channel.h"

#include <deque>
#include <mutex>
#include <optional>
#include <string>
#include <utility>

#include "pulsebeam-webrtc-sys/native/peer.h"
#include "pulsebeam-webrtc-sys/src/lib.rs.h"
#include "rtc_base/copy_on_write_buffer.h"
#include "rtc_base/thread.h"

namespace pulsebeam::webrtc_sys {
namespace {

enum EventKind : std::uint8_t {
  kNoEvent = 0,
  kStateChanged = 1,
  kMessage = 2,
  kBufferedAmountChanged = 3,
};

enum SendResult : std::uint8_t {
  kSent = 0,
  kNotOpen = 1,
  kBackpressure = 2,
  kSendFailed = 3,
};

struct EventState {
  void PushState() {
    std::lock_guard lock(mutex);
    if (closed) {
      return;
    }
    FfiDataChannelEvent event;
    event.kind = kStateChanged;
    event.state = 255;
    events.push_back(std::move(event));
  }

  void PushInitialState(webrtc::DataChannelInterface::DataState state) {
    std::lock_guard lock(mutex);
    FfiDataChannelEvent event;
    event.kind = kStateChanged;
    event.state = static_cast<std::uint8_t>(state);
    events.push_back(std::move(event));
  }

  void PushMessage(const webrtc::DataBuffer& buffer) {
    FfiDataChannelEvent event;
    event.kind = kMessage;
    event.binary = buffer.binary;
    event.data.reserve(buffer.data.size());
    for (std::uint8_t byte : buffer.data) {
      event.data.push_back(byte);
    }
    std::lock_guard lock(mutex);
    if (!closed) {
      events.push_back(std::move(event));
    }
  }

  void PushBufferedAmount(std::uint64_t sent_data_size) {
    FfiDataChannelEvent event;
    event.kind = kBufferedAmountChanged;
    event.sent_data_size = sent_data_size;
    std::lock_guard lock(mutex);
    if (!closed) {
      events.push_back(std::move(event));
    }
  }

  FfiDataChannelEvent Take(webrtc::DataChannelInterface::DataState state) {
    std::lock_guard lock(mutex);
    if (events.empty()) {
      return FfiDataChannelEvent{};
    }
    FfiDataChannelEvent event = std::move(events.front());
    events.pop_front();
    if (event.kind == kStateChanged) {
      if (event.state == 255) {
        event.state = static_cast<std::uint8_t>(state);
      }
      if (event.state == webrtc::DataChannelInterface::kClosed) {
        closed = true;
        events.clear();
      }
    }
    return event;
  }

  std::mutex mutex;
  std::deque<FfiDataChannelEvent> events;
  bool closed = false;
};

class DataChannelObserver final : public webrtc::DataChannelObserver {
 public:
  explicit DataChannelObserver(std::shared_ptr<EventState> events) noexcept
      : events_(std::move(events)) {}

  void OnStateChange() override { events_->PushState(); }
  void OnMessage(const webrtc::DataBuffer& buffer) override {
    events_->PushMessage(buffer);
  }
  void OnBufferedAmountChange(std::uint64_t sent_data_size) override {
    events_->PushBufferedAmount(sent_data_size);
  }

 private:
  std::shared_ptr<EventState> events_;
};

std::optional<webrtc::Priority> Priority(std::int8_t value) {
  switch (value) {
    case 0:
      return webrtc::Priority::kVeryLow;
    case 1:
      return webrtc::Priority::kLow;
    case 2:
      return webrtc::Priority::kMedium;
    case 3:
      return webrtc::Priority::kHigh;
    default:
      return std::nullopt;
  }
}

std::uint8_t PriorityIndex(webrtc::PriorityValue value) {
  switch (value.value()) {
    case 128:
      return 0;
    case 256:
      return 1;
    case 512:
      return 2;
    case 1024:
      return 3;
    default:
      return 1;
  }
}

}  // namespace

struct NativeDataChannel::State {
  webrtc::scoped_refptr<webrtc::DataChannelInterface> channel;
  webrtc::scoped_refptr<webrtc::PeerConnectionInterface> peer;
  std::shared_ptr<EventState> events;
  std::unique_ptr<DataChannelObserver> observer;
  webrtc::Thread* signaling_thread = nullptr;
};

NativeDataChannel::NativeDataChannel(std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeDataChannel::~NativeDataChannel() {
  if (!state_ || !state_->signaling_thread) {
    return;
  }
  state_->signaling_thread->BlockingCall([this] {
    if (state_->channel) {
      state_->channel->UnregisterObserver();
      state_->channel->Close();
      state_->channel = nullptr;
    }
    state_->peer = nullptr;
  });
  state_->observer.reset();
}
const std::unique_ptr<NativeDataChannel::State>& NativeDataChannel::state()
    const noexcept {
  return state_;
}

std::unique_ptr<NativeDataChannel> wrap_data_channel(
    webrtc::scoped_refptr<webrtc::DataChannelInterface> channel,
    webrtc::scoped_refptr<webrtc::PeerConnectionInterface> peer,
    webrtc::Thread* signaling_thread) noexcept {
  if (!channel || !peer || !signaling_thread) {
    return nullptr;
  }
  auto state = std::make_unique<NativeDataChannel::State>();
  state->channel = std::move(channel);
  state->peer = std::move(peer);
  state->events = std::make_shared<EventState>();
  state->observer = std::make_unique<DataChannelObserver>(state->events);
  state->signaling_thread = signaling_thread;
  state->signaling_thread->BlockingCall([&] {
    state->events->PushInitialState(state->channel->state());
    state->channel->RegisterObserver(state->observer.get());
  });
  return std::make_unique<NativeDataChannel>(std::move(state));
}

std::unique_ptr<NativeDataChannel> create_data_channel(
    const NativePeerConnection& peer,
    rust::Str label,
    bool ordered,
    std::int32_t max_retransmit_time_ms,
    std::int32_t max_retransmits,
    rust::Str protocol,
    bool negotiated,
    std::int32_t id,
    std::int8_t priority,
    std::uint8_t& error_type,
    rust::String& error) noexcept {
  webrtc::DataChannelInit configuration;
  configuration.ordered = ordered;
  if (max_retransmit_time_ms >= 0) {
    configuration.maxRetransmitTime = max_retransmit_time_ms;
  }
  if (max_retransmits >= 0) {
    configuration.maxRetransmits = max_retransmits;
  }
  configuration.protocol.assign(protocol.data(), protocol.size());
  configuration.negotiated = negotiated;
  configuration.id = id;
  if (auto parsed_priority = Priority(priority)) {
    configuration.priority = webrtc::PriorityValue(*parsed_priority);
  }
  auto result = peer.peer()->CreateDataChannelOrError(
      std::string(label.data(), label.size()), &configuration);
  if (!result.ok()) {
    error_type = static_cast<std::uint8_t>(result.error().type());
    error = result.error().message();
    return nullptr;
  }
  return wrap_data_channel(result.MoveValue(), peer.peer(),
                           peer.signaling_thread());
}

rust::String data_channel_label(const NativeDataChannel& channel) noexcept {
  return channel.state()->channel->label();
}
bool data_channel_ordered(const NativeDataChannel& channel) noexcept {
  return channel.state()->channel->ordered();
}
std::int32_t data_channel_max_retransmit_time_ms(
    const NativeDataChannel& channel) noexcept {
  return channel.state()->channel->maxPacketLifeTime().value_or(-1);
}
std::int32_t data_channel_max_retransmits(
    const NativeDataChannel& channel) noexcept {
  return channel.state()->channel->maxRetransmitsOpt().value_or(-1);
}
rust::String data_channel_protocol(const NativeDataChannel& channel) noexcept {
  return channel.state()->channel->protocol();
}
bool data_channel_negotiated(const NativeDataChannel& channel) noexcept {
  return channel.state()->channel->negotiated();
}
std::int32_t data_channel_id(const NativeDataChannel& channel) noexcept {
  return channel.state()->channel->id();
}
std::uint8_t data_channel_priority(const NativeDataChannel& channel) noexcept {
  return PriorityIndex(channel.state()->channel->priority());
}
std::uint8_t data_channel_state(const NativeDataChannel& channel) noexcept {
  return static_cast<std::uint8_t>(channel.state()->channel->state());
}
std::uint64_t data_channel_buffered_amount(
    const NativeDataChannel& channel) noexcept {
  return channel.state()->channel->buffered_amount();
}
std::uint8_t data_channel_send(const NativeDataChannel& channel,
                               rust::Slice<const std::uint8_t> data,
                               bool binary) noexcept {
  const auto& native = channel.state()->channel;
  if (native->state() != webrtc::DataChannelInterface::kOpen) {
    return kNotOpen;
  }
  const std::uint64_t queued = native->buffered_amount();
  const std::uint64_t capacity =
      webrtc::DataChannelInterface::MaxSendQueueSize();
  if (queued >= capacity || data.size() > capacity - queued) {
    return kBackpressure;
  }
  const webrtc::CopyOnWriteBuffer bytes(data.data(), data.size());
  return native->Send(webrtc::DataBuffer(bytes, binary)) ? kSent : kSendFailed;
}
FfiDataChannelEvent data_channel_take_event(
    const NativeDataChannel& channel) noexcept {
  return channel.state()->events->Take(channel.state()->channel->state());
}
bool close_data_channel(const NativeDataChannel& channel) noexcept {
  channel.state()->channel->Close();
  return true;
}

}  // namespace pulsebeam::webrtc_sys
