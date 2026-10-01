#include "pulsebeam-webrtc-sys/native/codec.h"

#include <algorithm>
#include <cstring>
#include <limits>
#include <map>
#include <mutex>
#include <optional>
#include <string>
#include <thread>
#include <utility>
#include <vector>

#include "absl/strings/match.h"
#include "api/audio_codecs/audio_decoder.h"
#include "api/audio_codecs/audio_decoder_factory.h"
#include "api/audio_codecs/audio_encoder_factory.h"
#include "api/audio_codecs/builtin_audio_decoder_factory.h"
#include "api/audio_codecs/builtin_audio_encoder_factory.h"
#include "api/environment/environment_factory.h"
#include "api/scoped_refptr.h"
#include "api/units/data_rate.h"
#include "api/video/encoded_image.h"
#include "api/video/i420_buffer.h"
#include "api/video/video_frame.h"
#include "api/video/video_frame_type.h"
#include "api/video_codecs/scalability_mode.h"
#include "api/video_codecs/scalability_mode_helper.h"
#include "api/video_codecs/sdp_video_format.h"
#include "api/video_codecs/video_codec.h"
#include "api/video_codecs/video_decoder.h"
#include "api/video_codecs/video_decoder_factory.h"
#include "api/video_codecs/video_encoder.h"
#include "api/video_codecs/video_encoder_factory.h"
#include "modules/video_coding/codecs/vp8/include/vp8.h"
#include "modules/video_coding/include/video_codec_interface.h"
#include "modules/video_coding/include/video_error_codes.h"
#include "media/engine/simulcast_encoder_adapter.h"
#include "pulsebeam-webrtc-sys/src/lib.rs.h"
#include "pulsebeam-webrtc-sys/native/opus_carrier.h"

namespace pulsebeam::webrtc_sys {

struct CodecDecoderObservations {
  void Hit(std::uint64_t FfiDecoderStatistics::*field) {
    std::lock_guard lock(mutex);
    ObserveLocked();
    ++(value.*field);
  }
  void Observe() {
    std::lock_guard lock(mutex);
    ObserveLocked();
  }
  void Input(const std::uint8_t* data, std::size_t size) {
    std::lock_guard lock(mutex);
    ObserveLocked();
    ++value.input_packets;
    std::uint64_t hash = 14695981039346656037ULL;
    for (std::size_t i = 0; i < size; ++i) hash = (hash ^ data[i]) * 1099511628211ULL;
    value.last_input_hash = hash;
  }
  FfiDecoderStatistics Snapshot() const {
    std::lock_guard lock(mutex);
    return value;
  }
 private:
  void ObserveLocked() {
    const auto current = std::this_thread::get_id();
    if (!thread) {
      thread = current;
      value.thread_token = native_codec_thread_token();
    } else if (*thread != current) {
      ++value.thread_mismatches;
    }
  }
  mutable std::mutex mutex;
  std::optional<std::thread::id> thread;
  FfiDecoderStatistics value{};
};

namespace {
class ObservedVp8Decoder final : public webrtc::VideoDecoder {
 public:
  ObservedVp8Decoder(std::unique_ptr<webrtc::VideoDecoder> decoder,
                    std::shared_ptr<CodecDecoderObservations> observations)
      : callback_(observations), decoder_(std::move(decoder)),
        observations_(std::move(observations)) {}
  bool Configure(const Settings& settings) override {
    observations_->Hit(&FfiDecoderStatistics::configure_calls);
    Settings serial = settings;
    serial.set_number_of_cores(1);
    const bool result = decoder_->Configure(serial);
    if (!result) observations_->Hit(&FfiDecoderStatistics::decode_errors);
    return result;
  }
  std::int32_t Decode(const webrtc::EncodedImage& image,
                      std::int64_t render_time_ms) override {
    observations_->Hit(&FfiDecoderStatistics::decode_calls);
    observations_->Input(image.data(), image.size());
    const auto result = decoder_->Decode(image, render_time_ms);
    if (result < 0) observations_->Hit(&FfiDecoderStatistics::decode_errors);
    return result;
  }
  std::int32_t Decode(const webrtc::EncodedImage& image, bool missing,
                      std::int64_t render_time_ms) override {
    observations_->Hit(&FfiDecoderStatistics::decode_calls);
    observations_->Input(image.data(), image.size());
    const auto result = decoder_->Decode(image, missing, render_time_ms);
    if (result < 0) observations_->Hit(&FfiDecoderStatistics::decode_errors);
    return result;
  }
  std::int32_t RegisterDecodeCompleteCallback(
      webrtc::DecodedImageCallback* callback) override {
    observations_->Observe();
    callback_.target = callback;
    return decoder_->RegisterDecodeCompleteCallback(callback ? &callback_ : nullptr);
  }
  std::int32_t Release() override {
    observations_->Observe();
    decoder_->RegisterDecodeCompleteCallback(nullptr);
    callback_.target = nullptr;
    return decoder_->Release();
  }
  DecoderInfo GetDecoderInfo() const override { return decoder_->GetDecoderInfo(); }
  const char* ImplementationName() const override { return decoder_->ImplementationName(); }
 private:
  class Callback final : public webrtc::DecodedImageCallback {
   public:
    explicit Callback(std::shared_ptr<CodecDecoderObservations> observations)
        : observations_(std::move(observations)) {}
    std::int32_t Decoded(webrtc::VideoFrame& frame) override {
      observations_->Hit(&FfiDecoderStatistics::decoded_outputs);
      if (!target) return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
      const auto result = target->Decoded(frame);
      if (result < 0) observations_->Hit(&FfiDecoderStatistics::decode_errors);
      return result;
    }
    std::int32_t Decoded(webrtc::VideoFrame& frame, std::int64_t time_ms) override {
      observations_->Hit(&FfiDecoderStatistics::decoded_outputs);
      if (!target) return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
      const auto result = target->Decoded(frame, time_ms);
      if (result < 0) observations_->Hit(&FfiDecoderStatistics::decode_errors);
      return result;
    }
    void Decoded(webrtc::VideoFrame& frame, std::optional<std::int32_t> time_ms,
                 std::optional<std::uint8_t> qp) override {
      observations_->Hit(&FfiDecoderStatistics::decoded_outputs);
      if (target) target->Decoded(frame, time_ms, qp);
    }
    webrtc::DecodedImageCallback* target = nullptr;
   private:
    std::shared_ptr<CodecDecoderObservations> observations_;
  };
  // Keep the callback proxy alive while the underlying decoder is destroyed.
  Callback callback_;
  std::unique_ptr<webrtc::VideoDecoder> decoder_;
  std::shared_ptr<CodecDecoderObservations> observations_;
};

class ObservedVp8Factory final : public webrtc::VideoDecoderFactory {
 public:
  explicit ObservedVp8Factory(std::shared_ptr<CodecDecoderObservations> observations)
      : observations_(std::move(observations)) {}
  std::vector<webrtc::SdpVideoFormat> GetSupportedFormats() const override {
    return {webrtc::SdpVideoFormat("VP8")};
  }
  CodecSupport QueryCodecSupport(const webrtc::SdpVideoFormat& format,
      bool reference_scaling, std::optional<webrtc::Resolution>) const override {
    return {absl::EqualsIgnoreCase(format.name, "VP8") && !reference_scaling, false};
  }
  std::unique_ptr<webrtc::VideoDecoder> Create(const webrtc::Environment& env,
      const webrtc::SdpVideoFormat& format) override {
    if (!absl::EqualsIgnoreCase(format.name, "VP8")) return nullptr;
    observations_->Hit(&FfiDecoderStatistics::decoder_creations);
    return std::make_unique<ObservedVp8Decoder>(
        webrtc::CreateVp8Decoder(env), observations_);
  }
 private:
  std::shared_ptr<CodecDecoderObservations> observations_;
};

class ObservedOpusDecoder final : public webrtc::AudioDecoder {
 public:
  ObservedOpusDecoder(std::unique_ptr<webrtc::AudioDecoder> decoder,
                     std::shared_ptr<CodecDecoderObservations> observations)
      : decoder_(std::move(decoder)), observations_(std::move(observations)) {}
  std::vector<ParseResult> ParsePayload(webrtc::Buffer&& payload,
                                        std::uint32_t timestamp) override {
    observations_->Observe();
    observations_->Input(payload.data(), payload.size());
    auto frames = decoder_->ParsePayload(std::move(payload), timestamp);
    for (auto& result : frames) {
      if (result.frame) result.frame = std::make_unique<Frame>(
          decoder_, std::move(result.frame), observations_);
    }
    return frames;
  }
  void Reset() override { observations_->Observe(); decoder_->Reset(); }
  int SampleRateHz() const override { return decoder_->SampleRateHz(); }
  std::size_t Channels() const override { return decoder_->Channels(); }
  int ErrorCode() override { return decoder_->ErrorCode(); }
  int PacketDuration(const std::uint8_t* data, std::size_t size) const override {
    return decoder_->PacketDuration(data, size);
  }
  int PacketDurationRedundant(const std::uint8_t* data, std::size_t size) const override {
    return decoder_->PacketDurationRedundant(data, size);
  }
  bool PacketHasFec(const std::uint8_t* data, std::size_t size) const override {
    return decoder_->PacketHasFec(data, size);
  }
  bool HasDecodePlc() const override { return decoder_->HasDecodePlc(); }
  std::size_t DecodePlc(std::size_t frames, std::int16_t* decoded) override {
    observations_->Observe();
    return decoder_->DecodePlc(frames, decoded);
  }
  void GeneratePlc(std::size_t samples, webrtc::BufferT<std::int16_t>* output) override {
    observations_->Observe(); decoder_->GeneratePlc(samples, output);
  }
 protected:
  int DecodeInternal(const std::uint8_t* data, std::size_t size, int rate,
                     std::int16_t* decoded, SpeechType* speech) override {
    observations_->Hit(&FfiDecoderStatistics::decode_calls);
    // The wrapper's nonvirtual public Decode has already checked capacity
    // against the same delegated PacketDuration/Channels before reaching here.
    observations_->Input(data, size);
    const int result = decoder_->Decode(data, size, rate,
        std::numeric_limits<std::size_t>::max(), decoded, speech);
    CountResult(result);
    return result;
  }
  int DecodeRedundantInternal(const std::uint8_t* data, std::size_t size, int rate,
                     std::int16_t* decoded, SpeechType* speech) override {
    observations_->Hit(&FfiDecoderStatistics::decode_calls);
    observations_->Input(data, size);
    const int result = decoder_->DecodeRedundant(data, size, rate,
        std::numeric_limits<std::size_t>::max(), decoded, speech);
    CountResult(result);
    return result;
  }
 private:
  class Frame final : public EncodedAudioFrame {
   public:
    Frame(std::shared_ptr<webrtc::AudioDecoder> decoder,
          std::unique_ptr<EncodedAudioFrame> frame,
          std::shared_ptr<CodecDecoderObservations> observations)
        : decoder_(std::move(decoder)), frame_(std::move(frame)),
          observations_(std::move(observations)) {}
    std::size_t Duration() const override { return frame_->Duration(); }
    bool IsDtxPacket() const override { return frame_->IsDtxPacket(); }
    std::optional<DecodeResult> Decode(std::span<std::int16_t> output) const override {
      observations_->Hit(&FfiDecoderStatistics::decode_calls);
      auto result = frame_->Decode(output);
      if (!result) observations_->Hit(&FfiDecoderStatistics::decode_errors);
      else if (result->num_decoded_samples != 0)
        observations_->Hit(&FfiDecoderStatistics::decoded_outputs);
      return result;
    }
   private:
    // Frames hold raw decoder pointers upstream. Destroy each frame before
    // releasing its decoder even if it outlives our wrapping decoder.
    std::shared_ptr<webrtc::AudioDecoder> decoder_;
    std::unique_ptr<EncodedAudioFrame> frame_;
    std::shared_ptr<CodecDecoderObservations> observations_;
  };
  void CountResult(int result) {
    if (result < 0) observations_->Hit(&FfiDecoderStatistics::decode_errors);
    else if (result > 0) observations_->Hit(&FfiDecoderStatistics::decoded_outputs);
  }
  std::shared_ptr<webrtc::AudioDecoder> decoder_;
  std::shared_ptr<CodecDecoderObservations> observations_;
};

class ObservedOpusFactory : public webrtc::AudioDecoderFactory {
 public:
  explicit ObservedOpusFactory(std::shared_ptr<CodecDecoderObservations> observations)
      : builtin_(webrtc::CreateBuiltinAudioDecoderFactory()),
        observations_(std::move(observations)) {}
  std::vector<webrtc::AudioCodecSpec> GetSupportedDecoders() override {
    auto formats = builtin_->GetSupportedDecoders();
    std::erase_if(formats, [](const auto& spec) {
      return !absl::EqualsIgnoreCase(spec.format.name, "opus");
    });
    return formats;
  }
  bool IsSupportedDecoder(const webrtc::SdpAudioFormat& format) override {
    return absl::EqualsIgnoreCase(format.name, "opus") && builtin_->IsSupportedDecoder(format);
  }
  std::unique_ptr<webrtc::AudioDecoder> Create(const webrtc::Environment& env,
      const webrtc::SdpAudioFormat& format) override {
    return Create(env, format, std::nullopt);
  }
  std::unique_ptr<webrtc::AudioDecoder> Create(const webrtc::Environment& env,
      const webrtc::SdpAudioFormat& format,
      std::optional<webrtc::AudioCodecPairId> pair_id) override {
    if (!IsSupportedDecoder(format)) return nullptr;
    auto decoder = builtin_->Create(env, format, pair_id);
    if (!decoder) return nullptr;
    observations_->Hit(&FfiDecoderStatistics::decoder_creations);
    return std::make_unique<ObservedOpusDecoder>(std::move(decoder), observations_);
  }
 private:
  webrtc::scoped_refptr<webrtc::AudioDecoderFactory> builtin_;
  std::shared_ptr<CodecDecoderObservations> observations_;
};
} // namespace

struct NativeVideoFrame::State {
  std::uint32_t width = 0, height = 0, rtp_timestamp = 0;
  std::int64_t timestamp_us = 0;
  std::uint16_t rotation = 0;
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
  webrtc::VideoCodecType codec_type = webrtc::kVideoCodecGeneric;
  webrtc::H264PacketizationMode h264_packetization_mode =
      webrtc::H264PacketizationMode::NonInterleaved;
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
  std::shared_ptr<CodecDecoderObservations> observations;
};

namespace {

webrtc::SdpVideoFormat ToNative(const FfiCodecFormat &format) {
  std::map<std::string, std::string> parameters;
  for (const auto &parameter : format.parameters) {
    parameters.emplace(std::string(parameter.key),
                       std::string(parameter.value));
  }
  absl::InlinedVector<webrtc::ScalabilityMode, webrtc::kScalabilityModeCount> modes;
  for (const auto& name : format.scalability_modes) {
    const auto mode = webrtc::ScalabilityModeStringToEnum(std::string(name));
    // Rust's public mode wrapper is constructed only through the native parser
    // or from native enum serialization; invalid names cannot enter this path.
    RTC_CHECK(mode.has_value());
    modes.push_back(*mode);
  }
  return webrtc::SdpVideoFormat(std::string(format.name), parameters, modes);
}

FfiCodecFormat FromNative(const webrtc::SdpVideoFormat &format) {
  FfiCodecFormat result;
  result.name = format.name;
  for (const auto mode : format.scalability_modes) {
    result.scalability_modes.push_back(
        std::string(webrtc::ScalabilityModeToString(mode)));
  }
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
  RustEncoder(rust::Box<RustVideoEncoder> encoder,
              const webrtc::SdpVideoFormat &format) noexcept
      : encoder_(std::move(encoder)),
        callback_(std::make_shared<NativeEncodedImageCallback>(
            std::make_shared<NativeEncodedImageCallback::State>())),
        failed_(!encoder_is_valid(*encoder_)) {
    if (format.name == "H264") callback_->state()->codec_type = webrtc::kVideoCodecH264;
    else if (format.name == "VP8") callback_->state()->codec_type = webrtc::kVideoCodecVP8;
    else if (format.name == "VP9") callback_->state()->codec_type = webrtc::kVideoCodecVP9;
    else if (format.name == "AV1") callback_->state()->codec_type = webrtc::kVideoCodecAV1;
    else if (format.name == "H265") callback_->state()->codec_type = webrtc::kVideoCodecH265;
    const auto packetization_mode =
        format.parameters.find("packetization-mode");
    if (packetization_mode != format.parameters.end() &&
        packetization_mode->second == "0") {
      callback_->state()->h264_packetization_mode =
          webrtc::H264PacketizationMode::SingleNalUnit;
    }
  }
  ~RustEncoder() override { Release(); }

  int InitEncode(const webrtc::VideoCodec *codec,
                 const Settings &settings) override {
    if (codec == nullptr || released_)
      return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
    std::lock_guard lock(mutex_);
    // Keep the failed sentinel alive through WebRTC's encoder lifecycle. An
    // InitEncode error requests no format while default fallback is disabled,
    // which hits a DCHECK in this pinned revision. Encode can fail safely.
    if (failed_)
      return WEBRTC_VIDEO_CODEC_OK;
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
    if (released_ || failed_)
      return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
    auto state = std::make_unique<NativeVideoFrame::State>();
    state->width = static_cast<std::uint32_t>(frame.width());
    state->height = static_cast<std::uint32_t>(frame.height());
    state->timestamp_us = frame.timestamp_us();
    state->rtp_timestamp = frame.rtp_timestamp();
    state->rotation = static_cast<std::uint16_t>(frame.rotation());
    state->i420 = CopyI420(frame);
    std::vector<std::uint8_t> types;
    if (frame_types != nullptr) {
      for (auto type : *frame_types)
        types.push_back(static_cast<std::uint8_t>(type));
    }
    const auto presentation = frame.presentation_timestamp();
    return encoder_encode(
        *encoder_, std::make_unique<NativeVideoFrame>(std::move(state)),
        rust::Slice<const std::uint8_t>(types.data(), types.size()),
        presentation ? presentation->us() : 0, callback_);
  }

  void SetRates(const RateControlParameters &parameters) override {
    std::lock_guard lock(mutex_);
    if (!released_ && !failed_) {
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
    result.supports_simulcast = info.supports_simulcast;
    static_assert(webrtc::kMaxSpatialLayers == 5,
                  "Rust EncoderInfo must match native spatial capacity");
    // Rust represents either no override or the exact native array shape.
    // Preserve the constructor defaults when no capability was supplied.
    if (!info.fps_allocation.empty()) {
      for (std::size_t spatial = 0; spatial < webrtc::kMaxSpatialLayers; ++spatial) {
        const auto& fractions = info.fps_allocation[spatial].fractions;
        result.fps_allocation[spatial].assign(fractions.begin(), fractions.end());
      }
    }
    return result;
  }

private:
  mutable std::mutex mutex_;
  rust::Box<RustVideoEncoder> encoder_;
  std::shared_ptr<NativeEncodedImageCallback> callback_;
  bool released_ = false;
  const bool failed_;
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

class RustPlainEncoderFactory final : public webrtc::VideoEncoderFactory {
public:
  explicit RustPlainEncoderFactory(rust::Box<RustVideoEncoderFactory> factory)
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
    return std::make_unique<RustEncoder>(std::move(encoder), format);
  }

private:
  rust::Box<RustVideoEncoderFactory> factory_;
};

class RustEncoderFactory final : public webrtc::VideoEncoderFactory {
public:
  explicit RustEncoderFactory(rust::Box<RustVideoEncoderFactory> factory)
      : plain_(std::make_unique<RustPlainEncoderFactory>(std::move(factory))) {}
  std::vector<webrtc::SdpVideoFormat> GetSupportedFormats() const override {
    return plain_->GetSupportedFormats();
  }
  CodecSupport QueryCodecSupport(
      const webrtc::SdpVideoFormat &format,
      std::optional<std::string> scalability_mode,
      std::optional<webrtc::Resolution> resolution) const override {
    return plain_->QueryCodecSupport(format, scalability_mode, resolution);
  }
  std::unique_ptr<webrtc::VideoEncoder> Create(
      const webrtc::Environment &env,
      const webrtc::SdpVideoFormat &format) override {
    return std::make_unique<webrtc::SimulcastEncoderAdapter>(
        env, plain_.get(), nullptr, format);
  }

private:
  std::unique_ptr<RustPlainEncoderFactory> plain_;
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
std::unique_ptr<NativeVideoFrame>
wrap_video_frame(const webrtc::VideoFrame &frame) noexcept {
  auto state = std::make_unique<NativeVideoFrame::State>();
  state->width = static_cast<std::uint32_t>(frame.width());
  state->height = static_cast<std::uint32_t>(frame.height());
  state->timestamp_us = frame.timestamp_us();
  state->rtp_timestamp = frame.rtp_timestamp();
  state->rotation = static_cast<std::uint16_t>(frame.rotation());
  state->i420 = CopyI420(frame);
  return std::make_unique<NativeVideoFrame>(std::move(state));
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
  return failed_ ? WEBRTC_VIDEO_CODEC_OK : encoder_register_callback(*encoder_);
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
  return failed_ ? WEBRTC_VIDEO_CODEC_OK : encoder_release(*encoder_);
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
    std::unique_ptr<webrtc::VideoDecoderFactory> factory,
    std::shared_ptr<CodecDecoderObservations> observations) noexcept
    : factory_(std::move(factory)), observations_(std::move(observations)) {}
NativeVideoDecoderFactory::~NativeVideoDecoderFactory() = default;
webrtc::VideoDecoderFactory &
NativeVideoDecoderFactory::factory() const noexcept {
  return *factory_;
}
const std::shared_ptr<CodecDecoderObservations>&
NativeVideoDecoderFactory::observations() const noexcept { return observations_; }

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
namespace {
// This factory is exclusively for Opus-frame tracks. Ordinary PCM must use
// a separate peer factory: a PCM sentinel cannot be made collision-free.
class OpusCarrierEncoder final : public webrtc::AudioEncoder {
 public:
  OpusCarrierEncoder(webrtc::AudioEncoderFactory::Options options,
                     std::uint8_t channels)
      : options_(std::move(options)), channels_(channels) {}
  ~OpusCarrierEncoder() override { ReleasePacket(); }

  int SampleRateHz() const override { return 48000; }
  size_t NumChannels() const override { return channels_; }
  size_t Num10MsFramesInNextPacket() const override {
    return next_slots_;
  }
  size_t Max10MsFramesInAPacket() const override { return 6; }
  int GetTargetBitrate() const override {
    return 32000;
  }
  void Reset() override {
    ReleasePacket();
    remaining_ = 0;
    bytes_.clear();
    first_timestamp_.reset();
  }
  std::optional<std::pair<webrtc::TimeDelta, webrtc::TimeDelta>>
  GetFrameLengthRange() const override {
    return std::pair(webrtc::TimeDelta::Millis(10),
                     webrtc::TimeDelta::Millis(60));
  }

 private:
  EncodedInfo EncodeImpl(std::uint32_t timestamp,
                         std::span<const std::int16_t> audio,
                         webrtc::Buffer* encoded) override {
    using namespace opus_carrier;
    EncodedInfo info;
    auto block = std::span(reinterpret_cast<const std::uint8_t*>(audio.data()),
                           audio.size_bytes());
    const auto block_bytes = kBytesPerChannelBlock * channels_;
    if (block.size() != block_bytes) return info;
    if (remaining_ == 0) {
      if (!HasMagic(block)) return info;
      expected_bytes_ = Load16(block.data() + 8);
      const auto duration = Load16(block.data() + 10);
      if (duration < 480 || duration > 2880 || duration % 480 != 0 ||
          expected_bytes_ == 0 ||
          expected_bytes_ + kHeaderBytes > (duration / 480) * block_bytes) {
        return info;
      }
      next_slots_ = duration / 480;
      remaining_ = next_slots_;
      packet_timestamp_ = Load32(block.data() + 12);
      checksum_ = Load32(block.data() + 16);
      source_id_ = Load32(block.data() + 20);
      packet_id_ = Load32(block.data() + 24);
      if (!first_timestamp_) {
        first_timestamp_ = packet_timestamp_;
        first_encoder_timestamp_ = timestamp;
      }
      bytes_.clear();
      bytes_.reserve(expected_bytes_);
      block = block.subspan(kHeaderBytes);
    }
    const auto needed = expected_bytes_ - bytes_.size();
    bytes_.insert(bytes_.end(), block.begin(),
                  block.begin() + std::min(needed, block.size()));
    if (--remaining_ != 0) return info;
    if (bytes_.size() != expected_bytes_ || Checksum(bytes_) != checksum_) {
      bytes_.clear();
      ReleasePacket();
      return info;
    }
    encoded->AppendData(bytes_.data(), bytes_.size());
    info.encoded_bytes = bytes_.size();
    info.encoded_timestamp = first_encoder_timestamp_ +
                             (packet_timestamp_ - *first_timestamp_);
    info.payload_type = options_.payload_type;
    info.encoder_type = CodecType::kOpus;
    bytes_.clear();
    ReleasePacket();
    return info;
  }

  void ReleasePacket() {
    if (source_id_) opus_carrier::Acknowledge(source_id_, packet_id_);
    source_id_ = 0;
    packet_id_ = 0;
  }

  const webrtc::AudioEncoderFactory::Options options_;
  const std::uint8_t channels_;
  std::vector<std::uint8_t> bytes_;
  std::optional<std::uint32_t> first_timestamp_;
  std::uint32_t first_encoder_timestamp_ = 0;
  std::uint32_t packet_timestamp_ = 0;
  std::uint32_t checksum_ = 0;
  std::uint32_t source_id_ = 0;
  std::uint32_t packet_id_ = 0;
  size_t expected_bytes_ = 0;
  size_t next_slots_ = 2;
  size_t remaining_ = 0;
};

class OpusCarrierFactory : public webrtc::AudioEncoderFactory {
 public:
  OpusCarrierFactory() : builtin_(webrtc::CreateBuiltinAudioEncoderFactory()) {}
  std::vector<webrtc::AudioCodecSpec> GetSupportedEncoders() override {
    std::vector<webrtc::AudioCodecSpec> opus;
    for (auto spec : builtin_->GetSupportedEncoders()) {
      if (OpusChannels(spec.format)) {
        spec.format.parameters["stereo"] = "0";
        spec.info.num_channels = 1;
        // Opus has one RFC 7587 RTP identity (opus/48000/2). The pinned
        // payload mapper ignores stereo fmtp when matching audio codecs, so
        // advertising stereo=0 and stereo=1 separately creates duplicate PTs.
        // Advertise the canonical format once; Query/Create still honor the
        // remote stereo preference for mono and stereo carrier streams.
        opus.push_back(std::move(spec));
      }
    }
    return opus;
  }
  std::optional<webrtc::AudioCodecInfo> QueryAudioEncoder(
      const webrtc::SdpAudioFormat& format) override {
    const auto channels = OpusChannels(format);
    if (!channels) return std::nullopt;
    auto info = builtin_->QueryAudioEncoder(format);
    if (info) info->num_channels = *channels;
    return info;
  }
  std::unique_ptr<webrtc::AudioEncoder> Create(
      const webrtc::Environment&, const webrtc::SdpAudioFormat& format,
      Options options) override {
    if (!QueryAudioEncoder(format)) return nullptr;
    return std::make_unique<OpusCarrierEncoder>(
        std::move(options), *OpusChannels(format));
  }
 private:
  static std::optional<std::uint8_t> OpusChannels(
      const webrtc::SdpAudioFormat& format) {
    if (format.name != "opus" || format.clockrate_hz != 48000 ||
        format.num_channels != 2) return std::nullopt;
    const auto stereo = format.parameters.find("stereo");
    if (stereo == format.parameters.end() || stereo->second == "0") return 1;
    if (stereo->second == "1") return 2;
    return std::nullopt;
  }
 private:
  webrtc::scoped_refptr<webrtc::AudioEncoderFactory> builtin_;
};
} // namespace

std::unique_ptr<NativeAudioEncoderFactory>
new_opus_carrier_audio_encoder_factory() noexcept {
  auto state = std::make_unique<NativeAudioEncoderFactory::State>();
  state->factory = webrtc::make_ref_counted<OpusCarrierFactory>();
  return std::make_unique<NativeAudioEncoderFactory>(std::move(state));
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

std::unique_ptr<NativeVideoDecoderFactory>
new_builtin_vp8_decoder_factory() noexcept {
  auto observations = std::make_shared<CodecDecoderObservations>();
  return std::make_unique<NativeVideoDecoderFactory>(
      std::make_unique<ObservedVp8Factory>(observations), observations);
}

std::unique_ptr<NativeAudioDecoderFactory>
new_builtin_opus_decoder_factory() noexcept {
  auto state = std::make_unique<NativeAudioDecoderFactory::State>();
  state->observations = std::make_shared<CodecDecoderObservations>();
  state->factory = webrtc::make_ref_counted<ObservedOpusFactory>(state->observations);
  return std::make_unique<NativeAudioDecoderFactory>(std::move(state));
}

FfiDecoderStatistics video_decoder_statistics(
    const NativeVideoDecoderFactory& factory) noexcept {
  return factory.observations() ? factory.observations()->Snapshot() : FfiDecoderStatistics{};
}
FfiDecoderStatistics audio_decoder_statistics(
    const NativeAudioDecoderFactory& factory) noexcept {
  return factory.state().observations ? factory.state().observations->Snapshot() : FfiDecoderStatistics{};
}
std::uint64_t native_codec_thread_token() noexcept {
  return static_cast<std::uint64_t>(std::hash<std::thread::id>{}(std::this_thread::get_id()));
}

bool video_scalability_mode_valid(rust::Str name) noexcept {
  return webrtc::ScalabilityModeStringToEnum(std::string(name)).has_value();
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
std::uint16_t native_video_frame_rotation(const NativeVideoFrame &frame) noexcept {
  return frame.state().rotation;
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
                           std::int32_t qp,
                           const FfiEncodedVideoMetadata &metadata) noexcept {
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
  if (metadata.simulcast_index < -1 ||
      metadata.simulcast_index >= static_cast<int>(webrtc::kMaxSimulcastStreams) ||
      metadata.spatial_index < -1 ||
      metadata.spatial_index >= static_cast<int>(webrtc::kMaxSpatialLayers) ||
      metadata.temporal_index < -1 ||
      metadata.temporal_index >= static_cast<int>(webrtc::kMaxTemporalStreams) ||
      metadata.num_spatial_layers == 0 ||
      metadata.num_spatial_layers > webrtc::kMaxVp9NumberOfSpatialLayers)
    return false;
  if (metadata.simulcast_index >= 0)
    image.SetSimulcastIndex(metadata.simulcast_index);
  if (metadata.spatial_index >= 0)
    image.SetSpatialIndex(metadata.spatial_index);
  if (metadata.temporal_index >= 0)
    image.SetTemporalIndex(metadata.temporal_index);
  const auto codec = state->codec_type;
  if (metadata.codec != 0 &&
      !((metadata.codec == 1 && codec == webrtc::kVideoCodecVP8) ||
        (metadata.codec == 2 && codec == webrtc::kVideoCodecVP9) ||
        (metadata.codec == 3 && codec == webrtc::kVideoCodecH264) ||
        (metadata.codec == 4 && codec == webrtc::kVideoCodecAV1) ||
        (metadata.codec == 5 && codec == webrtc::kVideoCodecH265)))
    return false;
  std::optional<webrtc::CodecSpecificInfo> codec_specific;
  if (codec != webrtc::kVideoCodecGeneric) {
    codec_specific.emplace();
    codec_specific->codecType = codec;
    codec_specific->end_of_picture = metadata.end_of_picture;
    if (codec == webrtc::kVideoCodecH264) {
      codec_specific->codecSpecific.H264.packetization_mode =
          state->h264_packetization_mode;
      codec_specific->codecSpecific.H264.temporal_idx =
          metadata.temporal_index < 0 ? webrtc::kNoTemporalIdx : metadata.temporal_index;
      codec_specific->codecSpecific.H264.base_layer_sync = metadata.layer_sync;
      codec_specific->codecSpecific.H264.idr_frame = key_frame;
    } else if (codec == webrtc::kVideoCodecVP8) {
      auto &vp8 = codec_specific->codecSpecific.VP8;
      vp8.nonReference = metadata.non_reference;
      vp8.temporalIdx = metadata.temporal_index < 0
          ? webrtc::kNoTemporalIdx : metadata.temporal_index;
      vp8.layerSync = metadata.layer_sync;
      vp8.keyIdx = metadata.key_index;
    } else if (codec == webrtc::kVideoCodecVP9) {
      auto &vp9 = codec_specific->codecSpecific.VP9;
      vp9.first_frame_in_picture = metadata.first_frame_in_picture;
      vp9.inter_pic_predicted = metadata.inter_picture_predicted;
      vp9.flexible_mode = metadata.flexible_mode;
      vp9.temporal_idx = metadata.temporal_index < 0
          ? webrtc::kNoTemporalIdx : metadata.temporal_index;
      vp9.temporal_up_switch = metadata.temporal_up_switch;
      vp9.inter_layer_predicted = metadata.inter_layer_predicted;
      vp9.num_spatial_layers = metadata.num_spatial_layers;
      vp9.spatial_layer_resolution_present = key_frame && metadata.num_spatial_layers == 1;
      vp9.width[0] = width;
      vp9.height[0] = height;
    }
  }
  return state->callback->OnEncodedImage(
             image, codec_specific ? &*codec_specific : nullptr).error ==
         webrtc::EncodedImageCallback::Result::OK;
}
bool decoded_callback_emit(const NativeDecodedImageCallback &callback,
                           rust::Slice<const std::uint8_t> data,
                           std::uint32_t width, std::uint32_t height,
                           std::int64_t timestamp_us,
                           std::uint32_t rtp_timestamp,
                           std::uint16_t rotation) noexcept {
  const std::size_t y = static_cast<std::size_t>(width) * height;
  const std::uint32_t cw = (width + 1) / 2, ch = (height + 1) / 2;
  const std::size_t c = static_cast<std::size_t>(cw) * ch;
  if (width == 0 || height == 0 || data.size() != y + 2 * c ||
      (rotation != 0 && rotation != 90 && rotation != 180 && rotation != 270))
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
                   .set_rotation(static_cast<webrtc::VideoRotation>(rotation))
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
