#include "pulsebeam-webrtc-sys/native/audio.h"

#include <atomic>
#include <list>
#include <mutex>
#include <optional>
#include <string>

#include "api/make_ref_counted.h"
#include "api/media_stream_interface.h"
#include "api/notifier.h"
#include "api/peer_connection_interface.h"
#include "api/scoped_refptr.h"
#include "api/rtp_transceiver_interface.h"
#include "pulsebeam-webrtc-sys/native/peer.h"
#include "pulsebeam-webrtc-sys/native/video.h"
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

}  // namespace

struct NativeAudioSource::State {
  webrtc::scoped_refptr<PushAudioSource> source;
  webrtc::Thread* signaling_thread = nullptr;
};

struct NativeAudioTrack::State {
  webrtc::scoped_refptr<webrtc::AudioTrackInterface> track;
};

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
