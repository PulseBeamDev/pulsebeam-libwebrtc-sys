#include "pulsebeam-webrtc-sys/native/audio.h"

#include <atomic>
#include <cstring>
#include <deque>
#include <limits>
#include <list>
#include <mutex>
#include <optional>
#include <string>
#include <vector>

#include "api/make_ref_counted.h"
#include "api/media_stream_interface.h"
#include "api/notifier.h"
#include "api/peer_connection_interface.h"
#include "api/scoped_refptr.h"
#include "api/rtp_transceiver_interface.h"
#include "pulsebeam-webrtc-sys/native/peer.h"
#include "pulsebeam-webrtc-sys/native/video.h"
#include "pulsebeam-webrtc-sys/src/lib.rs.h"
#include "rtc_base/thread.h"

namespace pulsebeam::webrtc_sys {
namespace {

// Keep the sink list synchronized with synchronous source delivery. RemoveSink
// cannot return until any in-flight OnData has finished using that sink.
class PushAudioSource : public webrtc::Notifier<webrtc::AudioSourceInterface> {
 public:
  SourceState state() const override { return closed_.load() ? kEnded : kLive; }
  bool remote() const override { return false; }

  void AddSink(webrtc::AudioTrackSinkInterface* sink) override {
    std::lock_guard lock(mutex_);
    if (!closed_ && sink) sinks_.push_back(sink);
  }
  void RemoveSink(webrtc::AudioTrackSinkInterface* sink) override {
    std::lock_guard lock(mutex_);
    sinks_.remove(sink);
  }
  bool Push(const std::int16_t* samples, int sample_rate, size_t channels,
            size_t frames) {
    std::lock_guard lock(mutex_);
    if (closed_) return false;
    for (auto* sink : sinks_) {
      // An arbitrary caller timestamp is not a WebRTC absolute capture clock.
      // Do not misreport it to the voice engine as TimeMillis()-based metadata.
      sink->OnData(samples, 16, sample_rate, channels, frames, std::nullopt);
    }
    return true;
  }
  void Close() {
    {
      std::lock_guard lock(mutex_);
      if (closed_.exchange(true)) return;
    }
    FireOnChanged();
  }

 private:
  std::mutex mutex_;
  std::list<webrtc::AudioTrackSinkInterface*> sinks_;
  std::atomic<bool> closed_{false};
};

// Receive callbacks arrive on a WebRTC audio thread. The queue retains at
// most eight 10 ms blocks; consumers observe loss through DroppedFrames().
class ReceivedAudioCollector final : public webrtc::AudioTrackSinkInterface {
 public:
  void OnData(const void* data, int bits, int rate, size_t channels,
              size_t frames, std::optional<int64_t> capture_ms) override {
    std::lock_guard lock(mutex_);
    if (!active_) return;
    if (!data || bits != 16 || rate <= 0 || channels < 1 || channels > 2 ||
        frames == 0 || frames > 480 || channels * frames > 960) {
      if (dropped_ != std::numeric_limits<std::uint64_t>::max()) ++dropped_;
      return;
    }
    Frame frame;
    frame.rate = static_cast<std::uint32_t>(rate);
    frame.channels = static_cast<std::uint8_t>(channels);
    frame.frames = static_cast<std::uint32_t>(frames);
    if (capture_ms && *capture_ms >= 0 &&
        *capture_ms <= std::numeric_limits<int64_t>::max() / 1000) {
      frame.capture_us = *capture_ms * 1000;
    }
    frame.samples.resize(channels * frames);
    std::memcpy(frame.samples.data(), data, frame.samples.size() * sizeof(int16_t));
    if (frames_.size() == 8) {
      frames_.pop_front();
      if (dropped_ != std::numeric_limits<std::uint64_t>::max()) ++dropped_;
    }
    frames_.push_back(std::move(frame));
  }
  FfiReceivedAudioFrame Take() {
    std::lock_guard lock(mutex_);
    FfiReceivedAudioFrame out;
    if (frames_.empty()) return out;
    auto frame = std::move(frames_.front());
    frames_.pop_front();
    out.valid = true;
    out.sample_rate_hz = frame.rate;
    out.channels = frame.channels;
    out.samples_per_channel = frame.frames;
    out.has_capture_time = frame.capture_us.has_value();
    out.capture_time_us = frame.capture_us.value_or(0);
    for (auto sample : frame.samples) out.samples.push_back(sample);
    return out;
  }
  std::uint64_t DroppedFrames() const {
    std::lock_guard lock(mutex_);
    return dropped_;
  }
  void Deactivate() {
    std::lock_guard lock(mutex_);
    active_ = false;
    frames_.clear();
  }

 private:
  struct Frame {
    std::vector<std::int16_t> samples;
    std::uint32_t rate = 0;
    std::uint8_t channels = 0;
    std::uint32_t frames = 0;
    std::optional<std::int64_t> capture_us;
  };
  mutable std::mutex mutex_;
  std::deque<Frame> frames_;
  std::uint64_t dropped_ = 0;
  bool active_ = true;
};

}  // namespace

struct NativeAudioSink::State {
  const NativePeerConnection* peer = nullptr;
  webrtc::scoped_refptr<webrtc::AudioTrackInterface> track;
  std::unique_ptr<ReceivedAudioCollector> collector;
  webrtc::Thread* signaling_thread = nullptr;
  std::atomic<bool> closed{false};
};

struct NativeAudioSource::State {
  webrtc::scoped_refptr<PushAudioSource> source;
  webrtc::Thread* signaling_thread = nullptr;
};

struct NativeAudioTrack::State {
  webrtc::scoped_refptr<webrtc::AudioTrackInterface> track;
};

NativeAudioSink::NativeAudioSink(std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeAudioSink::~NativeAudioSink() { close_audio_sink(*this); }
const std::unique_ptr<NativeAudioSink::State>& NativeAudioSink::state() const noexcept {
  return state_;
}

NativeAudioSource::NativeAudioSource(std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeAudioSource::~NativeAudioSource() {
  close_audio_source(*this);
  if (state_ && state_->signaling_thread) {
    state_->signaling_thread->BlockingCall([this] { state_->source = nullptr; });
  }
}
const std::unique_ptr<NativeAudioSource::State>& NativeAudioSource::state() const noexcept {
  return state_;
}
NativeAudioTrack::NativeAudioTrack(std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeAudioTrack::~NativeAudioTrack() = default;
const std::unique_ptr<NativeAudioTrack::State>& NativeAudioTrack::state() const noexcept {
  return state_;
}

std::unique_ptr<NativeAudioSource> create_audio_source(
    const NativePeerConnectionFactory& factory) noexcept {
  auto state = std::make_unique<NativeAudioSource::State>();
  state->signaling_thread = factory.signaling_thread();
  if (!state->signaling_thread) return nullptr;
  state->signaling_thread->BlockingCall(
      [&] { state->source = webrtc::make_ref_counted<PushAudioSource>(); });
  return std::make_unique<NativeAudioSource>(std::move(state));
}
bool close_audio_source(const NativeAudioSource& source) noexcept {
  auto& state = *source.state();
  if (!state.source || !state.signaling_thread) return false;
  state.signaling_thread->BlockingCall([&] { state.source->Close(); });
  return true;
}
bool audio_source_push_pcm(const NativeAudioSource& source,
                           rust::Slice<const std::int16_t> samples,
                           std::uint32_t sample_rate_hz,
                           std::uint8_t channels,
                           std::int64_t timestamp_us) noexcept {
  if (timestamp_us < 0 ||
      (sample_rate_hz != 8000 && sample_rate_hz != 16000 &&
       sample_rate_hz != 32000 && sample_rate_hz != 48000) ||
      (channels != 1 && channels != 2) ||
      samples.size() != (sample_rate_hz / 100) * channels) return false;
  return source.state()->source->Push(samples.data(), static_cast<int>(sample_rate_hz),
                                      channels, sample_rate_hz / 100);
}
std::unique_ptr<NativeAudioTrack> create_audio_track(
    const NativePeerConnectionFactory& factory, const NativeAudioSource& source,
    rust::Str id) noexcept {
  if (!source.state()->source || source.state()->source->state() !=
      webrtc::MediaSourceInterface::kLive || id.empty()) return nullptr;
  auto track = factory.factory()->CreateAudioTrack(
      std::string(id.data(), id.size()), source.state()->source.get());
  if (!track) return nullptr;
  auto state = std::make_unique<NativeAudioTrack::State>();
  state->track = std::move(track);
  return std::make_unique<NativeAudioTrack>(std::move(state));
}
rust::String audio_track_id(const NativeAudioTrack& track) noexcept {
  return track.state()->track->id();
}
bool audio_track_enabled(const NativeAudioTrack& track) noexcept {
  return track.state()->track->enabled();
}
bool audio_track_set_enabled(const NativeAudioTrack& track, bool enabled) noexcept {
  return track.state()->track->set_enabled(enabled);
}
std::unique_ptr<NativeAudioSink> rtp_receiver_attach_audio_sink(
    const NativePeerConnection& peer, const NativeRtpReceiver& receiver) noexcept {
  if (!peer.peer() || !peer.signaling_thread()) return nullptr;
  const auto remote = receiver.receiver();
  bool belongs = false;
  for (const auto& candidate : peer.peer()->GetReceivers()) {
    if (candidate == remote) belongs = true;
  }
  if (!belongs || !remote || !remote->track() ||
      remote->track()->kind() != webrtc::MediaStreamTrackInterface::kAudioKind) {
    return nullptr;
  }
  auto state = std::make_unique<NativeAudioSink::State>();
  state->peer = &peer;
  state->track = static_cast<webrtc::AudioTrackInterface*>(remote->track().get());
  state->collector = std::make_unique<ReceivedAudioCollector>();
  state->signaling_thread = peer.signaling_thread();
  auto* track = state->track.get();
  auto* collector = state->collector.get();
  state->signaling_thread->BlockingCall([&] { track->AddSink(collector); });
  return std::make_unique<NativeAudioSink>(std::move(state));
}
FfiReceivedAudioFrame audio_sink_take_frame(const NativeAudioSink& sink) noexcept {
  auto frame = sink.state()->collector->Take();
  if (!frame.valid && !sink.state()->closed.load() &&
      pump_headless_audio(*sink.state()->peer)) {
    frame = sink.state()->collector->Take();
  }
  return frame;
}
std::uint64_t audio_sink_dropped_frames(const NativeAudioSink& sink) noexcept {
  return sink.state()->collector->DroppedFrames();
}
bool close_audio_sink(const NativeAudioSink& sink) noexcept {
  auto& state = *sink.state();
  if (state.closed.exchange(true)) return true;
  state.collector->Deactivate();
  state.signaling_thread->BlockingCall([&] {
    state.track->RemoveSink(state.collector.get());
  });
  return true;
}
std::unique_ptr<NativeRtpTransceiver> peer_add_audio_transceiver(
    const NativePeerConnection& peer, const NativeAudioTrack& track,
    std::uint8_t direction, std::uint8_t& error_type,
    rust::String& error) noexcept {
  webrtc::RtpTransceiverInit init;
  switch (direction) {
    case 0: init.direction = webrtc::RtpTransceiverDirection::kSendRecv; break;
    case 1: init.direction = webrtc::RtpTransceiverDirection::kSendOnly; break;
    case 2: init.direction = webrtc::RtpTransceiverDirection::kRecvOnly; break;
    case 3: init.direction = webrtc::RtpTransceiverDirection::kInactive; break;
    default:
      error_type = static_cast<std::uint8_t>(webrtc::RTCErrorType::INVALID_PARAMETER);
      error = "invalid RTP transceiver direction";
      return nullptr;
  }
  auto result = peer.peer()->AddTransceiver(track.state()->track, init);
  if (!result.ok()) {
    error_type = static_cast<std::uint8_t>(result.error().type());
    error = result.error().message();
    return nullptr;
  }
  return wrap_rtp_transceiver(result.MoveValue());
}

}  // namespace pulsebeam::webrtc_sys
