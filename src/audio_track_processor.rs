use crate::ffi;

/// Exact optional APM observations, including the deprecated voice-detection
/// value. Missing observations are not replaced with zero or false.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioProcessingStats {
    pub voice_detected: Option<bool>,
    pub echo_return_loss: Option<f64>,
    pub echo_return_loss_enhancement: Option<f64>,
    pub divergent_filter_fraction: Option<f64>,
    pub delay_median_ms: Option<i32>,
    pub delay_standard_deviation_ms: Option<i32>,
    pub residual_echo_likelihood: Option<f64>,
    pub residual_echo_likelihood_recent_max: Option<f64>,
    pub delay_ms: Option<i32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioProcessorStatistics {
    pub typing_noise_detected: bool,
    pub apm_statistics: AudioProcessingStats,
}
impl AudioProcessorStatistics {
    fn from_native(value: ffi::FfiAudioProcessorStatistics) -> Self {
        Self {
            typing_noise_detected: value.typing_noise_detected,
            apm_statistics: AudioProcessingStats {
                voice_detected: value.has_voice_detected.then_some(value.voice_detected),
                echo_return_loss: value.has_echo_return_loss.then_some(value.echo_return_loss),
                echo_return_loss_enhancement: value
                    .has_echo_return_loss_enhancement
                    .then_some(value.echo_return_loss_enhancement),
                divergent_filter_fraction: value
                    .has_divergent_filter_fraction
                    .then_some(value.divergent_filter_fraction),
                delay_median_ms: value.has_delay_median_ms.then_some(value.delay_median_ms),
                delay_standard_deviation_ms: value
                    .has_delay_standard_deviation_ms
                    .then_some(value.delay_standard_deviation_ms),
                residual_echo_likelihood: value
                    .has_residual_echo_likelihood
                    .then_some(value.residual_echo_likelihood),
                residual_echo_likelihood_recent_max: value
                    .has_residual_echo_likelihood_recent_max
                    .then_some(value.residual_echo_likelihood_recent_max),
                delay_ms: value.has_delay_ms.then_some(value.delay_ms),
            },
        }
    }
}
impl Default for AudioProcessorStatistics {
    fn default() -> Self {
        Self::from_native(ffi::audio_processor_statistics_default())
    }
}
impl Default for AudioProcessingStats {
    fn default() -> Self {
        AudioProcessorStatistics::default().apm_statistics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_projection_keeps_presence_false_zero_and_float_bits() {
        // This is a DTO projection test, not evidence of an installed processor.
        let mut raw = ffi::audio_processor_statistics_default();
        raw.typing_noise_detected = true;
        raw.has_voice_detected = true;
        raw.voice_detected = false;
        raw.has_echo_return_loss = true;
        raw.echo_return_loss = f64::from_bits(0x7ff8_1234_5678_9abc);
        raw.has_echo_return_loss_enhancement = true;
        raw.echo_return_loss_enhancement = -0.0;
        raw.has_divergent_filter_fraction = true;
        raw.divergent_filter_fraction = f64::INFINITY;
        raw.has_delay_median_ms = true;
        raw.delay_median_ms = i32::MIN;
        raw.has_delay_standard_deviation_ms = true;
        raw.delay_standard_deviation_ms = i32::MAX;
        raw.has_residual_echo_likelihood = true;
        raw.residual_echo_likelihood = 0.0;
        raw.has_residual_echo_likelihood_recent_max = true;
        raw.residual_echo_likelihood_recent_max = -7.25;
        raw.has_delay_ms = true;
        raw.delay_ms = 0;
        let value = AudioProcessorStatistics::from_native(raw);
        assert!(value.typing_noise_detected);
        let value = value.apm_statistics;
        assert_eq!(value.voice_detected, Some(false));
        assert_eq!(
            value.echo_return_loss.unwrap().to_bits(),
            0x7ff8_1234_5678_9abc
        );
        assert_eq!(
            value.echo_return_loss_enhancement.unwrap().to_bits(),
            (-0.0f64).to_bits()
        );
        assert_eq!(value.divergent_filter_fraction, Some(f64::INFINITY));
        assert_eq!(value.delay_median_ms, Some(i32::MIN));
        assert_eq!(value.delay_standard_deviation_ms, Some(i32::MAX));
        assert_eq!(value.residual_echo_likelihood, Some(0.0));
        assert_eq!(value.residual_echo_likelihood_recent_max, Some(-7.25));
        assert_eq!(value.delay_ms, Some(0));
    }
}
