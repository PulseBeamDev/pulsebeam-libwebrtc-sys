use std::{
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::ffi;

const CODEC_OK: i32 = 0;
const CODEC_ERROR: i32 = -1;
const CODEC_PARAMETER: i32 = -4;
const CODEC_UNINITIALIZED: i32 = -7;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodecParameter {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VideoCodecFormat {
    pub name: String,
    pub parameters: Vec<CodecParameter>,
}

impl VideoCodecFormat {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            parameters: Vec::new(),
        }
    }

    pub fn with_parameter(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.parameters.push(CodecParameter {
            key: key.into(),
            value: value.into(),
        });
        self
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CodecSupport {
    pub supported: bool,
    pub power_efficient: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VideoResolution {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VideoFrameType {
    Key,
    Delta,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VideoFrameBuffer {
    data: Vec<u8>,
}

impl VideoFrameBuffer {
    pub fn i420(width: u32, height: u32, data: Vec<u8>) -> Result<Self, CodecError> {
        if width == 0 || height == 0 || data.len() != i420_len(width, height)? {
            return Err(CodecError::InvalidFrame);
        }
        Ok(Self { data })
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.data
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub timestamp_us: i64,
    pub rtp_timestamp: u32,
    pub buffer: VideoFrameBuffer,
}

impl VideoFrame {
    pub fn i420(
        width: u32,
        height: u32,
        data: Vec<u8>,
        timestamp_us: i64,
        rtp_timestamp: u32,
    ) -> Result<Self, CodecError> {
        Ok(Self {
            width,
            height,
            timestamp_us,
            rtp_timestamp,
            buffer: VideoFrameBuffer::i420(width, height, data)?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedVideoFrame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub rtp_timestamp: u32,
    pub frame_type: VideoFrameType,
    pub qp: Option<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VideoEncoderSettings {
    pub width: u32,
    pub height: u32,
    pub start_bitrate_bps: u32,
    pub max_bitrate_bps: u32,
    pub min_bitrate_bps: u32,
    pub max_framerate: u32,
    pub cores: u32,
    pub max_payload_size: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VideoDecoderSettings {
    pub cores: u32,
    pub max_resolution: Option<VideoResolution>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VideoRateControl {
    pub bitrate_bps: u32,
    pub framerate_fps: f64,
    pub bandwidth_bps: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VideoEncoderInfo {
    pub implementation_name: String,
    pub hardware_accelerated: bool,
    pub supports_native_handle: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VideoDecoderInfo {
    pub implementation_name: String,
    pub hardware_accelerated: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodecError {
    UnsupportedFormat,
    ConstructionFailed,
    InvalidConfiguration,
    InvalidFrame,
    CallbackUnavailable,
    Released,
    ProviderPanicked,
}

impl fmt::Display for CodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}",
            match self {
                Self::UnsupportedFormat => "unsupported codec format",
                Self::ConstructionFailed => "codec construction failed",
                Self::InvalidConfiguration => "invalid codec configuration",
                Self::InvalidFrame => "invalid video frame",
                Self::CallbackUnavailable => "codec output callback is unavailable",
                Self::Released => "codec has been released",
                Self::ProviderPanicked => "codec provider panicked",
            }
        )
    }
}

impl std::error::Error for CodecError {}

pub trait VideoEncoderFactory: Send + Sync + 'static {
    fn supported_formats(&self) -> Vec<VideoCodecFormat>;
    fn query_support(
        &self,
        format: &VideoCodecFormat,
        scalability_mode: Option<&str>,
        resolution: Option<VideoResolution>,
    ) -> CodecSupport;
    fn create(&self, format: &VideoCodecFormat) -> Result<Box<dyn VideoEncoder>, CodecError>;
}

pub trait VideoDecoderFactory: Send + Sync + 'static {
    fn supported_formats(&self) -> Vec<VideoCodecFormat>;
    fn query_support(
        &self,
        format: &VideoCodecFormat,
        reference_scaling: bool,
        resolution: Option<VideoResolution>,
    ) -> CodecSupport;
    fn create(&self, format: &VideoCodecFormat) -> Result<Box<dyn VideoDecoder>, CodecError>;
}

pub trait VideoEncoder: Send + 'static {
    fn initialize(&mut self, settings: VideoEncoderSettings) -> Result<(), CodecError>;
    fn register_callback(&mut self) -> Result<(), CodecError> {
        Ok(())
    }
    fn encode(
        &mut self,
        frame: VideoFrame,
        frame_types: &[VideoFrameType],
        callback: EncodedImageCallback,
    ) -> Result<(), CodecError>;
    fn set_rates(&mut self, rates: VideoRateControl) -> Result<(), CodecError>;
    fn release(&mut self) -> Result<(), CodecError>;
    fn info(&self) -> VideoEncoderInfo;
}

pub trait VideoDecoder: Send + 'static {
    fn configure(&mut self, settings: VideoDecoderSettings) -> Result<(), CodecError>;
    fn register_callback(&mut self) -> Result<(), CodecError> {
        Ok(())
    }
    fn decode(
        &mut self,
        frame: EncodedVideoFrame,
        callback: DecodedImageCallback,
    ) -> Result<(), CodecError>;
    fn release(&mut self) -> Result<(), CodecError>;
    fn info(&self) -> VideoDecoderInfo;
}

#[derive(Clone)]
pub struct EncodedImageCallback {
    native: cxx::SharedPtr<ffi::NativeEncodedImageCallback>,
}

// SAFETY: the native holder serializes invocation and release around its callback pointer.
unsafe impl Send for EncodedImageCallback {}
// SAFETY: shared invocations are serialized by the native holder's mutex.
unsafe impl Sync for EncodedImageCallback {}

impl EncodedImageCallback {
    pub fn emit(&self, frame: &EncodedVideoFrame) -> Result<(), CodecError> {
        let native = self
            .native
            .as_ref()
            .ok_or(CodecError::CallbackUnavailable)?;
        ffi::encoded_callback_emit(
            native,
            &frame.data,
            frame.width,
            frame.height,
            frame.rtp_timestamp,
            frame.frame_type == VideoFrameType::Key,
            frame.qp.map_or(-1, i32::from),
        )
        .then_some(())
        .ok_or(CodecError::Released)
    }
}

#[derive(Clone)]
pub struct DecodedImageCallback {
    native: cxx::SharedPtr<ffi::NativeDecodedImageCallback>,
}

// SAFETY: the native holder serializes invocation and release around its callback pointer.
unsafe impl Send for DecodedImageCallback {}
// SAFETY: shared invocations are serialized by the native holder's mutex.
unsafe impl Sync for DecodedImageCallback {}

impl DecodedImageCallback {
    pub fn emit(&self, frame: &VideoFrame) -> Result<(), CodecError> {
        let native = self
            .native
            .as_ref()
            .ok_or(CodecError::CallbackUnavailable)?;
        ffi::decoded_callback_emit(
            native,
            frame.buffer.as_bytes(),
            frame.width,
            frame.height,
            frame.timestamp_us,
            frame.rtp_timestamp,
        )
        .then_some(())
        .ok_or(CodecError::Released)
    }
}

pub struct RustVideoEncoderFactory {
    provider: Box<dyn VideoEncoderFactory>,
    terminal: AtomicBool,
}
pub struct RustVideoDecoderFactory {
    provider: Box<dyn VideoDecoderFactory>,
    terminal: AtomicBool,
}
pub struct RustVideoEncoder {
    codec: Option<Box<dyn VideoEncoder>>,
    terminal: AtomicBool,
}
pub struct RustVideoDecoder {
    codec: Option<Box<dyn VideoDecoder>>,
    terminal: AtomicBool,
}

struct EncoderFactoryInner {
    native: cxx::UniquePtr<ffi::NativeVideoEncoderFactory>,
}
struct DecoderFactoryInner {
    native: cxx::UniquePtr<ffi::NativeVideoDecoderFactory>,
}

// SAFETY: the provider is Send + Sync and native factory methods only perform shared calls.
unsafe impl Send for EncoderFactoryInner {}
// SAFETY: see the Send rationale; provider discovery and creation accept shared access.
unsafe impl Sync for EncoderFactoryInner {}
// SAFETY: the provider is Send + Sync and native factory methods only perform shared calls.
unsafe impl Send for DecoderFactoryInner {}
// SAFETY: see the Send rationale; provider discovery and creation accept shared access.
unsafe impl Sync for DecoderFactoryInner {}

#[derive(Clone)]
pub struct VideoEncoderFactoryHandle(Arc<EncoderFactoryInner>);
#[derive(Clone)]
pub struct VideoDecoderFactoryHandle(Arc<DecoderFactoryInner>);

impl VideoEncoderFactoryHandle {
    pub fn new(provider: impl VideoEncoderFactory) -> Result<Self, CodecError> {
        let native = ffi::new_video_encoder_factory(Box::new(RustVideoEncoderFactory {
            provider: Box::new(provider),
            terminal: AtomicBool::new(false),
        }));
        if native.is_null() {
            Err(CodecError::ConstructionFailed)
        } else {
            Ok(Self(Arc::new(EncoderFactoryInner { native })))
        }
    }
    pub fn supported_formats(&self) -> Vec<VideoCodecFormat> {
        ffi::video_encoder_formats(self.native())
            .into_iter()
            .map(from_ffi_format)
            .collect()
    }
    pub fn query_support(
        &self,
        format: &VideoCodecFormat,
        scalability_mode: Option<&str>,
        resolution: Option<VideoResolution>,
    ) -> CodecSupport {
        let resolution = resolution.unwrap_or(VideoResolution {
            width: 0,
            height: 0,
        });
        let result = ffi::video_encoder_query(
            self.native(),
            &to_ffi_format(format),
            scalability_mode.unwrap_or(""),
            resolution.width != 0 && resolution.height != 0,
            resolution.width,
            resolution.height,
        );
        CodecSupport {
            supported: result.supported,
            power_efficient: result.power_efficient,
        }
    }
    pub(crate) fn native(&self) -> &ffi::NativeVideoEncoderFactory {
        self.0.native.as_ref().expect("validated encoder factory")
    }
}

impl VideoDecoderFactoryHandle {
    pub fn new(provider: impl VideoDecoderFactory) -> Result<Self, CodecError> {
        let native = ffi::new_video_decoder_factory(Box::new(RustVideoDecoderFactory {
            provider: Box::new(provider),
            terminal: AtomicBool::new(false),
        }));
        if native.is_null() {
            Err(CodecError::ConstructionFailed)
        } else {
            Ok(Self(Arc::new(DecoderFactoryInner { native })))
        }
    }
    pub fn supported_formats(&self) -> Vec<VideoCodecFormat> {
        ffi::video_decoder_formats(self.native())
            .into_iter()
            .map(from_ffi_format)
            .collect()
    }
    pub fn query_support(
        &self,
        format: &VideoCodecFormat,
        reference_scaling: bool,
        resolution: Option<VideoResolution>,
    ) -> CodecSupport {
        let resolution = resolution.unwrap_or(VideoResolution {
            width: 0,
            height: 0,
        });
        let result = ffi::video_decoder_query(
            self.native(),
            &to_ffi_format(format),
            reference_scaling,
            resolution.width != 0 && resolution.height != 0,
            resolution.width,
            resolution.height,
        );
        CodecSupport {
            supported: result.supported,
            power_efficient: result.power_efficient,
        }
    }
    pub(crate) fn native(&self) -> &ffi::NativeVideoDecoderFactory {
        self.0.native.as_ref().expect("validated decoder factory")
    }
}

struct AudioEncoderFactoryInner {
    native: cxx::UniquePtr<ffi::NativeAudioEncoderFactory>,
}
struct AudioDecoderFactoryInner {
    native: cxx::UniquePtr<ffi::NativeAudioDecoderFactory>,
}
// SAFETY: upstream audio codec factories are ref-counted, thread-safe factory interfaces.
unsafe impl Send for AudioEncoderFactoryInner {}
// SAFETY: only the immutable factory handle is exposed.
unsafe impl Sync for AudioEncoderFactoryInner {}
// SAFETY: upstream audio codec factories are ref-counted, thread-safe factory interfaces.
unsafe impl Send for AudioDecoderFactoryInner {}
// SAFETY: only the immutable factory handle is exposed.
unsafe impl Sync for AudioDecoderFactoryInner {}

#[derive(Clone)]
pub struct AudioEncoderFactory(Arc<AudioEncoderFactoryInner>);
#[derive(Clone)]
pub struct AudioDecoderFactory(Arc<AudioDecoderFactoryInner>);

impl AudioEncoderFactory {
    pub fn builtin() -> Result<Self, CodecError> {
        let native = ffi::new_builtin_audio_encoder_factory();
        if native.is_null() {
            Err(CodecError::ConstructionFailed)
        } else {
            Ok(Self(Arc::new(AudioEncoderFactoryInner { native })))
        }
    }
    pub fn is_available(&self) -> bool {
        !self.0.native.is_null()
    }
    pub(crate) fn native(&self) -> &ffi::NativeAudioEncoderFactory {
        self.0
            .native
            .as_ref()
            .expect("validated audio encoder factory")
    }
}
impl AudioDecoderFactory {
    pub fn builtin() -> Result<Self, CodecError> {
        let native = ffi::new_builtin_audio_decoder_factory();
        if native.is_null() {
            Err(CodecError::ConstructionFailed)
        } else {
            Ok(Self(Arc::new(AudioDecoderFactoryInner { native })))
        }
    }
    pub fn is_available(&self) -> bool {
        !self.0.native.is_null()
    }
    pub(crate) fn native(&self) -> &ffi::NativeAudioDecoderFactory {
        self.0
            .native
            .as_ref()
            .expect("validated audio decoder factory")
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CodecTestResult {
    pub status: i32,
    pub encoded_frames: u32,
    pub decoded_frames: u32,
    pub checksum: u64,
}

impl VideoEncoderFactoryHandle {
    #[cfg(test)]
    fn test_roundtrip(&self, decoder: &VideoDecoderFactoryHandle, frames: u32) -> CodecTestResult {
        let result = ffi::test_codec_roundtrip(self.native(), decoder.native(), frames);
        CodecTestResult {
            status: result.status,
            encoded_frames: result.encoded_frames,
            decoded_frames: result.decoded_frames,
            checksum: result.checksum,
        }
    }
    #[cfg(test)]
    fn test_cross_thread_lifecycle(&self) -> bool {
        ffi::test_encoder_factory_cross_thread(self.native())
    }
}

fn i420_len(width: u32, height: u32) -> Result<usize, CodecError> {
    let y = usize::try_from(width)
        .ok()
        .and_then(|w| usize::try_from(height).ok().and_then(|h| w.checked_mul(h)))
        .ok_or(CodecError::InvalidFrame)?;
    let cw = usize::try_from(width.div_ceil(2)).map_err(|_| CodecError::InvalidFrame)?;
    let ch = usize::try_from(height.div_ceil(2)).map_err(|_| CodecError::InvalidFrame)?;
    y.checked_add(
        cw.checked_mul(ch)
            .and_then(|c| c.checked_mul(2))
            .ok_or(CodecError::InvalidFrame)?,
    )
    .ok_or(CodecError::InvalidFrame)
}

fn to_ffi_format(format: &VideoCodecFormat) -> ffi::FfiCodecFormat {
    ffi::FfiCodecFormat {
        name: format.name.clone(),
        parameters: format
            .parameters
            .iter()
            .map(|p| ffi::FfiCodecParameter {
                key: p.key.clone(),
                value: p.value.clone(),
            })
            .collect(),
    }
}
fn from_ffi_format(format: ffi::FfiCodecFormat) -> VideoCodecFormat {
    VideoCodecFormat {
        name: format.name,
        parameters: format
            .parameters
            .into_iter()
            .map(|p| CodecParameter {
                key: p.key,
                value: p.value,
            })
            .collect(),
    }
}
fn from_ffi_format_ref(format: &ffi::FfiCodecFormat) -> VideoCodecFormat {
    VideoCodecFormat {
        name: format.name.clone(),
        parameters: format
            .parameters
            .iter()
            .map(|parameter| CodecParameter {
                key: parameter.key.clone(),
                value: parameter.value.clone(),
            })
            .collect(),
    }
}
fn resolution(has: bool, width: u32, height: u32) -> Option<VideoResolution> {
    has.then_some(VideoResolution { width, height })
}
fn status(result: Result<(), CodecError>) -> i32 {
    match result {
        Ok(()) => CODEC_OK,
        Err(CodecError::InvalidConfiguration | CodecError::InvalidFrame) => CODEC_PARAMETER,
        Err(CodecError::Released | CodecError::CallbackUnavailable) => CODEC_UNINITIALIZED,
        Err(_) => CODEC_ERROR,
    }
}

pub(crate) fn encoder_factory_formats(
    factory: &RustVideoEncoderFactory,
) -> Vec<ffi::FfiCodecFormat> {
    if factory.terminal.load(Ordering::Acquire) {
        return Vec::new();
    }
    match catch_unwind(AssertUnwindSafe(|| factory.provider.supported_formats())) {
        Ok(formats) => formats.iter().map(to_ffi_format).collect(),
        Err(_) => {
            factory.terminal.store(true, Ordering::Release);
            Vec::new()
        }
    }
}
pub(crate) fn encoder_factory_query(
    factory: &RustVideoEncoderFactory,
    format: &ffi::FfiCodecFormat,
    scalability_mode: &str,
    has_resolution: bool,
    width: u32,
    height: u32,
) -> ffi::FfiCodecSupport {
    if factory.terminal.load(Ordering::Acquire) {
        return ffi::FfiCodecSupport {
            supported: false,
            power_efficient: false,
        };
    }
    let format = from_ffi_format_ref(format);
    match catch_unwind(AssertUnwindSafe(|| {
        factory.provider.query_support(
            &format,
            (!scalability_mode.is_empty()).then_some(scalability_mode),
            resolution(has_resolution, width, height),
        )
    })) {
        Ok(s) => ffi::FfiCodecSupport {
            supported: s.supported,
            power_efficient: s.power_efficient,
        },
        Err(_) => {
            factory.terminal.store(true, Ordering::Release);
            ffi::FfiCodecSupport {
                supported: false,
                power_efficient: false,
            }
        }
    }
}
pub(crate) fn encoder_factory_create(
    factory: &RustVideoEncoderFactory,
    format: &ffi::FfiCodecFormat,
) -> Box<RustVideoEncoder> {
    if factory.terminal.load(Ordering::Acquire) {
        return Box::new(RustVideoEncoder {
            codec: None,
            terminal: AtomicBool::new(true),
        });
    }
    let format = from_ffi_format_ref(format);
    match catch_unwind(AssertUnwindSafe(|| factory.provider.create(&format))) {
        Ok(Ok(codec)) => Box::new(RustVideoEncoder {
            codec: Some(codec),
            terminal: AtomicBool::new(false),
        }),
        Ok(Err(_)) => Box::new(RustVideoEncoder {
            codec: None,
            terminal: AtomicBool::new(false),
        }),
        Err(_) => {
            factory.terminal.store(true, Ordering::Release);
            Box::new(RustVideoEncoder {
                codec: None,
                terminal: AtomicBool::new(true),
            })
        }
    }
}
pub(crate) fn encoder_is_valid(encoder: &RustVideoEncoder) -> bool {
    encoder.codec.is_some() && !encoder.terminal.load(Ordering::Acquire)
}
fn with_encoder(
    encoder: &mut RustVideoEncoder,
    operation: impl FnOnce(&mut dyn VideoEncoder) -> Result<(), CodecError>,
) -> i32 {
    if encoder.terminal.load(Ordering::Acquire) {
        return CODEC_ERROR;
    }
    let Some(codec) = encoder.codec.as_deref_mut() else {
        return CODEC_UNINITIALIZED;
    };
    match catch_unwind(AssertUnwindSafe(|| operation(codec))) {
        Ok(result) => status(result),
        Err(_) => {
            encoder.terminal.store(true, Ordering::Release);
            CODEC_ERROR
        }
    }
}
pub(crate) fn encoder_init(encoder: &mut RustVideoEncoder, s: ffi::FfiEncoderSettings) -> i32 {
    with_encoder(encoder, |c| {
        c.initialize(VideoEncoderSettings {
            width: s.width,
            height: s.height,
            start_bitrate_bps: s.start_bitrate_bps,
            max_bitrate_bps: s.max_bitrate_bps,
            min_bitrate_bps: s.min_bitrate_bps,
            max_framerate: s.max_framerate,
            cores: s.cores,
            max_payload_size: s.max_payload_size,
        })
    })
}
pub(crate) fn encoder_register_callback(encoder: &mut RustVideoEncoder) -> i32 {
    with_encoder(encoder, |c| c.register_callback())
}
pub(crate) fn encoder_encode(
    encoder: &mut RustVideoEncoder,
    frame: cxx::UniquePtr<ffi::NativeVideoFrame>,
    frame_types: &[u8],
    callback: cxx::SharedPtr<ffi::NativeEncodedImageCallback>,
) -> i32 {
    let Some(native) = frame.as_ref() else {
        return CODEC_PARAMETER;
    };
    let frame = match VideoFrame::i420(
        ffi::native_video_frame_width(native),
        ffi::native_video_frame_height(native),
        ffi::native_video_frame_i420(native),
        ffi::native_video_frame_timestamp_us(native),
        ffi::native_video_frame_rtp_timestamp(native),
    ) {
        Ok(frame) => frame,
        Err(error) => return status(Err(error)),
    };
    let types: Vec<_> = frame_types
        .iter()
        .filter_map(|value| match value {
            3 => Some(VideoFrameType::Key),
            4 => Some(VideoFrameType::Delta),
            _ => None,
        })
        .collect();
    with_encoder(encoder, |c| {
        c.encode(frame, &types, EncodedImageCallback { native: callback })
    })
}
pub(crate) fn encoder_set_rates(encoder: &mut RustVideoEncoder, r: ffi::FfiRateControl) -> i32 {
    with_encoder(encoder, |c| {
        c.set_rates(VideoRateControl {
            bitrate_bps: r.bitrate_bps,
            framerate_fps: r.framerate_fps,
            bandwidth_bps: r.bandwidth_bps,
        })
    })
}
pub(crate) fn encoder_release(encoder: &mut RustVideoEncoder) -> i32 {
    let result = with_encoder(encoder, |c| c.release());
    encoder.codec = None;
    result
}
pub(crate) fn encoder_get_info(encoder: &RustVideoEncoder) -> ffi::FfiEncoderInfo {
    encoder
        .codec
        .as_deref()
        .and_then(|c| match catch_unwind(AssertUnwindSafe(|| c.info())) {
            Ok(info) => Some(info),
            Err(_) => {
                encoder.terminal.store(true, Ordering::Release);
                None
            }
        })
        .map_or_else(
            || ffi::FfiEncoderInfo {
                implementation_name: "failed Rust encoder".into(),
                hardware_accelerated: false,
                supports_native_handle: false,
            },
            |i| ffi::FfiEncoderInfo {
                implementation_name: i.implementation_name,
                hardware_accelerated: i.hardware_accelerated,
                supports_native_handle: i.supports_native_handle,
            },
        )
}

pub(crate) fn decoder_factory_formats(
    factory: &RustVideoDecoderFactory,
) -> Vec<ffi::FfiCodecFormat> {
    if factory.terminal.load(Ordering::Acquire) {
        return Vec::new();
    }
    match catch_unwind(AssertUnwindSafe(|| factory.provider.supported_formats())) {
        Ok(v) => v.iter().map(to_ffi_format).collect(),
        Err(_) => {
            factory.terminal.store(true, Ordering::Release);
            Vec::new()
        }
    }
}
pub(crate) fn decoder_factory_query(
    factory: &RustVideoDecoderFactory,
    format: &ffi::FfiCodecFormat,
    reference_scaling: bool,
    has_resolution: bool,
    width: u32,
    height: u32,
) -> ffi::FfiCodecSupport {
    if factory.terminal.load(Ordering::Acquire) {
        return ffi::FfiCodecSupport {
            supported: false,
            power_efficient: false,
        };
    }
    let format = from_ffi_format_ref(format);
    match catch_unwind(AssertUnwindSafe(|| {
        factory.provider.query_support(
            &format,
            reference_scaling,
            resolution(has_resolution, width, height),
        )
    })) {
        Ok(s) => ffi::FfiCodecSupport {
            supported: s.supported,
            power_efficient: s.power_efficient,
        },
        Err(_) => {
            factory.terminal.store(true, Ordering::Release);
            ffi::FfiCodecSupport {
                supported: false,
                power_efficient: false,
            }
        }
    }
}
pub(crate) fn decoder_factory_create(
    factory: &RustVideoDecoderFactory,
    format: &ffi::FfiCodecFormat,
) -> Box<RustVideoDecoder> {
    if factory.terminal.load(Ordering::Acquire) {
        return Box::new(RustVideoDecoder {
            codec: None,
            terminal: AtomicBool::new(true),
        });
    }
    let format = from_ffi_format_ref(format);
    match catch_unwind(AssertUnwindSafe(|| factory.provider.create(&format))) {
        Ok(Ok(codec)) => Box::new(RustVideoDecoder {
            codec: Some(codec),
            terminal: AtomicBool::new(false),
        }),
        Ok(Err(_)) => Box::new(RustVideoDecoder {
            codec: None,
            terminal: AtomicBool::new(false),
        }),
        Err(_) => {
            factory.terminal.store(true, Ordering::Release);
            Box::new(RustVideoDecoder {
                codec: None,
                terminal: AtomicBool::new(true),
            })
        }
    }
}
pub(crate) fn decoder_is_valid(decoder: &RustVideoDecoder) -> bool {
    decoder.codec.is_some() && !decoder.terminal.load(Ordering::Acquire)
}
fn with_decoder(
    decoder: &mut RustVideoDecoder,
    operation: impl FnOnce(&mut dyn VideoDecoder) -> Result<(), CodecError>,
) -> i32 {
    if decoder.terminal.load(Ordering::Acquire) {
        return CODEC_ERROR;
    }
    let Some(codec) = decoder.codec.as_deref_mut() else {
        return CODEC_UNINITIALIZED;
    };
    match catch_unwind(AssertUnwindSafe(|| operation(codec))) {
        Ok(result) => status(result),
        Err(_) => {
            decoder.terminal.store(true, Ordering::Release);
            CODEC_ERROR
        }
    }
}
pub(crate) fn decoder_configure(
    decoder: &mut RustVideoDecoder,
    s: ffi::FfiDecoderSettings,
) -> bool {
    with_decoder(decoder, |c| {
        c.configure(VideoDecoderSettings {
            cores: s.cores,
            max_resolution: resolution(
                s.max_width != 0 && s.max_height != 0,
                s.max_width,
                s.max_height,
            ),
        })
    }) == CODEC_OK
}
pub(crate) fn decoder_register_callback(decoder: &mut RustVideoDecoder) -> i32 {
    with_decoder(decoder, |c| c.register_callback())
}
pub(crate) fn decoder_decode(
    decoder: &mut RustVideoDecoder,
    frame: cxx::UniquePtr<ffi::NativeEncodedVideoFrame>,
    callback: cxx::SharedPtr<ffi::NativeDecodedImageCallback>,
) -> i32 {
    let Some(native) = frame.as_ref() else {
        return CODEC_PARAMETER;
    };
    let frame = EncodedVideoFrame {
        data: ffi::native_encoded_frame_data(native),
        width: ffi::native_encoded_frame_width(native),
        height: ffi::native_encoded_frame_height(native),
        rtp_timestamp: ffi::native_encoded_frame_rtp_timestamp(native),
        frame_type: if ffi::native_encoded_frame_key(native) {
            VideoFrameType::Key
        } else {
            VideoFrameType::Delta
        },
        qp: u8::try_from(ffi::native_encoded_frame_qp(native)).ok(),
    };
    with_decoder(decoder, |c| {
        c.decode(frame, DecodedImageCallback { native: callback })
    })
}
pub(crate) fn decoder_release(decoder: &mut RustVideoDecoder) -> i32 {
    let result = with_decoder(decoder, |c| c.release());
    decoder.codec = None;
    result
}
pub(crate) fn decoder_get_info(decoder: &RustVideoDecoder) -> ffi::FfiDecoderInfo {
    decoder
        .codec
        .as_deref()
        .and_then(|c| match catch_unwind(AssertUnwindSafe(|| c.info())) {
            Ok(info) => Some(info),
            Err(_) => {
                decoder.terminal.store(true, Ordering::Release);
                None
            }
        })
        .map_or_else(
            || ffi::FfiDecoderInfo {
                implementation_name: "failed Rust decoder".into(),
                hardware_accelerated: false,
            },
            |i| ffi::FfiDecoderInfo {
                implementation_name: i.implementation_name,
                hardware_accelerated: i.hardware_accelerated,
            },
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    const MASK: u8 = 0xa5;
    fn h264() -> VideoCodecFormat {
        VideoCodecFormat::new("H264").with_parameter("profile-level-id", "42e01f")
    }

    struct Factory {
        fail_create: bool,
        panic_create: bool,
        panic_encode: bool,
        saved: Arc<Mutex<Option<EncodedImageCallback>>>,
    }
    impl VideoEncoderFactory for Factory {
        fn supported_formats(&self) -> Vec<VideoCodecFormat> {
            vec![h264()]
        }
        fn query_support(
            &self,
            f: &VideoCodecFormat,
            _: Option<&str>,
            _: Option<VideoResolution>,
        ) -> CodecSupport {
            CodecSupport {
                supported: f.name.eq_ignore_ascii_case("H264"),
                power_efficient: false,
            }
        }
        fn create(&self, f: &VideoCodecFormat) -> Result<Box<dyn VideoEncoder>, CodecError> {
            if self.panic_create {
                panic!("test provider panic")
            }
            if self.fail_create || !f.name.eq_ignore_ascii_case("H264") {
                Err(CodecError::UnsupportedFormat)
            } else {
                Ok(Box::new(TestEncoder {
                    panic_encode: self.panic_encode,
                    saved: self.saved.clone(),
                }))
            }
        }
    }
    struct TestEncoder {
        panic_encode: bool,
        saved: Arc<Mutex<Option<EncodedImageCallback>>>,
    }
    impl VideoEncoder for TestEncoder {
        fn initialize(&mut self, _: VideoEncoderSettings) -> Result<(), CodecError> {
            Ok(())
        }
        fn encode(
            &mut self,
            frame: VideoFrame,
            types: &[VideoFrameType],
            callback: EncodedImageCallback,
        ) -> Result<(), CodecError> {
            if self.panic_encode {
                panic!("test encode panic")
            }
            *self.saved.lock().unwrap() = Some(callback.clone());
            let mut data = frame.buffer.into_bytes();
            for b in &mut data {
                *b ^= MASK;
            }
            callback.emit(&EncodedVideoFrame {
                data,
                width: frame.width,
                height: frame.height,
                rtp_timestamp: frame.rtp_timestamp,
                frame_type: types.first().copied().unwrap_or(VideoFrameType::Delta),
                qp: Some(23),
            })
        }
        fn set_rates(&mut self, _: VideoRateControl) -> Result<(), CodecError> {
            Ok(())
        }
        fn release(&mut self) -> Result<(), CodecError> {
            Ok(())
        }
        fn info(&self) -> VideoEncoderInfo {
            VideoEncoderInfo {
                implementation_name: "test-only reversible H264-shaped encoder".into(),
                hardware_accelerated: false,
                supports_native_handle: false,
            }
        }
    }
    struct DecoderFactory;
    impl VideoDecoderFactory for DecoderFactory {
        fn supported_formats(&self) -> Vec<VideoCodecFormat> {
            vec![h264()]
        }
        fn query_support(
            &self,
            f: &VideoCodecFormat,
            _: bool,
            _: Option<VideoResolution>,
        ) -> CodecSupport {
            CodecSupport {
                supported: f.name.eq_ignore_ascii_case("H264"),
                power_efficient: false,
            }
        }
        fn create(&self, f: &VideoCodecFormat) -> Result<Box<dyn VideoDecoder>, CodecError> {
            if f.name.eq_ignore_ascii_case("H264") {
                Ok(Box::new(TestDecoder))
            } else {
                Err(CodecError::UnsupportedFormat)
            }
        }
    }
    struct TestDecoder;
    impl VideoDecoder for TestDecoder {
        fn configure(&mut self, _: VideoDecoderSettings) -> Result<(), CodecError> {
            Ok(())
        }
        fn decode(
            &mut self,
            frame: EncodedVideoFrame,
            callback: DecodedImageCallback,
        ) -> Result<(), CodecError> {
            let mut data = frame.data;
            for b in &mut data {
                *b ^= MASK;
            }
            let decoded = VideoFrame::i420(
                frame.width,
                frame.height,
                data,
                i64::from(frame.rtp_timestamp) * 1000,
                frame.rtp_timestamp,
            )?;
            callback.emit(&decoded)
        }
        fn release(&mut self) -> Result<(), CodecError> {
            Ok(())
        }
        fn info(&self) -> VideoDecoderInfo {
            VideoDecoderInfo {
                implementation_name: "test-only reversible H264-shaped decoder".into(),
                hardware_accelerated: false,
            }
        }
    }

    #[test]
    fn real_virtual_factories_roundtrip_multiple_frames_and_release_callbacks() {
        let saved = Arc::new(Mutex::new(None));
        let encoder = VideoEncoderFactoryHandle::new(Factory {
            fail_create: false,
            panic_create: false,
            panic_encode: false,
            saved: saved.clone(),
        })
        .unwrap();
        let decoder = VideoDecoderFactoryHandle::new(DecoderFactory).unwrap();
        assert_eq!(encoder.supported_formats(), vec![h264()]);
        assert!(
            encoder
                .query_support(
                    &h264(),
                    None,
                    Some(VideoResolution {
                        width: 16,
                        height: 16
                    })
                )
                .supported
        );
        assert!(
            !encoder
                .query_support(&VideoCodecFormat::new("AV1"), None, None)
                .supported
        );
        assert!(AudioEncoderFactory::builtin().unwrap().is_available());
        assert!(AudioDecoderFactory::builtin().unwrap().is_available());
        let result = encoder.test_roundtrip(&decoder, 4);
        assert_eq!(
            (result.status, result.encoded_frames, result.decoded_frames),
            (0, 4, 4)
        );
        assert_ne!(result.checksum, 0);
        assert_eq!(
            saved
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .emit(&EncodedVideoFrame {
                    data: vec![1],
                    width: 1,
                    height: 1,
                    rtp_timestamp: 9,
                    frame_type: VideoFrameType::Delta,
                    qp: None
                }),
            Err(CodecError::Released)
        );
        assert!(encoder.test_cross_thread_lifecycle());
    }

    #[test]
    fn construction_failure_and_provider_panic_are_terminal_errors() {
        let saved = Arc::new(Mutex::new(None));
        let decoder = VideoDecoderFactoryHandle::new(DecoderFactory).unwrap();
        let failed = VideoEncoderFactoryHandle::new(Factory {
            fail_create: true,
            panic_create: false,
            panic_encode: false,
            saved: saved.clone(),
        })
        .unwrap();
        assert!(failed.test_roundtrip(&decoder, 1).status < 0);
        let panicked = VideoEncoderFactoryHandle::new(Factory {
            fail_create: false,
            panic_create: true,
            panic_encode: false,
            saved,
        })
        .unwrap();
        assert!(panicked.test_roundtrip(&decoder, 1).status < 0);
        assert!(panicked.supported_formats().is_empty());

        let panicked_during_encode = VideoEncoderFactoryHandle::new(Factory {
            fail_create: false,
            panic_create: false,
            panic_encode: true,
            saved: Arc::new(Mutex::new(None)),
        })
        .unwrap();
        assert!(panicked_during_encode.test_roundtrip(&decoder, 2).status < 0);
    }

    #[test]
    fn invalid_i420_buffers_are_rejected() {
        assert_eq!(
            VideoFrameBuffer::i420(4, 4, vec![0; 23]),
            Err(CodecError::InvalidFrame)
        );
    }

    #[test]
    fn factory_and_callback_handles_have_justified_thread_traits() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<VideoEncoderFactoryHandle>();
        assert_send_sync::<VideoDecoderFactoryHandle>();
        assert_send_sync::<AudioEncoderFactory>();
        assert_send_sync::<AudioDecoderFactory>();
        assert_send_sync::<EncodedImageCallback>();
        assert_send_sync::<DecodedImageCallback>();
    }
}
