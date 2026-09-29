#include "pulsebeam-webrtc-sys/native/video.h"

#include <algorithm>
#include <atomic>
#include <cstring>
#include <deque>
#include <limits>
#include <map>
#include <mutex>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include "api/frame_transformer_interface.h"
#include "api/make_ref_counted.h"
#include "api/media_stream_interface.h"
#include "api/peer_connection_interface.h"
#include "api/rtp_receiver_interface.h"
#include "api/rtp_parameters.h"
#include "api/rtp_sender_interface.h"
#include "api/rtp_transceiver_interface.h"
#include "api/scoped_refptr.h"
#include "api/video/i420_buffer.h"
#include "api/video_codecs/scalability_mode.h"
#include "api/video/video_frame.h"
#include "api/video/video_sink_interface.h"
#include "media/base/video_broadcaster.h"
#if defined(PULSEBEAM_WEBRTC_NATIVE_AUDIO) && !defined(WEBRTC_ANDROID) && !defined(WEBRTC_IOS)
#include "modules/video_capture/video_capture.h"
#include "modules/video_capture/video_capture_factory.h"
#include "modules/desktop_capture/desktop_capturer.h"
#include "modules/desktop_capture/delegated_source_list_controller.h"
#include "modules/desktop_capture/desktop_frame.h"
#include "modules/desktop_capture/desktop_capture_options.h"
#if defined(WEBRTC_USE_PIPEWIRE)
#include "modules/desktop_capture/linux/wayland/base_capturer_pipewire.h"
#endif
#include "libyuv/convert.h"
#include "rtc_base/time_utils.h"
#endif
#include "pc/video_track_source.h"
#include "pulsebeam-webrtc-sys/native/codec.h"
#include "pulsebeam-webrtc-sys/native/peer.h"
#include "pulsebeam-webrtc-sys/src/lib.rs.h"
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

#if defined(PULSEBEAM_WEBRTC_NATIVE_AUDIO) && !defined(WEBRTC_ANDROID) && !defined(WEBRTC_IOS)
class ScreenFrameSink final : public webrtc::DesktopCapturer::Callback {
 public:
  ScreenFrameSink(webrtc::scoped_refptr<PushVideoSource> source,
                  const std::atomic<bool>& source_closed,
                  std::atomic<std::uint8_t>& status)
      : source_(std::move(source)), source_closed_(source_closed), status_(status) {}
  void OnCaptureResult(webrtc::DesktopCapturer::Result result,
                       std::unique_ptr<webrtc::DesktopFrame> frame) override {
    if (source_closed_.load()) return;
    if (result != webrtc::DesktopCapturer::Result::SUCCESS || !frame) {
      if (result == webrtc::DesktopCapturer::Result::ERROR_PERMANENT) {
        auto current = status_.load();
        while (current != 6 && !status_.compare_exchange_weak(current, 4)) {}
      }
      Failed();
      return;
    }
    if (status_.load() == 3 || status_.load() == 4 || status_.load() == 6) return;
    std::uint8_t current = status_.load();
    while (current != 2 && current != 3 && current != 4 && current != 6 &&
           !status_.compare_exchange_weak(current, 2)) {}
    if (current == 3 || current == 4 || current == 6) return;
    const int width = frame->size().width();
    const int height = frame->size().height();
    // Portal/accelerated capturers can return texture-backed frames with no
    // CPU buffer. Do not dereference them or interpret non-ARGB pixels as ARGB.
    if (width <= 0 || height <= 0 || width > 4096 || height > 4096 ||
        !frame->data() || frame->pixel_format() != webrtc::FOURCC_ARGB ||
        frame->stride() < width * webrtc::DesktopFrame::kBytesPerPixel) {
      Failed();
      return;
    }
    auto buffer = webrtc::I420Buffer::Create(width, height);
    if (libyuv::ARGBToI420(frame->data(), frame->stride(),
                          buffer->MutableDataY(), buffer->StrideY(),
                          buffer->MutableDataU(), buffer->StrideU(),
                          buffer->MutableDataV(), buffer->StrideV(),
                          width, height) != 0) {
      Failed();
      return;
    }
    if (status_.load() == 6) return;
    source_->Push(webrtc::VideoFrame::Builder()
                      .set_video_frame_buffer(std::move(buffer))
                      .set_timestamp_us(webrtc::TimeMicros()).build());
  }
  std::uint64_t failed_frames() const { return failed_frames_.load(); }
 private:
  void Failed() {
    auto old = failed_frames_.load();
    while (old != std::numeric_limits<std::uint64_t>::max() &&
           !failed_frames_.compare_exchange_weak(old, old + 1)) {}
  }
  webrtc::scoped_refptr<PushVideoSource> source_;
  const std::atomic<bool>& source_closed_;
  std::atomic<std::uint8_t>& status_;
  std::atomic<std::uint64_t> failed_frames_{0};
};

class ScreenSelectionObserver final
    : public webrtc::DelegatedSourceListController::Observer {
 public:
  explicit ScreenSelectionObserver(std::atomic<std::uint8_t>& status)
      : status_(status) {}
  void OnSelection() override {
    std::uint8_t pending = 0;
    status_.compare_exchange_strong(pending, 1);
  }
  void OnCancelled() override { SetUnlessSessionClosed(3); }
  void OnError() override { SetUnlessSessionClosed(4); }
 private:
  void SetUnlessSessionClosed(std::uint8_t value) {
    auto current = status_.load();
    while (current != 6 && !status_.compare_exchange_weak(current, value)) {}
  }
  std::atomic<std::uint8_t>& status_;
};

#if defined(WEBRTC_USE_PIPEWIRE)
// Use the same upstream PipeWire capturer selected by the Linux factory, but
// retain the portal notifier event which DesktopCapturer does not expose.
class ObservablePipeWireCapturer final : public webrtc::BaseCapturerPipeWire {
 public:
  ObservablePipeWireCapturer(const webrtc::DesktopCaptureOptions& options,
                            webrtc::CaptureType type,
                            std::atomic<std::uint8_t>& status,
                            const std::atomic<bool>& closed)
      : BaseCapturerPipeWire(options, type), status_(status), closed_(closed) {}
  void OnScreenCastSessionClosed() override {
    auto current = status_.load();
    while (!closed_.load() && current != 3 && current != 4 && current != 6 &&
           !status_.compare_exchange_weak(current, 6)) {}
    BaseCapturerPipeWire::OnScreenCastSessionClosed();
  }
 private:
  std::atomic<std::uint8_t>& status_;
  const std::atomic<bool>& closed_;
};
#endif

class CameraFrameSink final : public webrtc::VideoSinkInterface<webrtc::VideoFrame> {
 public:
  CameraFrameSink(webrtc::scoped_refptr<PushVideoSource> source,
                  const std::atomic<bool>& source_closed,
                  std::atomic<std::int64_t>& last_frame_us,
                  std::atomic<bool>& frame_seen)
      : source_(std::move(source)), source_closed_(source_closed),
        last_frame_us_(last_frame_us), frame_seen_(frame_seen) {}
  void OnFrame(const webrtc::VideoFrame& frame) override {
    if (source_closed_.load()) return;
    last_frame_us_.store(webrtc::TimeMicros());
    frame_seen_.store(true);
    source_->Push(frame);
  }
 private:
  webrtc::scoped_refptr<PushVideoSource> source_;
  const std::atomic<bool>& source_closed_;
  std::atomic<std::int64_t>& last_frame_us_;
  std::atomic<bool>& frame_seen_;
};
#endif

class FrameSink final : public webrtc::VideoSinkInterface<webrtc::VideoFrame> {
 public:
  void OnFrame(const webrtc::VideoFrame& frame) override {
    std::lock_guard lock(mutex_);
    if (!active_) {
      return;
    }
    // Keep a small, byte-bounded queue of the most recent decoded frames.
    // An oversized frame is discarded rather than retained beyond the budget.
    if (frame.width() <= 0 || frame.height() <= 0) {
      Lost();
      return;
    }
    const std::uint64_t width = static_cast<std::uint64_t>(frame.width());
    const std::uint64_t height = static_cast<std::uint64_t>(frame.height());
    const std::uint64_t bytes = width * height +
                                2 * ((width + 1) / 2) * ((height + 1) / 2);
    if (bytes > kMaxBytes) {
      Lost();
      return;
    }
    while (frames_.size() >= kMaxFrames || queued_bytes_ + bytes > kMaxBytes) {
      queued_bytes_ -= frames_.front().bytes;
      frames_.pop_front();
      Lost();
    }
    frames_.push_back({wrap_video_frame(frame), static_cast<std::size_t>(bytes)});
    queued_bytes_ += static_cast<std::size_t>(bytes);
  }

  std::unique_ptr<NativeVideoFrame> Take() {
    std::lock_guard lock(mutex_);
    if (frames_.empty()) {
      return nullptr;
    }
    auto frame = std::move(frames_.front());
    frames_.pop_front();
    queued_bytes_ -= frame.bytes;
    return std::move(frame.frame);
  }

  std::uint64_t DroppedFrames() {
    std::lock_guard lock(mutex_);
    return dropped_frames_;
  }

  void Deactivate() {
    std::lock_guard lock(mutex_);
    active_ = false;
    frames_.clear();
    queued_bytes_ = 0;
  }

 private:
  void Lost() {
    if (dropped_frames_ != std::numeric_limits<std::uint64_t>::max()) {
      ++dropped_frames_;
    }
  }

  struct QueuedFrame {
    std::unique_ptr<NativeVideoFrame> frame;
    std::size_t bytes;
  };
  static constexpr std::size_t kMaxFrames = 4;
  static constexpr std::size_t kMaxBytes = 16 * 1024 * 1024;
  std::mutex mutex_;
  std::deque<QueuedFrame> frames_;
  std::size_t queued_bytes_ = 0;
  std::uint64_t dropped_frames_ = 0;
  bool active_ = true;
};

// Receive-side transformer consumes encoded frames before the decoder. When
// deactivated, future frames pass through to the upstream decoder instead.
// Keep one transformer per receiver lifetime: replacing upstream's delegate
// does not reset the old callback registration in this pinned revision.
class EncodedFrameCollector : public webrtc::FrameTransformerInterface {
 public:
  void Transform(std::unique_ptr<webrtc::TransformableFrameInterface> frame) override {
    webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback;
    {
      std::lock_guard lock(mutex_);
      if (active_ && frame->GetDirection() ==
                         webrtc::TransformableFrameInterface::Direction::kReceiver) {
        auto* video = static_cast<webrtc::TransformableVideoFrameInterface*>(frame.get());
        const auto data = frame->GetData();
        if (data.size() > kMaxBytes) {
          Lost();
          return;
        }
        while (!frames_.empty() &&
               (frames_.size() >= kMaxFrames || queued_bytes_ > kMaxBytes - data.size())) {
          queued_bytes_ -= frames_.front().data.size();
          frames_.pop_front();
          Lost();
        }
        StoredFrame snapshot;
        snapshot.data.assign(data.begin(), data.end());
        snapshot.mime_type = frame->GetMimeType();
        snapshot.rtp_timestamp = frame->GetTimestamp();
        snapshot.ssrc = frame->GetSsrc();
        snapshot.payload_type = frame->GetPayloadType();
        snapshot.key_frame = video->IsKeyFrame();
        snapshot.rid = video->Rid();
        if (auto time = video->CaptureTime()) snapshot.capture_time_us = time->us();
        if (auto time = video->ReceiveTime()) snapshot.receive_time_us = time->us();
        const auto metadata = video->Metadata();
        snapshot.frame_id = metadata.GetFrameId();
        if (snapshot.frame_id) {
          snapshot.spatial_index = metadata.GetSpatialIndex();
          snapshot.temporal_index = metadata.GetTemporalIndex();
          if (auto dependencies = metadata.GetDependencies()) {
            snapshot.dependencies.assign(dependencies->begin(), dependencies->end());
          }
          for (auto indication : metadata.GetDecodeTargetIndications())
            snapshot.decode_target_indications.push_back(static_cast<std::uint8_t>(indication));
        }
        queued_bytes_ += snapshot.data.size();
        frames_.push_back(std::move(snapshot));
        return;
      }
      const auto it = callbacks_.find(frame->GetSsrc());
      if (it != callbacks_.end()) callback = it->second;
    }
    if (callback) callback->OnTransformedFrame(std::move(frame));
  }

  void RegisterTransformedFrameSinkCallback(
      webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback,
      std::uint32_t ssrc) override {
    std::lock_guard lock(mutex_);
    callbacks_[ssrc] = std::move(callback);
  }
  void UnregisterTransformedFrameSinkCallback(std::uint32_t ssrc) override {
    std::lock_guard lock(mutex_);
    callbacks_.erase(ssrc);
  }

  FfiEncodedVideoFrame Take() {
    std::lock_guard lock(mutex_);
    FfiEncodedVideoFrame result{};
    if (frames_.empty()) return result;
    auto frame = std::move(frames_.front());
    frames_.pop_front();
    queued_bytes_ -= frame.data.size();
    result.available = true;
    for (auto byte : frame.data) result.data.push_back(byte);
    result.mime_type = frame.mime_type;
    result.rtp_timestamp = frame.rtp_timestamp;
    result.ssrc = frame.ssrc;
    result.payload_type = frame.payload_type;
    result.key_frame = frame.key_frame;
    result.has_rid = frame.rid.has_value();
    result.rid = frame.rid.value_or("");
    result.has_capture_time = frame.capture_time_us.has_value();
    result.capture_time_us = frame.capture_time_us.value_or(0);
    result.has_receive_time = frame.receive_time_us.has_value();
    result.receive_time_us = frame.receive_time_us.value_or(0);
    result.has_frame_id = frame.frame_id.has_value();
    result.frame_id = frame.frame_id.value_or(0);
    result.spatial_index = frame.spatial_index;
    result.temporal_index = frame.temporal_index;
    for (auto dependency : frame.dependencies) result.dependencies.push_back(dependency);
    for (auto indication : frame.decode_target_indications)
      result.decode_target_indications.push_back(indication);
    return result;
  }
  std::uint64_t DroppedFrames() {
    std::lock_guard lock(mutex_);
    return dropped_frames_;
  }
  void Deactivate() {
    std::lock_guard lock(mutex_);
    active_ = false;
    frames_.clear();
    queued_bytes_ = 0;
  }

 private:
  void Lost() {
    if (dropped_frames_ != std::numeric_limits<std::uint64_t>::max()) ++dropped_frames_;
  }
  struct StoredFrame {
    std::vector<std::uint8_t> data;
    std::string mime_type;
    std::uint32_t rtp_timestamp = 0;
    std::uint32_t ssrc = 0;
    std::uint8_t payload_type = 0;
    bool key_frame = false;
    std::optional<std::string> rid;
    std::optional<std::int64_t> capture_time_us;
    std::optional<std::int64_t> receive_time_us;
    std::optional<std::int64_t> frame_id;
    std::int32_t spatial_index = 0;
    std::int32_t temporal_index = 0;
    std::vector<std::int64_t> dependencies;
    std::vector<std::uint8_t> decode_target_indications;
  };
  static constexpr std::size_t kMaxFrames = 4;
  static constexpr std::size_t kMaxBytes = 4 * 1024 * 1024;
  std::mutex mutex_;
  std::map<std::uint32_t, webrtc::scoped_refptr<webrtc::TransformedFrameCallback>> callbacks_;
  std::deque<StoredFrame> frames_;
  std::size_t queued_bytes_ = 0;
  std::uint64_t dropped_frames_ = 0;
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

rust::String FeedbackName(const webrtc::RtcpFeedback& feedback) {
  std::string name;
  switch (feedback.type) {
    case webrtc::RtcpFeedbackType::NONE: name = "none"; break;
    case webrtc::RtcpFeedbackType::CCM: name = "ccm"; break;
    case webrtc::RtcpFeedbackType::LNTF: name = "goog-lntf"; break;
    case webrtc::RtcpFeedbackType::NACK: name = "nack"; break;
    case webrtc::RtcpFeedbackType::REMB: name = "goog-remb"; break;
    case webrtc::RtcpFeedbackType::TRANSPORT_CC: name = "transport-cc"; break;
    case webrtc::RtcpFeedbackType::CCFB: name = "ccfb"; break;
    default: name = "unknown:" + std::to_string(static_cast<int>(feedback.type)); break;
  }
  if (feedback.message_type) {
    switch (*feedback.message_type) {
      case webrtc::RtcpFeedbackMessageType::GENERIC_NACK: name += "/generic"; break;
      case webrtc::RtcpFeedbackMessageType::PLI: name += "/pli"; break;
      case webrtc::RtcpFeedbackMessageType::FIR: name += "/fir"; break;
      default: name += "/unknown:" + std::to_string(static_cast<int>(*feedback.message_type)); break;
    }
  }
  return name;
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

struct NativeScreenCapture::State {
#if defined(PULSEBEAM_WEBRTC_NATIVE_AUDIO) && !defined(WEBRTC_ANDROID) && !defined(WEBRTC_IOS)
  std::unique_ptr<webrtc::DesktopCapturer> capturer;
  std::unique_ptr<ScreenFrameSink> sink;
  std::unique_ptr<ScreenSelectionObserver> selection;
#endif
  std::atomic<std::uint8_t> status{0};
  std::uint64_t final_failed_frames = 0;
  std::atomic<bool> closed{false};
};

struct NativeCamera::State {
#if defined(PULSEBEAM_WEBRTC_NATIVE_AUDIO) && !defined(WEBRTC_ANDROID) && !defined(WEBRTC_IOS)
  webrtc::scoped_refptr<webrtc::VideoCaptureModule> module;
  std::unique_ptr<CameraFrameSink> sink;
  std::atomic<std::int64_t> last_frame_us{0};
  std::atomic<bool> frame_seen{false};
#endif
  std::atomic<bool> closed{false};
};

struct NativeVideoSource::State {
  webrtc::scoped_refptr<PushVideoSource> source;
  webrtc::Thread* signaling_thread = nullptr;
  std::atomic<bool> closed{false};
};

struct NativeVideoTrack::State {
  webrtc::scoped_refptr<webrtc::VideoTrackInterface> track;
};

struct NativeEncodedVideoSink::State {
  webrtc::scoped_refptr<EncodedFrameCollector> collector;
};

struct NativeVideoSink::State {
  webrtc::scoped_refptr<webrtc::VideoTrackInterface> track;
  std::unique_ptr<FrameSink> sink;
  std::atomic<bool> closed{false};
};

struct NativeRtpSender::State {
  webrtc::scoped_refptr<webrtc::RtpSenderInterface> sender;
  std::optional<webrtc::RtpParameters> parameters;
};

struct NativeRtpReceiver::State {
  webrtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver;
};

struct NativeRtpTransceiver::State {
  webrtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver;
};

struct NativeTransceiverList::State {
  std::vector<webrtc::scoped_refptr<webrtc::RtpTransceiverInterface>> transceivers;
};

NativeScreenCapture::NativeScreenCapture(std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeScreenCapture::~NativeScreenCapture() { close_screen(*this); }
const std::unique_ptr<NativeScreenCapture::State>&
NativeScreenCapture::state() const noexcept { return state_; }

NativeCamera::NativeCamera(std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeCamera::~NativeCamera() { close_camera(*this); }
const std::unique_ptr<NativeCamera::State>& NativeCamera::state() const noexcept {
  return state_;
}

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

NativeEncodedVideoSink::NativeEncodedVideoSink(std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeEncodedVideoSink::~NativeEncodedVideoSink() {
  close_encoded_video_sink(*this);
}
const std::unique_ptr<NativeEncodedVideoSink::State>&
NativeEncodedVideoSink::state() const noexcept {
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
webrtc::scoped_refptr<webrtc::RtpReceiverInterface>
NativeRtpReceiver::receiver() const noexcept { return state_->receiver; }

NativeRtpTransceiver::NativeRtpTransceiver(
    std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeRtpTransceiver::~NativeRtpTransceiver() = default;
const std::unique_ptr<NativeRtpTransceiver::State>&
NativeRtpTransceiver::state() const noexcept {
  return state_;
}

bool screen_sources(bool windows, rust::Vec<FfiScreenSource>& screens,
                    rust::String& error) noexcept {
#if defined(PULSEBEAM_WEBRTC_NATIVE_AUDIO) && !defined(WEBRTC_ANDROID) && !defined(WEBRTC_IOS)
  auto options = webrtc::DesktopCaptureOptions::CreateDefault();
  auto capturer = windows
      ? webrtc::DesktopCapturer::CreateWindowCapturer(options)
      : webrtc::DesktopCapturer::CreateScreenCapturer(options);
  if (!capturer) { error = "desktop capture unavailable on this platform"; return false; }
  webrtc::DesktopCapturer::SourceList sources;
  if (!capturer->GetSourceList(&sources)) {
    error = "screen source enumeration failed";
    return false;
  }
  for (const auto& source : sources) {
    FfiScreenSource screen{};
    screen.id = static_cast<std::int64_t>(source.id);
    screen.name = source.title;
    screens.push_back(std::move(screen));
  }
  return true;
#else
  error = "screen capture requires a native artifact";
  return false;
#endif
}
std::unique_ptr<NativeScreenCapture> open_screen(
    const NativeVideoSource& source, std::int64_t screen_id, bool window,
    rust::String& error) noexcept {
#if defined(PULSEBEAM_WEBRTC_NATIVE_AUDIO) && !defined(WEBRTC_ANDROID) && !defined(WEBRTC_IOS)
  if (source.state()->closed.load() || !source.state()->source) {
    error = "desktop source is closed";
    return nullptr;
  }
  auto options = webrtc::DesktopCaptureOptions::CreateDefault();
  auto state = std::make_unique<NativeScreenCapture::State>();
  std::unique_ptr<webrtc::DesktopCapturer> capturer;
#if defined(WEBRTC_USE_PIPEWIRE)
  if (webrtc::BaseCapturerPipeWire::IsSupported()) {
    capturer = std::make_unique<ObservablePipeWireCapturer>(
        options, window ? webrtc::CaptureType::kWindow
                        : webrtc::CaptureType::kScreen,
        state->status, state->closed);
  }
#endif
  if (!capturer) {
    capturer = window
        ? webrtc::DesktopCapturer::CreateWindowCapturer(options)
        : webrtc::DesktopCapturer::CreateScreenCapturer(options);
  }
  if (!capturer) { error = "desktop capture unavailable on this platform"; return nullptr; }
  webrtc::DesktopCapturer::SourceList available;
  if (!capturer->GetSourceList(&available) ||
      std::none_of(available.begin(), available.end(), [screen_id](const auto& item) {
        return static_cast<std::int64_t>(item.id) == screen_id;
      }) ||
      !capturer->SelectSource(static_cast<webrtc::DesktopCapturer::SourceId>(screen_id))) {
    error = "screen source is not available";
    return nullptr;
  }
  state->sink = std::make_unique<ScreenFrameSink>(
      source.state()->source, source.state()->closed, state->status);
  if (auto* delegated = capturer->GetDelegatedSourceListController()) {
    state->selection = std::make_unique<ScreenSelectionObserver>(state->status);
    delegated->Observe(state->selection.get());
  } else {
    state->status.store(1);
  }
  capturer->Start(state->sink.get());
  state->capturer = std::move(capturer);
  return std::make_unique<NativeScreenCapture>(std::move(state));
#else
  error = "screen capture requires a native artifact";
  return nullptr;
#endif
}
bool screen_capture_next_frame(const NativeScreenCapture& capture) noexcept {
#if defined(PULSEBEAM_WEBRTC_NATIVE_AUDIO) && !defined(WEBRTC_ANDROID) && !defined(WEBRTC_IOS)
  if (capture.state()->closed.load() || !capture.state()->capturer) return false;
  capture.state()->capturer->CaptureFrame();
  return true;
#else
  return false;
#endif
}
std::uint8_t screen_capture_status(const NativeScreenCapture& capture) noexcept {
  if (capture.state()->closed.load()) return 5;
  return capture.state()->status.load();
}
std::uint64_t screen_capture_failed_frames(const NativeScreenCapture& capture) noexcept {
#if defined(PULSEBEAM_WEBRTC_NATIVE_AUDIO) && !defined(WEBRTC_ANDROID) && !defined(WEBRTC_IOS)
  return capture.state()->sink ? capture.state()->sink->failed_frames()
                               : capture.state()->final_failed_frames;
#else
  return 0;
#endif
}
bool close_screen(const NativeScreenCapture& capture) noexcept {
  if (capture.state()->closed.exchange(true)) return true;
#if defined(PULSEBEAM_WEBRTC_NATIVE_AUDIO) && !defined(WEBRTC_ANDROID) && !defined(WEBRTC_IOS)
  capture.state()->capturer.reset();
  capture.state()->final_failed_frames = capture.state()->sink->failed_frames();
  capture.state()->sink.reset();
  capture.state()->selection.reset();
#endif
  return true;
}

bool camera_devices(rust::Vec<FfiCameraDevice>& devices,
                    rust::String& error) noexcept {
#if defined(PULSEBEAM_WEBRTC_NATIVE_AUDIO) && !defined(WEBRTC_ANDROID) && !defined(WEBRTC_IOS)
  std::unique_ptr<webrtc::VideoCaptureModule::DeviceInfo> info(
      webrtc::VideoCaptureFactory::CreateDeviceInfo());
  if (!info) {
    error = "camera enumeration unavailable on this platform";
    return false;
  }
  for (std::uint32_t i = 0; i < info->NumberOfDevices(); ++i) {
    char name[webrtc::kVideoCaptureDeviceNameLength] = {};
    char id[webrtc::kVideoCaptureUniqueNameLength] = {};
    if (info->GetDeviceName(i, name, sizeof(name), id, sizeof(id)) != 0) {
      devices.clear();
      error = "camera device lookup failed";
      return false;
    }
    FfiCameraDevice device{};
    device.name = name;
    device.id = id;
    devices.push_back(std::move(device));
  }
  return true;
#else
  error = "camera enumeration requires a supported native artifact";
  return false;
#endif
}

bool camera_formats(rust::Str device_id,
                    rust::Vec<FfiCameraFormat>& formats,
                    rust::String& error) noexcept {
#if defined(PULSEBEAM_WEBRTC_NATIVE_AUDIO) && !defined(WEBRTC_ANDROID) && !defined(WEBRTC_IOS)
  const std::string id(device_id.data(), device_id.size());
  if (id.empty() || id.find('\0') != std::string::npos) {
    error = "invalid camera device id";
    return false;
  }
  std::unique_ptr<webrtc::VideoCaptureModule::DeviceInfo> info(
      webrtc::VideoCaptureFactory::CreateDeviceInfo());
  if (!info) { error = "camera formats unavailable on this platform"; return false; }
  const int count = info->NumberOfCapabilities(id.c_str());
  if (count < 0) { error = "camera device is unavailable"; return false; }
  for (int i = 0; i < count; ++i) {
    webrtc::VideoCaptureCapability cap{};
    if (info->GetCapability(id.c_str(), i, cap) != 0) {
      formats.clear();
      error = "camera capability lookup failed";
      return false;
    }
    if (cap.width <= 0 || cap.height <= 0 || cap.maxFPS <= 0) continue;
    FfiCameraFormat format{};
    format.width = static_cast<std::uint32_t>(cap.width);
    format.height = static_cast<std::uint32_t>(cap.height);
    format.max_fps = static_cast<std::uint32_t>(cap.maxFPS);
    format.pixel_format = static_cast<std::int32_t>(cap.videoType);
    formats.push_back(format);
  }
  return true;
#else
  error = "camera formats require a supported native artifact";
  return false;
#endif
}

std::unique_ptr<NativeCamera> open_camera(const NativeVideoSource& source,
                                           rust::Str device_id,
                                           std::uint32_t width,
                                           std::uint32_t height,
                                           std::uint32_t fps,
                                           rust::String& error) noexcept {
#if defined(PULSEBEAM_WEBRTC_NATIVE_AUDIO) && !defined(WEBRTC_ANDROID) && !defined(WEBRTC_IOS)
  if (source.state()->closed.load() || !source.state()->source ||
      device_id.empty() || width == 0 || height == 0 || fps == 0 ||
      width > 4096 || height > 4096 || fps > 120) {
    error = "invalid camera source or capture settings";
    return nullptr;
  }
  const std::string id(device_id.data(), device_id.size());
  if (id.find('\0') != std::string::npos) {
    error = "camera device id contains a NUL byte";
    return nullptr;
  }
  std::unique_ptr<webrtc::VideoCaptureModule::DeviceInfo> info(
      webrtc::VideoCaptureFactory::CreateDeviceInfo());
  if (!info) { error = "camera capture unavailable on this platform"; return nullptr; }
  webrtc::VideoCaptureCapability requested{};
  requested.width = static_cast<int>(width);
  requested.height = static_cast<int>(height);
  requested.maxFPS = static_cast<int>(fps);
  webrtc::VideoCaptureCapability chosen{};
  if (info->GetBestMatchedCapability(id.c_str(), requested, chosen) < 0) {
    error = "camera has no matching capture capability";
    return nullptr;
  }
  auto module = webrtc::VideoCaptureFactory::Create(id.c_str());
  if (!module) { error = "camera capture module unavailable"; return nullptr; }
  auto state = std::make_unique<NativeCamera::State>();
  state->last_frame_us.store(webrtc::TimeMicros());
  auto sink = std::make_unique<CameraFrameSink>(source.state()->source,
                                                source.state()->closed,
                                                state->last_frame_us,
                                                state->frame_seen);
  module->RegisterCaptureDataCallback(sink.get());
  if (module->StartCapture(chosen) != 0) {
    module->DeRegisterCaptureDataCallback();
    error = "camera start failed";
    return nullptr;
  }
  state->module = std::move(module);
  state->sink = std::move(sink);
  return std::make_unique<NativeCamera>(std::move(state));
#else
  error = "camera capture requires a supported native artifact";
  return nullptr;
#endif
}
std::uint8_t camera_capture_status(const NativeCamera& camera,
                                   std::uint64_t stale_after_ms) noexcept {
  const auto& state = camera.state();
  if (state->closed.load()) return 3;
#if defined(PULSEBEAM_WEBRTC_NATIVE_AUDIO) && !defined(WEBRTC_ANDROID) && !defined(WEBRTC_IOS)
  if (!state->module || !state->module->CaptureStarted()) return 4;
  const auto last = state->last_frame_us.load();
  const auto elapsed = webrtc::TimeMicros() - last;
  // Saturate instead of overflowing when an application supplies a very
  // large timeout. A stale frame means delivery stopped, not a diagnosed
  // hardware or permission cause.
  const auto limit = stale_after_ms > static_cast<std::uint64_t>(INT64_MAX / 1000)
                         ? INT64_MAX
                         : static_cast<std::int64_t>(stale_after_ms * 1000);
  if (elapsed >= limit) return 2;
  return state->frame_seen.load() ? 1 : 0;
#else
  (void)stale_after_ms;
  return 4;
#endif
}

bool close_camera(const NativeCamera& camera) noexcept {
  if (camera.state()->closed.exchange(true)) return true;
  bool stopped = true;
#if defined(PULSEBEAM_WEBRTC_NATIVE_AUDIO) && !defined(WEBRTC_ANDROID) && !defined(WEBRTC_IOS)
  if (camera.state()->module) {
    stopped = camera.state()->module->StopCapture() == 0;
    camera.state()->module->DeRegisterCaptureDataCallback();
    camera.state()->sink.reset();
    camera.state()->module = nullptr;
  }
#endif
  return stopped;
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

bool video_source_push_encoded_trigger(
    const NativeVideoSource& source, std::uint32_t width,
    std::uint32_t height, std::int64_t timestamp_us,
    std::int64_t token) noexcept {
  if (source.state()->closed.load() || width == 0 || height == 0 ||
      width > 4096 || height > 4096 || token <= 0 || timestamp_us < 0) return false;
  auto buffer = webrtc::I420Buffer::Create(static_cast<int>(width),
                                           static_cast<int>(height));
  buffer->InitializeData();
  source.state()->source->Push(
      webrtc::VideoFrame::Builder()
          .set_video_frame_buffer(std::move(buffer))
          .set_timestamp_us(timestamp_us)
          .set_presentation_timestamp(webrtc::Timestamp::Micros(token))
          .build());
  return true;
}

bool video_source_push_frame(const NativeVideoSource& source,
                             rust::Slice<const std::uint8_t> data,
                             std::uint32_t width, std::uint32_t height,
                             std::int64_t timestamp_us,
                             std::uint32_t rtp_timestamp,
                             std::uint16_t rotation) noexcept {
  if (source.state()->closed.load() || width == 0 || height == 0 ||
      width > static_cast<std::uint32_t>(INT32_MAX) ||
      height > static_cast<std::uint32_t>(INT32_MAX) ||
      (rotation != 0 && rotation != 90 && rotation != 180 && rotation != 270)) {
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
          .set_rotation(static_cast<webrtc::VideoRotation>(rotation))
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

std::uint64_t video_sink_dropped_frames(const NativeVideoSink& sink) noexcept {
  return sink.state()->sink ? sink.state()->sink->DroppedFrames() : 0;
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

NativeTransceiverList::NativeTransceiverList(std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeTransceiverList::~NativeTransceiverList() = default;
const std::unique_ptr<NativeTransceiverList::State>&
NativeTransceiverList::state() const noexcept { return state_; }

std::unique_ptr<NativeTransceiverList> peer_audio_transceivers(
    const NativePeerConnection& peer) noexcept {
  if (!peer.peer()) return nullptr;
  auto state = std::make_unique<NativeTransceiverList::State>();
  for (auto& transceiver : peer.peer()->GetTransceivers()) {
    if (transceiver && transceiver->media_type() == webrtc::MediaType::AUDIO) {
      state->transceivers.push_back(std::move(transceiver));
    }
  }
  return std::make_unique<NativeTransceiverList>(std::move(state));
}

std::unique_ptr<NativeTransceiverList> peer_video_transceivers(
    const NativePeerConnection& peer) noexcept {
  if (!peer.peer()) {
    return nullptr;
  }
  auto state = std::make_unique<NativeTransceiverList::State>();
  for (auto& transceiver : peer.peer()->GetTransceivers()) {
    if (transceiver && transceiver->media_type() == webrtc::MediaType::VIDEO) {
      state->transceivers.push_back(std::move(transceiver));
    }
  }
  return std::make_unique<NativeTransceiverList>(std::move(state));
}

rust::Vec<FfiVideoCodecCapability> peer_video_codec_capabilities(
    const NativePeerConnectionFactory& factory, bool sender) noexcept {
  rust::Vec<FfiVideoCodecCapability> result;
  if (!factory.factory() || !factory.signaling_thread()) {
    return result;
  }
  const auto capabilities = factory.signaling_thread()->BlockingCall([&] {
    return sender
        ? factory.factory()->GetRtpSenderCapabilities(webrtc::MediaType::VIDEO)
        : factory.factory()->GetRtpReceiverCapabilities(webrtc::MediaType::VIDEO);
  });
  for (const auto& codec : capabilities.codecs) {
    FfiVideoCodecCapability item;
    item.format.name = codec.name;
    for (const auto& [key, value] : codec.parameters) {
      item.format.parameters.push_back({key, value});
    }
    item.clock_rate = codec.clock_rate.value_or(-1);
    item.preferred_payload_type = codec.preferred_payload_type.value_or(-1);
    for (const auto& feedback : codec.rtcp_feedback) {
      item.rtcp_feedback.push_back(FeedbackName(feedback));
    }
    for (const auto mode : codec.scalability_modes) {
      item.scalability_modes.push_back(
          std::string(webrtc::ScalabilityModeToString(mode)));
    }
    result.push_back(std::move(item));
  }
  return result;
}

std::size_t transceiver_list_len(const NativeTransceiverList& list) noexcept {
  return list.state()->transceivers.size();
}

std::unique_ptr<NativeRtpTransceiver> transceiver_list_at(
    const NativeTransceiverList& list, std::size_t index) noexcept {
  if (index >= list.state()->transceivers.size()) {
    return nullptr;
  }
  return wrap_rtp_transceiver(list.state()->transceivers[index]);
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

bool rtp_receiver_request_keyframe(
    const NativePeerConnection& peer, const NativeRtpReceiver& receiver) noexcept {
  if (!peer.worker_thread()) return false;
  const auto receivers = peer.peer()->GetReceivers();
  if (std::find(receivers.begin(), receivers.end(), receiver.state()->receiver) ==
      receivers.end()) return false;
  auto track = VideoTrack(receiver.state()->receiver->track());
  if (!track || track->state() != webrtc::MediaStreamTrackInterface::kLive ||
      !track->GetSource() || !track->GetSource()->remote()) return false;
  peer.worker_thread()->BlockingCall([&] { track->GetSource()->GenerateKeyFrame(); });
  return true;
}

std::unique_ptr<NativeEncodedVideoSink> rtp_receiver_attach_encoded_video_sink(
    const NativePeerConnection& peer, const NativeRtpReceiver& receiver) noexcept {
  if (!peer.worker_thread()) return nullptr;
  // Receiver handles may outlive their original peer or come from another peer.
  const auto receivers = peer.peer()->GetReceivers();
  if (std::find(receivers.begin(), receivers.end(), receiver.state()->receiver) ==
      receivers.end()) return nullptr;
  if (!peer.reserve_encoded_receiver(receiver.state()->receiver->id())) return nullptr;
  auto state = std::make_unique<NativeEncodedVideoSink::State>();
  state->collector = webrtc::make_ref_counted<EncodedFrameCollector>();
  peer.worker_thread()->BlockingCall([&] {
    receiver.state()->receiver->SetFrameTransformer(state->collector);
  });
  return std::make_unique<NativeEncodedVideoSink>(std::move(state));
}

FfiEncodedVideoFrame encoded_video_sink_take_frame(
    const NativeEncodedVideoSink& sink) noexcept {
  return sink.state()->collector->Take();
}
std::uint64_t encoded_video_sink_dropped_frames(
    const NativeEncodedVideoSink& sink) noexcept {
  return sink.state()->collector->DroppedFrames();
}
bool close_encoded_video_sink(const NativeEncodedVideoSink& sink) noexcept {
  sink.state()->collector->Deactivate();
  return true;
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
    std::uint8_t direction, rust::Slice<const rust::String> rids,
    std::uint8_t& error_type, rust::String& error) noexcept {
  const auto parsed = Direction(direction);
  if (!parsed) {
    error_type = static_cast<std::uint8_t>(webrtc::RTCErrorType::INVALID_PARAMETER);
    error = "invalid RTP transceiver direction";
    return nullptr;
  }
  webrtc::RtpTransceiverInit init;
  init.direction = *parsed;
  for (const auto& rid : rids) {
    webrtc::RtpEncodingParameters encoding;
    encoding.rid = std::string(rid);
    init.send_encodings.push_back(std::move(encoding));
  }
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

bool rtp_transceiver_mid(const NativeRtpTransceiver& transceiver,
                         rust::String& mid) noexcept {
  const auto value = transceiver.state()->transceiver->mid();
  if (!value) {
    return false;
  }
  mid = *value;
  return true;
}

bool rtp_transceiver_set_video_codec_preferences(
    const NativeRtpTransceiver& transceiver, const NativePeerConnectionFactory& factory,
    rust::Slice<const FfiCodecFormat> formats, std::uint8_t& error_type,
    rust::String& error) noexcept {
  if (!factory.factory() || !factory.signaling_thread()) {
    error_type = static_cast<std::uint8_t>(webrtc::RTCErrorType::INVALID_STATE);
    error = "peer is unavailable";
    return false;
  }
  const auto capabilities = factory.signaling_thread()->BlockingCall([&] {
    return factory.factory()->GetRtpSenderCapabilities(webrtc::MediaType::VIDEO);
  });
  std::vector<webrtc::RtpCodecCapability> selected;
  for (const auto& format : formats) {
    std::map<std::string, std::string> parameters;
    for (const auto& item : format.parameters) {
      if (!parameters.emplace(std::string(item.key), std::string(item.value)).second) {
        error_type = static_cast<std::uint8_t>(webrtc::RTCErrorType::INVALID_PARAMETER);
        error = "duplicate codec parameter";
        return false;
      }
    }
    const auto it = std::find_if(capabilities.codecs.begin(), capabilities.codecs.end(),
        [&](const auto& codec) {
          return codec.name == std::string(format.name) && codec.parameters == parameters;
        });
    if (it == capabilities.codecs.end() ||
        std::find(selected.begin(), selected.end(), *it) != selected.end()) {
      error_type = static_cast<std::uint8_t>(webrtc::RTCErrorType::INVALID_PARAMETER);
      error = "unknown or repeated video codec capability";
      return false;
    }
    selected.push_back(*it);
  }
  auto result = factory.signaling_thread()->BlockingCall([&] {
    return transceiver.state()->transceiver->SetCodecPreferences(selected);
  });
  if (!result.ok()) {
    SetError(result, error_type, error);
    return false;
  }
  return true;
}

bool rtp_transceiver_stop(const NativeRtpTransceiver& transceiver,
                          std::uint8_t& error_type,
                          rust::String& error) noexcept {
  auto result = transceiver.state()->transceiver->StopStandard();
  if (!result.ok()) {
    SetError(result, error_type, error);
    return false;
  }
  return true;
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

bool rtp_sender_request_keyframe(const NativeRtpSender& sender,
                                 const NativePeerConnection& peer,
                                 rust::Slice<const rust::String> rids,
                                 std::uint8_t& error_type,
                                 rust::String& error) noexcept {
  if (!peer.peer() || !peer.signaling_thread()) {
    error_type = static_cast<std::uint8_t>(webrtc::RTCErrorType::INVALID_STATE);
    error = "peer is unavailable";
    return false;
  }
  std::vector<std::string> names;
  names.reserve(rids.size());
  for (const auto& rid : rids) {
    names.emplace_back(std::string(rid));
  }
  const auto result = peer.signaling_thread()->BlockingCall([&] {
    return sender.state()->sender->GenerateKeyFrame(names);
  });
  if (!result.ok()) {
    SetError(result, error_type, error);
    return false;
  }
  return true;
}

bool rtp_sender_get_parameters(const NativeRtpSender& sender,
                               FfiSenderParameters& output) noexcept {
  auto& state = *sender.state();
  if (!state.sender) {
    return false;
  }
  state.parameters = state.sender->GetParameters();
  output.transaction_id = state.parameters->transaction_id;
  output.encodings.clear();
  for (const auto& encoding : state.parameters->encodings) {
    FfiSenderEncoding copied;
    copied.rid = encoding.rid;
    copied.active = encoding.active;
    copied.has_max_bitrate = encoding.max_bitrate_bps.has_value();
    copied.max_bitrate_bps = encoding.max_bitrate_bps.value_or(0);
    copied.has_max_framerate = encoding.max_framerate.has_value();
    copied.max_framerate = encoding.max_framerate.value_or(0);
    copied.has_scale_by = encoding.scale_resolution_down_by.has_value();
    copied.scale_by = encoding.scale_resolution_down_by.value_or(0);
    copied.has_scale_to = encoding.scale_resolution_down_to.has_value();
    copied.scale_to_width = encoding.scale_resolution_down_to
                                ? encoding.scale_resolution_down_to->width : 0;
    copied.scale_to_height = encoding.scale_resolution_down_to
                                 ? encoding.scale_resolution_down_to->height : 0;
    copied.has_scalability_mode = encoding.scalability_mode.has_value();
    copied.scalability_mode = encoding.scalability_mode.value_or("");
    output.encodings.push_back(std::move(copied));
  }
  return true;
}

bool rtp_sender_set_parameters(const NativeRtpSender& sender,
                               const FfiSenderParameters& input,
                               std::uint8_t& error_type,
                               rust::String& error) noexcept {
  auto& state = *sender.state();
  if (!state.sender || !state.parameters ||
      input.transaction_id != state.parameters->transaction_id) {
    error_type = static_cast<std::uint8_t>(webrtc::RTCErrorType::INVALID_STATE);
    error = "get current sender parameters before updating";
    return false;
  }
  if (input.encodings.size() != state.parameters->encodings.size()) {
    error_type = static_cast<std::uint8_t>(webrtc::RTCErrorType::INVALID_MODIFICATION);
    error = "encoding count cannot change without renegotiation";
    return false;
  }
  auto updated = *state.parameters;
  for (std::size_t i = 0; i < input.encodings.size(); ++i) {
    const auto& source = input.encodings[i];
    auto& target = updated.encodings[i];
    if (std::string(source.rid) != target.rid) {
      error_type = static_cast<std::uint8_t>(webrtc::RTCErrorType::INVALID_MODIFICATION);
      error = "encoding RID cannot change without renegotiation";
      return false;
    }
    target.active = source.active;
    target.max_bitrate_bps = source.has_max_bitrate
                                 ? std::optional<int>(source.max_bitrate_bps) : std::nullopt;
    target.max_framerate = source.has_max_framerate
                               ? std::optional<double>(source.max_framerate) : std::nullopt;
    target.scale_resolution_down_by = source.has_scale_by
                                           ? std::optional<double>(source.scale_by) : std::nullopt;
    target.scale_resolution_down_to = source.has_scale_to
        ? std::optional<webrtc::Resolution>(webrtc::Resolution{
              source.scale_to_width, source.scale_to_height}) : std::nullopt;
    target.scalability_mode = source.has_scalability_mode
                                  ? std::optional<std::string>(std::string(source.scalability_mode))
                                  : std::nullopt;
  }
  auto result = state.sender->SetParameters(updated);
  if (!result.ok()) {
    SetError(result, error_type, error);
    return false;
  }
  state.parameters.reset();  // A successful update consumes the transaction.
  return true;
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

bool rtp_sender_set_video_track(const NativeRtpSender& sender,
                                const NativeVideoTrack& track) noexcept {
  return sender.state()->sender->SetTrack(track.state()->track.get());
}

bool rtp_sender_clear_track(const NativeRtpSender& sender) noexcept {
  return sender.state()->sender->SetTrack(nullptr);
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
