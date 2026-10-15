// Implementation-level SDK consumer. No bridge adapters or substitute codecs.
#if defined(__linux__) && defined(__x86_64__)
#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <memory>
#include <string>
#include <vector>

#include "api/audio/builtin_audio_processing_builder.h"
#include "api/audio_codecs/builtin_audio_decoder_factory.h"
#include "api/audio_codecs/builtin_audio_encoder_factory.h"
#include "api/environment/environment.h"
#include "api/rtc_event_log/rtc_event_log_factory.h"
#include "api/rtc_event_log_output.h"
#include "api/video/i420_buffer.h"
#include "api/video_codecs/builtin_video_decoder_factory.h"
#include "api/video_codecs/builtin_video_encoder_factory.h"
#include "api/video_codecs/video_codec.h"
#include "logging/rtc_event_log/events/rtc_event_alr_state.h"
#if PULSEBEAM_EXPECT_PROTOBUF
#include "logging/rtc_event_log/rtc_event_log_parser.h"
#endif
#include "modules/video_coding/include/video_codec_interface.h"
#include "modules/video_coding/codecs/interface/common_constants.h"
#include "modules/rtp_rtcp/include/rtp_header_extension_map.h"
#include "modules/rtp_rtcp/source/rtp_header_extensions.h"
#include "modules/rtp_rtcp/source/rtp_packet_received.h"
#include "modules/rtp_rtcp/source/rtp_packet_to_send.h"

namespace {
class Encoded final : public webrtc::EncodedImageCallback {
 public:
  std::vector<webrtc::EncodedImage> images;
  bool valid = true;
  webrtc::VideoCodecType expected;
  webrtc::H264PacketizationMode packetization;
  webrtc::ScalabilityMode scalability;
  unsigned layers;
  unsigned temporal_mask = 0;
  Encoded(webrtc::VideoCodecType type, const webrtc::SdpVideoFormat& format,
          webrtc::ScalabilityMode mode, unsigned temporal_layers)
      : expected(type), scalability(mode), layers(temporal_layers) {
    const auto packet_mode = format.parameters.find("packetization-mode");
    packetization = packet_mode != format.parameters.end() && packet_mode->second == "1"
        ? webrtc::H264PacketizationMode::NonInterleaved
        : webrtc::H264PacketizationMode::SingleNalUnit;
  }
  Result OnEncodedImage(const webrtc::EncodedImage& image,
                        const webrtc::CodecSpecificInfo* info) override {
    valid = valid && info && info->codecType == expected;
    if (info && expected == webrtc::kVideoCodecH264) {
      valid = valid && info->codecSpecific.H264.packetization_mode == packetization &&
          info->scalability_mode == scalability &&
          info->codecSpecific.H264.idr_frame == images.empty();
      const auto temporal = info->codecSpecific.H264.temporal_idx;
      if (layers == 1) valid = valid && temporal == webrtc::kNoTemporalIdx;
      else if (temporal < layers) temporal_mask |= 1u << temporal;
      else valid = false;
    }
    images.push_back(image);
    return Result(Result::OK);
  }
  void OnFrameDropped(uint32_t, int, bool) override {}
};
class DecodedFrames final : public webrtc::DecodedImageCallback {
 public:
  size_t count = 0;
  bool valid = true;
  int32_t Decoded(webrtc::VideoFrame& frame) override {
    auto buffer = frame.video_frame_buffer()->ToI420();
    valid = valid && buffer && buffer->width() == 64 && buffer->height() == 64;
    if (valid) {
      for (int row = 0; row < 64; ++row)
        for (int column = 0; column < 64; ++column)
          valid = valid && std::abs(int(buffer->DataY()[row * buffer->StrideY() + column]) - 48) <= 4;
    }
    ++count;
    return 0;
  }
};

bool Video(const webrtc::Environment& env, webrtc::VideoEncoderFactory& encoders,
           webrtc::VideoDecoderFactory& decoders, const webrtc::SdpVideoFormat& format,
           unsigned layers = 1) {
  const auto type = webrtc::PayloadStringToCodecType(format.name);
  // Callbacks outlive both products, including final Release/destruction.
  const auto mode = layers == 1 ? webrtc::ScalabilityMode::kL1T1 :
                    layers == 2 ? webrtc::ScalabilityMode::kL1T2 : webrtc::ScalabilityMode::kL1T3;
  const std::string mode_name(webrtc::ScalabilityModeToString(mode));
  Encoded encoded(type, format, mode, layers);
  DecodedFrames decoded;
  auto encoder = encoders.Create(env, format);
  auto decoder = decoders.Create(env, format);
  if (!encoder || !decoder || !encoders.QueryCodecSupport(format, mode_name, std::nullopt).is_supported ||
      !decoders.QueryCodecSupport(format, false).is_supported)
    return false;
  webrtc::VideoCodec codec{};
  codec.codecType = type;
  codec.width = 64;
  codec.height = 64;
  codec.minBitrate = 16;
  codec.startBitrate = 256;
  codec.maxBitrate = 512;
  codec.maxFramerate = 30;
  codec.active = true;
  codec.qpMax = 56;
  codec.SetScalabilityMode(mode);
  if (type == webrtc::kVideoCodecVP8) codec.VP8()->numberOfTemporalLayers = 1;
  if (type == webrtc::kVideoCodecVP9) {
    codec.VP9()->numberOfTemporalLayers = 1;
    codec.VP9()->numberOfSpatialLayers = 1;
  }
  if (type == webrtc::kVideoCodecH264) codec.H264()->numberOfTemporalLayers = layers;
  webrtc::VideoDecoder::Settings settings;
  settings.set_codec_type(type);
  settings.set_number_of_cores(1);
  settings.set_max_render_resolution({64, 64});
  if (encoder->InitEncode(&codec, {webrtc::VideoEncoder::Capabilities(false), 1, 1200}) != 0 ||
      encoder->RegisterEncodeCompleteCallback(&encoded) != 0 ||
      !decoder->Configure(settings) || decoder->RegisterDecodeCompleteCallback(&decoded) != 0)
    return false;
  webrtc::VideoBitrateAllocation allocation;
  for (unsigned layer = 0; layer < layers; ++layer)
    allocation.SetBitrate(0, layer, 256000 / layers);
  encoder->SetRates({allocation, 30});
  for (unsigned index = 0; index < 4; ++index) {
    auto buffer = webrtc::I420Buffer::Create(64, 64);
    std::memset(buffer->MutableDataY(), 48, 64 * 64);
    std::memset(buffer->MutableDataU(), 96, 32 * 32);
    std::memset(buffer->MutableDataV(), 160, 32 * 32);
    auto frame = webrtc::VideoFrame::Builder().set_video_frame_buffer(buffer)
                     .set_timestamp_us(123000 + index * 33333)
                     .set_rtp_timestamp(9000 + index * 3000).build();
    std::vector<webrtc::VideoFrameType> types{index == 0 ? webrtc::VideoFrameType::kVideoFrameKey
                                                       : webrtc::VideoFrameType::kVideoFrameDelta};
    if (encoder->Encode(frame, &types) != 0) return false;
  }
  for (size_t index = 0; index < encoded.images.size(); ++index) {
    const auto& image = encoded.images[index];
    if (image.size() == 0 || image.RtpTimestamp() != 9000 + index * 3000 ||
        decoder->Decode(image, 0) != 0) return false;
  }
  const auto encoder_release = encoder->Release();
  const auto decoder_release = decoder->Release();
  return encoder_release == 0 && decoder_release == 0 && encoded.valid &&
         encoded.images.size() == 4 && decoded.count == 4 && decoded.valid &&
         (layers == 1 || encoded.temporal_mask == (1u << layers) - 1);
}

bool Audio(const webrtc::Environment& env) {
  auto encoders = webrtc::CreateBuiltinAudioEncoderFactory();
  auto decoders = webrtc::CreateBuiltinAudioDecoderFactory();
  const auto formats = encoders->GetSupportedEncoders();
  const auto decoder_formats = decoders->GetSupportedDecoders();
  if (formats.size() < 4 || decoder_formats.size() != formats.size()) return false;
  // Equal counts do not establish equal advertised codecs/configurations.
  // Require every decoder format to have the encoder exercised below.
  for (const auto& spec : decoder_formats) {
    if (std::none_of(formats.begin(), formats.end(), [&](const auto& encoder) {
          return encoder.format == spec.format;
        })) return false;
  }
  for (const auto& spec : formats) {
    if (!encoders->QueryAudioEncoder(spec.format) || !decoders->IsSupportedDecoder(spec.format)) return false;
    auto encoder = encoders->Create(env, spec.format, {.payload_type = 111});
    auto decoder = decoders->Create(env, spec.format);
    auto legacy_decoder = decoders->Create(env, spec.format, std::nullopt);
    if (!encoder || !decoder || !legacy_decoder) return false;
    std::vector<int16_t> samples(encoder->SampleRateHz() / 100 * encoder->NumChannels());
    for (size_t i = 0; i < samples.size(); ++i)
      samples[i] = static_cast<int16_t>(6000 * std::sin(2 * 3.141592653589793 * 440 *
                                         (i / encoder->NumChannels()) / encoder->SampleRateHz()));
    size_t packets = 0;
    double energy = 0;
    // Exercise stateful packet accumulation and real decoding, not just creation.
    for (unsigned index = 0; index < 20; ++index) {
      webrtc::Buffer packet;
      const auto info = encoder->Encode(index * encoder->RtpTimestampRateHz() / 100, samples, &packet);
      if (info.encoded_bytes == 0) continue;
      if (info.encoded_bytes != packet.size()) return false;
      std::vector<int16_t> pcm(decoder->SampleRateHz() * decoder->Channels() * 120 / 1000);
      webrtc::AudioDecoder::SpeechType speech;
      const auto count = decoder->Decode(packet.data(), packet.size(), decoder->SampleRateHz(),
                                          pcm.size() * sizeof(int16_t), pcm.data(), &speech);
      if (count <= 0 || static_cast<size_t>(count) > pcm.size()) return false;
      std::vector<int16_t> legacy_pcm(pcm.size());
      const auto legacy_count = legacy_decoder->Decode(packet.data(), packet.size(), legacy_decoder->SampleRateHz(),
          legacy_pcm.size() * sizeof(int16_t), legacy_pcm.data(), &speech);
      if (legacy_count != count || !std::equal(pcm.begin(), pcm.begin() + count, legacy_pcm.begin())) return false;
      for (int i = 0; i < count; ++i) energy += double(pcm[i]) * pcm[i];
      ++packets;
    }
    if (packets == 0 || energy < 1000000) return false;
    encoder->Reset();
    decoder->Reset();
    std::printf("audio %s/%d/%zu: encode/decode passed\n", spec.format.name.c_str(),
                spec.format.clockrate_hz, spec.format.num_channels);
  }
  return true;
}

bool Processing(const webrtc::Environment& env) {
  webrtc::AudioProcessing::Config config;
  config.capture_level_adjustment.enabled = true;
  config.capture_level_adjustment.pre_gain_factor = 2;
  webrtc::BuiltinAudioProcessingBuilder builder;
  auto apm = builder.SetConfig(config).Build(env);
  if (!apm || apm->Initialize() != 0) return false;
  webrtc::StreamConfig stream(48000, 1);
  std::vector<int16_t> input(480, 1000), output(480);
  for (int i = 0; i < 10; ++i) {
    if (apm->ProcessStream(input.data(), stream, stream, output.data()) != 0 ||
        apm->ProcessReverseStream(input.data(), stream, stream, output.data()) != 0) return false;
  }
  if (apm->ProcessStream(input.data(), stream, stream, output.data()) != 0) return false;
  if (!std::all_of(output.begin(), output.end(), [](int16_t sample) { return sample >= 1990 && sample <= 2010; })) return false;
  std::puts("software APM: processed PCM gain passed (no devices)");
  return true;
}

class LogOutput final : public webrtc::RtcEventLogOutput {
 public:
  explicit LogOutput(std::shared_ptr<std::string> bytes) : bytes_(std::move(bytes)) {}
  bool IsActive() const override { return true; }
  bool Write(absl::string_view bytes) override { bytes_->append(bytes); return true; }
 private:
  std::shared_ptr<std::string> bytes_;
};
bool Helpers(const webrtc::Environment& env) {
  auto bytes = std::make_shared<std::string>();
  auto log = webrtc::RtcEventLogFactory().Create(env);
  const bool started = log->StartLogging(std::make_unique<LogOutput>(bytes), 0);
  log->Log(std::make_unique<webrtc::RtcEventAlrState>(true));
  log->StopLogging();
  if (started != bool(PULSEBEAM_EXPECT_PROTOBUF) || bytes->empty() == bool(PULSEBEAM_EXPECT_PROTOBUF)) return false;
#if PULSEBEAM_EXPECT_PROTOBUF
  webrtc::ParsedRtcEventLog parsed;
  if (!parsed.ParseString(*bytes).ok() || parsed.alr_state_events().size() != 1 ||
      !parsed.alr_state_events()[0].in_alr) return false;
#endif
  webrtc::RtpHeaderExtensionMap extensions;
  extensions.Register<webrtc::AudioLevelExtension>(1);
  webrtc::RtpPacketToSend outgoing(&extensions);
  outgoing.SetSsrc(1234);
  outgoing.SetSequenceNumber(5678);
  if (!outgoing.SetExtension<webrtc::AudioLevelExtension>(webrtc::AudioLevel(true, 37))) return false;
  webrtc::RtpPacketReceived incoming(&extensions);
  if (!incoming.Parse(outgoing.Buffer())) return false;
  webrtc::AudioLevel level;
  if (!incoming.GetExtension<webrtc::AudioLevelExtension>(&level) || !level.voice_activity() || level.level() != 37 || incoming.Ssrc() != 1234) return false;
  std::printf("event log protobuf=%d: start=%d bytes=%zu; RTP extension passed\n",
              PULSEBEAM_EXPECT_PROTOBUF, started, bytes->size());
  return true;
}
}  // namespace

bool SoftwareImplementations(const webrtc::Environment& env) {
  auto encoders = webrtc::CreateBuiltinVideoEncoderFactory();
  auto decoders = webrtc::CreateBuiltinVideoDecoderFactory();
  for (const char* name : {"VP8", "VP9", "AV1", "H264"}) {
    bool found = false;
    for (const auto& format : encoders->GetSupportedFormats()) {
      if (format.name != name) continue;
      found = true;
      if (!Video(env, *encoders, *decoders, format)) {
        std::fprintf(stderr, "video %s implementation failed\n", name);
        return false;
      }
      if (format.name == "H264") {
        for (unsigned layers : {2u, 3u})
          if (!Video(env, *encoders, *decoders, format, layers)) return false;
      }
      std::printf("video %s: encode/decode and native metadata passed\n", name);
    }
    if (!found) { std::fprintf(stderr, "missing required video codec %s\n", name); return false; }
  }
  return Audio(env) && Processing(env) && Helpers(env);
}
#endif
