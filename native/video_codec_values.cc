#include "pulsebeam-webrtc-sys/native/video_codec_values.h"
#include "absl/strings/match.h"
#include "pulsebeam-webrtc-sys/native/rtc_error.h"
#include "pulsebeam-webrtc-sys/native/string_builder.h"
#include "pulsebeam-webrtc-sys/src/lib.rs.h"
#include "rtc_base/logging.h"
#include "rtc_base/checks.h"
#include <limits>

namespace pulsebeam::webrtc_sys {
namespace {
template <typename Bytes> std::string Text(const Bytes &bytes) {
  return std::string(reinterpret_cast<const char *>(bytes.data()),
                     bytes.size());
}
rust::Vec<std::uint8_t> Bytes(const std::string &text) {
  rust::Vec<std::uint8_t> bytes;
  bytes.reserve(text.size());
  for (unsigned char b : text)
    bytes.push_back(b);
  return bytes;
}
template <typename Pairs>
webrtc::CodecParameterMap Parameters(const Pairs &pairs) {
  webrtc::CodecParameterMap result;
  for (const auto &pair : pairs)
    result.emplace(Text(pair.key), Text(pair.value));
  return result;
}
template <typename Input>
absl::InlinedVector<webrtc::ScalabilityMode, webrtc::kScalabilityModeCount>
Modes(const Input &input) {
  absl::InlinedVector<webrtc::ScalabilityMode, webrtc::kScalabilityModeCount>
      result;
  for (auto mode : input)
    result.push_back(static_cast<webrtc::ScalabilityMode>(mode));
  return result;
}
} // namespace
std::unique_ptr<NativeVideoDecoderSettings> video_decoder_settings_new() noexcept {
  return std::make_unique<NativeVideoDecoderSettings>();
}
std::unique_ptr<NativeVideoDecoderSettings> video_decoder_settings_clone(const NativeVideoDecoderSettings &source) noexcept {
  return std::make_unique<NativeVideoDecoderSettings>(source.value());
}
void video_decoder_settings_assign(NativeVideoDecoderSettings &target, const NativeVideoDecoderSettings &source) noexcept { target.value() = source.value(); }
FfiOwnedVideoDecoderSettings video_decoder_settings_fields(const NativeVideoDecoderSettings &source) noexcept {
  const auto &value = source.value();
  const auto resolution = value.max_render_resolution();
  return {value.buffer_pool_size().has_value(), value.buffer_pool_size().value_or(0),
      resolution.Width(), resolution.Height(), value.number_of_cores(), static_cast<std::int32_t>(value.codec_type())};
}
std::int32_t video_decoder_settings_set_fields(NativeVideoDecoderSettings &target, FfiOwnedVideoDecoderSettings fields) noexcept {
  // This pin's non-fixed enum has enumerators 0..5, hence its C++ value range
  // is 0..7. Keep unnamed 6/7 metadata; never cast outside that range.
  static_assert(webrtc::kVideoCodecGeneric == 0 && webrtc::kVideoCodecH265 == 5);
  if (fields.codec_type < 0 || fields.codec_type > 7) return 2;
#if RTC_DCHECK_IS_ON
  if (fields.cores <= 0) return 1;
#endif
  auto value = target.value();
  value.set_buffer_pool_size(fields.has_buffer_pool_size ? std::optional<int>(fields.buffer_pool_size) : std::nullopt);
  value.set_max_render_resolution(webrtc::RenderResolution(fields.width, fields.height));
  value.set_number_of_cores(fields.cores);
  value.set_codec_type(static_cast<webrtc::VideoCodecType>(fields.codec_type));
  target.value() = value;
  return 0;
}
std::unique_ptr<NativeVideoDecoderInfo> video_decoder_info_new() noexcept { return std::make_unique<NativeVideoDecoderInfo>(); }
std::unique_ptr<NativeVideoDecoderInfo> video_decoder_info_clone(const NativeVideoDecoderInfo &source) noexcept {
  return std::make_unique<NativeVideoDecoderInfo>(source.value());
}
void video_decoder_info_assign(NativeVideoDecoderInfo &target, const NativeVideoDecoderInfo &source) noexcept { target.value() = source.value(); }
FfiOwnedVideoDecoderInfo video_decoder_info_fields(const NativeVideoDecoderInfo &source) noexcept {
  return {Bytes(source.value().implementation_name), source.value().is_hardware_accelerated};
}
void video_decoder_info_set_fields(NativeVideoDecoderInfo &target, FfiOwnedVideoDecoderInfo fields) noexcept {
  target.value().implementation_name = Text(fields.implementation_name);
  target.value().is_hardware_accelerated = fields.hardware_accelerated;
}
bool video_decoder_info_equal(const NativeVideoDecoderInfo &left, const NativeVideoDecoderInfo &right) noexcept { return left.value() == right.value(); }
rust::Vec<std::uint8_t> video_decoder_info_string(const NativeVideoDecoderInfo &value) noexcept { return Bytes(value.value().ToString()); }
bool SdpVideoFormatStringSafe(const webrtc::SdpVideoFormat &value) noexcept {
  for (auto mode : value.scalability_modes)
    if (static_cast<std::uint8_t>(mode) >= webrtc::kScalabilityModeCount)
      return false;
  return true;
}
bool string_builder_append_sdp_video_format(
    NativeStringBuilder &target, const NativeSdpVideoFormat &value) noexcept {
  if (!SdpVideoFormatStringSafe(value.value()))
    return false;
  target.value() << value.value();
  return true;
}
bool rtc_error_builder_append_sdp_video_format(
    NativeRtcError &target, const NativeSdpVideoFormat &value) noexcept {
  if (!SdpVideoFormatStringSafe(value.value()))
    return false;
  target.value().string_builder() << value.value();
  return true;
}
std::unique_ptr<NativeRtcError>
rtc_error_stream_sdp_video_format(std::unique_ptr<NativeRtcError> target,
                                  const NativeSdpVideoFormat &value,
                                  bool &valid) noexcept {
  valid = SdpVideoFormatStringSafe(value.value());
  if (!valid)
    return target;
  return std::make_unique<NativeRtcError>(std::move(target->value())
                                          << value.value());
}
std::unique_ptr<NativeSdpVideoFormat>
sdp_video_format_new(rust::Slice<const std::uint8_t> name) noexcept {
  return std::make_unique<NativeSdpVideoFormat>(
      webrtc::SdpVideoFormat(Text(name)));
}
std::unique_ptr<NativeSdpVideoFormat> sdp_video_format_with_parameters(
    rust::Slice<const std::uint8_t> name,
    rust::Slice<const FfiRtpCodecParameter> parameters,
    rust::Slice<const std::uint8_t> modes, bool with_modes) noexcept {
  const auto map = Parameters(parameters);
  return std::make_unique<NativeSdpVideoFormat>(
      with_modes ? webrtc::SdpVideoFormat(Text(name), map, Modes(modes))
                 : webrtc::SdpVideoFormat(Text(name), map));
}
std::unique_ptr<NativeSdpVideoFormat>
sdp_video_format_with_modes(const NativeSdpVideoFormat &source,
                            rust::Slice<const std::uint8_t> modes) noexcept {
  return std::make_unique<NativeSdpVideoFormat>(
      webrtc::SdpVideoFormat(source.value(), Modes(modes)));
}
std::unique_ptr<NativeSdpVideoFormat>
sdp_video_format_clone(const NativeSdpVideoFormat &value) noexcept {
  return std::make_unique<NativeSdpVideoFormat>(value.value());
}
std::unique_ptr<NativeSdpVideoFormat>
sdp_video_format_move(std::unique_ptr<NativeSdpVideoFormat> source) noexcept {
  return std::make_unique<NativeSdpVideoFormat>(std::move(source->value()));
}
void sdp_video_format_assign(NativeSdpVideoFormat &target,
                             const NativeSdpVideoFormat &source) noexcept {
  target.value() = source.value();
}
void sdp_video_format_move_assign(
    NativeSdpVideoFormat &target,
    std::unique_ptr<NativeSdpVideoFormat> source) noexcept {
  target.value() = std::move(source->value());
}
FfiSdpVideoFormatFields
sdp_video_format_fields(const NativeSdpVideoFormat &input) noexcept {
  const auto &value = input.value();
  FfiSdpVideoFormatFields result;
  result.name = Bytes(value.name);
  for (const auto &pair : value.parameters)
    result.parameters.push_back(
        FfiRtpCodecParameter{Bytes(pair.first), Bytes(pair.second)});
  for (auto mode : value.scalability_modes)
    result.scalability_modes.push_back(static_cast<std::uint8_t>(mode));
  result.has_packetization = value.packetization.has_value();
  if (value.packetization)
    result.packetization = Bytes(*value.packetization);
  result.has_tx_mode = value.tx_mode.has_value();
  if (value.tx_mode)
    result.tx_mode = Bytes(*value.tx_mode);
  return result;
}
void sdp_video_format_set_fields(NativeSdpVideoFormat &target,
                                 FfiSdpVideoFormatFields fields) noexcept {
  webrtc::SdpVideoFormat value(Text(fields.name), Parameters(fields.parameters),
                               Modes(fields.scalability_modes));
  if (fields.has_packetization)
    value.packetization = Text(fields.packetization);
  if (fields.has_tx_mode)
    value.tx_mode = Text(fields.tx_mode);
  target.value() = std::move(value);
}
bool sdp_video_format_equal(const NativeSdpVideoFormat &l,
                            const NativeSdpVideoFormat &r) noexcept {
  return l.value() == r.value();
}
bool sdp_video_format_same_codec(const NativeSdpVideoFormat &l,
                                 const NativeSdpVideoFormat &r) noexcept {
  return l.value().IsSameCodec(r.value());
}
rust::Vec<std::uint8_t>
sdp_video_format_string(const NativeSdpVideoFormat &value,
                        bool &valid) noexcept {
  valid = SdpVideoFormatStringSafe(value.value());
  return valid ? Bytes(value.value().ToString()) : rust::Vec<std::uint8_t>();
}
std::unique_ptr<NativeSdpVideoFormat>
sdp_video_format_named(std::uint8_t kind) noexcept {
  switch (kind) {
  case 0:
    return std::make_unique<NativeSdpVideoFormat>(
        webrtc::SdpVideoFormat::VP8());
  case 1:
    return std::make_unique<NativeSdpVideoFormat>(
        webrtc::SdpVideoFormat::H264());
  case 2:
    return std::make_unique<NativeSdpVideoFormat>(
        webrtc::SdpVideoFormat::H265());
  case 3:
    return std::make_unique<NativeSdpVideoFormat>(
        webrtc::SdpVideoFormat::VP9Profile0());
  case 4:
    return std::make_unique<NativeSdpVideoFormat>(
        webrtc::SdpVideoFormat::VP9Profile1());
  case 5:
    return std::make_unique<NativeSdpVideoFormat>(
        webrtc::SdpVideoFormat::VP9Profile2());
  case 6:
    return std::make_unique<NativeSdpVideoFormat>(
        webrtc::SdpVideoFormat::VP9Profile3());
  case 7:
    return std::make_unique<NativeSdpVideoFormat>(
        webrtc::SdpVideoFormat::AV1Profile0());
  case 8:
    return std::make_unique<NativeSdpVideoFormat>(
        webrtc::SdpVideoFormat::AV1Profile1());
  default:
    return nullptr;
  }
}
std::unique_ptr<NativeSdpVideoFormatList> sdp_video_format_list_new() noexcept {
  return std::make_unique<NativeSdpVideoFormatList>();
}
std::size_t
sdp_video_format_list_len(const NativeSdpVideoFormatList &list) noexcept {
  return list.values.size();
}
std::unique_ptr<NativeSdpVideoFormat>
sdp_video_format_list_get(const NativeSdpVideoFormatList &list,
                          std::size_t index) noexcept {
  return index < list.values.size()
             ? std::make_unique<NativeSdpVideoFormat>(list.values[index])
             : nullptr;
}
void sdp_video_format_list_add(NativeSdpVideoFormatList &list,
                               const NativeSdpVideoFormat &value) noexcept {
  list.values.push_back(value.value());
}
bool sdp_video_format_in_list(const NativeSdpVideoFormat &value,
                              const NativeSdpVideoFormatList &list) noexcept {
  return value.value().IsCodecInList(list.values);
}
std::int32_t
SdpVideoFormatFuzzyMatchStatus(const std::vector<webrtc::SdpVideoFormat> &list,
                               const webrtc::SdpVideoFormat &request) noexcept {
  // Preflight only the reached arithmetic and logging sinks. Do not substitute
  // the candidate/result: the actual SDK helper below performs the match.
  const webrtc::SdpVideoFormat *selected = nullptr;
  std::size_t best = 0;
  for (const auto &format : list) {
    if (!absl::EqualsIgnoreCase(format.name, request.name))
      continue;
    std::size_t matches = 0;
    for (const auto &pair : format.parameters) {
      const auto found = request.parameters.find(pair.first);
      if (found != request.parameters.end() && found->second == pair.second) {
        if (matches ==
            static_cast<std::size_t>(std::numeric_limits<int>::max())) {
          return 2;
        }
        ++matches;
      }
    }
    if (!selected || matches > best) {
      selected = &format;
      best = matches;
    }
  }
#if RTC_LOG_ENABLED()
  if (!selected || *selected != request) {
    // Logging severity is process-global and can change concurrently. When
    // logging is compiled in, these native branches can consume ToString;
    // sampling IsNoop before dispatch would not retain that safety evidence.
    if (!SdpVideoFormatStringSafe(request) ||
        (selected && !SdpVideoFormatStringSafe(*selected))) {
      return 1;
    }
  }
#endif
  return 0;
}
std::unique_ptr<NativeSdpVideoFormat>
sdp_video_format_fuzzy_match(const NativeSdpVideoFormatList &list,
                             const NativeSdpVideoFormat &request,
                             std::int32_t &status) noexcept {
  status = SdpVideoFormatFuzzyMatchStatus(list.values, request.value());
  if (status != 0)
    return nullptr;
  auto result = webrtc::FuzzyMatchSdpVideoFormat(list.values, request.value());
  return result ? std::make_unique<NativeSdpVideoFormat>(std::move(*result))
                : nullptr;
}
std::unique_ptr<NativeRenderResolution> render_resolution_default() noexcept {
  return std::make_unique<NativeRenderResolution>();
}
std::unique_ptr<NativeRenderResolution>
render_resolution_new(std::int32_t width, std::int32_t height) noexcept {
  return std::make_unique<NativeRenderResolution>(
      webrtc::RenderResolution(width, height));
}
std::unique_ptr<NativeRenderResolution>
render_resolution_clone(const NativeRenderResolution &value) noexcept {
  return std::make_unique<NativeRenderResolution>(value.value());
}
void render_resolution_assign(NativeRenderResolution &target,
                              const NativeRenderResolution &source) noexcept {
  target.value() = source.value();
}
std::int32_t
render_resolution_width(const NativeRenderResolution &value) noexcept {
  return value.value().Width();
}
std::int32_t
render_resolution_height(const NativeRenderResolution &value) noexcept {
  return value.value().Height();
}
bool render_resolution_valid(const NativeRenderResolution &value) noexcept {
  return value.value().Valid();
}
bool render_resolution_equal(const NativeRenderResolution &l,
                             const NativeRenderResolution &r) noexcept {
  return l.value() == r.value();
}
} // namespace pulsebeam::webrtc_sys
