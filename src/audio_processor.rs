//! Owning standalone stock APM operations, separate from peer capture.
use crate::{AudioProcessingConfig, Environment, ffi};
use std::{marker::PhantomData, rc::Rc};

/// Exact native status, including nonfatal warnings and unnamed future codes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioProcessingStatus(pub i32);
impl AudioProcessingStatus {
    pub const OK: Self = Self(0);
    pub const BAD_PARAMETER: Self = Self(-6);
    pub const BAD_SAMPLE_RATE: Self = Self(-7);
    pub const BAD_CHANNEL_COUNT: Self = Self(-9);
    pub const STREAM_PARAMETER_NOT_SET: Self = Self(-11);
    pub const BAD_STREAM_PARAMETER_WARNING: Self = Self(-13);
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioProcessorError {
    CreationFailed,
    InvalidDimensions,
    BufferTooShort,
    InvalidAnalogLevel,
    InvalidFloatInput,
    UnsafeFieldTrial,
    InvalidConfiguration,
    /// Raw SDK value is preserved, but this consuming range is not yet qualified.
    UnqualifiedGainRange,
}
impl std::fmt::Display for AudioProcessorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for AudioProcessorError {}

/// Native StreamConfig. Frame size is floor(rate / 100), not rounded up.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioProcessingStream {
    pub sample_rate_hz: i32,
    pub channels: usize,
}
impl AudioProcessingStream {
    pub fn frame_size(self) -> Result<usize, AudioProcessorError> {
        if self.sample_rate_hz < 0 || self.channels > i32::MAX as usize {
            return Err(AudioProcessorError::InvalidDimensions);
        }
        Ok(self.sample_rate_hz as usize / 100)
    }
    pub fn sample_count(self) -> Result<usize, AudioProcessorError> {
        self.frame_size()?
            .checked_mul(self.channels)
            .ok_or(AudioProcessorError::InvalidDimensions)
    }
}

/// Exact four native stream descriptions. Default preserves the SDK's empty
/// ProcessingConfig; empty capture is not safe to initialize in this pin.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioProcessingFormats {
    pub input: AudioProcessingStream,
    pub output: AudioProcessingStream,
    pub reverse_input: AudioProcessingStream,
    pub reverse_output: AudioProcessingStream,
}
impl Default for AudioProcessingFormats {
    fn default() -> Self {
        let f = ffi::audio_processor_formats_default();
        Self {
            input: AudioProcessingStream {
                sample_rate_hz: f.input_rate,
                channels: f.input_channels,
            },
            output: AudioProcessingStream {
                sample_rate_hz: f.output_rate,
                channels: f.output_channels,
            },
            reverse_input: AudioProcessingStream {
                sample_rate_hz: f.reverse_input_rate,
                channels: f.reverse_input_channels,
            },
            reverse_output: AudioProcessingStream {
                sample_rate_hz: f.reverse_output_rate,
                channels: f.reverse_output_channels,
            },
        }
    }
}
impl AudioProcessingFormats {
    fn ffi(self) -> ffi::FfiApmFormats {
        ffi::FfiApmFormats {
            input_rate: self.input.sample_rate_hz,
            input_channels: self.input.channels,
            output_rate: self.output.sample_rate_hz,
            output_channels: self.output.channels,
            reverse_input_rate: self.reverse_input.sample_rate_hz,
            reverse_input_channels: self.reverse_input.channels,
            reverse_output_rate: self.reverse_output.sample_rate_hz,
            reverse_output_channels: self.reverse_output.channels,
        }
    }
}

/// Native rates/channels reported by the currently initialized processing graph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioProcessingFormatState {
    pub processing_rate_hz: i32,
    pub split_rate_hz: i32,
    pub input_channels: usize,
    pub processing_channels: usize,
    pub output_channels: usize,
    pub reverse_channels: usize,
}

/// Single-sequence owning stock module. The exact environment is retained until
/// after native module destruction. No device, worker or hidden pump is created.
pub struct AudioProcessor {
    native: cxx::UniquePtr<ffi::NativeAudioProcessor>,
    _environment: Environment,
    _sequence: PhantomData<Rc<()>>,
}
impl AudioProcessor {
    fn admission(error: u8) -> Result<(), AudioProcessorError> {
        match error {
            0 => Ok(()),
            1 => Err(AudioProcessorError::UnsafeFieldTrial),
            2 => Err(AudioProcessorError::InvalidDimensions),
            4 => Err(AudioProcessorError::UnqualifiedGainRange),
            _ => Err(AudioProcessorError::InvalidConfiguration),
        }
    }
    /// Drive the builtin builder with every native config member unchanged.
    pub fn builtin_with_config(
        environment: &Environment,
        config: &crate::AudioProcessorConfig,
    ) -> Result<Self, AudioProcessorError> {
        let environment = environment.clone();
        let mut error = 0;
        let native =
            ffi::audio_processor_new_config(environment.native(), &config.ffi(), &mut error);
        Self::admission(error)?;
        if native.is_null() {
            return Err(AudioProcessorError::CreationFailed);
        }
        Ok(Self {
            native,
            _environment: environment,
            _sequence: PhantomData,
        })
    }
    /// Native ApplyConfig, including its actual GC2 validation/reset behavior.
    pub fn apply_config(
        &mut self,
        config: &crate::AudioProcessorConfig,
    ) -> Result<(), AudioProcessorError> {
        Self::admission(ffi::audio_processor_config_apply(
            self.native.pin_mut(),
            &config.ffi(),
        ))
    }
    /// Select builtin APM with the same initial effect choices as peer capture.
    /// This creates a standalone module, not an installed peer component.
    pub fn builtin(
        environment: &Environment,
        config: AudioProcessingConfig,
    ) -> Result<Self, AudioProcessorError> {
        let environment = environment.clone();
        if !ffi::audio_processor_effects_admitted(environment.native(), &config.ffi()) {
            return Err(AudioProcessorError::UnsafeFieldTrial);
        }
        let native = ffi::audio_processor_new(environment.native(), &config.ffi());
        if native.is_null() {
            return Err(AudioProcessorError::CreationFailed);
        }
        Ok(Self {
            native,
            _environment: environment,
            _sequence: PhantomData,
        })
    }
    pub fn initialize(&mut self) -> AudioProcessingStatus {
        AudioProcessingStatus(ffi::audio_processor_initialize(self.native.pin_mut()))
    }
    /// Initialize the actual four-stream graph. This native overload always
    /// returns OK, even for unsafe dimensions, so inadmissible layouts are
    /// wrapper errors before entry, never invented native status codes.
    pub fn initialize_with_formats(
        &mut self,
        formats: AudioProcessingFormats,
    ) -> Result<AudioProcessingStatus, AudioProcessorError> {
        for s in [
            formats.input,
            formats.output,
            formats.reverse_input,
            formats.reverse_output,
        ] {
            s.sample_count()?;
        }
        let mut admitted = false;
        let status = ffi::audio_processor_initialize_formats(
            self.native.pin_mut(),
            &formats.ffi(),
            &mut admitted,
        );
        if !admitted {
            return Err(AudioProcessorError::InvalidDimensions);
        }
        Ok(AudioProcessingStatus(status))
    }
    /// Apply effect selection to this actual module without substituting a
    /// product. Settings not represented by these choices remain unchanged.
    pub fn configure_effects(
        &mut self,
        config: AudioProcessingConfig,
    ) -> Result<(), AudioProcessorError> {
        Self::admission(ffi::audio_processor_configure(
            self.native.pin_mut(),
            &config.ffi(),
        ))
    }
    /// Current actual SDK configuration, including disabled nested fields.
    pub fn configuration(&self) -> crate::AudioProcessorConfig {
        crate::AudioProcessorConfig::from_ffi(ffi::audio_processor_config_get(
            self.native.as_ref().unwrap(),
        ))
    }
    pub fn format_state(&self) -> AudioProcessingFormatState {
        let s = ffi::audio_processor_format(self.native.as_ref().unwrap());
        AudioProcessingFormatState {
            processing_rate_hz: s.processing_rate_hz,
            split_rate_hz: s.split_rate_hz,
            input_channels: s.input_channels,
            processing_channels: s.processing_channels,
            output_channels: s.output_channels,
            reverse_channels: s.reverse_channels,
        }
    }
    pub fn set_output_will_be_muted(&mut self, muted: bool) {
        ffi::audio_processor_set_muted(self.native.pin_mut(), muted);
    }
    pub fn output_will_be_muted(&mut self) -> bool {
        ffi::audio_processor_muted(self.native.pin_mut())
    }
    /// The SDK preserves its actual clamp and warning for delays outside 0..500.
    pub fn set_stream_delay_ms(&mut self, delay: i32) -> AudioProcessingStatus {
        AudioProcessingStatus(ffi::audio_processor_set_delay(self.native.pin_mut(), delay))
    }
    pub fn stream_delay_ms(&self) -> i32 {
        ffi::audio_processor_delay(self.native.as_ref().unwrap())
    }
    pub fn set_stream_analog_level(&mut self, level: i32) -> Result<(), AudioProcessorError> {
        if !(0..=255).contains(&level) {
            return Err(AudioProcessorError::InvalidAnalogLevel);
        }
        ffi::audio_processor_set_analog_level(self.native.pin_mut(), level);
        Ok(())
    }
    pub fn recommended_stream_analog_level(&self) -> i32 {
        ffi::audio_processor_analog_level(self.native.as_ref().unwrap())
    }
    pub fn set_stream_key_pressed(&mut self, pressed: bool) {
        ffi::audio_processor_set_key(self.native.pin_mut(), pressed);
    }

    /// Process only the native 10-ms prefix in place. Tail samples are untouched.
    /// The returned native error/warning does not imply that the prefix was
    /// untouched: unsupported formats may produce native best-effort output.
    pub fn process_interleaved(
        &mut self,
        stream: AudioProcessingStream,
        samples: &mut [i16],
    ) -> Result<AudioProcessingStatus, AudioProcessorError> {
        self.process_i16(stream, samples, false)
    }
    pub fn process_reverse_interleaved(
        &mut self,
        stream: AudioProcessingStream,
        samples: &mut [i16],
    ) -> Result<AudioProcessingStatus, AudioProcessorError> {
        self.process_i16(stream, samples, true)
    }
    fn process_i16(
        &mut self,
        stream: AudioProcessingStream,
        samples: &mut [i16],
        reverse: bool,
    ) -> Result<AudioProcessingStatus, AudioProcessorError> {
        // Stock mono downmix sums into int32 and divides by a narrowed int16
        // channel count. 65536 would yield zero; larger counts can overflow.
        if stream.channels > u16::MAX as usize {
            return Err(AudioProcessorError::InvalidDimensions);
        }
        if samples.len() < stream.sample_count()? {
            return Err(AudioProcessorError::BufferTooShort);
        }
        Ok(AudioProcessingStatus(ffi::audio_processor_i16(
            self.native.pin_mut(),
            stream.sample_rate_hz,
            stream.channels,
            samples,
            reverse,
        )))
    }
    /// Deinterleaved normalized float channels in contiguous channel-major
    /// storage. Input/output layouts may differ exactly as allowed by the SDK.
    /// Only native-sized prefixes of caller-initialized output are available.
    pub fn process_planar(
        &mut self,
        input: AudioProcessingStream,
        samples: &[f32],
        output: AudioProcessingStream,
        destination: &mut [f32],
    ) -> Result<AudioProcessingStatus, AudioProcessorError> {
        self.process_float(input, samples, output, destination, false)
    }
    pub fn process_reverse_planar(
        &mut self,
        input: AudioProcessingStream,
        samples: &[f32],
        output: AudioProcessingStream,
        destination: &mut [f32],
    ) -> Result<AudioProcessingStatus, AudioProcessorError> {
        self.process_float(input, samples, output, destination, true)
    }
    fn float_input(
        stream: AudioProcessingStream,
        samples: &[f32],
    ) -> Result<(), AudioProcessorError> {
        let count = stream.sample_count()?;
        if samples.len() < count {
            return Err(AudioProcessorError::BufferTooShort);
        }
        if samples[..count]
            .iter()
            .any(|v| !v.is_finite() || !(-1.0..=1.0).contains(v))
        {
            return Err(AudioProcessorError::InvalidFloatInput);
        }
        Ok(())
    }
    fn process_float(
        &mut self,
        input: AudioProcessingStream,
        samples: &[f32],
        output: AudioProcessingStream,
        destination: &mut [f32],
        reverse: bool,
    ) -> Result<AudioProcessingStatus, AudioProcessorError> {
        Self::float_input(input, samples)?;
        if destination.len() < output.sample_count()? {
            return Err(AudioProcessorError::BufferTooShort);
        }
        Ok(AudioProcessingStatus(ffi::audio_processor_float(
            self.native.pin_mut(),
            input.sample_rate_hz,
            input.channels,
            samples,
            output.sample_rate_hz,
            output.channels,
            destination,
            reverse,
        )))
    }
    pub fn analyze_reverse_planar(
        &mut self,
        stream: AudioProcessingStream,
        samples: &[f32],
    ) -> Result<AudioProcessingStatus, AudioProcessorError> {
        Self::float_input(stream, samples)?;
        Ok(AudioProcessingStatus(ffi::audio_processor_analyze(
            self.native.pin_mut(),
            stream.sample_rate_hz,
            stream.channels,
            samples,
        )))
    }
}
