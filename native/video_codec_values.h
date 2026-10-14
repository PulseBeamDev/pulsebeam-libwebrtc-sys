#pragma once

#include "api/video/render_resolution.h"
#include "api/video_codecs/sdp_video_format.h"
#include "api/video_codecs/video_decoder.h"
#include "rust/cxx.h"
#include <cstdint>
#include <memory>
#include <utility>
#include <vector>

namespace pulsebeam::webrtc_sys {
struct FfiOwnedVideoDecoderSettings;
struct FfiOwnedVideoDecoderInfo;
class NativeVideoDecoderSettings final {
public:
  NativeVideoDecoderSettings() = default;
  explicit NativeVideoDecoderSettings(webrtc::VideoDecoder::Settings value) : value_(std::move(value)) {}
  const webrtc::VideoDecoder::Settings &value() const noexcept { return value_; }
  webrtc::VideoDecoder::Settings &value() noexcept { return value_; }
private:
  webrtc::VideoDecoder::Settings value_;
};
class NativeVideoDecoderInfo final {
public:
  NativeVideoDecoderInfo() = default;
  explicit NativeVideoDecoderInfo(webrtc::VideoDecoder::DecoderInfo value) : value_(std::move(value)) {}
  const webrtc::VideoDecoder::DecoderInfo &value() const noexcept { return value_; }
  webrtc::VideoDecoder::DecoderInfo &value() noexcept { return value_; }
private:
  webrtc::VideoDecoder::DecoderInfo value_;
};
std::unique_ptr<NativeVideoDecoderSettings> video_decoder_settings_new() noexcept;
std::unique_ptr<NativeVideoDecoderSettings> video_decoder_settings_clone(const NativeVideoDecoderSettings &) noexcept;
void video_decoder_settings_assign(NativeVideoDecoderSettings &, const NativeVideoDecoderSettings &) noexcept;
FfiOwnedVideoDecoderSettings video_decoder_settings_fields(const NativeVideoDecoderSettings &) noexcept;
std::int32_t video_decoder_settings_set_fields(NativeVideoDecoderSettings &, FfiOwnedVideoDecoderSettings) noexcept;
std::unique_ptr<NativeVideoDecoderInfo> video_decoder_info_new() noexcept;
std::unique_ptr<NativeVideoDecoderInfo> video_decoder_info_clone(const NativeVideoDecoderInfo &) noexcept;
void video_decoder_info_assign(NativeVideoDecoderInfo &, const NativeVideoDecoderInfo &) noexcept;
FfiOwnedVideoDecoderInfo video_decoder_info_fields(const NativeVideoDecoderInfo &) noexcept;
void video_decoder_info_set_fields(NativeVideoDecoderInfo &, FfiOwnedVideoDecoderInfo) noexcept;
bool video_decoder_info_equal(const NativeVideoDecoderInfo &, const NativeVideoDecoderInfo &) noexcept;
rust::Vec<std::uint8_t> video_decoder_info_string(const NativeVideoDecoderInfo &) noexcept;
struct FfiSdpVideoFormatFields;
struct FfiRtpCodecParameter;
class NativeSdpVideoFormat final {
public:
  explicit NativeSdpVideoFormat(webrtc::SdpVideoFormat value)
      : value_(std::move(value)) {}
  const webrtc::SdpVideoFormat &value() const noexcept { return value_; }
  webrtc::SdpVideoFormat &value() noexcept { return value_; }

private:
  webrtc::SdpVideoFormat value_;
};
bool SdpVideoFormatStringSafe(const webrtc::SdpVideoFormat &) noexcept;
std::int32_t
SdpVideoFormatFuzzyMatchStatus(const std::vector<webrtc::SdpVideoFormat> &,
                               const webrtc::SdpVideoFormat &) noexcept;
class NativeStringBuilder;
class NativeRtcError;
bool string_builder_append_sdp_video_format(
    NativeStringBuilder &, const NativeSdpVideoFormat &) noexcept;
bool rtc_error_builder_append_sdp_video_format(
    NativeRtcError &, const NativeSdpVideoFormat &) noexcept;
std::unique_ptr<NativeRtcError>
rtc_error_stream_sdp_video_format(std::unique_ptr<NativeRtcError>,
                                  const NativeSdpVideoFormat &,
                                  bool &) noexcept;
std::unique_ptr<NativeSdpVideoFormat>
    sdp_video_format_new(rust::Slice<const std::uint8_t>) noexcept;
std::unique_ptr<NativeSdpVideoFormat> sdp_video_format_with_parameters(
    rust::Slice<const std::uint8_t>, rust::Slice<const FfiRtpCodecParameter>,
    rust::Slice<const std::uint8_t>, bool) noexcept;
std::unique_ptr<NativeSdpVideoFormat>
sdp_video_format_with_modes(const NativeSdpVideoFormat &,
                            rust::Slice<const std::uint8_t>) noexcept;
std::unique_ptr<NativeSdpVideoFormat>
sdp_video_format_clone(const NativeSdpVideoFormat &) noexcept;
std::unique_ptr<NativeSdpVideoFormat>
    sdp_video_format_move(std::unique_ptr<NativeSdpVideoFormat>) noexcept;
void sdp_video_format_assign(NativeSdpVideoFormat &,
                             const NativeSdpVideoFormat &) noexcept;
void sdp_video_format_move_assign(
    NativeSdpVideoFormat &, std::unique_ptr<NativeSdpVideoFormat>) noexcept;
FfiSdpVideoFormatFields
sdp_video_format_fields(const NativeSdpVideoFormat &) noexcept;
void sdp_video_format_set_fields(NativeSdpVideoFormat &,
                                 FfiSdpVideoFormatFields) noexcept;
bool sdp_video_format_equal(const NativeSdpVideoFormat &,
                            const NativeSdpVideoFormat &) noexcept;
bool sdp_video_format_same_codec(const NativeSdpVideoFormat &,
                                 const NativeSdpVideoFormat &) noexcept;
rust::Vec<std::uint8_t> sdp_video_format_string(const NativeSdpVideoFormat &,
                                                bool &) noexcept;
std::unique_ptr<NativeSdpVideoFormat>
sdp_video_format_named(std::uint8_t) noexcept;
class NativeSdpVideoFormatList final {
public:
  std::vector<webrtc::SdpVideoFormat> values;
};
std::unique_ptr<NativeSdpVideoFormatList> sdp_video_format_list_new() noexcept;
std::size_t
sdp_video_format_list_len(const NativeSdpVideoFormatList &) noexcept;
std::unique_ptr<NativeSdpVideoFormat>
sdp_video_format_list_get(const NativeSdpVideoFormatList &,
                          std::size_t) noexcept;
void sdp_video_format_list_add(NativeSdpVideoFormatList &,
                               const NativeSdpVideoFormat &) noexcept;
bool sdp_video_format_in_list(const NativeSdpVideoFormat &,
                              const NativeSdpVideoFormatList &) noexcept;
std::unique_ptr<NativeSdpVideoFormat>
sdp_video_format_fuzzy_match(const NativeSdpVideoFormatList &,
                             const NativeSdpVideoFormat &,
                             std::int32_t &) noexcept;

class NativeRenderResolution final {
public:
  NativeRenderResolution() = default;
  explicit NativeRenderResolution(webrtc::RenderResolution value)
      : value_(value) {}
  const webrtc::RenderResolution &value() const noexcept { return value_; }
  webrtc::RenderResolution &value() noexcept { return value_; }

private:
  webrtc::RenderResolution value_;
};
std::unique_ptr<NativeRenderResolution> render_resolution_default() noexcept;
std::unique_ptr<NativeRenderResolution>
render_resolution_new(std::int32_t, std::int32_t) noexcept;
std::unique_ptr<NativeRenderResolution>
render_resolution_clone(const NativeRenderResolution &) noexcept;
void render_resolution_assign(NativeRenderResolution &,
                              const NativeRenderResolution &) noexcept;
std::int32_t render_resolution_width(const NativeRenderResolution &) noexcept;
std::int32_t render_resolution_height(const NativeRenderResolution &) noexcept;
bool render_resolution_valid(const NativeRenderResolution &) noexcept;
bool render_resolution_equal(const NativeRenderResolution &,
                             const NativeRenderResolution &) noexcept;
} // namespace pulsebeam::webrtc_sys
