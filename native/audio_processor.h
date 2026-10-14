#pragma once
#include <cstddef>
#include <cstdint>
#include <memory>
#include "rust/cxx.h"
namespace pulsebeam::webrtc_sys {
class NativeEnvironment;
struct FfiAudioProcessingConfig;
struct FfiAudioProcessingFormatState;
class NativeAudioProcessor final {
 public:
  struct State;
  explicit NativeAudioProcessor(std::unique_ptr<State>) noexcept;
  ~NativeAudioProcessor();
  State& state() noexcept;
  const State& state() const noexcept;
 private:
  std::unique_ptr<State> state_;
};
std::unique_ptr<NativeAudioProcessor> audio_processor_new(const NativeEnvironment&, const FfiAudioProcessingConfig&) noexcept;
bool audio_processor_effects_admitted(const NativeEnvironment&, const FfiAudioProcessingConfig&) noexcept;
std::int32_t audio_processor_initialize(NativeAudioProcessor&) noexcept;
void audio_processor_configure(NativeAudioProcessor&, const FfiAudioProcessingConfig&) noexcept;
FfiAudioProcessingFormatState audio_processor_format(const NativeAudioProcessor&) noexcept;
void audio_processor_set_muted(NativeAudioProcessor&, bool) noexcept;
bool audio_processor_muted(NativeAudioProcessor&) noexcept;
std::int32_t audio_processor_set_delay(NativeAudioProcessor&, std::int32_t) noexcept;
std::int32_t audio_processor_delay(const NativeAudioProcessor&) noexcept;
void audio_processor_set_analog_level(NativeAudioProcessor&, std::int32_t) noexcept;
std::int32_t audio_processor_analog_level(const NativeAudioProcessor&) noexcept;
void audio_processor_set_key(NativeAudioProcessor&, bool) noexcept;
std::int32_t audio_processor_i16(NativeAudioProcessor&, std::int32_t, std::size_t, rust::Slice<std::int16_t>, bool) noexcept;
std::int32_t audio_processor_float(NativeAudioProcessor&, std::int32_t, std::size_t, rust::Slice<const float>, std::int32_t, std::size_t, rust::Slice<float>, bool) noexcept;
std::int32_t audio_processor_analyze(NativeAudioProcessor&, std::int32_t, std::size_t, rust::Slice<const float>) noexcept;
}
