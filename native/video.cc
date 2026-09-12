#include "pulsebeam-webrtc-sys/native/video.h"

#include <atomic>
#include <cstring>
#include <deque>
#include <mutex>
#include <optional>
#include <string>
#include <utility>

#include "api/make_ref_counted.h"
#include "api/media_stream_interface.h"
#include "api/peer_connection_interface.h"
#include "api/rtp_receiver_interface.h"
#include "api/rtp_sender_interface.h"
#include "api/rtp_transceiver_interface.h"
#include "api/scoped_refptr.h"
#include "api/video/i420_buffer.h"
#include "api/video/video_frame.h"
#include "api/video/video_sink_interface.h"
#include "media/base/video_broadcaster.h"
#include "pc/video_track_source.h"
#include "pulsebeam-webrtc-sys/native/codec.h"
#include "pulsebeam-webrtc-sys/native/peer.h"
#include "rtc_base/thread.h"

namespace pulsebeam::webrtc_sys {
namespace {

class PushVideoSource : public webrtc::VideoTrackSource {
 public:
  PushVideoSource() : VideoTrackSource(false) {}

  void Push(const webrtc::VideoFrame& frame) { broadcaster_.OnFrame(frame); }

 protected:
  webrtc::VideoSourceInterface<webrtc::VideoFrame>* source() override {
    return &broadcaster_;
  }

 private:
  webrtc::VideoBroadcaster broadcaster_;
};

class FrameSink final : public webrtc::VideoSinkInterface<webrtc::VideoFrame> {
 public:
  void OnFrame(const webrtc::VideoFrame& frame) override {
    std::lock_guard lock(mutex_);
    if (active_) {
      frames_.push_back(wrap_video_frame(frame));
    }
  }

  std::unique_ptr<NativeVideoFrame> Take() {
    std::lock_guard lock(mutex_);
    if (frames_.empty()) {
      return nullptr;
    }
    auto frame = std::move(frames_.front());
    frames_.pop_front();
    return frame;
  }

  void Deactivate() {
    std::lock_guard lock(mutex_);
    active_ = false;
    frames_.clear();
  }

 private:
  std::mutex mutex_;
  std::deque<std::unique_ptr<NativeVideoFrame>> frames_;
  bool active_ = true;
};

std::optional<webrtc::RtpTransceiverDirection> Direction(std::uint8_t value) {
  if (value > static_cast<std::uint8_t>(
                  webrtc::RtpTransceiverDirection::kInactive)) {
    return std::nullopt;
  }
  return static_cast<webrtc::RtpTransceiverDirection>(value);
}

void SetError(const webrtc::RTCError& rtc_error, std::uint8_t& error_type,
              rust::String& error) {
  error_type = static_cast<std::uint8_t>(rtc_error.type());
  error = rtc_error.message();
}

webrtc::scoped_refptr<webrtc::VideoTrackInterface> VideoTrack(
    const webrtc::scoped_refptr<webrtc::MediaStreamTrackInterface>& track) {
  if (!track || track->kind() != webrtc::MediaStreamTrackInterface::kVideoKind) {
    return nullptr;
  }
  return webrtc::scoped_refptr<webrtc::VideoTrackInterface>(
      static_cast<webrtc::VideoTrackInterface*>(track.get()));
}

}  // namespace

struct NativeVideoSource::State {
  webrtc::scoped_refptr<PushVideoSource> source;
  webrtc::Thread* signaling_thread = nullptr;
  std::atomic<bool> closed{false};
};

struct NativeVideoTrack::State {
  webrtc::scoped_refptr<webrtc::VideoTrackInterface> track;
};

struct NativeVideoSink::State {
  webrtc::scoped_refptr<webrtc::VideoTrackInterface> track;
  std::unique_ptr<FrameSink> sink;
  std::atomic<bool> closed{false};
};

struct NativeRtpSender::State {
  webrtc::scoped_refptr<webrtc::RtpSenderInterface> sender;
};

struct NativeRtpReceiver::State {
  webrtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver;
};

struct NativeRtpTransceiver::State {
  webrtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver;
};

NativeVideoSource::NativeVideoSource(std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeVideoSource::~NativeVideoSource() {
  close_video_source(*this);
  if (state_ && state_->source && state_->signaling_thread) {
    state_->signaling_thread->BlockingCall([this] { state_->source = nullptr; });
  }
}
const std::unique_ptr<NativeVideoSource::State>&
NativeVideoSource::state() const noexcept {
  return state_;
}

NativeVideoTrack::NativeVideoTrack(std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeVideoTrack::~NativeVideoTrack() = default;
const std::unique_ptr<NativeVideoTrack::State>&
NativeVideoTrack::state() const noexcept {
  return state_;
}

NativeVideoSink::NativeVideoSink(std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeVideoSink::~NativeVideoSink() { close_video_sink(*this); }
const std::unique_ptr<NativeVideoSink::State>&
NativeVideoSink::state() const noexcept {
  return state_;
}

NativeRtpSender::NativeRtpSender(std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeRtpSender::~NativeRtpSender() = default;
const std::unique_ptr<NativeRtpSender::State>&
NativeRtpSender::state() const noexcept {
  return state_;
}

NativeRtpReceiver::NativeRtpReceiver(std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeRtpReceiver::~NativeRtpReceiver() = default;
const std::unique_ptr<NativeRtpReceiver::State>&
NativeRtpReceiver::state() const noexcept {
  return state_;
}

NativeRtpTransceiver::NativeRtpTransceiver(
    std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeRtpTransceiver::~NativeRtpTransceiver() = default;
const std::unique_ptr<NativeRtpTransceiver::State>&
NativeRtpTransceiver::state() const noexcept {
  return state_;
}

std::unique_ptr<NativeVideoSource> create_video_source(
    const NativePeerConnectionFactory& factory) noexcept {
  auto state = std::make_unique<NativeVideoSource::State>();
  state->signaling_thread = factory.signaling_thread();
  if (!state->signaling_thread) {
    return nullptr;
  }
  state->signaling_thread->BlockingCall(
      [&state] {
        state->source = webrtc::make_ref_counted<PushVideoSource>();
        state->source->SetState(webrtc::MediaSourceInterface::kLive);
      });
  return std::make_unique<NativeVideoSource>(std::move(state));
}

bool close_video_source(const NativeVideoSource& source) noexcept {
  auto& state = *source.state();
  if (state.closed.exchange(true)) {
    return true;
  }
  if (!state.source || !state.signaling_thread) {
    return false;
  }
  state.signaling_thread->BlockingCall(
      [&state] { state.source->SetState(webrtc::MediaSourceInterface::kEnded); });
  return true;
}

std::uint8_t video_source_state(const NativeVideoSource& source) noexcept {
  return source.state()->closed.load()
             ? static_cast<std::uint8_t>(webrtc::MediaSourceInterface::kEnded)
             : static_cast<std::uint8_t>(webrtc::MediaSourceInterface::kLive);
}

bool video_source_push_frame(const NativeVideoSource& source,
                             rust::Slice<const std::uint8_t> data,
                             std::uint32_t width, std::uint32_t height,
                             std::int64_t timestamp_us,
                             std::uint32_t rtp_timestamp) noexcept {
  if (source.state()->closed.load() || width == 0 || height == 0 ||
      width > static_cast<std::uint32_t>(INT32_MAX) ||
      height > static_cast<std::uint32_t>(INT32_MAX)) {
    return false;
  }
  const std::size_t chroma_width = (static_cast<std::size_t>(width) + 1) / 2;
  const std::size_t chroma_height = (static_cast<std::size_t>(height) + 1) / 2;
  const std::size_t y_size = static_cast<std::size_t>(width) * height;
  const std::size_t chroma_size = chroma_width * chroma_height;
  if (data.size() != y_size + 2 * chroma_size) {
    return false;
  }
  auto buffer = webrtc::I420Buffer::Create(static_cast<int>(width),
                                           static_cast<int>(height));
  std::memcpy(buffer->MutableDataY(), data.data(), y_size);
  std::memcpy(buffer->MutableDataU(), data.data() + y_size, chroma_size);
  std::memcpy(buffer->MutableDataV(), data.data() + y_size + chroma_size,
              chroma_size);
  source.state()->source->Push(
      webrtc::VideoFrame::Builder()
          .set_video_frame_buffer(std::move(buffer))
          .set_timestamp_us(timestamp_us)
          .set_rtp_timestamp(rtp_timestamp)
          .build());
  return true;
}

std::unique_ptr<NativeVideoTrack> create_video_track(
    const NativePeerConnectionFactory& factory,
    const NativeVideoSource& source, rust::Str id) noexcept {
  if (source.state()->closed.load()) {
    return nullptr;
  }
  auto track = factory.factory()->CreateVideoTrack(
      source.state()->source, std::string_view(id.data(), id.size()));
  if (!track) {
    return nullptr;
  }
  auto state = std::make_unique<NativeVideoTrack::State>();
  state->track = std::move(track);
  return std::make_unique<NativeVideoTrack>(std::move(state));
}

rust::String video_track_id(const NativeVideoTrack& track) noexcept {
  return track.state()->track->id();
}
bool video_track_enabled(const NativeVideoTrack& track) noexcept {
  return track.state()->track->enabled();
}
bool video_track_set_enabled(const NativeVideoTrack& track,
                             bool enabled) noexcept {
  return track.state()->track->set_enabled(enabled);
}
std::uint8_t video_track_state(const NativeVideoTrack& track) noexcept {
  return static_cast<std::uint8_t>(track.state()->track->state());
}

std::unique_ptr<NativeVideoSink> video_track_attach_sink(
    const NativeVideoTrack& track) noexcept {
  auto state = std::make_unique<NativeVideoSink::State>();
  state->track = track.state()->track;
  state->sink = std::make_unique<FrameSink>();
  state->track->AddOrUpdateSink(state->sink.get(), webrtc::VideoSinkWants{});
  return std::make_unique<NativeVideoSink>(std::move(state));
}

std::unique_ptr<NativeVideoFrame> video_sink_take_frame(
    const NativeVideoSink& sink) noexcept {
  return sink.state()->sink ? sink.state()->sink->Take() : nullptr;
}

bool close_video_sink(const NativeVideoSink& sink) noexcept {
  auto& state = *sink.state();
  if (state.closed.exchange(true)) {
    return true;
  }
  if (!state.track || !state.sink) {
    return false;
  }
  state.sink->Deactivate();
  state.track->RemoveSink(state.sink.get());
  return true;
}

std::unique_ptr<NativeRtpTransceiver> wrap_rtp_transceiver(
    webrtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver) noexcept {
  if (!transceiver) {
    return nullptr;
  }
  auto state = std::make_unique<NativeRtpTransceiver::State>();
  state->transceiver = std::move(transceiver);
  return std::make_unique<NativeRtpTransceiver>(std::move(state));
}

std::unique_ptr<NativeRtpReceiver> wrap_rtp_receiver(
    webrtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver) noexcept {
  if (!receiver) {
    return nullptr;
  }
  auto state = std::make_unique<NativeRtpReceiver::State>();
  state->receiver = std::move(receiver);
  return std::make_unique<NativeRtpReceiver>(std::move(state));
}

std::unique_ptr<NativeRtpTransceiver> peer_add_video_transceiver(
    const NativePeerConnection& peer, const NativeVideoTrack& track,
    std::uint8_t direction, std::uint8_t& error_type,
    rust::String& error) noexcept {
  const auto parsed = Direction(direction);
  if (!parsed) {
    error_type = static_cast<std::uint8_t>(webrtc::RTCErrorType::INVALID_PARAMETER);
    error = "invalid RTP transceiver direction";
    return nullptr;
  }
  webrtc::RtpTransceiverInit init;
  init.direction = *parsed;
  auto result = peer.peer()->AddTransceiver(track.state()->track, init);
  if (!result.ok()) {
    SetError(result.error(), error_type, error);
    return nullptr;
  }
  return wrap_rtp_transceiver(result.MoveValue());
}

bool peer_remove_track(const NativePeerConnection& peer,
                       const NativeRtpSender& sender,
                       std::uint8_t& error_type, rust::String& error) noexcept {
  auto result = peer.peer()->RemoveTrackOrError(sender.state()->sender);
  if (!result.ok()) {
    SetError(result, error_type, error);
    return false;
  }
  return true;
}

std::unique_ptr<NativeRtpSender> rtp_transceiver_sender(
    const NativeRtpTransceiver& transceiver) noexcept {
  auto state = std::make_unique<NativeRtpSender::State>();
  state->sender = transceiver.state()->transceiver->sender();
  return std::make_unique<NativeRtpSender>(std::move(state));
}

std::unique_ptr<NativeRtpReceiver> rtp_transceiver_receiver(
    const NativeRtpTransceiver& transceiver) noexcept {
  return wrap_rtp_receiver(transceiver.state()->transceiver->receiver());
}

std::uint8_t rtp_transceiver_direction(
    const NativeRtpTransceiver& transceiver) noexcept {
  return static_cast<std::uint8_t>(
      transceiver.state()->transceiver->direction());
}

std::int8_t rtp_transceiver_current_direction(
    const NativeRtpTransceiver& transceiver) noexcept {
  const auto direction = transceiver.state()->transceiver->current_direction();
  return direction ? static_cast<std::int8_t>(*direction) : -1;
}

bool rtp_transceiver_stopped(
    const NativeRtpTransceiver& transceiver) noexcept {
  return transceiver.state()->transceiver->stopped();
}

bool rtp_transceiver_set_direction(const NativeRtpTransceiver& transceiver,
                                   std::uint8_t direction,
                                   std::uint8_t& error_type,
                                   rust::String& error) noexcept {
  const auto parsed = Direction(direction);
  if (!parsed) {
    error_type = static_cast<std::uint8_t>(webrtc::RTCErrorType::INVALID_PARAMETER);
    error = "invalid RTP transceiver direction";
    return false;
  }
  auto result = transceiver.state()->transceiver->SetDirectionWithError(*parsed);
  if (!result.ok()) {
    SetError(result, error_type, error);
    return false;
  }
  return true;
}

rust::String rtp_sender_id(const NativeRtpSender& sender) noexcept {
  return sender.state()->sender->id();
}
std::unique_ptr<NativeVideoTrack> rtp_sender_track(
    const NativeRtpSender& sender) noexcept {
  auto track = VideoTrack(sender.state()->sender->track());
  if (!track) {
    return nullptr;
  }
  auto state = std::make_unique<NativeVideoTrack::State>();
  state->track = std::move(track);
  return std::make_unique<NativeVideoTrack>(std::move(state));
}
rust::String rtp_receiver_id(const NativeRtpReceiver& receiver) noexcept {
  return receiver.state()->receiver->id();
}
std::unique_ptr<NativeVideoTrack> rtp_receiver_track(
    const NativeRtpReceiver& receiver) noexcept {
  auto track = VideoTrack(receiver.state()->receiver->track());
  if (!track) {
    return nullptr;
  }
  auto state = std::make_unique<NativeVideoTrack::State>();
  state->track = std::move(track);
  return std::make_unique<NativeVideoTrack>(std::move(state));
}

}  // namespace pulsebeam::webrtc_sys
