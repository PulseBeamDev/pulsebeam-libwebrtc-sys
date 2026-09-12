#include "pulsebeam-webrtc-sys/native/codec.h"

#include <algorithm>
#include <cstring>
#include <map>
#include <mutex>
#include <optional>
#include <string>
#include <thread>
#include <utility>
#include <vector>

#include "api/audio_codecs/builtin_audio_decoder_factory.h"
#include "api/audio_codecs/builtin_audio_encoder_factory.h"
#include "api/environment/environment_factory.h"
#include "api/scoped_refptr.h"
#include "api/units/data_rate.h"
#include "api/video/encoded_image.h"
#include "api/video/i420_buffer.h"
#include "api/video/video_frame.h"
#include "api/video/video_frame_type.h"
#include "api/video_codecs/sdp_video_format.h"
#include "api/video_codecs/video_codec.h"
#include "api/video_codecs/video_decoder.h"
#include "api/video_codecs/video_decoder_factory.h"
#include "api/video_codecs/video_encoder.h"
#include "api/video_codecs/video_encoder_factory.h"
#include "modules/video_coding/include/video_error_codes.h"
#include "pulsebeam-webrtc-sys/src/lib.rs.h"

namespace pulsebeam::webrtc_sys {

struct NativeVideoFrame::State {
  std::uint32_t width = 0, height = 0, rtp_timestamp = 0;
  std::int64_t timestamp_us = 0;
  std::vector<std::uint8_t> i420;
};
struct NativeEncodedVideoFrame::State {
  std::uint32_t width = 0, height = 0, rtp_timestamp = 0;
  bool key_frame = false;
  std::int32_t qp = -1;
  std::vector<std::uint8_t> data;
};
struct NativeEncodedImageCallback::State {
  std::mutex mutex;
  webrtc::EncodedImageCallback *callback = nullptr;
  bool active = false;
};
struct NativeDecodedImageCallback::State {
  std::mutex mutex;
  webrtc::DecodedImageCallback *callback = nullptr;
  bool active = false;
};
struct NativeAudioEncoderFactory::State {
  webrtc::scoped_refptr<webrtc::AudioEncoderFactory> factory;
};
struct NativeAudioDecoderFactory::State {
  webrtc::scoped_refptr<webrtc::AudioDecoderFactory> factory;
};

namespace {

webrtc::SdpVideoFormat ToNative(const FfiCodecFormat &format) {
  std::map<std::string, std::string> parameters;
  for (const auto &parameter : format.parameters) {
    parameters.emplace(std::string(parameter.key),
                       std::string(parameter.value));
  }
  return webrtc::SdpVideoFormat(std::string(format.name),
                                std::move(parameters));
}

FfiCodecFormat FromNative(const webrtc::SdpVideoFormat &format) {
  FfiCodecFormat result;
  result.name = format.name;
  result.parameters.reserve(format.parameters.size());
  for (const auto &[key, value] : format.parameters) {
    result.parameters.push_back(FfiCodecParameter{key, value});
  }
  return result;
}

std::optional<webrtc::Resolution> Resolution(bool present, std::uint32_t width,
                                             std::uint32_t height) {
  if (!present || width == 0 || height == 0) {
    return std::nullopt;
  }
  return webrtc::Resolution{static_cast<int>(width), static_cast<int>(height)};
}

rust::Vec<std::uint8_t> ToRust(const std::vector<std::uint8_t> &data) {
  rust::Vec<std::uint8_t> result;
  result.reserve(data.size());
  for (std::uint8_t value : data) {
    result.push_back(value);
  }
  return result;
}

std::vector<std::uint8_t> CopyI420(const webrtc::VideoFrame &frame) {
  auto buffer = frame.video_frame_buffer()->ToI420();
  const int width = buffer->width();
  const int height = buffer->height();
  const int chroma_width = (width + 1) / 2;
  const int chroma_height = (height + 1) / 2;
  std::vector<std::uint8_t> result;
  result.reserve(static_cast<std::size_t>(width) * height +
                 2 * static_cast<std::size_t>(chroma_width) * chroma_height);
  auto append_plane = [&](const std::uint8_t *source, int stride, int row_size,
                          int rows) {
    for (int row = 0; row < rows; ++row) {
      result.insert(result.end(), source + row * stride,
                    source + row * stride + row_size);
    }
  };
  append_plane(buffer->DataY(), buffer->StrideY(), width, height);
  append_plane(buffer->DataU(), buffer->StrideU(), chroma_width, chroma_height);
  append_plane(buffer->DataV(), buffer->StrideV(), chroma_width, chroma_height);
  return result;
}

class RustEncoder final : public webrtc::VideoEncoder {
public:
  explicit RustEncoder(rust::Box<RustVideoEncoder> encoder) noexcept
      : encoder_(std::move(encoder)),
        callback_(std::make_shared<NativeEncodedImageCallback>(
            std::make_shared<NativeEncodedImageCallback::State>())) {}
  ~RustEncoder() override { Release(); }

  int InitEncode(const webrtc::VideoCodec *codec,
                 const Settings &settings) override {
    if (codec == nullptr || released_)
      return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
    std::lock_guard lock(mutex_);
    return encoder_init(
        *encoder_, FfiEncoderSettings{
                       static_cast<std::uint32_t>(codec->width),
                       static_cast<std::uint32_t>(codec->height),
                       codec->startBitrate * 1000, codec->maxBitrate * 1000,
                       codec->minBitrate * 1000, codec->maxFramerate,
                       static_cast<std::uint32_t>(settings.number_of_cores),
                       static_cast<std::uint32_t>(settings.max_payload_size)});
  }

  int32_t RegisterEncodeCompleteCallback(
      webrtc::EncodedImageCallback *callback) override;

  int32_t Release() override;

  int32_t
  Encode(const webrtc::VideoFrame &frame,
         const std::vector<webrtc::VideoFrameType> *frame_types) override {
    std::lock_guard lock(mutex_);
    if (released_)
      return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
    auto state = std::make_unique<NativeVideoFrame::State>();
    state->width = static_cast<std::uint32_t>(frame.width());
    state->height = static_cast<std::uint32_t>(frame.height());
    state->timestamp_us = frame.timestamp_us();
    state->rtp_timestamp = frame.rtp_timestamp();
    state->i420 = CopyI420(frame);
    std::vector<std::uint8_t> types;
    if (frame_types != nullptr) {
      for (auto type : *frame_types)
        types.push_back(static_cast<std::uint8_t>(type));
    }
    return encoder_encode(
        *encoder_, std::make_unique<NativeVideoFrame>(std::move(state)),
        rust::Slice<const std::uint8_t>(types.data(), types.size()), callback_);
  }

  void SetRates(const RateControlParameters &parameters) override {
    std::lock_guard lock(mutex_);
    if (!released_) {
      encoder_set_rates(
          *encoder_,
          FfiRateControl{parameters.bitrate.get_sum_bps(),
                         parameters.framerate_fps,
                         static_cast<std::uint64_t>(
                             parameters.bandwidth_allocation.bps())});
    }
  }

  EncoderInfo GetEncoderInfo() const override {
    std::lock_guard lock(mutex_);
    const auto info = encoder_get_info(*encoder_);
    EncoderInfo result;
    result.implementation_name = std::string(info.implementation_name);
    result.is_hardware_accelerated = info.hardware_accelerated;
    result.supports_native_handle = info.supports_native_handle;
    return result;
  }

private:
  mutable std::mutex mutex_;
  rust::Box<RustVideoEncoder> encoder_;
  std::shared_ptr<NativeEncodedImageCallback> callback_;
  bool released_ = false;
};

class RustDecoder final : public webrtc::VideoDecoder {
public:
  explicit RustDecoder(rust::Box<RustVideoDecoder> decoder) noexcept
      : decoder_(std::move(decoder)),
        callback_(std::make_shared<NativeDecodedImageCallback>(
            std::make_shared<NativeDecodedImageCallback::State>())) {}
  ~RustDecoder() override { Release(); }

  bool Configure(const Settings &settings) override {
    std::lock_guard lock(mutex_);
    if (released_)
      return false;
    const auto max = settings.max_render_resolution();
    return decoder_configure(
        *decoder_, FfiDecoderSettings{
                       static_cast<std::uint32_t>(settings.number_of_cores()),
                       static_cast<std::uint32_t>(std::max(0, max.Width())),
                       static_cast<std::uint32_t>(std::max(0, max.Height()))});
  }

  int32_t RegisterDecodeCompleteCallback(
      webrtc::DecodedImageCallback *callback) override;
  int32_t Release() override;

  int32_t Decode(const webrtc::EncodedImage &image, int64_t) override {
    std::lock_guard lock(mutex_);
    if (released_)
      return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
    auto state = std::make_unique<NativeEncodedVideoFrame::State>();
    state->width = image._encodedWidth;
    state->height = image._encodedHeight;
    state->rtp_timestamp = image.RtpTimestamp();
    state->key_frame =
        image.FrameType() == webrtc::VideoFrameType::kVideoFrameKey;
    state->qp = image.qp_;
    state->data.assign(image.data(), image.data() + image.size());
    return decoder_decode(
        *decoder_, std::make_unique<NativeEncodedVideoFrame>(std::move(state)),
        callback_);
  }

  DecoderInfo GetDecoderInfo() const override {
    std::lock_guard lock(mutex_);
    const auto info = decoder_get_info(*decoder_);
    return DecoderInfo{std::string(info.implementation_name),
                       info.hardware_accelerated};
  }

private:
  mutable std::mutex mutex_;
  rust::Box<RustVideoDecoder> decoder_;
  std::shared_ptr<NativeDecodedImageCallback> callback_;
  bool released_ = false;
};

class RustEncoderFactory final : public webrtc::VideoEncoderFactory {
public:
  explicit RustEncoderFactory(rust::Box<RustVideoEncoderFactory> factory)
      : factory_(std::move(factory)) {}
  std::vector<webrtc::SdpVideoFormat> GetSupportedFormats() const override {
    std::vector<webrtc::SdpVideoFormat> result;
    for (const auto &format : encoder_factory_formats(*factory_))
      result.push_back(ToNative(format));
    return result;
  }
  CodecSupport QueryCodecSupport(
      const webrtc::SdpVideoFormat &format,
      std::optional<std::string> scalability_mode,
      std::optional<webrtc::Resolution> resolution) const override {
    auto native = FromNative(format);
    auto support = encoder_factory_query(
        *factory_, native, scalability_mode.value_or(""),
        resolution.has_value(), resolution ? resolution->width : 0,
        resolution ? resolution->height : 0);
    return {support.supported, support.power_efficient};
  }
  std::unique_ptr<webrtc::VideoEncoder>
  Create(const webrtc::Environment &,
         const webrtc::SdpVideoFormat &format) override {
    auto encoder = encoder_factory_create(*factory_, FromNative(format));
    if (!encoder_is_valid(*encoder))
      return nullptr;
    return std::make_unique<RustEncoder>(std::move(encoder));
  }

private:
  rust::Box<RustVideoEncoderFactory> factory_;
};

class RustDecoderFactory final : public webrtc::VideoDecoderFactory {
public:
  explicit RustDecoderFactory(rust::Box<RustVideoDecoderFactory> factory)
      : factory_(std::move(factory)) {}
  std::vector<webrtc::SdpVideoFormat> GetSupportedFormats() const override {
    std::vector<webrtc::SdpVideoFormat> result;
    for (const auto &format : decoder_factory_formats(*factory_))
      result.push_back(ToNative(format));
    return result;
  }
  CodecSupport QueryCodecSupport(
      const webrtc::SdpVideoFormat &format, bool reference_scaling,
      std::optional<webrtc::Resolution> resolution) const override {
    auto native = FromNative(format);
    auto support = decoder_factory_query(*factory_, native, reference_scaling,
                                         resolution.has_value(),
                                         resolution ? resolution->width : 0,
                                         resolution ? resolution->height : 0);
    return {support.supported, support.power_efficient};
  }
  std::unique_ptr<webrtc::VideoDecoder>
  Create(const webrtc::Environment &,
         const webrtc::SdpVideoFormat &format) override {
    auto decoder = decoder_factory_create(*factory_, FromNative(format));
    if (!decoder_is_valid(*decoder))
      return nullptr;
    return std::make_unique<RustDecoder>(std::move(decoder));
  }

private:
  rust::Box<RustVideoDecoderFactory> factory_;
};

class EncodedCollector final : public webrtc::EncodedImageCallback {
public:
  Result OnEncodedImage(const webrtc::EncodedImage &image,
                        const webrtc::CodecSpecificInfo *) override {
    images.push_back(image);
    return Result(Result::OK, image.RtpTimestamp());
  }
  void OnFrameDropped(uint32_t, int, bool) override {}
  std::vector<webrtc::EncodedImage> images;
};

class DecodedCollector final : public webrtc::DecodedImageCallback {
public:
  int32_t Decoded(webrtc::VideoFrame &frame) override {
    ++count;
    for (std::uint8_t value : CopyI420(frame))
      checksum = checksum * 131 + value;
    return 0;
  }
  std::uint32_t count = 0;
  std::uint64_t checksum = 0;
};

} // namespace

NativeVideoFrame::NativeVideoFrame(std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeVideoFrame::~NativeVideoFrame() = default;
const NativeVideoFrame::State &NativeVideoFrame::state() const noexcept {
  return *state_;
}
NativeEncodedVideoFrame::NativeEncodedVideoFrame(
    std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeEncodedVideoFrame::~NativeEncodedVideoFrame() = default;
const NativeEncodedVideoFrame::State &
NativeEncodedVideoFrame::state() const noexcept {
  return *state_;
}
NativeEncodedImageCallback::NativeEncodedImageCallback(
    std::shared_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeEncodedImageCallback::~NativeEncodedImageCallback() = default;
const std::shared_ptr<NativeEncodedImageCallback::State> &
NativeEncodedImageCallback::state() const noexcept {
  return state_;
}
NativeDecodedImageCallback::NativeDecodedImageCallback(
    std::shared_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeDecodedImageCallback::~NativeDecodedImageCallback() = default;
const std::shared_ptr<NativeDecodedImageCallback::State> &
NativeDecodedImageCallback::state() const noexcept {
  return state_;
}

int32_t RustEncoder::RegisterEncodeCompleteCallback(
    webrtc::EncodedImageCallback *callback) {
  std::lock_guard lock(mutex_);
  if (released_)
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  {
    std::lock_guard callback_lock(callback_->state()->mutex);
    callback_->state()->callback = callback;
    callback_->state()->active = callback != nullptr;
  }
  return encoder_register_callback(*encoder_);
}
int32_t RustEncoder::Release() {
  std::lock_guard lock(mutex_);
  if (released_)
    return WEBRTC_VIDEO_CODEC_OK;
  {
    std::lock_guard callback_lock(callback_->state()->mutex);
    callback_->state()->active = false;
    callback_->state()->callback = nullptr;
  }
  released_ = true;
  return encoder_release(*encoder_);
}
int32_t RustDecoder::RegisterDecodeCompleteCallback(
    webrtc::DecodedImageCallback *callback) {
  std::lock_guard lock(mutex_);
  if (released_)
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  {
    std::lock_guard callback_lock(callback_->state()->mutex);
    callback_->state()->callback = callback;
    callback_->state()->active = callback != nullptr;
  }
  return decoder_register_callback(*decoder_);
}
int32_t RustDecoder::Release() {
  std::lock_guard lock(mutex_);
  if (released_)
    return WEBRTC_VIDEO_CODEC_OK;
  {
    std::lock_guard callback_lock(callback_->state()->mutex);
    callback_->state()->active = false;
    callback_->state()->callback = nullptr;
  }
  released_ = true;
  return decoder_release(*decoder_);
}

NativeVideoEncoderFactory::NativeVideoEncoderFactory(
    std::unique_ptr<webrtc::VideoEncoderFactory> factory) noexcept
    : factory_(std::move(factory)) {}
NativeVideoEncoderFactory::~NativeVideoEncoderFactory() = default;
webrtc::VideoEncoderFactory &
NativeVideoEncoderFactory::factory() const noexcept {
  return *factory_;
}
NativeVideoDecoderFactory::NativeVideoDecoderFactory(
    std::unique_ptr<webrtc::VideoDecoderFactory> factory) noexcept
    : factory_(std::move(factory)) {}
NativeVideoDecoderFactory::~NativeVideoDecoderFactory() = default;
webrtc::VideoDecoderFactory &
NativeVideoDecoderFactory::factory() const noexcept {
  return *factory_;
}
NativeAudioEncoderFactory::NativeAudioEncoderFactory(
    std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeAudioEncoderFactory::~NativeAudioEncoderFactory() = default;
const NativeAudioEncoderFactory::State &
NativeAudioEncoderFactory::state() const noexcept {
  return *state_;
}
webrtc::scoped_refptr<webrtc::AudioEncoderFactory>
NativeAudioEncoderFactory::factory() const noexcept {
  return state_->factory;
}
NativeAudioDecoderFactory::NativeAudioDecoderFactory(
    std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeAudioDecoderFactory::~NativeAudioDecoderFactory() = default;
const NativeAudioDecoderFactory::State &
NativeAudioDecoderFactory::state() const noexcept {
  return *state_;
}
webrtc::scoped_refptr<webrtc::AudioDecoderFactory>
NativeAudioDecoderFactory::factory() const noexcept {
  return state_->factory;
}

std::unique_ptr<NativeVideoEncoderFactory>
new_video_encoder_factory(rust::Box<RustVideoEncoderFactory> factory) noexcept {
  return std::make_unique<NativeVideoEncoderFactory>(
      std::make_unique<RustEncoderFactory>(std::move(factory)));
}
std::unique_ptr<NativeVideoDecoderFactory>
new_video_decoder_factory(rust::Box<RustVideoDecoderFactory> factory) noexcept {
  return std::make_unique<NativeVideoDecoderFactory>(
      std::make_unique<RustDecoderFactory>(std::move(factory)));
}
std::unique_ptr<NativeAudioEncoderFactory>
new_builtin_audio_encoder_factory() noexcept {
  auto state = std::make_unique<NativeAudioEncoderFactory::State>();
  state->factory = webrtc::CreateBuiltinAudioEncoderFactory();
  return state->factory
             ? std::make_unique<NativeAudioEncoderFactory>(std::move(state))
             : nullptr;
}
std::unique_ptr<NativeAudioDecoderFactory>
new_builtin_audio_decoder_factory() noexcept {
  auto state = std::make_unique<NativeAudioDecoderFactory::State>();
  state->factory = webrtc::CreateBuiltinAudioDecoderFactory();
  return state->factory
             ? std::make_unique<NativeAudioDecoderFactory>(std::move(state))
             : nullptr;
}

rust::Vec<FfiCodecFormat>
video_encoder_formats(const NativeVideoEncoderFactory &factory) noexcept {
  rust::Vec<FfiCodecFormat> result;
  for (const auto &f : factory.factory().GetSupportedFormats())
    result.push_back(FromNative(f));
  return result;
}
FfiCodecSupport video_encoder_query(const NativeVideoEncoderFactory &factory,
                                    const FfiCodecFormat &format,
                                    rust::Str mode, bool has,
                                    std::uint32_t width,
                                    std::uint32_t height) noexcept {
  auto support = factory.factory().QueryCodecSupport(
      ToNative(format),
      mode.empty() ? std::nullopt
                   : std::optional<std::string>(std::string(mode)),
      Resolution(has, width, height));
  return {support.is_supported, support.is_power_efficient};
}
rust::Vec<FfiCodecFormat>
video_decoder_formats(const NativeVideoDecoderFactory &factory) noexcept {
  rust::Vec<FfiCodecFormat> result;
  for (const auto &f : factory.factory().GetSupportedFormats())
    result.push_back(FromNative(f));
  return result;
}
FfiCodecSupport video_decoder_query(const NativeVideoDecoderFactory &factory,
                                    const FfiCodecFormat &format,
                                    bool reference_scaling, bool has,
                                    std::uint32_t width,
                                    std::uint32_t height) noexcept {
  auto support = factory.factory().QueryCodecSupport(
      ToNative(format), reference_scaling, Resolution(has, width, height));
  return {support.is_supported, support.is_power_efficient};
}

std::uint32_t native_video_frame_width(const NativeVideoFrame &frame) noexcept {
  return frame.state().width;
}
std::uint32_t
native_video_frame_height(const NativeVideoFrame &frame) noexcept {
  return frame.state().height;
}
std::int64_t
native_video_frame_timestamp_us(const NativeVideoFrame &frame) noexcept {
  return frame.state().timestamp_us;
}
std::uint32_t
native_video_frame_rtp_timestamp(const NativeVideoFrame &frame) noexcept {
  return frame.state().rtp_timestamp;
}
rust::Vec<std::uint8_t>
native_video_frame_i420(const NativeVideoFrame &frame) noexcept {
  return ToRust(frame.state().i420);
}
std::uint32_t
native_encoded_frame_width(const NativeEncodedVideoFrame &frame) noexcept {
  return frame.state().width;
}
std::uint32_t
native_encoded_frame_height(const NativeEncodedVideoFrame &frame) noexcept {
  return frame.state().height;
}
std::uint32_t native_encoded_frame_rtp_timestamp(
    const NativeEncodedVideoFrame &frame) noexcept {
  return frame.state().rtp_timestamp;
}
bool native_encoded_frame_key(const NativeEncodedVideoFrame &frame) noexcept {
  return frame.state().key_frame;
}
std::int32_t
native_encoded_frame_qp(const NativeEncodedVideoFrame &frame) noexcept {
  return frame.state().qp;
}
rust::Vec<std::uint8_t>
native_encoded_frame_data(const NativeEncodedVideoFrame &frame) noexcept {
  return ToRust(frame.state().data);
}

bool encoded_callback_emit(const NativeEncodedImageCallback &callback,
                           rust::Slice<const std::uint8_t> data,
                           std::uint32_t width, std::uint32_t height,
                           std::uint32_t rtp_timestamp, bool key_frame,
                           std::int32_t qp) noexcept {
  auto state = callback.state();
  std::lock_guard lock(state->mutex);
  if (!state->active || state->callback == nullptr)
    return false;
  webrtc::EncodedImage image;
  image.SetEncodedData(
      webrtc::EncodedImageBuffer::Create(data.data(), data.size()));
  image._encodedWidth = width;
  image._encodedHeight = height;
  image.SetRtpTimestamp(rtp_timestamp);
  image.SetFrameType(key_frame ? webrtc::VideoFrameType::kVideoFrameKey
                               : webrtc::VideoFrameType::kVideoFrameDelta);
  image.qp_ = qp;
  return state->callback->OnEncodedImage(image, nullptr).error ==
         webrtc::EncodedImageCallback::Result::OK;
}
bool decoded_callback_emit(const NativeDecodedImageCallback &callback,
                           rust::Slice<const std::uint8_t> data,
                           std::uint32_t width, std::uint32_t height,
                           std::int64_t timestamp_us,
                           std::uint32_t rtp_timestamp) noexcept {
  const std::size_t y = static_cast<std::size_t>(width) * height;
  const std::uint32_t cw = (width + 1) / 2, ch = (height + 1) / 2;
  const std::size_t c = static_cast<std::size_t>(cw) * ch;
  if (width == 0 || height == 0 || data.size() != y + 2 * c)
    return false;
  auto state = callback.state();
  std::lock_guard lock(state->mutex);
  if (!state->active || state->callback == nullptr)
    return false;
  auto buffer =
      webrtc::I420Buffer::Copy(width, height, data.data(), width,
                               data.data() + y, cw, data.data() + y + c, cw);
  auto frame = webrtc::VideoFrame::Builder()
                   .set_video_frame_buffer(buffer)
                   .set_timestamp_us(timestamp_us)
                   .set_rtp_timestamp(rtp_timestamp)
                   .build();
  return state->callback->Decoded(frame) == 0;
}

FfiCodecTestResult
test_codec_roundtrip(const NativeVideoEncoderFactory &encoder_factory,
                     const NativeVideoDecoderFactory &decoder_factory,
                     std::uint32_t frames) noexcept {
  FfiCodecTestResult result{WEBRTC_VIDEO_CODEC_ERROR, 0, 0, 0};
  webrtc::Environment env = webrtc::EnvironmentFactory().Create();
  webrtc::SdpVideoFormat format("H264", {{"profile-level-id", "42e01f"}});
  auto encoder = encoder_factory.factory().Create(env, format);
  auto decoder = decoder_factory.factory().Create(env, format);
  if (!encoder || !decoder)
    return result;
  webrtc::VideoCodec codec{};
  codec.codecType = webrtc::kVideoCodecH264;
  codec.width = 16;
  codec.height = 16;
  codec.startBitrate = 64;
  codec.minBitrate = 16;
  codec.maxBitrate = 256;
  codec.maxFramerate = 30;
  webrtc::VideoEncoder::Settings encoder_settings(
      webrtc::VideoEncoder::Capabilities(false), 1, 1200);
  webrtc::VideoDecoder::Settings decoder_settings;
  decoder_settings.set_codec_type(webrtc::kVideoCodecH264);
  decoder_settings.set_number_of_cores(1);
  decoder_settings.set_max_render_resolution({16, 16});
  EncodedCollector encoded;
  DecodedCollector decoded;
  if (encoder->InitEncode(&codec, encoder_settings) != 0 ||
      encoder->RegisterEncodeCompleteCallback(&encoded) != 0 ||
      !decoder->Configure(decoder_settings) ||
      decoder->RegisterDecodeCompleteCallback(&decoded) != 0)
    return result;
  webrtc::VideoBitrateAllocation allocation;
  allocation.SetBitrate(0, 0, 64000);
  encoder->SetRates({allocation, 30.0, webrtc::DataRate::BitsPerSec(64000)});
  for (std::uint32_t index = 0; index < frames; ++index) {
    auto buffer = webrtc::I420Buffer::Create(16, 16);
    std::memset(buffer->MutableDataY(), static_cast<int>(index + 1), 16 * 16);
    std::memset(buffer->MutableDataU(), 64, 8 * 8);
    std::memset(buffer->MutableDataV(), 192, 8 * 8);
    auto frame = webrtc::VideoFrame::Builder()
                     .set_video_frame_buffer(buffer)
                     .set_timestamp_us(index * 1000)
                     .set_rtp_timestamp(9000 + index)
                     .build();
    std::vector<webrtc::VideoFrameType> types{
        index == 0 ? webrtc::VideoFrameType::kVideoFrameKey
                   : webrtc::VideoFrameType::kVideoFrameDelta};
    if (encoder->Encode(frame, &types) != 0)
      break;
  }
  result.encoded_frames = encoded.images.size();
  for (const auto &image : encoded.images) {
    if (decoder->Decode(image, 0) != 0)
      break;
  }
  result.decoded_frames = decoded.count;
  result.checksum = decoded.checksum;
  const int encoder_release = encoder->Release();
  const int decoder_release = decoder->Release();
  result.status =
      (encoder_release == 0 && decoder_release == 0 &&
       result.encoded_frames == frames && result.decoded_frames == frames)
          ? 0
          : WEBRTC_VIDEO_CODEC_ERROR;
  return result;
}
bool test_encoder_factory_cross_thread(
    const NativeVideoEncoderFactory &factory) noexcept {
  std::unique_ptr<webrtc::VideoEncoder> encoder;
  std::thread worker([&] {
    auto formats = factory.factory().GetSupportedFormats();
    if (formats.empty())
      return;
    webrtc::Environment env = webrtc::EnvironmentFactory().Create();
    encoder = factory.factory().Create(env, formats.front());
  });
  worker.join();
  const bool created = encoder != nullptr;
  encoder.reset();
  return created;
}

} // namespace pulsebeam::webrtc_sys
