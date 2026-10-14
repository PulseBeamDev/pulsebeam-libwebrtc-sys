#include <algorithm>
#include <cstdint>
#include <cstring>
#include <memory>
#include <vector>

#include "api/audio_codecs/builtin_audio_decoder_factory.h"
#include "api/audio_codecs/builtin_audio_encoder_factory.h"
#include "api/create_modular_peer_connection_factory.h"
#include "api/environment/environment_factory.h"
#include "api/packet_socket_factory.h"
#include "api/task_queue/default_task_queue_factory.h"
#include "api/video/i420_buffer.h"
#include "api/video/video_bitrate_allocation.h"
#include "api/video_codecs/video_codec.h"
#include "api/video_codecs/builtin_video_decoder_factory.h"
#include "api/video_codecs/builtin_video_encoder_factory.h"
#include "rtc_base/crypto_random.h"
#include "rtc_base/thread.h"
#include "system_wrappers/include/clock.h"

class SeededRandom final : public webrtc::RandomGenerator {
public:
  bool Generate(void *output, size_t length) override {
    auto *bytes = static_cast<uint8_t *>(output);
    for (size_t i = 0; i < length; ++i)
      bytes[i] = static_cast<uint8_t>(i);
    return true;
  }
};

class PacketSockets final : public webrtc::PacketSocketFactory {
public:
  std::unique_ptr<webrtc::AsyncPacketSocket>
  CreateUdpSocket(const webrtc::Environment &, const webrtc::SocketAddress &,
                  uint16_t, uint16_t) override {
    return nullptr;
  }
  std::unique_ptr<webrtc::AsyncListenSocket>
  CreateServerTcpSocket(const webrtc::Environment &,
                        const webrtc::SocketAddress &, uint16_t, uint16_t,
                        int) override {
    return nullptr;
  }
  std::unique_ptr<webrtc::AsyncPacketSocket>
  CreateClientTcpSocket(const webrtc::Environment &,
                        const webrtc::SocketAddress &,
                        const webrtc::SocketAddress &,
                        const webrtc::PacketSocketTcpOptions &) override {
    return nullptr;
  }
  std::unique_ptr<webrtc::AsyncDnsResolverInterface>
  CreateAsyncDnsResolver() override {
    return nullptr;
  }
};

class EncoderFactory final : public webrtc::VideoEncoderFactory {
public:
  std::vector<webrtc::SdpVideoFormat> GetSupportedFormats() const override {
    return {webrtc::SdpVideoFormat("H264")};
  }
  std::unique_ptr<webrtc::VideoEncoder>
  Create(const webrtc::Environment &, const webrtc::SdpVideoFormat &) override {
    return nullptr;
  }
};

class DecoderFactory final : public webrtc::VideoDecoderFactory {
public:
  std::vector<webrtc::SdpVideoFormat> GetSupportedFormats() const override {
    return {webrtc::SdpVideoFormat("H264")};
  }
  std::unique_ptr<webrtc::VideoDecoder>
  Create(const webrtc::Environment &, const webrtc::SdpVideoFormat &) override {
    return nullptr;
  }
};

#if defined(__linux__) && defined(__x86_64__)
// A shipped-archive gate, not a codec-shaped mock: these callbacks consume
// actual OpenH264 bytes and FFmpeg output using only public SDK headers.
class H264Encoded final : public webrtc::EncodedImageCallback {
public:
  std::vector<webrtc::EncodedImage> images;
  Result OnEncodedImage(const webrtc::EncodedImage &image,
                        const webrtc::CodecSpecificInfo *) override {
    images.push_back(image);
    return Result(Result::OK);
  }
  void OnFrameDropped(uint32_t, int, bool) override {}
};

class H264Decoded final : public webrtc::DecodedImageCallback {
public:
  unsigned count = 0;
  bool valid = true;
  int32_t Decoded(webrtc::VideoFrame &frame) override {
    auto buffer = frame.video_frame_buffer()->ToI420();
    valid = valid && buffer && buffer->width() == 64 && buffer->height() == 64;
    if (valid) {
      for (int row = 0; row < 64; ++row) {
        for (int column = 0; column < 64; ++column) {
          const int sample = buffer->DataY()[row * buffer->StrideY() + column];
          valid = valid && sample >= 46 && sample <= 50;
        }
      }
    }
    ++count;
    return 0;
  }
};

static bool H264Roundtrip(const webrtc::Environment &environment,
                          webrtc::VideoEncoderFactory &encoders,
                          webrtc::VideoDecoderFactory &decoders) {
  const webrtc::SdpVideoFormat format(
      "H264", {{"profile-level-id", "42e01f"},
               {"packetization-mode", "1"}, {"level-asymmetry-allowed", "1"}});
  const auto encoder_formats = encoders.GetSupportedFormats();
  const auto decoder_formats = decoders.GetSupportedFormats();
  const auto advertises_h264 = [](const auto &formats) {
    return std::any_of(formats.begin(), formats.end(),
                       [](const auto &candidate) { return candidate.name == "H264"; });
  };
  if (!advertises_h264(encoder_formats) || !advertises_h264(decoder_formats))
    return false;
  H264Encoded encoded;
  H264Decoded decoded;
  auto encoder = encoders.Create(environment, format);
  auto decoder = decoders.Create(environment, format);
  if (!encoder || !decoder)
    return false;
  webrtc::VideoCodec codec{};
  codec.codecType = webrtc::kVideoCodecH264;
  codec.width = 64;
  codec.height = 64;
  codec.minBitrate = 16;
  codec.startBitrate = 256;
  codec.maxBitrate = 512;
  codec.maxFramerate = 30;
  codec.H264()->numberOfTemporalLayers = 1;
  webrtc::VideoDecoder::Settings settings;
  settings.set_codec_type(webrtc::kVideoCodecH264);
  settings.set_number_of_cores(1);
  settings.set_max_render_resolution({64, 64});
  if (encoder->InitEncode(&codec, {webrtc::VideoEncoder::Capabilities(false), 1, 1200}) != 0 ||
      encoder->RegisterEncodeCompleteCallback(&encoded) != 0 ||
      !decoder->Configure(settings) ||
      decoder->RegisterDecodeCompleteCallback(&decoded) != 0)
    return false;
  for (unsigned index = 0; index < 3; ++index) {
    auto buffer = webrtc::I420Buffer::Create(64, 64);
    std::memset(buffer->MutableDataY(), 48, 64 * 64);
    std::memset(buffer->MutableDataU(), 96, 32 * 32);
    std::memset(buffer->MutableDataV(), 160, 32 * 32);
    auto frame = webrtc::VideoFrame::Builder()
                     .set_video_frame_buffer(buffer)
                     .set_timestamp_us(123000 + index * 33333)
                     .set_rtp_timestamp(9000 + index * 3000)
                     .build();
    std::vector<webrtc::VideoFrameType> types{
        index == 0 ? webrtc::VideoFrameType::kVideoFrameKey
                   : webrtc::VideoFrameType::kVideoFrameDelta};
    if (encoder->Encode(frame, &types) != 0)
      return false;
  }
  for (const auto &image : encoded.images) {
    if (image.size() == 0 || decoder->Decode(image, 0) != 0)
      return false;
  }
  const auto encoder_release = encoder->Release();
  const auto decoder_release = decoder->Release();
  return encoder_release == 0 && decoder_release == 0 &&
         encoded.images.size() == 3 && decoded.count == 3 && decoded.valid;
}
#endif

void FrameCallbackSlotRegression();
bool VideoFrameMetadataInitializationRegression();

int main() {
  FrameCallbackSlotRegression();
  if (!VideoFrameMetadataInitializationRegression()) return 3;
  webrtc::SetRandomGenerator(std::make_unique<SeededRandom>());
  auto random_id = webrtc::CreateRandomId64();
  webrtc::SetDefaultRandomGenerator();
  webrtc::EnvironmentFactory environment_factory;
  environment_factory.Set(std::make_unique<webrtc::SimulatedClock>(123000));
  environment_factory.Set(webrtc::CreateDefaultTaskQueueFactory());
  auto environment = environment_factory.Create();
  auto network = webrtc::Thread::CreateWithSocketServer();
  auto worker = webrtc::Thread::Create();
  auto signaling = webrtc::Thread::Create();
  // Supplied SDK threads must run before factory creation dispatches to them.
  if (!network->Start() || !worker->Start() || !signaling->Start())
    return 4;
  webrtc::PeerConnectionFactoryDependencies dependencies;
  dependencies.env = environment;
  dependencies.network_thread = network.get();
  dependencies.worker_thread = worker.get();
  dependencies.signaling_thread = signaling.get();
  dependencies.packet_socket_factory = std::make_unique<PacketSockets>();
  dependencies.video_encoder_factory = std::make_unique<EncoderFactory>();
  dependencies.video_decoder_factory = std::make_unique<DecoderFactory>();
  auto factory =
      webrtc::CreateModularPeerConnectionFactory(std::move(dependencies));
  auto audio_encoders = webrtc::CreateBuiltinAudioEncoderFactory();
  auto audio_decoders = webrtc::CreateBuiltinAudioDecoderFactory();
  auto video_encoders = webrtc::CreateBuiltinVideoEncoderFactory();
  auto video_decoders = webrtc::CreateBuiltinVideoDecoderFactory();
  if (random_id == 0 || !factory || !audio_encoders || !audio_decoders ||
      !video_encoders || !video_decoders)
    return 1;
#if defined(__linux__) && defined(__x86_64__)
  if (!H264Roundtrip(environment, *video_encoders, *video_decoders))
    return 2;
#endif
  return 0;
}
