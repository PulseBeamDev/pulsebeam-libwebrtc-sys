#include <cstdint>
#include <memory>
#include <vector>

#include "api/audio_codecs/builtin_audio_decoder_factory.h"
#include "api/audio_codecs/builtin_audio_encoder_factory.h"
#include "api/create_modular_peer_connection_factory.h"
#include "api/environment/environment_factory.h"
#include "api/packet_socket_factory.h"
#include "api/task_queue/default_task_queue_factory.h"
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

int main() {
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
  return random_id == 0 || !factory || !audio_encoders || !audio_decoders ||
                 !video_encoders || !video_decoders
             ? 1
             : 0;
}
