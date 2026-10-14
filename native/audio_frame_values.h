#pragma once
#include <cstddef>
#include <cstdint>
#include <memory>
#include "rust/cxx.h"
namespace webrtc { class AudioFrame; }
namespace pulsebeam::webrtc_sys {
struct FfiAudioFrameFields;

// Native operations mutate one uniquely owned frame. The full backing buffer
// is initialized once before any Rust view or public-field assignment.
class NativeAudioFrame final {
 public:
  NativeAudioFrame();
  NativeAudioFrame(std::int32_t, std::size_t, std::int32_t);
  // Internal synchronous callback view. Never transfer this wrapper to an
  // owning Rust AudioFrame; the SDK frame and its backing storage stay borrowed.
  explicit NativeAudioFrame(webrtc::AudioFrame& frame) noexcept;
  ~NativeAudioFrame();
  webrtc::AudioFrame& value() noexcept;
  const webrtc::AudioFrame& value() const noexcept;
 private:
  std::unique_ptr<webrtc::AudioFrame> owned_;
  webrtc::AudioFrame* frame_;
};
bool MediaAudioFrameViewValid(const webrtc::AudioFrame&) noexcept;
std::unique_ptr<NativeAudioFrame> media_audio_frame_new() noexcept;
std::unique_ptr<NativeAudioFrame> media_audio_frame_with_format(std::int32_t, std::size_t, std::int32_t) noexcept;
std::size_t media_audio_default_channel_size(std::size_t) noexcept;
std::int32_t media_channel_layout_count(std::int32_t) noexcept;
std::int32_t media_channel_order(std::int32_t, std::int32_t) noexcept;
std::int32_t media_channel_layout_guess(std::int32_t) noexcept;
rust::String media_channel_layout_string(std::int32_t) noexcept;
FfiAudioFrameFields media_audio_frame_fields(const NativeAudioFrame&) noexcept;
bool media_audio_frame_set_fields(NativeAudioFrame&, const FfiAudioFrameFields&) noexcept;
void media_audio_frame_reset(NativeAudioFrame&, bool) noexcept;
bool media_audio_frame_update(NativeAudioFrame&, std::uint32_t, rust::Slice<const std::int16_t>, bool,
    std::size_t, std::int32_t, std::int32_t, std::int32_t, std::size_t) noexcept;
bool media_audio_frame_copy(NativeAudioFrame&, const NativeAudioFrame&) noexcept;
rust::Slice<const std::int16_t> media_audio_frame_data(const NativeAudioFrame&) noexcept;
rust::Slice<const std::int16_t> media_audio_frame_data_view(const NativeAudioFrame&, bool&) noexcept;
rust::Slice<std::int16_t> media_audio_frame_mutable_data(NativeAudioFrame&) noexcept;
rust::Slice<std::int16_t> media_audio_frame_mutable_shape(NativeAudioFrame&, std::size_t, std::size_t, bool&) noexcept;
rust::Slice<std::int16_t> media_audio_frame_fill_shape(NativeAudioFrame&, std::size_t, std::size_t, std::int16_t, bool&) noexcept;
void media_audio_frame_mute(NativeAudioFrame&) noexcept;
bool media_audio_frame_muted(const NativeAudioFrame&) noexcept;
std::size_t media_audio_frame_max_samples(const NativeAudioFrame&) noexcept;
std::size_t media_audio_frame_samples_per_channel(const NativeAudioFrame&) noexcept;
std::size_t media_audio_frame_num_channels(const NativeAudioFrame&) noexcept;
std::int32_t media_audio_frame_layout(const NativeAudioFrame&) noexcept;
std::int32_t media_audio_frame_sample_rate(const NativeAudioFrame&) noexcept;
bool media_audio_frame_set_layout(NativeAudioFrame&, std::int32_t, std::size_t) noexcept;
std::int64_t media_audio_frame_capture_timestamp(const NativeAudioFrame&, bool&) noexcept;
void media_audio_frame_set_capture_timestamp(NativeAudioFrame&, std::int64_t) noexcept;
bool media_audio_frame_set_sample_rate(NativeAudioFrame&, std::int32_t) noexcept;
}  // namespace pulsebeam::webrtc_sys
