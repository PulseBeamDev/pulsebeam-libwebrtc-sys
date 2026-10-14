//! Caller-sequence owning products returned by the actual selected SDK factory.
use crate::{Environment, OwnedVideoDecoderInfo, SdpVideoFormat, VideoDecoderFactoryHandle, ffi};
use std::{fmt, marker::PhantomData, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoDecoderCreationError {
    /// The actual selected SDK Create returned null. No fallback was selected.
    NativeNotCreated,
    /// Unsupported-format logging would fatally name unknown scalability metadata.
    UnsafeNativeFormatFormatting,
}
impl fmt::Display for VideoDecoderCreationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for VideoDecoderCreationError {}

/// Move-only, caller-sequence SDK decoder. Creation, methods and final release
/// stay on the construction thread. This is not an actor-importable local handle.
/// The actual creation Environment and its Rust leases, private factory adapter
/// and selected registration outlive the decoder and its final native teardown.
///
/// ```compile_fail
/// fn assert_send<T: Send>() {}
/// assert_send::<pulsebeam_webrtc_sys::OwnedVideoDecoder>();
/// ```
pub struct OwnedVideoDecoder {
    // Drop native product before the selected factory registration.
    native: cxx::UniquePtr<ffi::NativeVideoDecoderProduct>,
    factory: VideoDecoderFactoryHandle,
    _caller: PhantomData<Rc<()>>,
}
impl fmt::Debug for OwnedVideoDecoder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OwnedVideoDecoder").finish_non_exhaustive()
    }
}
impl VideoDecoderFactoryHandle {
    /// Call the selected SDK Create with the exact owned format/environment.
    /// A null result is not converted to stock selection or a failed sentinel.
    pub fn create_decoder(
        &self,
        environment: &Environment,
        format: &SdpVideoFormat,
    ) -> Result<OwnedVideoDecoder, VideoDecoderCreationError> {
        let mut status = 0;
        let native = ffi::video_decoder_create_product(
            self.native(),
            environment.native(),
            Box::new(crate::video_factory_values::RustVideoFactoryEnvironment(
                environment.clone(),
            )),
            format.native(),
            &mut status,
        );
        match status {
            1 => return Err(VideoDecoderCreationError::UnsafeNativeFormatFormatting),
            0 => {}
            _ => unreachable!("closed decoder creation admission status"),
        }
        if native.is_null() {
            return Err(VideoDecoderCreationError::NativeNotCreated);
        }
        Ok(OwnedVideoDecoder {
            native,
            factory: self.clone(),
            _caller: PhantomData,
        })
    }
}
impl OwnedVideoDecoder {
    fn native(&self) -> &ffi::NativeVideoDecoderProduct {
        self.native.as_ref().unwrap()
    }
    /// Actual SDK GetDecoderInfo as an independent byte-preserving value copy.
    pub fn info(&self) -> OwnedVideoDecoderInfo {
        OwnedVideoDecoderInfo::from_native(ffi::video_decoder_product_info(self.native()))
    }
    /// The separate deprecated SDK ImplementationName method. Its result need
    /// not equal GetDecoderInfo's name, particularly on caller adapters.
    pub fn implementation_name_bytes(&self) -> Vec<u8> {
        ffi::video_decoder_product_implementation_name(self.native())
    }
    /// Actual SDK Release status. Calls are forwarded, not cached or normalized;
    /// final destruction retains normal selected SDK destructor behavior.
    pub fn release(&mut self) -> i32 {
        ffi::video_decoder_product_release(self.native.pin_mut())
    }
    pub fn failure_observer(&self) -> Option<crate::VideoFactoryFailureObserver> {
        self.factory.failure_observer()
    }
}
