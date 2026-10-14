#include "pulsebeam-webrtc-sys/native/audio_processor.h"
#include <algorithm>
#include <cerrno>
#include <cctype>
#include <cmath>
#include <cstdlib>
#include <limits>
#include <string>
#include "api/audio/echo_canceller3_config.h"
#include "rtc_base/experiments/field_trial_parser.h"
#include <utility>
#include <vector>
#include "api/audio/builtin_audio_processing_builder.h"
#include "api/environment/environment.h"
#include "pulsebeam-webrtc-sys/native/execution.h"
#include "pulsebeam-webrtc-sys/src/lib.rs.h"
// Pinned internal preflight declaration from aec3/echo_canceller3.h. Importing
// the full internal class header would require its non-exported debug-dump
// layout definitions; only this free function's public value types cross here.
namespace webrtc {
EchoCanceller3Config AdjustConfig(const EchoCanceller3Config&, const FieldTrialsView&);
}
namespace pulsebeam::webrtc_sys {
struct NativeAudioProcessor::State {
  // Destroy the module before its original environment dependencies.
  webrtc::Environment environment;
  webrtc::scoped_refptr<webrtc::AudioProcessing> module;
};
NativeAudioProcessor::NativeAudioProcessor(std::unique_ptr<State> state) noexcept : state_(std::move(state)) {}
NativeAudioProcessor::~NativeAudioProcessor() = default;
NativeAudioProcessor::State& NativeAudioProcessor::state() noexcept { return *state_; }
const NativeAudioProcessor::State& NativeAudioProcessor::state() const noexcept { return *state_; }
namespace {
void SelectEffects(webrtc::AudioProcessing::Config& config, const FfiAudioProcessingConfig& selected) {
  config.echo_canceller.enabled = selected.echo_cancellation;
  config.noise_suppression.enabled = selected.noise_suppression != 0;
  if (selected.noise_suppression != 0) config.noise_suppression.level = static_cast<webrtc::AudioProcessing::Config::NoiseSuppression::Level>(selected.noise_suppression - 1);
  config.gain_controller1.enabled = selected.gain_control >= 1 && selected.gain_control <= 3;
  if (config.gain_controller1.enabled) config.gain_controller1.mode = static_cast<webrtc::AudioProcessing::Config::GainController1::Mode>(selected.gain_control - 1);
  config.gain_controller2.enabled = selected.gain_control == 4;
  if (config.gain_controller2.enabled) config.gain_controller2.adaptive_digital.enabled = true;
}
// Even zero-length configurations get real, nonnull pointers for native
// DCHECKs. Rust supplies checked channel-major spans; no output is invented.
std::vector<const float*> InputPointers(rust::Slice<const float> samples, const webrtc::StreamConfig& config, const float* empty) {
  std::vector<const float*> result(std::max(std::size_t{1}, config.num_channels()), empty);
  if (config.num_frames() != 0) for (std::size_t ch = 0; ch < config.num_channels(); ++ch) result[ch] = samples.data() + ch * config.num_frames();
  return result;
}
std::vector<float*> OutputPointers(rust::Slice<float> samples, const webrtc::StreamConfig& config, float* empty) {
  std::vector<float*> result(std::max(std::size_t{1}, config.num_channels()), empty);
  if (config.num_frames() != 0) for (std::size_t ch = 0; ch < config.num_channels(); ++ch) result[ch] = samples.data() + ch * config.num_frames();
  return result;
}
}
std::unique_ptr<NativeAudioProcessor> audio_processor_new(const NativeEnvironment& environment, const FfiAudioProcessingConfig& selected) noexcept {
  if (selected.noise_suppression > 4 || selected.gain_control > 4) return nullptr;
  webrtc::AudioProcessing::Config config;
  SelectEffects(config, selected);
  auto state = std::make_unique<NativeAudioProcessor::State>(NativeAudioProcessor::State{environment.environment(), nullptr});
  state->module = webrtc::BuiltinAudioProcessingBuilder(config).Build(state->environment);
  if (!state->module) return nullptr;
  return std::make_unique<NativeAudioProcessor>(std::move(state));
}
namespace {
// Use the SDK's actual token dispatch, including keyless values, duplicate
// parameters, percent units and ignored tokens. Check scanf representation
// before the SDK parser; do not edit, mask or replace the environment trials.
enum class NumericKind { Double, Int64, Int32 };
bool NumericRepresentable(const std::string& text, NumericKind kind) {
  char* end = nullptr;
  errno = 0;
  if (kind != NumericKind::Double) {
    const auto value = std::strtoll(text.c_str(), &end, 10);
    if (end == text.c_str()) return true;  // Native conversion also fails.
    if (errno == ERANGE) return false;
    return kind != NumericKind::Int32 ||
        (value >= std::numeric_limits<std::int32_t>::min() &&
         value <= std::numeric_limits<std::int32_t>::max());
  }
  double value = std::strtod(text.c_str(), &end);
  if (end == text.c_str()) return true;
  if (!std::isfinite(value)) return false;
  while (*end && std::isspace(static_cast<unsigned char>(*end))) ++end;
  if (*end == '%') value /= 100;
  return std::abs(value) <= std::numeric_limits<float>::max();
}
class NumericTrialCheck final : public webrtc::FieldTrialParameterInterface {
 public:
  NumericTrialCheck(const char* key, NumericKind kind)
      : FieldTrialParameterInterface(key), kind_(kind) {}
  bool safe = true;
 private:
  bool Parse(std::optional<std::string> value) override {
    if (value) safe = NumericRepresentable(*value, kind_) && safe;
    return true;
  }
  NumericKind kind_;
};
bool SafeAecTrials(const webrtc::FieldTrialsView& trials) {
  NumericTrialCheck default_len("default_len", NumericKind::Double);
  NumericTrialCheck nearend_len("nearend_len", NumericKind::Double);
  webrtc::ParseFieldTrial({&default_len, &nearend_len},
      trials.Lookup("WebRTC-Aec3UseNearendReverbLen"));
  if (!default_len.safe || !nearend_len.safe) return false;
  NumericTrialCheck p0("nearend_tuning_mask_lf_enr_transparent", NumericKind::Double);
  NumericTrialCheck p1("nearend_tuning_mask_lf_enr_suppress", NumericKind::Double);
  NumericTrialCheck p2("nearend_tuning_mask_hf_enr_transparent", NumericKind::Double);
  NumericTrialCheck p3("nearend_tuning_mask_hf_enr_suppress", NumericKind::Double);
  NumericTrialCheck p4("nearend_tuning_max_inc_factor", NumericKind::Double);
  NumericTrialCheck p5("nearend_tuning_max_dec_factor_lf", NumericKind::Double);
  NumericTrialCheck p6("normal_tuning_mask_lf_enr_transparent", NumericKind::Double);
  NumericTrialCheck p7("normal_tuning_mask_lf_enr_suppress", NumericKind::Double);
  NumericTrialCheck p8("normal_tuning_mask_hf_enr_transparent", NumericKind::Double);
  NumericTrialCheck p9("normal_tuning_mask_hf_enr_suppress", NumericKind::Double);
  NumericTrialCheck p10("normal_tuning_max_inc_factor", NumericKind::Double);
  NumericTrialCheck p11("normal_tuning_max_dec_factor_lf", NumericKind::Double);
  NumericTrialCheck p12("dominant_nearend_detection_enr_threshold", NumericKind::Double);
  NumericTrialCheck p13("dominant_nearend_detection_enr_exit_threshold", NumericKind::Double);
  NumericTrialCheck p14("dominant_nearend_detection_snr_threshold", NumericKind::Double);
  NumericTrialCheck p15("dominant_nearend_detection_hold_duration", NumericKind::Int64);
  NumericTrialCheck p16("dominant_nearend_detection_trigger_threshold", NumericKind::Int64);
  webrtc::ParseFieldTrial({&p0, &p1, &p2, &p3, &p4, &p5, &p6, &p7, &p8, &p9, &p10, &p11, &p12, &p13, &p14, &p15, &p16},
      trials.Lookup("WebRTC-Aec3SuppressorTuningOverride"));
  if (!(p0.safe && p1.safe && p2.safe && p3.safe && p4.safe && p5.safe && p6.safe && p7.safe && p8.safe && p9.safe && p10.safe && p11.safe && p12.safe && p13.safe && p14.safe && p15.safe && p16.safe)) return false;
  const char* floating_trials[] = {
    "WebRTC-Aec3SuppressorNearendLfMaskTransparentOverride",
    "WebRTC-Aec3SuppressorNearendLfMaskSuppressOverride",
    "WebRTC-Aec3SuppressorNearendHfMaskTransparentOverride",
    "WebRTC-Aec3SuppressorNearendHfMaskSuppressOverride",
    "WebRTC-Aec3SuppressorNearendMaxIncFactorOverride",
    "WebRTC-Aec3SuppressorNearendMaxDecFactorLfOverride",
    "WebRTC-Aec3SuppressorNormalLfMaskTransparentOverride",
    "WebRTC-Aec3SuppressorNormalLfMaskSuppressOverride",
    "WebRTC-Aec3SuppressorNormalHfMaskTransparentOverride",
    "WebRTC-Aec3SuppressorNormalHfMaskSuppressOverride",
    "WebRTC-Aec3SuppressorNormalMaxIncFactorOverride",
    "WebRTC-Aec3SuppressorNormalMaxDecFactorLfOverride",
    "WebRTC-Aec3SuppressorDominantNearendEnrThresholdOverride",
    "WebRTC-Aec3SuppressorDominantNearendEnrExitThresholdOverride",
    "WebRTC-Aec3SuppressorDominantNearendSnrThresholdOverride",
    "WebRTC-Aec3SuppressorAntiHowlingGainOverride",
    "WebRTC-Aec3DelayEstimateSmoothingOverride",
    "WebRTC-Aec3DelayEstimateSmoothingDelayFoundOverride",
  };
  for (const auto* key : floating_trials) {
    NumericTrialCheck p("", NumericKind::Double);
    webrtc::ParseFieldTrial({&p}, trials.Lookup(key));
    if (!p.safe) return false;
  }
  const char* integer_trials[] = {
    "WebRTC-Aec3SuppressorDominantNearendHoldDurationOverride",
    "WebRTC-Aec3SuppressorDominantNearendTriggerThresholdOverride",
    "WebRTC-Aec3BufferingMaxAllowedExcessRenderBlocksOverride",
  };
  for (const auto* key : integer_trials) {
    NumericTrialCheck p("", NumericKind::Int64);
    webrtc::ParseFieldTrial({&p}, trials.Lookup(key));
    if (!p.safe) return false;
  }
  // This exact SDK adjustment is a private preflight, not a public DSP API or
  // an alternate configuration. The real builtin still consumes original trials.
  auto adjusted = webrtc::AdjustConfig(webrtc::EchoCanceller3Config{}, trials);
  auto copy = adjusted;
  if (!webrtc::EchoCanceller3Config::Validate(&copy)) return false;
  for (const auto* tuning : {&adjusted.suppressor.nearend_tuning,
                            &adjusted.suppressor.normal_tuning}) {
    if (!(tuning->mask_lf.enr_transparent < tuning->mask_lf.enr_suppress) ||
        !(tuning->mask_hf.enr_transparent < tuning->mask_hf.enr_suppress))
      return false;
  }
  return true;
}
}  // namespace
bool audio_processor_effects_admitted(const NativeEnvironment& environment, const FfiAudioProcessingConfig& selected) noexcept {
  const auto env = environment.environment();
  const auto& trials = env.field_trials();
  if (selected.echo_cancellation && !SafeAecTrials(trials)) return false;
  if (selected.gain_control == 1 &&
      trials.IsEnabled("WebRTC-Audio-2ndAgcMinMicLevelExperiment")) {
    const auto group = trials.Lookup("WebRTC-Audio-2ndAgcMinMicLevelExperiment");
    if (group.starts_with("Enabled-") &&
        !NumericRepresentable(group.substr(8), NumericKind::Int32))
      return false;
  }
  return true;
}
std::int32_t audio_processor_initialize(NativeAudioProcessor& p) noexcept { return p.state().module->Initialize(); }
void audio_processor_configure(NativeAudioProcessor& p, const FfiAudioProcessingConfig& selected) noexcept {
  auto config = p.state().module->GetConfig();
  SelectEffects(config, selected);
  p.state().module->ApplyConfig(config);
}
FfiAudioProcessingFormatState audio_processor_format(const NativeAudioProcessor& p) noexcept {
  const auto& m = p.state().module;
  return {m->proc_sample_rate_hz(), m->proc_split_sample_rate_hz(), m->num_input_channels(), m->num_proc_channels(), m->num_output_channels(), m->num_reverse_channels()};
}
void audio_processor_set_muted(NativeAudioProcessor& p, bool v) noexcept { p.state().module->set_output_will_be_muted(v); }
bool audio_processor_muted(NativeAudioProcessor& p) noexcept { return p.state().module->get_output_will_be_muted(); }
std::int32_t audio_processor_set_delay(NativeAudioProcessor& p, std::int32_t v) noexcept { return p.state().module->set_stream_delay_ms(v); }
std::int32_t audio_processor_delay(const NativeAudioProcessor& p) noexcept { return p.state().module->stream_delay_ms(); }
void audio_processor_set_analog_level(NativeAudioProcessor& p, std::int32_t v) noexcept { p.state().module->set_stream_analog_level(v); }
std::int32_t audio_processor_analog_level(const NativeAudioProcessor& p) noexcept { return p.state().module->recommended_stream_analog_level(); }
void audio_processor_set_key(NativeAudioProcessor& p, bool v) noexcept { p.state().module->set_stream_key_pressed(v); }
std::int32_t audio_processor_i16(NativeAudioProcessor& p, std::int32_t rate, std::size_t channels, rust::Slice<std::int16_t> samples, bool reverse) noexcept {
  const webrtc::StreamConfig config(rate, channels);
  // Separate input also avoids memcpy overlap in native error-copy branches.
  std::vector<std::int16_t> input(samples.begin(), samples.begin() + config.num_samples());
  std::int16_t empty_input = 0, empty_output = 0;
  const auto* src = input.empty() ? &empty_input : input.data();
  auto* dest = samples.empty() ? &empty_output : samples.data();
  return reverse ? p.state().module->ProcessReverseStream(src, config, config, dest) : p.state().module->ProcessStream(src, config, config, dest);
}
std::int32_t audio_processor_float(NativeAudioProcessor& p, std::int32_t input_rate, std::size_t input_channels, rust::Slice<const float> samples, std::int32_t output_rate, std::size_t output_channels, rust::Slice<float> destination, bool reverse) noexcept {
  const webrtc::StreamConfig input(input_rate, input_channels), output(output_rate, output_channels);
  float empty_input = 0, empty_output = 0;
  auto src = InputPointers(samples, input, &empty_input);
  auto dest = OutputPointers(destination, output, &empty_output);
  return reverse ? p.state().module->ProcessReverseStream(src.data(), input, output, dest.data()) : p.state().module->ProcessStream(src.data(), input, output, dest.data());
}
std::int32_t audio_processor_analyze(NativeAudioProcessor& p, std::int32_t rate, std::size_t channels, rust::Slice<const float> samples) noexcept {
  const webrtc::StreamConfig input(rate, channels);
  float empty = 0;
  auto src = InputPointers(samples, input, &empty);
  return p.state().module->AnalyzeReverseStream(src.data(), input);
}
}
