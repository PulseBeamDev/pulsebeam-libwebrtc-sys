#include <algorithm>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <memory>
#include <string>
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

namespace {

bool HasName(const std::vector<webrtc::SdpVideoFormat> &formats,
             const std::string &name) {
  return std::any_of(formats.begin(), formats.end(),
                     [&](const auto &format) { return format.name == name; });
}

class SeededRandom final : public webrtc::RandomGenerator {
public:
  explicit SeededRandom(uint64_t state) : state_(state) {}

  bool Generate(void *output, size_t length) override {
    auto *bytes = static_cast<uint8_t *>(output);
    for (size_t i = 0; i < length; ++i) {
      state_ ^= state_ << 13;
      state_ ^= state_ >> 7;
      state_ ^= state_ << 17;
      bytes[i] = static_cast<uint8_t>(state_);
    }
    return true;
  }

private:
  uint64_t state_;
};

class CountingTaskQueues final : public webrtc::TaskQueueFactory {
public:
  CountingTaskQueues() : inner_(webrtc::CreateDefaultTaskQueueFactory()) {}

  std::unique_ptr<webrtc::TaskQueueBase, webrtc::TaskQueueDeleter>
  CreateTaskQueue(absl::string_view name, Priority priority) const override {
    ++created_;
    return inner_->CreateTaskQueue(name, priority);
  }

  int created() const { return created_; }

private:
  std::unique_ptr<webrtc::TaskQueueFactory> inner_;
  mutable int created_ = 0;
};

class SimulatedPacketSockets final : public webrtc::PacketSocketFactory {
public:
  std::unique_ptr<webrtc::AsyncPacketSocket>
  CreateUdpSocket(const webrtc::Environment &, const webrtc::SocketAddress &,
                  uint16_t, uint16_t) override {
    ++requests_;
    return nullptr;
  }

  std::unique_ptr<webrtc::AsyncListenSocket>
  CreateServerTcpSocket(const webrtc::Environment &,
                        const webrtc::SocketAddress &, uint16_t, uint16_t,
                        int) override {
    ++requests_;
    return nullptr;
  }

  std::unique_ptr<webrtc::AsyncPacketSocket>
  CreateClientTcpSocket(const webrtc::Environment &,
                        const webrtc::SocketAddress &,
                        const webrtc::SocketAddress &,
                        const webrtc::PacketSocketTcpOptions &) override {
    ++requests_;
    return nullptr;
  }

  std::unique_ptr<webrtc::AsyncDnsResolverInterface>
  CreateAsyncDnsResolver() override {
    ++requests_;
    return nullptr;
  }

  int requests() const { return requests_; }

private:
  int requests_ = 0;
};

class ExternalEncoderFactory final : public webrtc::VideoEncoderFactory {
public:
  std::vector<webrtc::SdpVideoFormat> GetSupportedFormats() const override {
    return {webrtc::SdpVideoFormat("H264")};
  }

  std::unique_ptr<webrtc::VideoEncoder>
  Create(const webrtc::Environment &, const webrtc::SdpVideoFormat &) override {
    return nullptr;
  }
};

class ExternalDecoderFactory final : public webrtc::VideoDecoderFactory {
public:
  std::vector<webrtc::SdpVideoFormat> GetSupportedFormats() const override {
    return {webrtc::SdpVideoFormat("H264")};
  }

  std::unique_ptr<webrtc::VideoDecoder>
  Create(const webrtc::Environment &, const webrtc::SdpVideoFormat &) override {
    return nullptr;
  }
};

int Fail(const std::string &message) {
  std::cerr << "core smoke test: " << message << '\n';
  return 1;
}

} // namespace

int main() {
  webrtc::SetRandomGenerator(std::make_unique<SeededRandom>(0x150));
  const uint64_t first_random_id = webrtc::CreateRandomId64();
  webrtc::SetRandomGenerator(std::make_unique<SeededRandom>(0x150));
  if (first_random_id == 0 || first_random_id != webrtc::CreateRandomId64()) {
    return Fail("seeded WebRTC randomness is not repeatable");
  }
  webrtc::SetDefaultRandomGenerator();

  auto clock = std::make_unique<webrtc::SimulatedClock>(123000);
  auto queues = std::make_unique<CountingTaskQueues>();
  CountingTaskQueues *queue_probe = queues.get();
  webrtc::EnvironmentFactory environment_factory;
  environment_factory.Set(std::move(clock));
  environment_factory.Set(std::move(queues));
  const webrtc::Environment environment = environment_factory.Create();
  if (environment.clock().CurrentTime().us() != 123000) {
    return Fail("caller clock was not installed");
  }
  auto probe_queue = environment.task_queue_factory().CreateTaskQueue(
      "consumer-probe", webrtc::TaskQueueFactory::Priority::kNormal);
  if (!probe_queue || queue_probe->created() != 1) {
    return Fail("caller task queue factory was not used");
  }
  probe_queue.reset();

  auto audio_encoders = webrtc::CreateBuiltinAudioEncoderFactory();
  auto audio_decoders = webrtc::CreateBuiltinAudioDecoderFactory();
  const auto encoded_audio = audio_encoders->GetSupportedEncoders();
  const auto decoded_audio = audio_decoders->GetSupportedDecoders();
  const auto has_opus = [](const auto &codec) {
    return codec.format.name == "opus";
  };
  if (!std::any_of(encoded_audio.begin(), encoded_audio.end(), has_opus) ||
      !std::any_of(decoded_audio.begin(), decoded_audio.end(), has_opus)) {
    return Fail("built-in Opus audio codec is unavailable");
  }

  auto built_in_encoders = webrtc::CreateBuiltinVideoEncoderFactory();
  auto built_in_decoders = webrtc::CreateBuiltinVideoDecoderFactory();
  const auto encoder_formats = built_in_encoders->GetSupportedFormats();
  const auto decoder_formats = built_in_decoders->GetSupportedFormats();
  for (const char *codec : {"VP8", "VP9", "AV1"}) {
    if (!HasName(encoder_formats, codec) || !HasName(decoder_formats, codec)) {
      return Fail(std::string("built-in codec is unavailable: ") + codec);
    }
  }
  if (HasName(encoder_formats, "H264") || HasName(decoder_formats, "H264")) {
    return Fail("built-in H.264 must be disabled");
  }

  auto network = webrtc::Thread::CreateWithSocketServer();
  auto worker = webrtc::Thread::Create();
  auto signaling = webrtc::Thread::Create();
  if (!network->Start() || !worker->Start() || !signaling->Start()) {
    return Fail("caller-owned thread startup failed");
  }

  webrtc::PeerConnectionFactoryDependencies dependencies;
  dependencies.network_thread = network.get();
  dependencies.worker_thread = worker.get();
  dependencies.signaling_thread = signaling.get();
  dependencies.env = environment;
  auto packet_sockets = std::make_unique<SimulatedPacketSockets>();
  SimulatedPacketSockets *packet_socket_probe = packet_sockets.get();
  packet_socket_probe->CreateUdpSocket(environment, webrtc::SocketAddress(), 0,
                                       0);
  if (packet_socket_probe->requests() != 1) {
    return Fail("simulated packet socket factory was not callable");
  }
  dependencies.packet_socket_factory = std::move(packet_sockets);
  dependencies.video_encoder_factory =
      std::make_unique<ExternalEncoderFactory>();
  dependencies.video_decoder_factory =
      std::make_unique<ExternalDecoderFactory>();
  auto peer_connection_factory =
      webrtc::CreateModularPeerConnectionFactory(std::move(dependencies));
  if (!peer_connection_factory) {
    return Fail("injected peer connection factory construction failed");
  }

  std::cout << "core controls/codecs smoke test passed\n";
  return 0;
}
