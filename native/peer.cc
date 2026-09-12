#include "pulsebeam-webrtc-sys/native/peer.h"

#include <deque>
#include <mutex>
#include <optional>
#include <set>
#include <string>
#include <utility>
#include <vector>

#include "api/create_modular_peer_connection_factory.h"
#include "api/enable_media.h"
#include "api/jsep.h"
#include "api/make_ref_counted.h"
#include "api/peer_connection_interface.h"
#include "api/rtc_error.h"
#include "api/scoped_refptr.h"
#include "api/video_codecs/video_decoder_factory.h"
#include "api/video_codecs/video_encoder_factory.h"
#include "pulsebeam-webrtc-sys/native/codec.h"
#include "pulsebeam-webrtc-sys/native/execution.h"
#include "pulsebeam-webrtc-sys/native/network.h"
#include "pulsebeam-webrtc-sys/src/lib.rs.h"
#include "rtc_base/thread.h"

namespace pulsebeam::webrtc_sys {
namespace {

enum EventKind : std::uint8_t {
  kNoEvent = 0,
  kOperationComplete = 1,
  kIceCandidate = 2,
  kConnectionState = 3,
  kSignalingState = 4,
  kIceGatheringState = 5,
  kNegotiationNeeded = 6,
  kIceCandidateError = 7,
  kClosed = 8,
};

constexpr std::uint8_t kClosedError = 255;

FfiPeerEvent EmptyEvent() { return FfiPeerEvent{}; }

FfiPeerEvent ErrorEvent(std::uint64_t operation_id,
                        webrtc::RTCErrorType type,
                        const std::string& message) {
  FfiPeerEvent event;
  event.kind = kOperationComplete;
  event.operation_id = operation_id;
  event.error_type = static_cast<std::uint8_t>(type);
  event.message = message;
  return event;
}

FfiPeerEvent ClosedOperation(std::uint64_t operation_id) {
  FfiPeerEvent event;
  event.kind = kOperationComplete;
  event.operation_id = operation_id;
  event.error_type = kClosedError;
  event.message = "peer connection is closed";
  return event;
}

struct EventState {
  bool Begin(std::uint64_t operation_id) {
    std::lock_guard lock(mutex);
    if (closed) {
      events.push_back(ClosedOperation(operation_id));
      return false;
    }
    return pending.insert(operation_id).second;
  }

  void Complete(FfiPeerEvent event) {
    std::lock_guard lock(mutex);
    if (pending.erase(event.operation_id) != 0) {
      events.push_back(std::move(event));
    }
  }

  void Push(FfiPeerEvent event) {
    std::lock_guard lock(mutex);
    if (!closed) {
      events.push_back(std::move(event));
    }
  }

  FfiPeerEvent Take() {
    std::lock_guard lock(mutex);
    if (events.empty()) {
      return EmptyEvent();
    }
    FfiPeerEvent event = std::move(events.front());
    events.pop_front();
    return event;
  }

  void Close() {
    std::lock_guard lock(mutex);
    if (closed) {
      return;
    }
    closed = true;
    for (std::uint64_t operation_id : pending) {
      events.push_back(ClosedOperation(operation_id));
    }
    pending.clear();
    FfiPeerEvent event;
    event.kind = kClosed;
    events.push_back(std::move(event));
  }

  std::mutex mutex;
  std::deque<FfiPeerEvent> events;
  std::set<std::uint64_t> pending;
  bool closed = false;
};

class PeerObserver final : public webrtc::PeerConnectionObserver {
 public:
  explicit PeerObserver(std::shared_ptr<EventState> state) noexcept
      : state_(std::move(state)) {}

  void OnSignalingChange(
      webrtc::PeerConnectionInterface::SignalingState state) override {
    FfiPeerEvent event;
    event.kind = kSignalingState;
    event.state = static_cast<std::uint8_t>(state);
    state_->Push(std::move(event));
  }

  void OnDataChannel(
      webrtc::scoped_refptr<webrtc::DataChannelInterface>) override {}

  void OnNegotiationNeededEvent(std::uint32_t event_id) override {
    FfiPeerEvent event;
    event.kind = kNegotiationNeeded;
    event.event_id = event_id;
    state_->Push(std::move(event));
  }

  void OnConnectionChange(
      webrtc::PeerConnectionInterface::PeerConnectionState state) override {
    FfiPeerEvent event;
    event.kind = kConnectionState;
    event.state = static_cast<std::uint8_t>(state);
    state_->Push(std::move(event));
  }

  void OnIceGatheringChange(
      webrtc::PeerConnectionInterface::IceGatheringState state) override {
    FfiPeerEvent event;
    event.kind = kIceGatheringState;
    event.state = static_cast<std::uint8_t>(state);
    state_->Push(std::move(event));
  }

  void OnIceCandidate(const webrtc::IceCandidate* candidate) override {
    if (candidate == nullptr) {
      return;
    }
    FfiPeerEvent event;
    event.kind = kIceCandidate;
    event.sdp_mid = candidate->sdp_mid();
    event.sdp_mline_index = candidate->sdp_mline_index();
    event.candidate = candidate->ToString();
    state_->Push(std::move(event));
  }

  void OnIceCandidateError(const std::string& address,
                           int port,
                           const std::string& url,
                           int error_code,
                           const std::string& error_text) override {
    FfiPeerEvent event;
    event.kind = kIceCandidateError;
    event.address = address;
    event.port = port;
    event.url = url;
    event.error_code = error_code;
    event.message = error_text;
    state_->Push(std::move(event));
  }

 private:
  std::shared_ptr<EventState> state_;
};

class CreateDescriptionObserver
    : public webrtc::CreateSessionDescriptionObserver {
 public:
  CreateDescriptionObserver(std::shared_ptr<EventState> state,
                            std::uint64_t operation_id) noexcept
      : state_(std::move(state)), operation_id_(operation_id) {}

  void OnSuccess(webrtc::SessionDescriptionInterface* description) override {
    std::unique_ptr<webrtc::SessionDescriptionInterface> owned(description);
    FfiPeerEvent event;
    event.kind = kOperationComplete;
    event.operation_id = operation_id_;
    if (owned) {
      event.has_description = true;
      event.sdp_type = static_cast<std::uint8_t>(owned->GetType());
      event.sdp = owned->ToString();
    } else {
      event.error_type = static_cast<std::uint8_t>(
          webrtc::RTCErrorType::INTERNAL_ERROR);
      event.message = "WebRTC returned an empty session description";
    }
    state_->Complete(std::move(event));
  }

  void OnFailure(webrtc::RTCError error) override {
    state_->Complete(ErrorEvent(operation_id_, error.type(), error.message()));
  }

 private:
  std::shared_ptr<EventState> state_;
  std::uint64_t operation_id_;
};

class SetLocalObserver
    : public webrtc::SetLocalDescriptionObserverInterface {
 public:
  SetLocalObserver(std::shared_ptr<EventState> state,
                   std::uint64_t operation_id) noexcept
      : state_(std::move(state)), operation_id_(operation_id) {}

  void OnSetLocalDescriptionComplete(webrtc::RTCError error) override {
    FfiPeerEvent event = error.ok()
                             ? EmptyEvent()
                             : ErrorEvent(operation_id_, error.type(),
                                          error.message());
    event.kind = kOperationComplete;
    event.operation_id = operation_id_;
    state_->Complete(std::move(event));
  }

 private:
  std::shared_ptr<EventState> state_;
  std::uint64_t operation_id_;
};

class SetRemoteObserver
    : public webrtc::SetRemoteDescriptionObserverInterface {
 public:
  SetRemoteObserver(std::shared_ptr<EventState> state,
                    std::uint64_t operation_id) noexcept
      : state_(std::move(state)), operation_id_(operation_id) {}

  void OnSetRemoteDescriptionComplete(webrtc::RTCError error) override {
    FfiPeerEvent event = error.ok()
                             ? EmptyEvent()
                             : ErrorEvent(operation_id_, error.type(),
                                          error.message());
    event.kind = kOperationComplete;
    event.operation_id = operation_id_;
    state_->Complete(std::move(event));
  }

 private:
  std::shared_ptr<EventState> state_;
  std::uint64_t operation_id_;
};

class BorrowedVideoEncoderFactory final : public webrtc::VideoEncoderFactory {
 public:
  explicit BorrowedVideoEncoderFactory(
      const NativeVideoEncoderFactory& factory) noexcept
      : factory_(factory) {}

  std::vector<webrtc::SdpVideoFormat> GetSupportedFormats() const override {
    return factory_.factory().GetSupportedFormats();
  }
  CodecSupport QueryCodecSupport(
      const webrtc::SdpVideoFormat& format,
      std::optional<std::string> scalability_mode,
      std::optional<webrtc::Resolution> resolution) const override {
    return factory_.factory().QueryCodecSupport(format, scalability_mode,
                                                resolution);
  }
  std::unique_ptr<webrtc::VideoEncoder> Create(
      const webrtc::Environment& environment,
      const webrtc::SdpVideoFormat& format) override {
    return factory_.factory().Create(environment, format);
  }

 private:
  const NativeVideoEncoderFactory& factory_;
};

class BorrowedVideoDecoderFactory final : public webrtc::VideoDecoderFactory {
 public:
  explicit BorrowedVideoDecoderFactory(
      const NativeVideoDecoderFactory& factory) noexcept
      : factory_(factory) {}

  std::vector<webrtc::SdpVideoFormat> GetSupportedFormats() const override {
    return factory_.factory().GetSupportedFormats();
  }
  CodecSupport QueryCodecSupport(
      const webrtc::SdpVideoFormat& format,
      bool reference_scaling,
      std::optional<webrtc::Resolution> resolution) const override {
    return factory_.factory().QueryCodecSupport(format, reference_scaling,
                                                resolution);
  }
  std::unique_ptr<webrtc::VideoDecoder> Create(
      const webrtc::Environment& environment,
      const webrtc::SdpVideoFormat& format) override {
    return factory_.factory().Create(environment, format);
  }

 private:
  const NativeVideoDecoderFactory& factory_;
};

std::optional<webrtc::SdpType> SdpType(std::uint8_t value) {
  if (value <= static_cast<std::uint8_t>(webrtc::SdpType::kRollback)) {
    return static_cast<webrtc::SdpType>(value);
  }
  return std::nullopt;
}

std::unique_ptr<webrtc::SessionDescriptionInterface> ParseDescription(
    std::uint8_t type,
    rust::Str sdp,
    webrtc::SdpParseError& error) {
  const auto parsed_type = SdpType(type);
  if (!parsed_type) {
    error.description = "invalid session description type";
    return nullptr;
  }
  return webrtc::CreateSessionDescription(
      *parsed_type, std::string_view(sdp.data(), sdp.size()), &error);
}

}  // namespace

struct NativePeerConnectionFactory::State {
  webrtc::scoped_refptr<webrtc::PeerConnectionFactoryInterface> factory;
  webrtc::Thread* signaling_thread = nullptr;
};

struct NativePeerConnection::State {
  webrtc::scoped_refptr<webrtc::PeerConnectionInterface> peer;
  std::shared_ptr<EventState> events;
  std::unique_ptr<PeerObserver> observer;
  webrtc::Thread* signaling_thread = nullptr;
  bool closed = false;
};

NativePeerConnectionFactory::NativePeerConnectionFactory(
    std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativePeerConnectionFactory::~NativePeerConnectionFactory() {
  if (state_ && state_->factory && state_->signaling_thread) {
    state_->signaling_thread->BlockingCall([this] { state_->factory = nullptr; });
  }
}
const std::unique_ptr<NativePeerConnectionFactory::State>&
NativePeerConnectionFactory::state() const noexcept {
  return state_;
}

NativePeerConnection::NativePeerConnection(std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativePeerConnection::~NativePeerConnection() {
  close_peer_connection(*this);
  if (state_ && state_->peer && state_->signaling_thread) {
    state_->signaling_thread->BlockingCall([this] { state_->peer = nullptr; });
  }
  if (state_) {
    state_->observer.reset();
  }
}
const std::unique_ptr<NativePeerConnection::State>&
NativePeerConnection::state() const noexcept {
  return state_;
}

std::unique_ptr<NativePeerConnectionFactory> new_peer_connection_factory(
    const NativeEnvironment& environment,
    const NativeThread& network_thread,
    const NativeThread& worker_thread,
    const NativeThread& signaling_thread,
    const NativeNetworkManagerProvider* network_manager,
    const NativePacketSocketFactoryProvider* packet_socket_factory,
    const NativeAudioEncoderFactory* audio_encoder,
    const NativeAudioDecoderFactory* audio_decoder,
    const NativeVideoEncoderFactory* video_encoder,
    const NativeVideoDecoderFactory* video_decoder,
    rust::String& error) noexcept {
  webrtc::PeerConnectionFactoryDependencies dependencies;
  dependencies.env = environment.environment();
  dependencies.network_thread = network_thread.thread();
  dependencies.worker_thread = worker_thread.thread();
  dependencies.signaling_thread = signaling_thread.thread();
  if (!dependencies.network_thread || !dependencies.worker_thread ||
      !dependencies.signaling_thread) {
    error = "a required WebRTC thread is closed";
    return nullptr;
  }
  if (network_manager) {
    dependencies.network_manager = network_manager->Create();
    if (!dependencies.network_manager) {
      error = "failed to create the network manager";
      return nullptr;
    }
  }
  if (packet_socket_factory) {
    dependencies.packet_socket_factory = packet_socket_factory->Create();
    if (!dependencies.packet_socket_factory) {
      error = "failed to create the packet socket factory";
      return nullptr;
    }
  }
  if (audio_encoder) {
    dependencies.audio_encoder_factory = audio_encoder->factory();
  }
  if (audio_decoder) {
    dependencies.audio_decoder_factory = audio_decoder->factory();
  }
  if (video_encoder) {
    dependencies.video_encoder_factory =
        std::make_unique<BorrowedVideoEncoderFactory>(*video_encoder);
  }
  if (video_decoder) {
    dependencies.video_decoder_factory =
        std::make_unique<BorrowedVideoDecoderFactory>(*video_decoder);
  }
  if (audio_encoder || audio_decoder || video_encoder || video_decoder) {
    webrtc::EnableMedia(dependencies);
  }
  auto factory =
      webrtc::CreateModularPeerConnectionFactory(std::move(dependencies));
  if (!factory) {
    error = "CreateModularPeerConnectionFactory failed";
    return nullptr;
  }
  auto state = std::make_unique<NativePeerConnectionFactory::State>();
  state->factory = std::move(factory);
  state->signaling_thread = signaling_thread.thread();
  return std::make_unique<NativePeerConnectionFactory>(std::move(state));
}

std::unique_ptr<NativePeerConnection> create_peer_connection(
    const NativePeerConnectionFactory& factory,
    std::uint16_t ice_candidate_pool_size,
    bool always_negotiate_data_channels,
    rust::String& error) noexcept {
  auto events = std::make_shared<EventState>();
  auto observer = std::make_unique<PeerObserver>(events);
  webrtc::PeerConnectionDependencies dependencies(observer.get());
  webrtc::PeerConnectionInterface::RTCConfiguration configuration;
  configuration.ice_candidate_pool_size = ice_candidate_pool_size;
  configuration.always_negotiate_data_channels =
      always_negotiate_data_channels;
  auto result = factory.state()->factory->CreatePeerConnectionOrError(
      configuration, std::move(dependencies));
  if (!result.ok()) {
    error = result.error().message();
    return nullptr;
  }
  auto state = std::make_unique<NativePeerConnection::State>();
  state->peer = result.MoveValue();
  state->events = std::move(events);
  state->observer = std::move(observer);
  state->signaling_thread = factory.state()->signaling_thread;
  return std::make_unique<NativePeerConnection>(std::move(state));
}

void peer_create_offer(const NativePeerConnection& peer,
                       std::uint64_t operation_id) noexcept {
  const auto& state = peer.state();
  if (!state->events->Begin(operation_id)) {
    return;
  }
  auto observer =
      webrtc::make_ref_counted<CreateDescriptionObserver>(state->events,
                                                          operation_id);
  state->peer->CreateOffer(observer.get(), {});
}

void peer_create_answer(const NativePeerConnection& peer,
                        std::uint64_t operation_id) noexcept {
  const auto& state = peer.state();
  if (!state->events->Begin(operation_id)) {
    return;
  }
  auto observer =
      webrtc::make_ref_counted<CreateDescriptionObserver>(state->events,
                                                          operation_id);
  state->peer->CreateAnswer(observer.get(), {});
}

void peer_set_local_description(const NativePeerConnection& peer,
                                std::uint64_t operation_id,
                                std::uint8_t sdp_type,
                                rust::Str sdp) noexcept {
  const auto& state = peer.state();
  if (!state->events->Begin(operation_id)) {
    return;
  }
  webrtc::SdpParseError parse_error;
  auto description = ParseDescription(sdp_type, sdp, parse_error);
  if (!description) {
    state->events->Complete(ErrorEvent(
        operation_id, webrtc::RTCErrorType::SYNTAX_ERROR,
        parse_error.description));
    return;
  }
  state->peer->SetLocalDescription(
      std::move(description),
      webrtc::make_ref_counted<SetLocalObserver>(state->events, operation_id));
}

void peer_set_remote_description(const NativePeerConnection& peer,
                                 std::uint64_t operation_id,
                                 std::uint8_t sdp_type,
                                 rust::Str sdp) noexcept {
  const auto& state = peer.state();
  if (!state->events->Begin(operation_id)) {
    return;
  }
  webrtc::SdpParseError parse_error;
  auto description = ParseDescription(sdp_type, sdp, parse_error);
  if (!description) {
    state->events->Complete(ErrorEvent(
        operation_id, webrtc::RTCErrorType::SYNTAX_ERROR,
        parse_error.description));
    return;
  }
  state->peer->SetRemoteDescription(
      std::move(description),
      webrtc::make_ref_counted<SetRemoteObserver>(state->events, operation_id));
}

void peer_add_ice_candidate(const NativePeerConnection& peer,
                            std::uint64_t operation_id,
                            rust::Str sdp_mid,
                            std::int32_t sdp_mline_index,
                            rust::Str candidate_sdp) noexcept {
  const auto& state = peer.state();
  if (!state->events->Begin(operation_id)) {
    return;
  }
  webrtc::SdpParseError parse_error;
  auto candidate = webrtc::IceCandidate::Create(
      std::string_view(sdp_mid.data(), sdp_mid.size()), sdp_mline_index,
      std::string_view(candidate_sdp.data(), candidate_sdp.size()),
      &parse_error);
  if (!candidate) {
    state->events->Complete(ErrorEvent(
        operation_id, webrtc::RTCErrorType::SYNTAX_ERROR,
        parse_error.description));
    return;
  }
  state->peer->AddIceCandidate(
      std::move(candidate),
      [events = state->events, operation_id](webrtc::RTCError error) {
        FfiPeerEvent event = error.ok()
                                 ? EmptyEvent()
                                 : ErrorEvent(operation_id, error.type(),
                                              error.message());
        event.kind = kOperationComplete;
        event.operation_id = operation_id;
        events->Complete(std::move(event));
      });
}

FfiPeerEvent peer_take_event(const NativePeerConnection& peer) noexcept {
  return peer.state()->events->Take();
}

bool close_peer_connection(NativePeerConnection& peer) noexcept {
  const auto& state = peer.state();
  if (!state || state->closed) {
    return true;
  }
  state->signaling_thread->BlockingCall([&] {
    if (!state->closed) {
      state->peer->Close();
      state->events->Close();
      state->closed = true;
    }
  });
  return true;
}

}  // namespace pulsebeam::webrtc_sys
