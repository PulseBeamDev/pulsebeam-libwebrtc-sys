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
    /// Apply effect selection to this actual module without substituting a
    /// product. Settings not represented by these choices remain unchanged.
    pub fn configure_effects(
        &mut self,
        config: AudioProcessingConfig,
    ) -> Result<(), AudioProcessorError> {
        if !ffi::audio_processor_effects_admitted(self._environment.native(), &config.ffi()) {
            return Err(AudioProcessorError::UnsafeFieldTrial);
        }
        ffi::audio_processor_configure(self.native.pin_mut(), &config.ffi());
        Ok(())
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
