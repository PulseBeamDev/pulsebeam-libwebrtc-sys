#include "pulsebeam-webrtc-sys/native/audio_frame_values.h"
#include <algorithm>
#include "api/audio/audio_frame.h"
#include "api/audio/audio_view.h"
#include "api/audio/channel_layout.h"
#include "api/rtp_packet_infos.h"
#include "pulsebeam-webrtc-sys/native/media_values.h"
#include "pulsebeam-webrtc-sys/src/lib.rs.h"

namespace pulsebeam::webrtc_sys {
namespace {
static_assert(webrtc::AudioFrame::kMaxDataSizeSamples == 7680);
static_assert(webrtc::kMaxNumberOfAudioChannels == 16);
bool CapacityShape(std::size_t samples, std::size_t channels) {
  // Division checks both multiplication overflow and the fixed buffer bound.
  return channels == 0 || samples <= webrtc::AudioFrame::kMaxDataSizeSamples / channels;
}
bool Enums(std::int32_t speech, std::int32_t vad) {
  return speech >= 0 && speech <= 7 && vad >= 0 && vad <= 3;
}
bool LayoutIndex(std::int32_t layout) { return layout >= 0 && layout <= 32; }
void Initialize(webrtc::AudioFrame& frame) {
  // A fresh frame is muted, so this zeros the entire backing array. Preserve
  // the constructor's mute state. Later shape changes may expose any prefix.
  frame.mutable_data();
  frame.Mute();
}
}  // namespace
NativeAudioFrame::NativeAudioFrame()
    : owned_(std::make_unique<webrtc::AudioFrame>()), frame_(owned_.get()) { Initialize(*frame_); }
NativeAudioFrame::NativeAudioFrame(std::int32_t rate, std::size_t channels, std::int32_t layout)
    : owned_(std::make_unique<webrtc::AudioFrame>(rate, channels, static_cast<webrtc::ChannelLayout>(layout))),
      frame_(owned_.get()) { Initialize(*frame_); }
NativeAudioFrame::NativeAudioFrame(webrtc::AudioFrame& frame) noexcept : frame_(&frame) {}
NativeAudioFrame::~NativeAudioFrame() = default;
webrtc::AudioFrame& NativeAudioFrame::value() noexcept { return *frame_; }
const webrtc::AudioFrame& NativeAudioFrame::value() const noexcept { return *frame_; }
bool MediaAudioFrameViewValid(const webrtc::AudioFrame& frame) noexcept {
  return CapacityShape(frame.samples_per_channel_, frame.num_channels_) &&
      frame.num_channels_ <= webrtc::kMaxNumberOfAudioChannels &&
      (frame.num_channels_ == 0 || frame.samples_per_channel_ != 0);
}
std::unique_ptr<NativeAudioFrame> media_audio_frame_new() noexcept { return std::make_unique<NativeAudioFrame>(); }
std::unique_ptr<NativeAudioFrame> media_audio_frame_with_format(std::int32_t rate, std::size_t channels, std::int32_t layout) noexcept {
  if (rate < 100 || rate > 192000 || channels > webrtc::kMaxNumberOfAudioChannels ||
      layout < 0 || layout > 63 || !CapacityShape(static_cast<std::size_t>(rate) / 100, channels)) return nullptr;
  return std::make_unique<NativeAudioFrame>(rate, channels, layout);
}
std::size_t media_audio_default_channel_size(std::size_t rate) noexcept {
  return rate > 192000 ? 0 : webrtc::SampleRateToDefaultChannelSize(rate);
}
std::int32_t media_channel_layout_count(std::int32_t layout) noexcept {
  if (!LayoutIndex(layout)) return 0;
  return webrtc::ChannelLayoutToChannelCount(static_cast<webrtc::ChannelLayout>(layout));
}
std::int32_t media_channel_order(std::int32_t layout, std::int32_t channel) noexcept {
  if (!LayoutIndex(layout) || channel < 0 || channel > 10) return -1;
  return webrtc::ChannelOrder(static_cast<webrtc::ChannelLayout>(layout), static_cast<webrtc::Channels>(channel));
}
std::int32_t media_channel_layout_guess(std::int32_t channels) noexcept {
  return static_cast<std::int32_t>(webrtc::GuessChannelLayout(channels));
}
rust::String media_channel_layout_string(std::int32_t layout) noexcept {
  if (!LayoutIndex(layout)) return rust::String();
  return rust::String(webrtc::ChannelLayoutToString(static_cast<webrtc::ChannelLayout>(layout)));
}
FfiAudioFrameFields media_audio_frame_fields(const NativeAudioFrame& owner) noexcept {
  const auto& frame = owner.value();
  FfiAudioFrameFields result{};
  result.timestamp = frame.timestamp_; result.elapsed_time_ms = frame.elapsed_time_ms_;
  result.ntp_time_ms = frame.ntp_time_ms_; result.samples_per_channel = frame.samples_per_channel_;
  result.sample_rate_hz = frame.sample_rate_hz_; result.num_channels = frame.num_channels_;
  result.speech_type = static_cast<std::int32_t>(frame.speech_type_);
  result.vad_activity = static_cast<std::int32_t>(frame.vad_activity_);
  result.packet_infos = MediaPacketInfosToFfi(frame.packet_infos_);
  return result;
}
bool media_audio_frame_set_fields(NativeAudioFrame& owner, const FfiAudioFrameFields& value) noexcept {
  if (!CapacityShape(value.samples_per_channel, value.num_channels) || !Enums(value.speech_type, value.vad_activity)) return false;
  auto& frame = owner.value();
  frame.timestamp_ = value.timestamp; frame.elapsed_time_ms_ = value.elapsed_time_ms;
  frame.ntp_time_ms_ = value.ntp_time_ms; frame.samples_per_channel_ = value.samples_per_channel;
  frame.sample_rate_hz_ = value.sample_rate_hz; frame.num_channels_ = value.num_channels;
  frame.speech_type_ = static_cast<webrtc::AudioFrame::SpeechType>(value.speech_type);
  frame.vad_activity_ = static_cast<webrtc::AudioFrame::VADActivity>(value.vad_activity);
  frame.packet_infos_ = MediaPacketInfosFromFfi(rust::Slice<const FfiRtpReceivedPacketInfo>(value.packet_infos.data(), value.packet_infos.size()));
  return true;
}
void media_audio_frame_reset(NativeAudioFrame& owner, bool mute) noexcept {
  if (mute) owner.value().Reset(); else owner.value().ResetWithoutMuting();
}
bool media_audio_frame_update(NativeAudioFrame& owner, std::uint32_t timestamp, rust::Slice<const std::int16_t> data,
    bool present, std::size_t samples, std::int32_t rate, std::int32_t speech, std::int32_t vad, std::size_t channels) noexcept {
  if (!CapacityShape(samples, channels) || channels > webrtc::kMaxNumberOfAudioChannels || !Enums(speech, vad)) return false;
  const auto length = samples * channels;
  if (present && data.size() < length) return false;
  const std::int16_t empty = 0;
  const auto* input = !present ? nullptr : length == 0 ? &empty : data.data();
  owner.value().UpdateFrame(timestamp, input, samples, rate, static_cast<webrtc::AudioFrame::SpeechType>(speech),
      static_cast<webrtc::AudioFrame::VADActivity>(vad), channels);
  return true;
}
bool media_audio_frame_copy(NativeAudioFrame& target, const NativeAudioFrame& source) noexcept {
  if (&target == &source) return true;
  if (!MediaAudioFrameViewValid(source.value())) return false;
  target.value().CopyFrom(source.value());
  return true;
}
rust::Slice<const std::int16_t> media_audio_frame_data(const NativeAudioFrame& owner) noexcept {
  return {owner.value().data(), owner.value().max_16bit_samples()};
}
rust::Slice<const std::int16_t> media_audio_frame_data_view(const NativeAudioFrame& owner, bool& valid) noexcept {
  valid = MediaAudioFrameViewValid(owner.value());
  if (!valid) return {};
  const auto view = owner.value().data_view().data();
  return {view.empty() ? owner.value().data() : view.data(), view.size()};
}
rust::Slice<std::int16_t> media_audio_frame_mutable_data(NativeAudioFrame& owner) noexcept {
  return {owner.value().mutable_data(), owner.value().max_16bit_samples()};
}
rust::Slice<std::int16_t> media_audio_frame_mutable_shape(NativeAudioFrame& owner, std::size_t samples, std::size_t channels, bool& valid) noexcept {
  valid = CapacityShape(samples, channels) && channels <= webrtc::kMaxNumberOfAudioChannels && channels <= samples;
  if (!valid) return {};
  auto view = owner.value().mutable_data(samples, channels).data();
  // This native view always starts at the real backing buffer, including empty
  // views. No second mutable_data() call: that would change prefix-only zeroing.
  return {view.data(), view.size()};
}
rust::Slice<std::int16_t> media_audio_frame_fill_shape(NativeAudioFrame& owner, std::size_t samples, std::size_t channels, std::int16_t value, bool& valid) noexcept {
  valid = CapacityShape(samples, channels) && channels <= webrtc::kMaxNumberOfAudioChannels && channels <= samples;
  if (!valid) return {};
  auto view = owner.value().mutable_data(samples, channels).data();
  // Initialize before constructing any Rust slice of a borrowed SDK buffer.
  std::fill(view.begin(), view.end(), value);
  return {view.data(), view.size()};
}
void media_audio_frame_mute(NativeAudioFrame& owner) noexcept { owner.value().Mute(); }
bool media_audio_frame_muted(const NativeAudioFrame& owner) noexcept { return owner.value().muted(); }
std::size_t media_audio_frame_max_samples(const NativeAudioFrame& owner) noexcept { return owner.value().max_16bit_samples(); }
std::size_t media_audio_frame_samples_per_channel(const NativeAudioFrame& owner) noexcept { return owner.value().samples_per_channel(); }
std::size_t media_audio_frame_num_channels(const NativeAudioFrame& owner) noexcept { return owner.value().num_channels(); }
std::int32_t media_audio_frame_layout(const NativeAudioFrame& owner) noexcept { return static_cast<std::int32_t>(owner.value().channel_layout()); }
std::int32_t media_audio_frame_sample_rate(const NativeAudioFrame& owner) noexcept { return owner.value().sample_rate_hz(); }
bool media_audio_frame_set_layout(NativeAudioFrame& owner, std::int32_t layout, std::size_t channels) noexcept {
  if (!LayoutIndex(layout) || !CapacityShape(owner.value().samples_per_channel_, channels)) return false;
  const auto native = static_cast<webrtc::ChannelLayout>(layout);
  const auto expected = webrtc::ChannelLayoutToChannelCount(native);
  if (expected != 0 && static_cast<std::size_t>(expected) != channels) return false;
  owner.value().SetLayoutAndNumChannels(native, channels);
  return true;
}
std::int64_t media_audio_frame_capture_timestamp(const NativeAudioFrame& owner, bool& present) noexcept {
  const auto value = owner.value().absolute_capture_timestamp_ms();
  present = value.has_value();
  return value.value_or(0);
}
void media_audio_frame_set_capture_timestamp(NativeAudioFrame& owner, std::int64_t timestamp) noexcept {
  owner.value().set_absolute_capture_timestamp_ms(timestamp);
}
bool media_audio_frame_set_sample_rate(NativeAudioFrame& owner, std::int32_t rate) noexcept {
  if (rate < 0 || rate > 192000 || !CapacityShape(static_cast<std::size_t>(rate) / 100, owner.value().num_channels_)) return false;
  owner.value().SetSampleRateAndChannelSize(rate);
  return true;
}
}  // namespace pulsebeam::webrtc_sys
