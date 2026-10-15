#pragma once
#include <cstddef>
#include <cstdint>
#include <memory>
#include "rust/cxx.h"
namespace pulsebeam::webrtc_sys {
class NativeEnvironment;
struct RustApmDumpWorker;
struct FfiApmConfig;
struct FfiApmFormats;
struct FfiApmStatistics;
struct FfiApmRuntimeSetting;
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
bool audio_processor_dump_path(NativeAudioProcessor&, rust::Box<RustApmDumpWorker>, rust::Slice<const std::uint8_t>, std::int64_t, std::uint8_t&) noexcept;
bool audio_processor_dump_file(NativeAudioProcessor&, rust::Box<RustApmDumpWorker>, std::int32_t, std::int64_t, std::uint8_t&) noexcept;
void audio_processor_dump_detach(NativeAudioProcessor&) noexcept;
bool audio_processor_runtime_setting(NativeAudioProcessor&, const FfiApmRuntimeSetting&, bool, std::uint8_t&) noexcept;
bool audio_processor_config_equal(const FfiApmConfig&, const FfiApmConfig&, std::uint8_t) noexcept;
std::unique_ptr<NativeAudioProcessor> audio_processor_new_config(const NativeEnvironment&, const FfiApmConfig&, bool, std::uint8_t&) noexcept;
std::uint8_t audio_processor_config_apply(NativeAudioProcessor&, const FfiApmConfig&) noexcept;
bool audio_processor_linear_output(NativeAudioProcessor&, rust::Slice<float>, std::uint8_t&) noexcept;
FfiApmStatistics audio_processor_statistics_default() noexcept;
FfiApmStatistics audio_processor_statistics(NativeAudioProcessor&, bool, bool) noexcept;
FfiApmConfig audio_processor_config_default() noexcept;
rust::String audio_processor_config_string(const FfiApmConfig&) noexcept;
FfiApmConfig audio_processor_config_get(const NativeAudioProcessor&) noexcept;
bool audio_processor_effects_admitted(const NativeEnvironment&, const FfiAudioProcessingConfig&) noexcept;
std::int32_t audio_processor_initialize(NativeAudioProcessor&, std::uint8_t&) noexcept;
FfiApmFormats audio_processor_formats_default() noexcept;
std::int32_t audio_processor_initialize_formats(NativeAudioProcessor&, const FfiApmFormats&, bool&, std::uint8_t&) noexcept;
std::uint8_t audio_processor_configure(NativeAudioProcessor&, const FfiAudioProcessingConfig&) noexcept;
FfiAudioProcessingFormatState audio_processor_format(const NativeAudioProcessor&) noexcept;
void audio_processor_set_muted(NativeAudioProcessor&, bool) noexcept;
bool audio_processor_muted(NativeAudioProcessor&) noexcept;
std::int32_t audio_processor_set_delay(NativeAudioProcessor&, std::int32_t) noexcept;
std::int32_t audio_processor_delay(const NativeAudioProcessor&) noexcept;
void audio_processor_set_analog_level(NativeAudioProcessor&, std::int32_t) noexcept;
std::int32_t audio_processor_analog_level(const NativeAudioProcessor&) noexcept;
void audio_processor_set_key(NativeAudioProcessor&, bool) noexcept;
std::int32_t audio_processor_i16(NativeAudioProcessor&, std::int32_t, std::size_t, rust::Slice<std::int16_t>, bool, std::uint8_t&) noexcept;
std::int32_t audio_processor_float(NativeAudioProcessor&, std::int32_t, std::size_t, rust::Slice<const float>, std::int32_t, std::size_t, rust::Slice<float>, bool, std::uint8_t&) noexcept;
std::int32_t audio_processor_analyze(NativeAudioProcessor&, std::int32_t, std::size_t, rust::Slice<const float>, std::uint8_t&) noexcept;
}
