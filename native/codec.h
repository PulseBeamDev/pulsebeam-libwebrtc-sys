#pragma once

#include <cstdint>
#include <memory>

#include "api/scoped_refptr.h"
#include "rust/cxx.h"

namespace webrtc {
class AudioDecoderFactory;
class AudioEncoderFactory;
class VideoFrame;
class VideoDecoderFactory;
class VideoEncoderFactory;
} // namespace webrtc

namespace pulsebeam::webrtc_sys {

struct FfiCodecFormat;
struct FfiEncodedVideoMetadata;
struct FfiCodecSupport;
struct FfiCodecTestResult;
struct FfiDecoderStatistics;
struct CodecDecoderObservations;
struct RustVideoDecoderFactory;
struct RustVideoEncoderFactory;

class NativeVideoFrame final {
public:
  struct State;
  explicit NativeVideoFrame(std::unique_ptr<State> state) noexcept;
  ~NativeVideoFrame();
  const State &state() const noexcept;

private:
  std::unique_ptr<State> state_;
};

class NativeEncodedVideoFrame final {
public:
  struct State;
  explicit NativeEncodedVideoFrame(std::unique_ptr<State> state) noexcept;
  ~NativeEncodedVideoFrame();
  const State &state() const noexcept;

private:
  std::unique_ptr<State> state_;
};

class NativeEncodedImageCallback final {
public:
  struct State;
  explicit NativeEncodedImageCallback(std::shared_ptr<State> state) noexcept;
  ~NativeEncodedImageCallback();
  const std::shared_ptr<State> &state() const noexcept;

private:
  std::shared_ptr<State> state_;
};

class NativeDecodedImageCallback final {
public:
  struct State;
  explicit NativeDecodedImageCallback(std::shared_ptr<State> state) noexcept;
  ~NativeDecodedImageCallback();
  const std::shared_ptr<State> &state() const noexcept;

private:
  std::shared_ptr<State> state_;
};

class NativeVideoEncoderFactory final {
public:
  explicit NativeVideoEncoderFactory(
      std::unique_ptr<webrtc::VideoEncoderFactory> factory) noexcept;
  ~NativeVideoEncoderFactory();
  webrtc::VideoEncoderFactory &factory() const noexcept;

private:
  std::unique_ptr<webrtc::VideoEncoderFactory> factory_;
};

class NativeVideoDecoderFactory final {
public:
  explicit NativeVideoDecoderFactory(
      std::unique_ptr<webrtc::VideoDecoderFactory> factory,
      std::shared_ptr<CodecDecoderObservations> observations = nullptr) noexcept;
  ~NativeVideoDecoderFactory();
  webrtc::VideoDecoderFactory &factory() const noexcept;
  const std::shared_ptr<CodecDecoderObservations> &observations() const noexcept;

private:
  std::unique_ptr<webrtc::VideoDecoderFactory> factory_;
  std::shared_ptr<CodecDecoderObservations> observations_;
};

class NativeAudioEncoderFactory final {
public:
  struct State;
  explicit NativeAudioEncoderFactory(std::unique_ptr<State> state) noexcept;
  ~NativeAudioEncoderFactory();
  const State &state() const noexcept;
  webrtc::scoped_refptr<webrtc::AudioEncoderFactory> factory() const noexcept;

private:
  std::unique_ptr<State> state_;
};

class NativeAudioDecoderFactory final {
public:
  struct State;
  explicit NativeAudioDecoderFactory(std::unique_ptr<State> state) noexcept;
  ~NativeAudioDecoderFactory();
  const State &state() const noexcept;
  webrtc::scoped_refptr<webrtc::AudioDecoderFactory> factory() const noexcept;

private:
  std::unique_ptr<State> state_;
};

std::unique_ptr<NativeVideoEncoderFactory>
new_video_encoder_factory(rust::Box<RustVideoEncoderFactory> factory) noexcept;
std::unique_ptr<NativeVideoDecoderFactory>
new_video_decoder_factory(rust::Box<RustVideoDecoderFactory> factory) noexcept;
std::unique_ptr<NativeAudioEncoderFactory>
new_builtin_audio_encoder_factory() noexcept;
std::unique_ptr<NativeAudioEncoderFactory>
new_opus_carrier_audio_encoder_factory() noexcept;
std::unique_ptr<NativeAudioDecoderFactory>
new_builtin_audio_decoder_factory() noexcept;
std::unique_ptr<NativeVideoDecoderFactory>
new_builtin_vp8_decoder_factory() noexcept;
std::unique_ptr<NativeAudioDecoderFactory>
new_builtin_opus_decoder_factory() noexcept;
FfiDecoderStatistics video_decoder_statistics(
    const NativeVideoDecoderFactory &factory) noexcept;
FfiDecoderStatistics audio_decoder_statistics(
    const NativeAudioDecoderFactory &factory) noexcept;
std::uint64_t native_codec_thread_token() noexcept;

bool video_scalability_mode_valid(rust::Str name) noexcept;
rust::Vec<FfiCodecFormat>
video_encoder_formats(const NativeVideoEncoderFactory &factory) noexcept;
FfiCodecSupport video_encoder_query(const NativeVideoEncoderFactory &factory,
                                    const FfiCodecFormat &format,
                                    rust::Str scalability_mode,
                                    bool has_resolution, std::uint32_t width,
                                    std::uint32_t height) noexcept;
rust::Vec<FfiCodecFormat>
video_decoder_formats(const NativeVideoDecoderFactory &factory) noexcept;
FfiCodecSupport video_decoder_query(const NativeVideoDecoderFactory &factory,
                                    const FfiCodecFormat &format,
                                    bool reference_scaling, bool has_resolution,
                                    std::uint32_t width,
                                    std::uint32_t height) noexcept;

std::uint32_t native_video_frame_width(const NativeVideoFrame &frame) noexcept;
std::uint32_t native_video_frame_height(const NativeVideoFrame &frame) noexcept;
std::int64_t
native_video_frame_timestamp_us(const NativeVideoFrame &frame) noexcept;
std::uint32_t
native_video_frame_rtp_timestamp(const NativeVideoFrame &frame) noexcept;
std::uint16_t native_video_frame_rotation(const NativeVideoFrame &frame) noexcept;
rust::Vec<std::uint8_t>
native_video_frame_i420(const NativeVideoFrame &frame) noexcept;
std::uint32_t
native_encoded_frame_width(const NativeEncodedVideoFrame &frame) noexcept;
std::uint32_t
native_encoded_frame_height(const NativeEncodedVideoFrame &frame) noexcept;
std::uint32_t native_encoded_frame_rtp_timestamp(
    const NativeEncodedVideoFrame &frame) noexcept;
bool native_encoded_frame_key(const NativeEncodedVideoFrame &frame) noexcept;
std::int32_t
native_encoded_frame_qp(const NativeEncodedVideoFrame &frame) noexcept;
rust::Vec<std::uint8_t>
native_encoded_frame_data(const NativeEncodedVideoFrame &frame) noexcept;

bool encoded_callback_emit(const NativeEncodedImageCallback &callback,
                           rust::Slice<const std::uint8_t> data,
                           std::uint32_t width, std::uint32_t height,
                           std::uint32_t rtp_timestamp, bool key_frame,
                           std::int32_t qp,
                           const FfiEncodedVideoMetadata &metadata) noexcept;
bool decoded_callback_emit(const NativeDecodedImageCallback &callback,
                           rust::Slice<const std::uint8_t> data,
                           std::uint32_t width, std::uint32_t height,
                           std::int64_t timestamp_us,
                           std::uint32_t rtp_timestamp,
                           std::uint16_t rotation) noexcept;

FfiCodecTestResult
test_codec_roundtrip(const NativeVideoEncoderFactory &encoder,
                     const NativeVideoDecoderFactory &decoder,
                     std::uint32_t frames) noexcept;
bool test_encoder_factory_cross_thread(
    const NativeVideoEncoderFactory &factory) noexcept;
std::unique_ptr<NativeVideoFrame>
wrap_video_frame(const webrtc::VideoFrame &frame) noexcept;

} // namespace pulsebeam::webrtc_sys
