//! Owning stock mixer and serialized, retained participant callbacks.
use crate::{AudioFrame, ffi};
use std::{
    cell::RefCell,
    marker::PhantomData,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
    sync::{Arc, Mutex},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioMixerError {
    CreationFailed,
    InvalidChannels,
    InvalidPreferredRate,
    InvalidFrame,
    CallbackRejected,
    CallbackPanicked,
    ReentrantCall,
}
impl std::fmt::Display for AudioMixerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for AudioMixerError {}

/// Native kError is an ordinary source disposition, not a callback failure.
#[derive(Debug)]
pub enum MixerAudioFrame {
    Normal(AudioFrame),
    Muted(AudioFrame),
    Error,
}
/// Calls are serialized even when one source is registered with multiple mixers.
/// Normal and muted frames must describe exactly the requested 10-ms format,
/// with one or two channels. Muted frame storage is never consumed.
pub trait AudioMixerSourceProvider: Send + 'static {
    fn audio_frame(&mut self, sample_rate_hz: i32) -> Result<MixerAudioFrame, AudioMixerError>;
    fn ssrc(&mut self) -> Result<i32, AudioMixerError>;
    fn preferred_sample_rate(&mut self) -> Result<i32, AudioMixerError>;
}
#[derive(Clone, Default)]
pub struct AudioMixerFailureObserver(Arc<Mutex<Option<AudioMixerError>>>);
impl AudioMixerFailureObserver {
    pub fn failure(&self) -> Option<AudioMixerError> {
        *self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub(crate) fn record(&self, e: AudioMixerError) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_or_insert(e);
    }
}
thread_local! { static ACTIVE: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) }; }
struct Scope(usize);
impl Scope {
    fn enter(id: usize) -> Result<Self, AudioMixerError> {
        ACTIVE.with(|active| {
            let mut active = active.borrow_mut();
            if active.contains(&id) {
                return Err(AudioMixerError::ReentrantCall);
            }
            active.push(id);
            Ok(Self(id))
        })
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        ACTIVE.with(|a| {
            let mut a = a.borrow_mut();
            debug_assert_eq!(a.pop(), Some(self.0));
        });
    }
}
pub(crate) struct RustMixerSource {
    provider: Mutex<Box<dyn AudioMixerSourceProvider>>,
    failure: AudioMixerFailureObserver,
}
impl RustMixerSource {
    fn call<T>(
        &self,
        f: impl FnOnce(&mut dyn AudioMixerSourceProvider) -> Result<T, AudioMixerError>,
    ) -> Result<T, AudioMixerError> {
        let result = (|| {
            let _scope = Scope::enter(self as *const Self as usize)?;
            let mut provider = self.provider.lock().unwrap_or_else(|e| e.into_inner());
            catch_unwind(AssertUnwindSafe(|| f(provider.as_mut())))
                .unwrap_or(Err(AudioMixerError::CallbackPanicked))
        })();
        if let Err(error) = result {
            self.failure.record(error);
        }
        result
    }
}
pub(crate) fn mixer_source_frame(source: &RustMixerSource, rate: i32) -> ffi::FfiMixerFrame {
    let mut result = ffi::FfiMixerFrame {
        disposition: 2,
        frame: cxx::UniquePtr::null(),
        failed: false,
    };
    match source.call(|p| p.audio_frame(rate)) {
        Ok(MixerAudioFrame::Normal(frame)) => {
            result.disposition = 0;
            result.frame = frame.native;
        }
        Ok(MixerAudioFrame::Muted(frame)) => {
            result.disposition = 1;
            result.frame = frame.native;
        }
        Ok(MixerAudioFrame::Error) => {}
        Err(_) => result.failed = true,
    }
    result
}
pub(crate) fn mixer_source_ssrc(source: &RustMixerSource, valid: &mut bool) -> i32 {
    match source.call(|p| p.ssrc()) {
        Ok(v) => {
            *valid = true;
            v
        }
        Err(_) => {
            *valid = false;
            0
        }
    }
}
pub(crate) fn mixer_source_rate(source: &RustMixerSource, valid: &mut bool) -> i32 {
    match source.call(|p| p.preferred_sample_rate()) {
        Ok(v) => {
            *valid = true;
            v
        }
        Err(_) => {
            *valid = false;
            0
        }
    }
}
pub(crate) fn mixer_source_invalid(source: &RustMixerSource) {
    source.failure.record(AudioMixerError::InvalidFrame);
}

/// Clones identify the exact same participant. Registration retains it even
/// after the outer source handle is dropped. No SSRC-based identity join.
#[derive(Clone)]
pub struct AudioMixerSource {
    native: Arc<cxx::UniquePtr<ffi::NativeMixerSource>>,
    failure: AudioMixerFailureObserver,
}
// Native source state is immutable apart from its Mutex-serialized Rust provider.
unsafe impl Send for AudioMixerSource {}
unsafe impl Sync for AudioMixerSource {}
impl AudioMixerSource {
    pub fn from_provider(provider: impl AudioMixerSourceProvider) -> Self {
        let failure = AudioMixerFailureObserver::default();
        let native = ffi::mixer_source_new(Box::new(RustMixerSource {
            provider: Mutex::new(Box::new(provider)),
            failure: failure.clone(),
        }));
        Self {
            native: Arc::new(native),
            failure,
        }
    }
    pub fn failure_observer(&self) -> AudioMixerFailureObserver {
        self.failure.clone()
    }
    fn inner(&self) -> &ffi::NativeMixerSource {
        self.native.as_ref().as_ref().expect("owned mixer source")
    }
    pub fn ssrc(&self) -> Result<i32, AudioMixerError> {
        let mut valid = false;
        let value = ffi::mixer_source_get_ssrc(self.inner(), &mut valid);
        if valid {
            Ok(value)
        } else {
            Err(self
                .failure
                .failure()
                .unwrap_or(AudioMixerError::CallbackRejected))
        }
    }
    pub fn preferred_sample_rate(&self) -> Result<i32, AudioMixerError> {
        let mut valid = false;
        let value = ffi::mixer_source_get_rate(self.inner(), &mut valid);
        if valid {
            Ok(value)
        } else {
            Err(self
                .failure
                .failure()
                .unwrap_or(AudioMixerError::CallbackRejected))
        }
    }
}

/// Mixing is sequence-bound; source registration ownership is shared, but no
/// mutable alias of the mixer is exposed. Callback reentry is refused natively
/// before the SDK mixer mutex. The default SDK calculator and limiter are used.
pub struct AudioMixer {
    native: cxx::UniquePtr<ffi::NativeAudioMixer>,
    _sequence: PhantomData<Rc<()>>,
}
impl AudioMixer {
    pub fn stock() -> Result<Self, AudioMixerError> {
        let native = ffi::audio_mixer_new();
        if native.is_null() {
            return Err(AudioMixerError::CreationFailed);
        }
        Ok(Self {
            native,
            _sequence: PhantomData,
        })
    }
    pub fn add_source(&mut self, source: &AudioMixerSource) -> Result<bool, AudioMixerError> {
        let mut error = 0;
        let added = ffi::audio_mixer_add(self.native.pin_mut(), source.inner(), &mut error);
        if error == 0 {
            Ok(added)
        } else {
            Err(AudioMixerError::ReentrantCall)
        }
    }
    /// Returns false for an absent source without invoking the SDK's asserting
    /// removal operation. An alias of a registered source removes that source.
    pub fn remove_source(&mut self, source: &AudioMixerSource) -> Result<bool, AudioMixerError> {
        let mut error = 0;
        let removed = ffi::audio_mixer_remove(self.native.pin_mut(), source.inner(), &mut error);
        if error == 0 {
            Ok(removed)
        } else {
            Err(AudioMixerError::ReentrantCall)
        }
    }
    pub fn mix(&mut self, channels: usize) -> Result<AudioFrame, AudioMixerError> {
        if channels == 0 {
            return Err(AudioMixerError::InvalidChannels);
        }
        let mut error = 0;
        let frame = ffi::audio_mixer_mix(self.native.pin_mut(), channels, &mut error);
        match error {
            0 if !frame.is_null() => Ok(AudioFrame::from_native(frame)),
            1 => Err(AudioMixerError::ReentrantCall),
            2 => Err(AudioMixerError::InvalidPreferredRate),
            3 => Err(AudioMixerError::InvalidFrame),
            _ => Err(AudioMixerError::CallbackRejected),
        }
    }
}
