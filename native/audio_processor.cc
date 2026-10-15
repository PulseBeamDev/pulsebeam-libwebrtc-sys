#include "pulsebeam-webrtc-sys/native/audio_processor.h"
#include <algorithm>
#include <array>
#include <span>
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
#include "common_audio/include/audio_util.h"
#include "rtc_base/checks.h"
#include "pulsebeam-webrtc-sys/native/execution.h"
#include "pulsebeam-webrtc-sys/src/lib.rs.h"
// Pinned internal preflight declaration from aec3/echo_canceller3.h. Importing
// the full internal class header would require its non-exported debug-dump
// layout definitions; only this free function's public value types cross here.
namespace webrtc {
EchoCanceller3Config AdjustConfig(const EchoCanceller3Config&, const FieldTrialsView&);
}
namespace pulsebeam::webrtc_sys {
namespace {
// A bounded safety envelope, not an acknowledgment cache or replacement queue.
// The SDK retains the original payloads. Max magnitude suffices for finite
// linear gain/product admission; NaN/Inf must be remembered independently.
struct PendingGainRange {
  bool any = false;
  bool nonfinite = false;
  float max_absolute = 0;
  void Include(float value) {
    any = true;
    if (!std::isfinite(value)) nonfinite = true;
    else max_absolute = std::max(max_absolute, std::abs(value));
  }
  void Merge(const PendingGainRange& other) {
    any |= other.any;
    nonfinite |= other.nonfinite;
    max_absolute = std::max(max_absolute, other.max_absolute);
  }
};
struct PendingGains {
  PendingGainRange pre, post;
  void Include(const FfiApmRuntimeSetting& f) {
    if (f.kind == 1) pre.Include(f.float_value);
    if (f.kind == 2) post.Include(f.float_value);
  }
  void Merge(const PendingGains& other) { pre.Merge(other.pre); post.Merge(other.post); }
  bool Safe(const webrtc::AudioProcessing::Config& c) const {
    if (pre.any && (c.pre_amplifier.enabled || c.capture_level_adjustment.enabled)) {
      if (pre.nonfinite) return false;
      if (c.pre_amplifier.enabled && c.capture_level_adjustment.enabled &&
          !std::isfinite(pre.max_absolute * std::abs(c.capture_level_adjustment.pre_gain_factor))) return false;
    }
    return !(post.any && c.capture_level_adjustment.enabled && post.nonfinite);
  }
};
}
struct NativeAudioProcessor::State {
  // Destroy the module before its original environment dependencies.
  webrtc::Environment environment;
  webrtc::scoped_refptr<webrtc::AudioProcessing> module;
  bool linear_echo_configured = false;
  bool linear_output_guaranteed = false;
  PendingGains queued_gains;
  PendingGains possibly_queued_gains;
  webrtc::ProcessingConfig format_hint = webrtc::ProcessingConfig({{{16000, 1}, {16000, 1}, {16000, 1}, {16000, 1}}});
  bool capture_format_known = true;
  bool render_format_known = true;
  bool capture_graph_dirty = false;
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
  if (config.gain_controller1.enabled) {
    config.gain_controller1.mode = static_cast<webrtc::AudioProcessing::Config::GainController1::Mode>(selected.gain_control - 1);
    // The pin selects the analog manager by this flag, not by Mode.
    config.gain_controller1.analog_gain_controller.enabled = selected.gain_control == 1;
  }
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
namespace {
webrtc::AudioProcessing::Config ConfigFromFfi(const FfiApmConfig& v) {
  webrtc::AudioProcessing::Config config;
  config.pipeline.maximum_internal_processing_rate = v.pipeline_maximum_internal_processing_rate;
  config.pipeline.multi_channel_render = v.pipeline_multi_channel_render;
  config.pipeline.multi_channel_capture = v.pipeline_multi_channel_capture;
  config.pipeline.capture_downmix_method = static_cast<webrtc::AudioProcessing::Config::Pipeline::DownmixMethod>(v.pipeline_capture_downmix_method);
  config.pipeline.capture_downmix_method_stereo_aec = static_cast<webrtc::AudioProcessing::Config::Pipeline::DownmixMethod>(v.pipeline_capture_downmix_method_stereo_aec);
  config.pre_amplifier.enabled = v.pre_amplifier_enabled;
  config.pre_amplifier.fixed_gain_factor = v.pre_amplifier_fixed_gain_factor;
  config.capture_level_adjustment.enabled = v.capture_level_adjustment_enabled;
  config.capture_level_adjustment.pre_gain_factor = v.capture_level_adjustment_pre_gain_factor;
  config.capture_level_adjustment.post_gain_factor = v.capture_level_adjustment_post_gain_factor;
  config.capture_level_adjustment.analog_mic_gain_emulation.enabled = v.capture_level_adjustment_analog_mic_gain_emulation_enabled;
  config.capture_level_adjustment.analog_mic_gain_emulation.initial_level = v.capture_level_adjustment_analog_mic_gain_emulation_initial_level;
  config.high_pass_filter.enabled = v.high_pass_filter_enabled;
  config.high_pass_filter.apply_in_full_band = v.high_pass_filter_apply_in_full_band;
  config.echo_canceller.enabled = v.echo_canceller_enabled;
  config.echo_canceller.export_linear_aec_output = v.echo_canceller_export_linear_aec_output;
  config.echo_canceller.enforce_high_pass_filtering = v.echo_canceller_enforce_high_pass_filtering;
  config.noise_suppression.enabled = v.noise_suppression_enabled;
  config.noise_suppression.level = static_cast<webrtc::AudioProcessing::Config::NoiseSuppression::Level>(v.noise_suppression_level);
  config.noise_suppression.analyze_linear_aec_output_when_available = v.noise_suppression_analyze_linear_aec_output_when_available;
  config.transient_suppression.enabled = v.transient_suppression_enabled;
  config.gain_controller1.enabled = v.gain_controller1_enabled;
  config.gain_controller1.mode = static_cast<webrtc::AudioProcessing::Config::GainController1::Mode>(v.gain_controller1_mode);
  config.gain_controller1.target_level_dbfs = v.gain_controller1_target_level_dbfs;
  config.gain_controller1.compression_gain_db = v.gain_controller1_compression_gain_db;
  config.gain_controller1.enable_limiter = v.gain_controller1_enable_limiter;
  config.gain_controller1.analog_gain_controller.enabled = v.gain_controller1_analog_gain_controller_enabled;
  config.gain_controller1.analog_gain_controller.startup_min_volume = v.gain_controller1_analog_gain_controller_startup_min_volume;
  config.gain_controller1.analog_gain_controller.clipped_level_min = v.gain_controller1_analog_gain_controller_clipped_level_min;
  config.gain_controller1.analog_gain_controller.enable_digital_adaptive = v.gain_controller1_analog_gain_controller_enable_digital_adaptive;
  config.gain_controller1.analog_gain_controller.clipped_level_step = v.gain_controller1_analog_gain_controller_clipped_level_step;
  config.gain_controller1.analog_gain_controller.clipped_ratio_threshold = v.gain_controller1_analog_gain_controller_clipped_ratio_threshold;
  config.gain_controller1.analog_gain_controller.clipped_wait_frames = v.gain_controller1_analog_gain_controller_clipped_wait_frames;
  config.gain_controller1.analog_gain_controller.clipping_predictor.enabled = v.gain_controller1_analog_gain_controller_clipping_predictor_enabled;
  config.gain_controller1.analog_gain_controller.clipping_predictor.mode = static_cast<webrtc::AudioProcessing::Config::GainController1::AnalogGainController::ClippingPredictor::Mode>(v.gain_controller1_analog_gain_controller_clipping_predictor_mode);
  config.gain_controller1.analog_gain_controller.clipping_predictor.window_length = v.gain_controller1_analog_gain_controller_clipping_predictor_window_length;
  config.gain_controller1.analog_gain_controller.clipping_predictor.reference_window_length = v.gain_controller1_analog_gain_controller_clipping_predictor_reference_window_length;
  config.gain_controller1.analog_gain_controller.clipping_predictor.reference_window_delay = v.gain_controller1_analog_gain_controller_clipping_predictor_reference_window_delay;
  config.gain_controller1.analog_gain_controller.clipping_predictor.clipping_threshold = v.gain_controller1_analog_gain_controller_clipping_predictor_clipping_threshold;
  config.gain_controller1.analog_gain_controller.clipping_predictor.crest_factor_margin = v.gain_controller1_analog_gain_controller_clipping_predictor_crest_factor_margin;
  config.gain_controller1.analog_gain_controller.clipping_predictor.use_predicted_step = v.gain_controller1_analog_gain_controller_clipping_predictor_use_predicted_step;
  config.gain_controller2.enabled = v.gain_controller2_enabled;
  config.gain_controller2.input_volume_controller.enabled = v.gain_controller2_input_volume_controller_enabled;
  config.gain_controller2.adaptive_digital.enabled = v.gain_controller2_adaptive_digital_enabled;
  config.gain_controller2.adaptive_digital.headroom_db = v.gain_controller2_adaptive_digital_headroom_db;
  config.gain_controller2.adaptive_digital.max_gain_db = v.gain_controller2_adaptive_digital_max_gain_db;
  config.gain_controller2.adaptive_digital.initial_gain_db = v.gain_controller2_adaptive_digital_initial_gain_db;
  config.gain_controller2.adaptive_digital.max_gain_change_db_per_second = v.gain_controller2_adaptive_digital_max_gain_change_db_per_second;
  config.gain_controller2.adaptive_digital.max_output_noise_level_dbfs = v.gain_controller2_adaptive_digital_max_output_noise_level_dbfs;
  config.gain_controller2.fixed_digital.gain_db = v.gain_controller2_fixed_digital_gain_db;
  return config;
}
FfiApmConfig ConfigToFfi(const webrtc::AudioProcessing::Config& config) {
  FfiApmConfig v;
  v.pipeline_maximum_internal_processing_rate = config.pipeline.maximum_internal_processing_rate;
  v.pipeline_multi_channel_render = config.pipeline.multi_channel_render;
  v.pipeline_multi_channel_capture = config.pipeline.multi_channel_capture;
  v.pipeline_capture_downmix_method = static_cast<std::int32_t>(config.pipeline.capture_downmix_method);
  v.pipeline_capture_downmix_method_stereo_aec = static_cast<std::int32_t>(config.pipeline.capture_downmix_method_stereo_aec);
  v.pre_amplifier_enabled = config.pre_amplifier.enabled;
  v.pre_amplifier_fixed_gain_factor = config.pre_amplifier.fixed_gain_factor;
  v.capture_level_adjustment_enabled = config.capture_level_adjustment.enabled;
  v.capture_level_adjustment_pre_gain_factor = config.capture_level_adjustment.pre_gain_factor;
  v.capture_level_adjustment_post_gain_factor = config.capture_level_adjustment.post_gain_factor;
  v.capture_level_adjustment_analog_mic_gain_emulation_enabled = config.capture_level_adjustment.analog_mic_gain_emulation.enabled;
  v.capture_level_adjustment_analog_mic_gain_emulation_initial_level = config.capture_level_adjustment.analog_mic_gain_emulation.initial_level;
  v.high_pass_filter_enabled = config.high_pass_filter.enabled;
  v.high_pass_filter_apply_in_full_band = config.high_pass_filter.apply_in_full_band;
  v.echo_canceller_enabled = config.echo_canceller.enabled;
  v.echo_canceller_export_linear_aec_output = config.echo_canceller.export_linear_aec_output;
  v.echo_canceller_enforce_high_pass_filtering = config.echo_canceller.enforce_high_pass_filtering;
  v.noise_suppression_enabled = config.noise_suppression.enabled;
  v.noise_suppression_level = static_cast<std::int32_t>(config.noise_suppression.level);
  v.noise_suppression_analyze_linear_aec_output_when_available = config.noise_suppression.analyze_linear_aec_output_when_available;
  v.transient_suppression_enabled = config.transient_suppression.enabled;
  v.gain_controller1_enabled = config.gain_controller1.enabled;
  v.gain_controller1_mode = static_cast<std::int32_t>(config.gain_controller1.mode);
  v.gain_controller1_target_level_dbfs = config.gain_controller1.target_level_dbfs;
  v.gain_controller1_compression_gain_db = config.gain_controller1.compression_gain_db;
  v.gain_controller1_enable_limiter = config.gain_controller1.enable_limiter;
  v.gain_controller1_analog_gain_controller_enabled = config.gain_controller1.analog_gain_controller.enabled;
  v.gain_controller1_analog_gain_controller_startup_min_volume = config.gain_controller1.analog_gain_controller.startup_min_volume;
  v.gain_controller1_analog_gain_controller_clipped_level_min = config.gain_controller1.analog_gain_controller.clipped_level_min;
  v.gain_controller1_analog_gain_controller_enable_digital_adaptive = config.gain_controller1.analog_gain_controller.enable_digital_adaptive;
  v.gain_controller1_analog_gain_controller_clipped_level_step = config.gain_controller1.analog_gain_controller.clipped_level_step;
  v.gain_controller1_analog_gain_controller_clipped_ratio_threshold = config.gain_controller1.analog_gain_controller.clipped_ratio_threshold;
  v.gain_controller1_analog_gain_controller_clipped_wait_frames = config.gain_controller1.analog_gain_controller.clipped_wait_frames;
  v.gain_controller1_analog_gain_controller_clipping_predictor_enabled = config.gain_controller1.analog_gain_controller.clipping_predictor.enabled;
  v.gain_controller1_analog_gain_controller_clipping_predictor_mode = static_cast<std::int32_t>(config.gain_controller1.analog_gain_controller.clipping_predictor.mode);
  v.gain_controller1_analog_gain_controller_clipping_predictor_window_length = config.gain_controller1.analog_gain_controller.clipping_predictor.window_length;
  v.gain_controller1_analog_gain_controller_clipping_predictor_reference_window_length = config.gain_controller1.analog_gain_controller.clipping_predictor.reference_window_length;
  v.gain_controller1_analog_gain_controller_clipping_predictor_reference_window_delay = config.gain_controller1.analog_gain_controller.clipping_predictor.reference_window_delay;
  v.gain_controller1_analog_gain_controller_clipping_predictor_clipping_threshold = config.gain_controller1.analog_gain_controller.clipping_predictor.clipping_threshold;
  v.gain_controller1_analog_gain_controller_clipping_predictor_crest_factor_margin = config.gain_controller1.analog_gain_controller.clipping_predictor.crest_factor_margin;
  v.gain_controller1_analog_gain_controller_clipping_predictor_use_predicted_step = config.gain_controller1.analog_gain_controller.clipping_predictor.use_predicted_step;
  v.gain_controller2_enabled = config.gain_controller2.enabled;
  v.gain_controller2_input_volume_controller_enabled = config.gain_controller2.input_volume_controller.enabled;
  v.gain_controller2_adaptive_digital_enabled = config.gain_controller2.adaptive_digital.enabled;
  v.gain_controller2_adaptive_digital_headroom_db = config.gain_controller2.adaptive_digital.headroom_db;
  v.gain_controller2_adaptive_digital_max_gain_db = config.gain_controller2.adaptive_digital.max_gain_db;
  v.gain_controller2_adaptive_digital_initial_gain_db = config.gain_controller2.adaptive_digital.initial_gain_db;
  v.gain_controller2_adaptive_digital_max_gain_change_db_per_second = config.gain_controller2.adaptive_digital.max_gain_change_db_per_second;
  v.gain_controller2_adaptive_digital_max_output_noise_level_dbfs = config.gain_controller2.adaptive_digital.max_output_noise_level_dbfs;
  v.gain_controller2_fixed_digital_gain_db = config.gain_controller2.fixed_digital.gain_db;
  return v;
}
// This is the pin's Validate predicate. Build does not run ApplyConfig's
// fallback; ApplyConfig does, even for a disabled GC2. Keep that distinction.
bool ValidGc2(const webrtc::AudioProcessing::Config::GainController2& g) {
  const auto& a = g.adaptive_digital;
  return g.fixed_digital.gain_db >= 0 && g.fixed_digital.gain_db < 50 &&
      a.headroom_db >= 0 && a.max_gain_db > 0 && a.initial_gain_db >= 0 &&
      a.max_gain_change_db_per_second > 0 && a.max_output_noise_level_dbfs <= 0;
}
bool RuntimeReinitializationUnsafe(const NativeAudioProcessor& p) {
#if RTC_DCHECK_IS_ON
  const auto c = p.state().module->GetConfig();
  return c.gain_controller2.enabled && !ValidGc2(c.gain_controller2);
#else
  return false;
#endif
}
bool FormatsReachInitialization(const webrtc::StreamConfig& input, const webrtc::StreamConfig& output) {
  const auto valid = [](const webrtc::StreamConfig& s) {
    return s.sample_rate_hz() >= 8000 && s.sample_rate_hz() <= 384000 && s.num_channels() > 0;
  };
  return valid(input) && valid(output) && (output.num_channels() == 1 || output.num_channels() == input.num_channels());
}
bool ImplicitInitializationUnsafe(const NativeAudioProcessor& p, const webrtc::StreamConfig& input, const webrtc::StreamConfig& output, bool reverse) {
  if (!RuntimeReinitializationUnsafe(p) || !FormatsReachInitialization(input, output)) return false;
  const auto& s = p.state();
  if (reverse) return !s.render_format_known || input != s.format_hint.reverse_input_stream() || output != s.format_hint.reverse_output_stream();
  return !s.capture_format_known || s.capture_graph_dirty || input != s.format_hint.input_stream() || output != s.format_hint.output_stream();
}
void NoteProcessingFormats(NativeAudioProcessor& p, const webrtc::StreamConfig& input, const webrtc::StreamConfig& output, bool reverse, int status) {
  if (reverse) {
    p.state().render_format_known = status == 0;
    if (status == 0) { p.state().format_hint.reverse_input_stream() = input; p.state().format_hint.reverse_output_stream() = output; }
  } else {
    p.state().capture_format_known = status == 0;
    if (status == 0) { p.state().format_hint.input_stream() = input; p.state().format_hint.output_stream() = output; }
  }
}
std::uint8_t AdmitPending(const NativeAudioProcessor& p, const webrtc::AudioProcessing::Config& c) {
  if (!p.state().queued_gains.Safe(c)) return 9;
  if (!p.state().possibly_queued_gains.Safe(c)) return 10;
  return 0;
}
void NoteCaptureResult(NativeAudioProcessor& p, int status) {
  if (status == webrtc::AudioProcessing::kNoError) {
    // Both pinned capture overloads finish the entire capture queue drain.
    p.state().queued_gains = {};
    p.state().possibly_queued_gains = {};
    p.state().capture_graph_dirty = false;
  } else {
    // A native error may precede or follow the drain. Do not invent an ack.
    p.state().possibly_queued_gains.Merge(p.state().queued_gains);
    p.state().queued_gains = {};
  }
}
std::uint8_t AdmitRuntime(const webrtc::AudioProcessing::Config& c, const FfiApmRuntimeSetting& f) {
  if (f.kind > 8) return 11;
#if RTC_DCHECK_IS_ON
  if (f.kind == 0) return 11;
#endif
  // These factories declare their domain before any deferred consumption.
  if (f.kind == 3 && (f.int_value < 0 || f.int_value > 90)) return 11;
  if (f.kind == 4 && !(f.float_value >= 0 && f.float_value <= 90)) return 11;
  PendingGains proposed;
  proposed.Include(f);
  return proposed.Safe(c) ? 0 : 11;
}
webrtc::AudioProcessing::RuntimeSetting RuntimeFromFfi(const FfiApmRuntimeSetting& f) {
  using Setting = webrtc::AudioProcessing::RuntimeSetting;
  switch (f.kind) {
    case 1: return Setting::CreateCapturePreGain(f.float_value);
    case 2: return Setting::CreateCapturePostGain(f.float_value);
    case 3: return Setting::CreateCompressionGainDb(f.int_value);
    case 4: return Setting::CreateCaptureFixedPostGain(f.float_value);
    case 5: return Setting::CreatePlayoutVolumeChange(f.int_value);
    case 6: return Setting::CreateCustomRenderSetting(f.float_value);
    case 7: return Setting::CreatePlayoutAudioDeviceChange({f.device_id, f.device_max_volume});
    case 8: return Setting::CreateCaptureOutputUsedSetting(f.bool_value);
    default: return Setting();
  }
}
std::uint8_t AdmitConfig(const webrtc::Environment& env, const webrtc::AudioProcessing::Config& c, bool build, bool linear_echo_configured = false) {
  if (c.echo_canceller.enabled && c.echo_canceller.export_linear_aec_output && !linear_echo_configured) return 7;
#if RTC_DCHECK_IS_ON
  if (c.pipeline.maximum_internal_processing_rate != 32000 &&
      c.pipeline.maximum_internal_processing_rate != 48000) return 3;
#endif
  const auto& g = c.gain_controller1;
  if (g.enabled) {
    if (!g.analog_gain_controller.enabled) {
      if (g.target_level_dbfs < 0 || g.target_level_dbfs > 31 ||
          g.compression_gain_db < 0 || g.compression_gain_db > 90) return 3;
    } else {
      const auto& a = g.analog_gain_controller;
      if (a.clipped_level_min < 0 || a.clipped_level_min > 255 ||
          a.clipped_level_step < 1 || a.clipped_level_step > 255 ||
          !(a.clipped_ratio_threshold > 0 && a.clipped_ratio_threshold < 1) ||
          a.clipped_wait_frames < 1) return 3;
      const auto& p = a.clipping_predictor;
      if (p.enabled) {
        const std::int64_t capacity = std::int64_t(p.reference_window_length) + p.reference_window_delay;
        if (p.window_length < 1 || p.reference_window_length < 1 || p.reference_window_delay < 0 ||
            capacity > std::numeric_limits<int>::max() || p.window_length >= capacity) return 3;
        // Event mode compares floats. Peak modes narrow the same field to int.
        if (p.mode != webrtc::AudioProcessing::Config::GainController1::AnalogGainController::ClippingPredictor::kClippingEventPrediction &&
            !(std::isfinite(p.clipping_threshold) && p.clipping_threshold >= -2147483648.0f && p.clipping_threshold < 2147483648.0f)) return 3;
      }
    }
  }
  if (c.pre_amplifier.enabled || c.capture_level_adjustment.enabled) {
    float pre = c.pre_amplifier.enabled ? c.pre_amplifier.fixed_gain_factor : 1.f;
    const float adjustment = c.capture_level_adjustment.enabled ? c.capture_level_adjustment.pre_gain_factor : 1.f;
    const double product = double(pre) * adjustment;
    if (!std::isfinite(pre) || !std::isfinite(adjustment) || !std::isfinite(product) ||
        std::abs(product) > std::numeric_limits<float>::max() ||
        !std::isfinite(c.capture_level_adjustment.post_gain_factor)) return 3;
    const auto& mic = c.capture_level_adjustment.analog_mic_gain_emulation;
    if (mic.enabled && (mic.initial_level < 0 || mic.initial_level > 255)) return 3;
  }
  const auto& g2 = c.gain_controller2;
  if (g2.enabled && (build || ValidGc2(g2))) {
    if (!ValidGc2(g2)) return 3;
    if (g2.adaptive_digital.enabled) {
      const auto& a = g2.adaptive_digital;
#if RTC_DCHECK_IS_ON
      if (a.max_output_noise_level_dbfs < -90 ||
          !(a.max_gain_change_db_per_second * 10.f / 1000.f > 0)) return 3;
#endif
      // Actual steady target is noise-limited, independently of max_gain_db.
      // A finite powf alone cannot certify an extreme initial gain: preceding
      // DSP is not hard-clipped to the input PCM range. Wider initial settings
      // remain an explicit admission gap, not native capability absence.
      const float max_target_db = -webrtc::FloatS16ToDbfs(0.f);
      if (!std::isfinite(webrtc::DbToRatio(a.initial_gain_db))) return 3;
      if (a.initial_gain_db > max_target_db) return 4;
    }
  }
  const auto& trials = env.field_trials();
  if (c.echo_canceller.enabled && !SafeAecTrials(trials)) return 1;
  if (g.enabled && g.analog_gain_controller.enabled &&
      trials.IsEnabled("WebRTC-Audio-2ndAgcMinMicLevelExperiment")) {
    const auto group = trials.Lookup("WebRTC-Audio-2ndAgcMinMicLevelExperiment");
    if (group.starts_with("Enabled-") && !NumericRepresentable(group.substr(8), NumericKind::Int32)) return 1;
  }
  return 0;
}
}
std::unique_ptr<NativeAudioProcessor> audio_processor_new_config(const NativeEnvironment& environment, const FfiApmConfig& f, bool linear_echo, std::uint8_t& error) noexcept {
  const auto env = environment.environment();
  const auto config = ConfigFromFfi(f);
  error = AdmitConfig(env, config, true, linear_echo);
  if (error) return nullptr;
  auto state = std::make_unique<NativeAudioProcessor::State>(NativeAudioProcessor::State{env, nullptr, linear_echo, config.echo_canceller.enabled && config.echo_canceller.export_linear_aec_output});
  webrtc::BuiltinAudioProcessingBuilder builder(config);
  if (linear_echo) {
    webrtc::EchoCanceller3Config mono;
    auto multi = webrtc::EchoCanceller3Config::CreateDefaultMultichannelConfig();
    mono.filter.export_linear_aec_output = true;
    multi.filter.export_linear_aec_output = true;
    builder.SetEchoCancellerConfig(mono, multi);
  }
  state->module = builder.Build(state->environment);
  if (!state->module) return nullptr;
  return std::make_unique<NativeAudioProcessor>(std::move(state));
}
std::uint8_t audio_processor_config_apply(NativeAudioProcessor& p, const FfiApmConfig& f) noexcept {
  const auto config = ConfigFromFfi(f);
  if (config.echo_canceller.enabled && p.state().module->num_reverse_channels() == 0) return 2;
  if (const auto error = AdmitPending(p, config)) return error;
  if (const auto error = AdmitConfig(p.state().environment, config, false, p.state().linear_echo_configured)) return error;
  const auto previous = p.state().module->GetConfig();
  p.state().module->ApplyConfig(config);
  p.state().capture_graph_dirty = true;
  if (!config.echo_canceller.enabled || !config.echo_canceller.export_linear_aec_output)
    p.state().linear_output_guaranteed = false;
  else if (!previous.echo_canceller.enabled)
    p.state().linear_output_guaranteed = true;
  return 0;
}
bool audio_processor_runtime_setting(NativeAudioProcessor& p, const FfiApmRuntimeSetting& f, bool post, std::uint8_t& error) noexcept {
  error = AdmitRuntime(p.state().module->GetConfig(), f);
  if (error) return false;
  const auto setting = RuntimeFromFfi(f);
  if (post) {
    const bool accepted = p.state().module->PostRuntimeSetting(setting);
    if (accepted && f.kind == 1) {
      p.state().queued_gains.pre = {};
      p.state().queued_gains.pre.Include(f.float_value);
      p.state().possibly_queued_gains.pre = {};
    }
    if (accepted && f.kind == 2) {
      p.state().queued_gains.post = {};
      p.state().queued_gains.post.Include(f.float_value);
      p.state().possibly_queued_gains.post = {};
    }
    return accepted;
  }
  p.state().module->SetRuntimeSetting(setting);
  // Void Set discards the native queue acknowledgment, including full-queue
  // failure. Preserve possible payloads without claiming enqueue success.
  p.state().possibly_queued_gains.Include(f);
  return false;  // No acknowledgment; the Rust void-set path ignores this.
}
bool audio_processor_config_equal(const FfiApmConfig& left, const FfiApmConfig& right, std::uint8_t group) noexcept {
  const auto a = ConfigFromFfi(left), b = ConfigFromFfi(right);
  switch (group) {
    case 0: return a.capture_level_adjustment == b.capture_level_adjustment;
    case 1: return a.capture_level_adjustment.analog_mic_gain_emulation == b.capture_level_adjustment.analog_mic_gain_emulation;
    case 2: return a.gain_controller1 == b.gain_controller1;
    case 3: return a.gain_controller2 == b.gain_controller2;
    case 4: return a.gain_controller2.input_volume_controller == b.gain_controller2.input_volume_controller;
    case 5: return a.gain_controller2.adaptive_digital == b.gain_controller2.adaptive_digital;
    default: return false;
  }
}
namespace {
FfiApmStatistics StatisticsToFfi(const webrtc::AudioProcessingStats& s) {
  FfiApmStatistics result{};
  if (s.voice_detected) { result.present |= std::uint16_t(1u << 0); result.voice_detected = *s.voice_detected; }
  if (s.echo_return_loss) { result.present |= std::uint16_t(1u << 1); result.echo_return_loss = *s.echo_return_loss; }
  if (s.echo_return_loss_enhancement) { result.present |= std::uint16_t(1u << 2); result.echo_return_loss_enhancement = *s.echo_return_loss_enhancement; }
  if (s.divergent_filter_fraction) { result.present |= std::uint16_t(1u << 3); result.divergent_filter_fraction = *s.divergent_filter_fraction; }
  if (s.delay_median_ms) { result.present |= std::uint16_t(1u << 4); result.delay_median_ms = *s.delay_median_ms; }
  if (s.delay_standard_deviation_ms) { result.present |= std::uint16_t(1u << 5); result.delay_standard_deviation_ms = *s.delay_standard_deviation_ms; }
  if (s.residual_echo_likelihood) { result.present |= std::uint16_t(1u << 6); result.residual_echo_likelihood = *s.residual_echo_likelihood; }
  if (s.residual_echo_likelihood_recent_max) { result.present |= std::uint16_t(1u << 7); result.residual_echo_likelihood_recent_max = *s.residual_echo_likelihood_recent_max; }
  if (s.delay_ms) { result.present |= std::uint16_t(1u << 8); result.delay_ms = *s.delay_ms; }
  return result;
}
}
bool audio_processor_linear_output(NativeAudioProcessor& p, rust::Slice<float> output, std::uint8_t& error) noexcept {
  error = 0;
#if RTC_DCHECK_IS_ON
  if (!p.state().linear_output_guaranteed) { error = 8; return false; }
#endif
  const auto channels = p.state().module->num_proc_channels();
  // Structural conversion only: uninitialized native array rows avoid aliasing
  // Rust arrays as C++ std::array subobjects. Copy solely after the SDK confirms
  // availability; its successful path writes every sample of every row.
  auto rows = std::unique_ptr<std::array<float, 160>[]>(new std::array<float, 160>[channels]);
  if (!p.state().module->GetLinearAecOutput(std::span<std::array<float, 160>>(rows.get(), channels))) return false;
  if (output.size() / 160 < channels) { error = 6; return false; }
  for (std::size_t ch = 0; ch < channels; ++ch)
    std::copy(rows[ch].begin(), rows[ch].end(), output.data() + ch * 160);
  return true;
}
FfiApmStatistics audio_processor_statistics_default() noexcept { return StatisticsToFfi(webrtc::AudioProcessingStats{}); }
FfiApmStatistics audio_processor_statistics(NativeAudioProcessor& p, bool has_argument, bool remote) noexcept {
  return StatisticsToFfi(has_argument ? p.state().module->GetStatistics(remote) : p.state().module->GetStatistics());
}
FfiApmConfig audio_processor_config_default() noexcept { return ConfigToFfi(webrtc::AudioProcessing::Config{}); }
rust::String audio_processor_config_string(const FfiApmConfig& v) noexcept { return ConfigFromFfi(v).ToString(); }
FfiApmConfig audio_processor_config_get(const NativeAudioProcessor& p) noexcept { return ConfigToFfi(p.state().module->GetConfig()); }
std::int32_t audio_processor_initialize(NativeAudioProcessor& p, std::uint8_t& error) noexcept {
  error = RuntimeReinitializationUnsafe(p) ? 12 : 0;
  if (error) return 0;
  const auto status = p.state().module->Initialize();
  if (status == 0) { const auto c = p.state().module->GetConfig(); p.state().linear_output_guaranteed = c.echo_canceller.enabled && c.echo_canceller.export_linear_aec_output; }
  return status;
}
FfiApmFormats audio_processor_formats_default() noexcept {
  const webrtc::ProcessingConfig config;
  return {config.input_stream().sample_rate_hz(), config.input_stream().num_channels(), config.output_stream().sample_rate_hz(), config.output_stream().num_channels(), config.reverse_input_stream().sample_rate_hz(), config.reverse_input_stream().num_channels(), config.reverse_output_stream().sample_rate_hz(), config.reverse_output_stream().num_channels()};
}
std::int32_t audio_processor_initialize_formats(NativeAudioProcessor& p, const FfiApmFormats& f, bool& admitted, std::uint8_t& error) noexcept {
  admitted = false;
  error = 0;
  const auto valid = [](std::int32_t rate, std::size_t channels) {
    return rate >= 8000 && rate <= 384000 && channels > 0 && channels <= std::numeric_limits<std::int32_t>::max();
  };
  if (!valid(f.input_rate, f.input_channels) || !valid(f.output_rate, f.output_channels) ||
      (f.output_channels != 1 && f.output_channels != f.input_channels)) return 0;
  if (f.reverse_input_channels == 0 && f.reverse_output_channels == 0) {
    // Absent render is allowed only when no selected echo controller needs it.
    if (p.state().module->GetConfig().echo_canceller.enabled) return 0;
  } else if (!valid(f.reverse_input_rate, f.reverse_input_channels) ||
             !valid(f.reverse_output_rate, f.reverse_output_channels) ||
             (f.reverse_input_channels != f.reverse_output_channels &&
              f.reverse_input_channels != 1 && f.reverse_output_channels != 1)) {
    return 0;
  }
  webrtc::ProcessingConfig config;
  config.input_stream() = webrtc::StreamConfig(f.input_rate, f.input_channels);
  config.output_stream() = webrtc::StreamConfig(f.output_rate, f.output_channels);
  config.reverse_input_stream() = webrtc::StreamConfig(f.reverse_input_rate, f.reverse_input_channels);
  config.reverse_output_stream() = webrtc::StreamConfig(f.reverse_output_rate, f.reverse_output_channels);
  error = RuntimeReinitializationUnsafe(p) ? 12 : 0;
  if (error) return 0;
  admitted = true;
  const auto status = p.state().module->Initialize(config);
  if (status == 0) {
    p.state().format_hint = config;
    p.state().capture_format_known = p.state().render_format_known = true;
    p.state().capture_graph_dirty = false;
  }
  if (status == 0) { const auto c = p.state().module->GetConfig(); p.state().linear_output_guaranteed = c.echo_canceller.enabled && c.echo_canceller.export_linear_aec_output; }
  return status;
}
std::uint8_t audio_processor_configure(NativeAudioProcessor& p, const FfiAudioProcessingConfig& selected) noexcept {
  if (selected.echo_cancellation && p.state().module->num_reverse_channels() == 0) return 2;
  auto config = p.state().module->GetConfig();
  SelectEffects(config, selected);
  if (const auto error = AdmitPending(p, config)) return error;
  if (const auto error = AdmitConfig(p.state().environment, config, false, p.state().linear_echo_configured)) return error;
  const auto previous = p.state().module->GetConfig();
  p.state().module->ApplyConfig(config);
  p.state().capture_graph_dirty = true;
  if (!config.echo_canceller.enabled || !config.echo_canceller.export_linear_aec_output)
    p.state().linear_output_guaranteed = false;
  else if (!previous.echo_canceller.enabled)
    p.state().linear_output_guaranteed = true;
  return 0;
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
std::int32_t audio_processor_i16(NativeAudioProcessor& p, std::int32_t rate, std::size_t channels, rust::Slice<std::int16_t> samples, bool reverse, std::uint8_t& error) noexcept {
  const webrtc::StreamConfig config(rate, channels);
  error = ImplicitInitializationUnsafe(p, config, config, reverse) ? 12 : 0;
  if (error) return 0;
  // Separate input also avoids memcpy overlap in native error-copy branches.
  std::vector<std::int16_t> input(samples.begin(), samples.begin() + config.num_samples());
  std::int16_t empty_input = 0, empty_output = 0;
  const auto* src = input.empty() ? &empty_input : input.data();
  auto* dest = samples.empty() ? &empty_output : samples.data();
  const auto status = reverse ? p.state().module->ProcessReverseStream(src, config, config, dest) : p.state().module->ProcessStream(src, config, config, dest);
  NoteProcessingFormats(p, config, config, reverse, status);
  if (!reverse) NoteCaptureResult(p, status);
  return status;
}
std::int32_t audio_processor_float(NativeAudioProcessor& p, std::int32_t input_rate, std::size_t input_channels, rust::Slice<const float> samples, std::int32_t output_rate, std::size_t output_channels, rust::Slice<float> destination, bool reverse, std::uint8_t& error) noexcept {
  const webrtc::StreamConfig input(input_rate, input_channels), output(output_rate, output_channels);
  error = ImplicitInitializationUnsafe(p, input, output, reverse) ? 12 : 0;
  if (error) return 0;
  float empty_input = 0, empty_output = 0;
  auto src = InputPointers(samples, input, &empty_input);
  auto dest = OutputPointers(destination, output, &empty_output);
  const auto status = reverse ? p.state().module->ProcessReverseStream(src.data(), input, output, dest.data()) : p.state().module->ProcessStream(src.data(), input, output, dest.data());
  NoteProcessingFormats(p, input, output, reverse, status);
  if (!reverse) NoteCaptureResult(p, status);
  return status;
}
std::int32_t audio_processor_analyze(NativeAudioProcessor& p, std::int32_t rate, std::size_t channels, rust::Slice<const float> samples, std::uint8_t& error) noexcept {
  const webrtc::StreamConfig input(rate, channels);
  error = ImplicitInitializationUnsafe(p, input, input, true) ? 12 : 0;
  if (error) return 0;
  float empty = 0;
  auto src = InputPointers(samples, input, &empty);
  const auto status = p.state().module->AnalyzeReverseStream(src.data(), input);
  NoteProcessingFormats(p, input, input, true, status);
  return status;
}
}
