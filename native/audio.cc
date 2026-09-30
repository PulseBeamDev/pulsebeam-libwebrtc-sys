#include "pulsebeam-webrtc-sys/native/audio.h"

#include <algorithm>
#include <array>
#include <atomic>
#include <cstring>
#include <deque>
#include <limits>
#include <list>
#include <mutex>
#include <optional>
#include <span>
#include <string>
#include <vector>

#include "api/frame_transformer_interface.h"
#include "api/make_ref_counted.h"
#include "api/media_stream_interface.h"
#include "api/notifier.h"
#include "api/peer_connection_interface.h"
#include "api/scoped_refptr.h"
#include "api/rtp_transceiver_interface.h"
#include "pulsebeam-webrtc-sys/native/peer.h"
#include "pulsebeam-webrtc-sys/native/opus_carrier.h"
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
  const webrtc::AudioOptions options() const override {
    std::lock_guard lock(mutex_);
    return options_;
  }
  void SetOptions(const webrtc::AudioOptions& options) override {
    {
      std::lock_guard lock(mutex_);
      options_ = options;
    }
    FireOnChanged();
  }

  void AddSink(webrtc::AudioTrackSinkInterface* sink) override {
    std::lock_guard lock(mutex_);
    if (!closed_ && sink) sinks_.push_back(sink);
  }
  void RemoveSink(webrtc::AudioTrackSinkInterface* sink) override {
    std::lock_guard lock(mutex_);
    sinks_.remove(sink);
  }
  bool HasSinks() {
    std::lock_guard lock(mutex_);
    return !closed_ && !sinks_.empty();
  }
  bool Push(const std::int16_t* samples, int sample_rate, size_t channels,
            size_t frames,
            std::optional<std::int64_t> capture_time_ms = std::nullopt) {
    std::lock_guard lock(mutex_);
    if (closed_) return false;
    for (auto* sink : sinks_) {
      // Only explicitly mapped controlled timing is forwarded. Ordinary raw
      // caller timestamps are not automatically a TimeMillis capture clock.
      sink->OnData(samples, 16, sample_rate, channels, frames, capture_time_ms);
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
  mutable std::mutex mutex_;
  std::list<webrtc::AudioTrackSinkInterface*> sinks_;
  webrtc::AudioOptions options_;
  std::atomic<bool> closed_{false};
};

// Receive callbacks arrive on a WebRTC audio thread. The queue retains at
// most eight 10 ms blocks; consumers observe loss through DroppedFrames().
// Opus TOC (RFC 6716 section 3) determines packet duration at 48 kHz.
std::uint32_t OpusSamples(std::span<const std::uint8_t> data) {
  if (data.empty()) return 0;
  const unsigned config = data[0] >> 3;
  const unsigned code = data[0] & 3;
  const unsigned count = code == 0 ? 1 : code == 3
      ? (data.size() > 1 ? data[1] & 63 : 0) : 2;
  if (count == 0 || count > 48) return 0;
  const unsigned per_frame = config < 12 ? (480u << (config & 3))
      : config < 16 ? (480u << (config & 1))
      : (120u << (config & 3));
  const unsigned samples = count * per_frame;
  return samples <= 5760 ? samples : 0;
}

// Swallow frames before NetEq so observing Opus never invokes its decoder.
// Upstream retains the transformer after close until the receiver dies.
class EncodedAudioCollector : public webrtc::FrameTransformerInterface {
 public:
  void Transform(std::unique_ptr<webrtc::TransformableFrameInterface> frame) override {
    std::lock_guard lock(mutex_);
    if (!active_) return;
    const auto data = frame->GetData();
    if (frame->GetDirection() !=
            webrtc::TransformableFrameInterface::Direction::kReceiver ||
        frame->GetMimeType() != "audio/opus" || data.empty() ||
        data.size() > 65536) {
      Lost();
      return;
    }
    const auto samples = OpusSamples(data);
    if (!samples) {
      Lost();
      return;
    }
    while (!frames_.empty() &&
           (frames_.size() >= 64 || bytes_ > 262144 - data.size())) {
      bytes_ -= frames_.front().data.size();
      frames_.pop_front();
      Lost();
    }
    Frame saved;
    saved.data.assign(data.begin(), data.end());
    saved.rtp_timestamp = frame->GetTimestamp();
    saved.ssrc = frame->GetSsrc();
    saved.payload_type = frame->GetPayloadType();
    saved.samples = static_cast<std::uint32_t>(samples);
    if (auto time = frame->CaptureTime()) saved.capture_us = time->us();
    if (auto time = frame->ReceiveTime()) saved.receive_us = time->us();
    bytes_ += saved.data.size();
    frames_.push_back(std::move(saved));
  }
  FfiEncodedAudioFrame Take() {
    std::lock_guard lock(mutex_);
    FfiEncodedAudioFrame out{};
    if (frames_.empty()) return out;
    auto frame = std::move(frames_.front());
    frames_.pop_front();
    bytes_ -= frame.data.size();
    out.available = true;
    for (auto byte : frame.data) out.data.push_back(byte);
    out.rtp_timestamp = frame.rtp_timestamp;
    out.ssrc = frame.ssrc;
    out.payload_type = frame.payload_type;
    out.samples_per_channel = frame.samples;
    out.has_capture_time = frame.capture_us.has_value();
    out.capture_time_us = frame.capture_us.value_or(0);
    out.has_receive_time = frame.receive_us.has_value();
    out.receive_time_us = frame.receive_us.value_or(0);
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
    bytes_ = 0;
  }

 private:
  void Lost() {
    if (dropped_ != std::numeric_limits<std::uint64_t>::max()) ++dropped_;
  }
  struct Frame {
    std::vector<std::uint8_t> data;
    std::uint32_t rtp_timestamp = 0;
    std::uint32_t ssrc = 0;
    std::uint8_t payload_type = 0;
    std::uint32_t samples = 0;
    std::optional<std::int64_t> capture_us;
    std::optional<std::int64_t> receive_us;
  };
  mutable std::mutex mutex_;
  std::deque<Frame> frames_;
  std::size_t bytes_ = 0;
  std::uint64_t dropped_ = 0;
  bool active_ = true;
};

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

struct NativeEncodedAudioSink::State {
  webrtc::scoped_refptr<EncodedAudioCollector> collector;
};

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
  std::mutex frame_mutex;
  std::shared_ptr<opus_carrier::Budget> budget;
  std::uint32_t budget_id = 0;
  std::uint8_t encoded_channels = 0;
};

struct NativeAudioTrack::State {
  webrtc::scoped_refptr<webrtc::AudioTrackInterface> track;
  webrtc::scoped_refptr<webrtc::AudioSourceInterface> microphone_source;
  webrtc::Thread* signaling_thread = nullptr;
};

NativeEncodedAudioSink::NativeEncodedAudioSink(std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeEncodedAudioSink::~NativeEncodedAudioSink() { close_encoded_audio_sink(*this); }
const std::unique_ptr<NativeEncodedAudioSink::State>&
NativeEncodedAudioSink::state() const noexcept { return state_; }

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
std::unique_ptr<NativeAudioSource> create_encoded_audio_source(
    const NativePeerConnectionFactory& factory, std::uint8_t channels) noexcept {
  if (channels != 1 && channels != 2) return nullptr;
  auto source = create_audio_source(factory);
  if (source) source->state()->encoded_channels = channels;
  return source;
}
rust::Vec<FfiAudioCodecCapability> peer_audio_codec_capabilities(
    const NativePeerConnectionFactory& factory, bool sender) noexcept {
  rust::Vec<FfiAudioCodecCapability> result;
  if (!factory.factory() || !factory.signaling_thread()) return result;
  const auto capabilities = factory.signaling_thread()->BlockingCall([&] {
    return sender
        ? factory.factory()->GetRtpSenderCapabilities(webrtc::MediaType::AUDIO)
        : factory.factory()->GetRtpReceiverCapabilities(webrtc::MediaType::AUDIO);
  });
  for (const auto& codec : capabilities.codecs) {
    FfiAudioCodecCapability item;
    item.format.name = codec.name;
    for (const auto& [key, value] : codec.parameters) {
      item.format.parameters.push_back({key, value});
    }
    item.clock_rate = codec.clock_rate.value_or(-1);
    item.channels = codec.num_channels.value_or(-1);
    result.push_back(std::move(item));
  }
  return result;
}
bool close_audio_source(const NativeAudioSource& source) noexcept {
  auto& state = *source.state();
  if (!state.source || !state.signaling_thread) return false;
  {
    std::lock_guard lock(state.frame_mutex);
    if (state.budget_id) {
      opus_carrier::Unregister(state.budget_id);
      state.budget_id = 0;
      state.budget.reset();
    }
    state.signaling_thread->BlockingCall([&] { state.source->Close(); });
  }
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
bool audio_source_push_opus(const NativeAudioSource& source,
                            rust::Slice<const std::uint8_t> payload,
                            std::uint32_t rtp_timestamp,
                            std::uint32_t samples_per_channel) noexcept {
  return audio_source_push_opus_at(source, payload, rtp_timestamp,
                                   samples_per_channel, -1);
}
bool audio_source_push_opus_at(const NativeAudioSource& source,
                            rust::Slice<const std::uint8_t> payload,
                            std::uint32_t rtp_timestamp,
                            std::uint32_t samples_per_channel,
                            std::int64_t capture_time_us) noexcept {
  if (capture_time_us < -1) return false;
  using namespace opus_carrier;
  // The private carrier matches the negotiated encoder's mono/stereo input.
  const auto channels = source.state()->encoded_channels;
  const auto block_bytes = kBytesPerChannelBlock * channels;
  if ((channels != 1 && channels != 2) ||
      samples_per_channel == 0 || samples_per_channel > 2880 ||
      samples_per_channel % 480 != 0 || payload.empty() ||
      payload.size() > kMaxPayloadBytes ||
      ((payload[0] & 4) != 0) != (channels == 2) ||
      OpusSamples({payload.data(), payload.size()}) != samples_per_channel ||
      payload.size() + kHeaderBytes >
          (samples_per_channel / 480) * block_bytes) return false;
  auto& state = *source.state();
  std::lock_guard lock(state.frame_mutex);
  if (!state.source->HasSinks()) return false;
  if (!state.budget) {
    state.budget = std::make_shared<Budget>();
    state.budget_id = Register(state.budget);
  }
  const auto slots = samples_per_channel / 480;
  const auto packet = Reserve(*state.budget, slots);
  if (!packet) return false;
  for (std::uint32_t slot = 0; slot < slots; ++slot) {
    std::array<std::int16_t, 960> samples{};
    auto* bytes = reinterpret_cast<std::uint8_t*>(samples.data());
    if (slot == 0) {
      std::memcpy(bytes, kMagic.data(), kMagic.size());
      Store16(bytes + 8, static_cast<std::uint16_t>(payload.size()));
      Store16(bytes + 10, static_cast<std::uint16_t>(samples_per_channel));
      Store32(bytes + 12, rtp_timestamp);
      Store32(bytes + 16, Checksum({payload.data(), payload.size()}));
      Store32(bytes + 20, state.budget_id);
      Store32(bytes + 24, *packet);
    }
    const auto available = slot == 0 ? block_bytes - kHeaderBytes : block_bytes;
    const auto payload_offset = slot == 0 ? 0 :
        (block_bytes - kHeaderBytes) + (slot - 1) * block_bytes;
    if (payload_offset < payload.size()) {
      std::memcpy(bytes + (slot == 0 ? kHeaderBytes : 0),
                  payload.data() + payload_offset,
                  std::min(available, payload.size() - payload_offset));
    }
    const auto capture_ms = capture_time_us < 0 ? std::nullopt :
        std::optional<std::int64_t>(capture_time_us / 1000 + slot * 10);
    if (!state.source->Push(samples.data(), 48000, channels, 480, capture_ms)) {
      Acknowledge(state.budget_id, *packet);
      return false;
    }
  }
  return true;
}
std::unique_ptr<NativeAudioTrack> create_microphone_track(
    const NativePeerConnectionFactory& factory, rust::Str id) noexcept {
  if (!factory.audio_device() || !factory.factory() || id.empty()) return nullptr;
  auto source = factory.factory()->CreateAudioSource(webrtc::AudioOptions{});
  if (!source) return nullptr;
  auto track = factory.factory()->CreateAudioTrack(
      std::string(id.data(), id.size()), source.get());
  if (!track) return nullptr;
  auto state = std::make_unique<NativeAudioTrack::State>();
  state->track = std::move(track);
  state->microphone_source = std::move(source);
  state->signaling_thread = factory.signaling_thread();
  return std::make_unique<NativeAudioTrack>(std::move(state));
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
  state->signaling_thread = factory.signaling_thread();
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
bool audio_track_set_processing_options(const NativeAudioTrack& track,
                                        std::uint8_t echo, std::uint8_t noise,
                                        std::uint8_t gain,
                                        rust::String& error) noexcept {
  if (!track.state()->signaling_thread || echo > 3 || noise > 3 || gain > 3) {
    error = "invalid audio processing options or closed signaling thread";
    return false;
  }
  const auto option = [](std::uint8_t choice, std::optional<bool>& enabled,
                         std::optional<webrtc::AudioProcessingMode>& mode) {
    enabled = choice != 0;
    if (choice != 0) {
      mode = static_cast<webrtc::AudioProcessingMode>(choice - 1);
    }
  };
  webrtc::AudioOptions options;
  option(echo, options.echo_cancellation, options.echo_cancellation_mode);
  option(noise, options.noise_suppression, options.noise_suppression_mode);
  option(gain, options.auto_gain_control, options.auto_gain_control_mode);
  webrtc::AudioProcessingOptionsResult result;
  track.state()->signaling_thread->BlockingCall([&] {
    result = track.state()->track->SetAudioProcessingOptions(options);
  });
  if (!result.ok()) {
    error = result.message;
    return false;
  }
  return true;
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
      remote->track()->kind() != webrtc::MediaStreamTrackInterface::kAudioKind ||
      !peer.reserve_audio_receiver(remote->id())) {
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
  // A retired receiver must not drain stale PCM or trigger headless playout
  // for unrelated live receivers when its empty sink is polled.
  if (sink.state()->closed.load() ||
      sink.state()->track->state() == webrtc::MediaStreamTrackInterface::kEnded)
    return {};
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
std::unique_ptr<NativeEncodedAudioSink> rtp_receiver_attach_encoded_audio_sink(
    const NativePeerConnection& peer, const NativeRtpReceiver& receiver) noexcept {
  if (!peer.peer() || !peer.worker_thread() || !receiver.receiver()) return nullptr;
  const auto remote = receiver.receiver();
  bool belongs = false;
  for (const auto& candidate : peer.peer()->GetReceivers()) {
    if (candidate == remote) belongs = true;
  }
  if (!belongs || remote->media_type() != webrtc::MediaType::AUDIO) return nullptr;
  bool opus = false;
  for (const auto& codec : remote->GetParameters().codecs) {
    if (codec.name == "opus" || codec.name == "OPUS") opus = true;
  }
  if (!opus || !peer.reserve_audio_receiver(remote->id())) return nullptr;
  auto state = std::make_unique<NativeEncodedAudioSink::State>();
  state->collector = webrtc::make_ref_counted<EncodedAudioCollector>();
  peer.worker_thread()->BlockingCall([&] {
    remote->SetFrameTransformer(state->collector);
  });
  return std::make_unique<NativeEncodedAudioSink>(std::move(state));
}
FfiEncodedAudioFrame encoded_audio_sink_take_frame(
    const NativeEncodedAudioSink& sink) noexcept {
  return sink.state()->collector->Take();
}
std::uint64_t encoded_audio_sink_dropped_frames(
    const NativeEncodedAudioSink& sink) noexcept {
  return sink.state()->collector->DroppedFrames();
}
bool close_encoded_audio_sink(const NativeEncodedAudioSink& sink) noexcept {
  sink.state()->collector->Deactivate();
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
