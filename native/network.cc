#include "pulsebeam-webrtc-sys/native/network.h"

#include <algorithm>
#include <cerrno>
#include <cstring>
#include <deque>
#include <map>
#include <mutex>
#include <optional>
#include <set>
#include <utility>
#include <vector>

#include "api/async_dns_resolver.h"
#include "api/environment/environment.h"
#include "api/environment/environment_factory.h"
#include "api/packet_socket_factory.h"
#include "pulsebeam-webrtc-sys/native/execution.h"
#include "rtc_base/async_packet_socket.h"
#include "rtc_base/ip_address.h"
#include "rtc_base/network.h"
#include "rtc_base/network/received_packet.h"
#include "rtc_base/network/sent_packet.h"
#include "rtc_base/socket.h"
#include "rtc_base/socket_address.h"
#include "rtc_base/thread.h"

namespace pulsebeam::webrtc_sys {
namespace {

enum Error : std::uint8_t {
  kOk = 0,
  kInvalidAddress = 1,
  kInvalidPort = 2,
  kAddressInUse = 3,
  kEndpointClosed = 4,
  kSocketClosed = 5,
  kPacketNotFound = 6,
  kDestinationUnavailable = 7,
  kUnsupportedTransport = 8,
  kUnsupportedDns = 9,
  kNativeConstructionFailed = 10,
};

class SimulatedAsyncPacketSocket;

struct SocketRegistration {
  std::mutex mutex;
  webrtc::Thread* thread = nullptr;
  SimulatedAsyncPacketSocket* socket = nullptr;
};

struct PacketData {
  std::uint64_t id;
  webrtc::SocketAddress source;
  webrtc::SocketAddress destination;
  std::vector<std::uint8_t> payload;
  std::int64_t deadline_us;
};

struct ReceivedData {
  webrtc::SocketAddress source;
  std::vector<std::uint8_t> payload;
};

std::optional<webrtc::IPAddress> ToIp(
    rust::Slice<const std::uint8_t> bytes) noexcept {
  if (bytes.size() == 4) {
    const std::uint32_t value =
        (static_cast<std::uint32_t>(bytes[0]) << 24) |
        (static_cast<std::uint32_t>(bytes[1]) << 16) |
        (static_cast<std::uint32_t>(bytes[2]) << 8) |
        static_cast<std::uint32_t>(bytes[3]);
    return webrtc::IPAddress(value);
  }
  if (bytes.size() == 16) {
    in6_addr value;
    std::memcpy(&value, bytes.data(), bytes.size());
    return webrtc::IPAddress(value);
  }
  return std::nullopt;
}

rust::Vec<std::uint8_t> FromIp(const webrtc::IPAddress& ip) noexcept {
  rust::Vec<std::uint8_t> result;
  if (ip.family() == AF_INET) {
    const std::uint32_t value = ip.v4AddressAsHostOrderInteger();
    result.push_back(static_cast<std::uint8_t>(value >> 24));
    result.push_back(static_cast<std::uint8_t>(value >> 16));
    result.push_back(static_cast<std::uint8_t>(value >> 8));
    result.push_back(static_cast<std::uint8_t>(value));
  } else if (ip.family() == AF_INET6) {
    const in6_addr value = ip.ipv6_address();
    const auto* bytes = reinterpret_cast<const std::uint8_t*>(&value);
    for (std::size_t index = 0; index < 16; ++index) {
      result.push_back(bytes[index]);
    }
  }
  return result;
}

rust::Vec<std::uint8_t> FromBytes(
    const std::vector<std::uint8_t>& bytes) noexcept {
  rust::Vec<std::uint8_t> result;
  result.reserve(bytes.size());
  for (std::uint8_t byte : bytes) {
    result.push_back(byte);
  }
  return result;
}

void CloseRegistration(const std::shared_ptr<SocketRegistration>& registration);

}  // namespace

struct NativeSimulatedNetwork::State {
  explicit State(const NativeManualClock* clock_value) noexcept
      : clock(clock_value), thread(webrtc::Thread::Create()) {}

  const NativeManualClock* clock;
  std::unique_ptr<webrtc::Thread> thread;
  std::mutex mutex;
  std::map<webrtc::IPAddress, std::weak_ptr<NativeNetworkEndpoint::State>>
      endpoints;
  std::map<webrtc::SocketAddress, std::weak_ptr<SocketRegistration>> sockets;
  std::map<std::uint64_t, PacketData> pending;
  std::deque<std::uint64_t> outbound;
  std::uint64_t next_packet_id = 1;
};

struct NativeNetworkEndpoint::State {
  State(std::shared_ptr<NativeSimulatedNetwork::State> network_value,
        webrtc::IPAddress ip_value) noexcept
      : network(std::move(network_value)), ip(std::move(ip_value)) {}

  std::shared_ptr<NativeSimulatedNetwork::State> network;
  webrtc::IPAddress ip;
  std::mutex mutex;
  bool closed = false;
  std::vector<std::weak_ptr<SocketRegistration>> sockets;
};

namespace {

bool EndpointClosed(
    const std::shared_ptr<NativeNetworkEndpoint::State>& endpoint) noexcept {
  std::lock_guard lock(endpoint->mutex);
  return endpoint->closed;
}

void RemoveSocket(const std::shared_ptr<NativeSimulatedNetwork::State>& network,
                  const webrtc::SocketAddress& address,
                  const std::shared_ptr<SocketRegistration>& registration) {
  std::lock_guard lock(network->mutex);
  const auto found = network->sockets.find(address);
  if (found != network->sockets.end() &&
      found->second.lock() == registration) {
    network->sockets.erase(found);
  }
}

class SimulatedAsyncPacketSocket final : public webrtc::AsyncPacketSocket {
 public:
  SimulatedAsyncPacketSocket(
      std::shared_ptr<NativeNetworkEndpoint::State> endpoint,
      webrtc::SocketAddress address,
      std::shared_ptr<SocketRegistration> registration) noexcept
      : endpoint_(std::move(endpoint)),
        address_(std::move(address)),
        registration_(std::move(registration)) {}

  ~SimulatedAsyncPacketSocket() override {
    Close();
    std::lock_guard lock(registration_->mutex);
    registration_->socket = nullptr;
    registration_->thread = nullptr;
  }

  webrtc::SocketAddress GetLocalAddress() const override { return address_; }
  webrtc::SocketAddress GetRemoteAddress() const override { return {}; }

  int Send(const void*,
           std::size_t,
           const webrtc::AsyncSocketPacketOptions&) override {
    error_ = EINVAL;
    return -1;
  }

  int SendTo(const void* data,
             std::size_t size,
             const webrtc::SocketAddress& destination,
             const webrtc::AsyncSocketPacketOptions& options) override {
    if (closed_) {
      error_ = EINVAL;
      return -1;
    }
    if (!destination.IsComplete()) {
      error_ = EINVAL;
      return -1;
    }
    auto network = endpoint_->network;
    PacketData packet;
    {
      std::lock_guard lock(network->mutex);
      packet.id = network->next_packet_id++;
      packet.source = address_;
      packet.destination = destination;
      const auto* bytes = static_cast<const std::uint8_t*>(data);
      packet.payload.assign(bytes, bytes + size);
      packet.deadline_us = manual_clock_time_us(*network->clock);
      network->pending.emplace(packet.id, packet);
      network->outbound.push_back(packet.id);
    }
    webrtc::SentPacketInfo sent(options.packet_id, packet.deadline_us / 1000);
    NotifySentPacket(this, sent);
    return static_cast<int>(size);
  }

  int Close() override {
    if (closed_) {
      return 0;
    }
    closed_ = true;
    RemoveSocket(endpoint_->network, address_, registration_);
    NotifyClosed(0);
    return 0;
  }

  State GetState() const override {
    return closed_ ? STATE_CLOSED : STATE_BOUND;
  }
  int GetOption(webrtc::Socket::Option option, int* value) override {
    const auto found = options_.find(option);
    if (found == options_.end()) {
      error_ = EINVAL;
      return -1;
    }
    *value = found->second;
    return 0;
  }
  int SetOption(webrtc::Socket::Option option, int value) override {
    options_[option] = value;
    return 0;
  }
  int GetError() const override { return error_; }
  void SetError(int error) override { error_ = error; }

  bool Receive(const PacketData& packet) {
    if (closed_ || EndpointClosed(endpoint_)) {
      return false;
    }
    webrtc::ReceivedIpPacket received(
        std::span<const std::uint8_t>(packet.payload), packet.source,
        webrtc::Timestamp::Micros(manual_clock_time_us(*endpoint_->network->clock)));
    NotifyPacketReceived(received);
    return true;
  }

 private:
  std::shared_ptr<NativeNetworkEndpoint::State> endpoint_;
  webrtc::SocketAddress address_;
  std::shared_ptr<SocketRegistration> registration_;
  std::map<webrtc::Socket::Option, int> options_;
  int error_ = 0;
  bool closed_ = false;
};

void CloseRegistration(
    const std::shared_ptr<SocketRegistration>& registration) {
  webrtc::Thread* thread;
  {
    std::lock_guard lock(registration->mutex);
    thread = registration->thread;
  }
  if (thread == nullptr) {
    return;
  }
  thread->BlockingCall([registration] {
    SimulatedAsyncPacketSocket* socket;
    {
      std::lock_guard lock(registration->mutex);
      socket = registration->socket;
    }
    if (socket != nullptr) {
      socket->Close();
    }
  });
}

std::unique_ptr<webrtc::AsyncPacketSocket> BindSocket(
    const std::shared_ptr<NativeNetworkEndpoint::State>& endpoint,
    const webrtc::SocketAddress& requested,
    std::uint16_t min_port,
    std::uint16_t max_port) {
  webrtc::Thread* current_thread = webrtc::Thread::Current();
  if (current_thread == nullptr) {
    return nullptr;
  }
  std::lock_guard endpoint_lock(endpoint->mutex);
  if (endpoint->closed) {
    return nullptr;
  }
  webrtc::IPAddress ip = requested.ipaddr();
  if (ip.IsNil()) {
    ip = endpoint->ip;
  }
  if (ip != endpoint->ip) {
    return nullptr;
  }
  std::uint16_t first = requested.port();
  std::uint16_t last = first;
  if (first == 0) {
    first = min_port == 0 ? 49152 : min_port;
    last = max_port < first ? 65535 : max_port;
  }

  auto network = endpoint->network;
  std::lock_guard lock(network->mutex);
  for (std::uint32_t candidate = first; candidate <= last; ++candidate) {
    const webrtc::SocketAddress address(ip, static_cast<int>(candidate));
    const auto found = network->sockets.find(address);
    if (found != network->sockets.end() && !found->second.expired()) {
      continue;
    }
    auto registration = std::make_shared<SocketRegistration>();
    auto socket = std::make_unique<SimulatedAsyncPacketSocket>(
        endpoint, address, registration);
    registration->thread = current_thread;
    registration->socket = socket.get();
    network->sockets[address] = registration;
    endpoint->sockets.push_back(registration);
    return socket;
  }
  return nullptr;
}

class SimulatedPacketSocketFactory final
    : public webrtc::PacketSocketFactory {
 public:
  explicit SimulatedPacketSocketFactory(
      std::shared_ptr<NativeNetworkEndpoint::State> endpoint) noexcept
      : endpoint_(std::move(endpoint)) {}

  std::unique_ptr<webrtc::AsyncPacketSocket> CreateUdpSocket(
      const webrtc::Environment&,
      const webrtc::SocketAddress& address,
      std::uint16_t min_port,
      std::uint16_t max_port) override {
    return BindSocket(endpoint_, address, min_port, max_port);
  }
  std::unique_ptr<webrtc::AsyncListenSocket> CreateServerTcpSocket(
      const webrtc::Environment&,
      const webrtc::SocketAddress&,
      std::uint16_t,
      std::uint16_t,
      int) override {
    return nullptr;
  }
  std::unique_ptr<webrtc::AsyncPacketSocket> CreateClientTcpSocket(
      const webrtc::Environment&,
      const webrtc::SocketAddress&,
      const webrtc::SocketAddress&,
      const webrtc::PacketSocketTcpOptions&) override {
    return nullptr;
  }
  std::unique_ptr<webrtc::AsyncDnsResolverInterface> CreateAsyncDnsResolver()
      override {
    return nullptr;
  }
  std::unique_ptr<webrtc::AsyncPacketSocket> CreateClientUdpSocket(
      const webrtc::Environment&,
      const webrtc::SocketAddress&,
      const webrtc::SocketAddress&,
      std::uint16_t,
      std::uint16_t,
      const webrtc::PacketSocketTcpOptions&) override {
    return nullptr;
  }

 private:
  std::shared_ptr<NativeNetworkEndpoint::State> endpoint_;
};

class SimulatedNetworkManager final : public webrtc::NetworkManagerBase {
 public:
  explicit SimulatedNetworkManager(
      std::shared_ptr<NativeNetworkEndpoint::State> endpoint) noexcept
      : endpoint_(std::move(endpoint)) {}

  void StartUpdating() override {
    ++started_;
    if (started_ != 1) {
      NotifyNetworksChanged();
      return;
    }
    const int prefix_length = endpoint_->ip.family() == AF_INET ? 24 : 64;
    auto network = std::make_unique<webrtc::Network>(
        "pulsebeam", "pulsebeam simulated network",
        webrtc::TruncateIP(endpoint_->ip, prefix_length), prefix_length,
        webrtc::ADAPTER_TYPE_ETHERNET);
    network->set_default_local_address_provider(this);
    network->AddIP(endpoint_->ip);
    if (endpoint_->ip.family() == AF_INET) {
      set_default_local_addresses(endpoint_->ip, webrtc::IPAddress());
    } else {
      set_default_local_addresses(webrtc::IPAddress(), endpoint_->ip);
    }
    bool changed = false;
    std::vector<std::unique_ptr<webrtc::Network>> networks;
    networks.push_back(std::move(network));
    MergeNetworkList(std::move(networks), &changed);
    NotifyNetworksChanged();
  }

  void StopUpdating() override {
    if (started_ != 0) {
      --started_;
    }
  }

 private:
  std::shared_ptr<NativeNetworkEndpoint::State> endpoint_;
  std::size_t started_ = 0;
};

void CloseEndpoint(
    const std::shared_ptr<NativeNetworkEndpoint::State>& endpoint) noexcept {
  std::vector<std::shared_ptr<SocketRegistration>> sockets;
  {
    std::lock_guard lock(endpoint->mutex);
    if (endpoint->closed) {
      return;
    }
    endpoint->closed = true;
    for (const auto& weak : endpoint->sockets) {
      if (auto socket = weak.lock()) {
        sockets.push_back(std::move(socket));
      }
    }
    endpoint->sockets.clear();
  }
  for (const auto& socket : sockets) {
    CloseRegistration(socket);
  }
  auto network = endpoint->network;
  std::lock_guard lock(network->mutex);
  network->endpoints.erase(endpoint->ip);
  std::set<std::uint64_t> removed;
  for (auto iterator = network->pending.begin();
       iterator != network->pending.end();) {
    if (iterator->second.source.ipaddr() == endpoint->ip ||
        iterator->second.destination.ipaddr() == endpoint->ip) {
      removed.insert(iterator->first);
      iterator = network->pending.erase(iterator);
    } else {
      ++iterator;
    }
  }
  std::erase_if(network->outbound,
                [&](std::uint64_t id) { return removed.contains(id); });
}

}  // namespace

struct NativeSimulatedUdpSocket::State {
  webrtc::Thread* thread = nullptr;
  std::unique_ptr<webrtc::AsyncPacketSocket> socket;
  std::mutex mutex;
  std::deque<ReceivedData> received;
};

struct NativeOutboundPacket::State {
  explicit State(PacketData packet_value) noexcept
      : packet(std::move(packet_value)) {}
  PacketData packet;
};

struct NativeReceivedPacket::State {
  explicit State(ReceivedData packet_value) noexcept
      : packet(std::move(packet_value)) {}
  ReceivedData packet;
};

NativeSimulatedNetwork::NativeSimulatedNetwork(
    std::shared_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeSimulatedNetwork::~NativeSimulatedNetwork() = default;
const std::shared_ptr<NativeSimulatedNetwork::State>&
NativeSimulatedNetwork::state() const noexcept {
  return state_;
}

NativeNetworkEndpoint::NativeNetworkEndpoint(
    std::shared_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeNetworkEndpoint::~NativeNetworkEndpoint() {
  CloseEndpoint(state_);
}
const std::shared_ptr<NativeNetworkEndpoint::State>& NativeNetworkEndpoint::state()
    const noexcept {
  return state_;
}

NativeNetworkManagerProvider::NativeNetworkManagerProvider(
    std::shared_ptr<NativeNetworkEndpoint::State> endpoint) noexcept
    : endpoint_(std::move(endpoint)) {}
NativeNetworkManagerProvider::~NativeNetworkManagerProvider() = default;
std::unique_ptr<webrtc::NetworkManager> NativeNetworkManagerProvider::Create()
    const noexcept {
  return EndpointClosed(endpoint_)
             ? nullptr
             : std::make_unique<SimulatedNetworkManager>(endpoint_);
}
const std::shared_ptr<NativeNetworkEndpoint::State>&
NativeNetworkManagerProvider::endpoint() const noexcept {
  return endpoint_;
}

NativePacketSocketFactoryProvider::NativePacketSocketFactoryProvider(
    std::shared_ptr<NativeNetworkEndpoint::State> endpoint) noexcept
    : endpoint_(std::move(endpoint)) {}
NativePacketSocketFactoryProvider::~NativePacketSocketFactoryProvider() =
    default;
std::unique_ptr<webrtc::PacketSocketFactory>
NativePacketSocketFactoryProvider::Create() const noexcept {
  return EndpointClosed(endpoint_)
             ? nullptr
             : std::make_unique<SimulatedPacketSocketFactory>(endpoint_);
}
const std::shared_ptr<NativeNetworkEndpoint::State>&
NativePacketSocketFactoryProvider::endpoint() const noexcept {
  return endpoint_;
}

NativeSimulatedUdpSocket::NativeSimulatedUdpSocket(
    std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeSimulatedUdpSocket::~NativeSimulatedUdpSocket() {
  if (state_ && state_->thread) {
    state_->thread->BlockingCall([this] { state_->socket.reset(); });
  }
}
const std::unique_ptr<NativeSimulatedUdpSocket::State>&
NativeSimulatedUdpSocket::state() const noexcept {
  return state_;
}

NativeOutboundPacket::NativeOutboundPacket(
    std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeOutboundPacket::~NativeOutboundPacket() = default;
const std::unique_ptr<NativeOutboundPacket::State>& NativeOutboundPacket::state()
    const noexcept {
  return state_;
}

NativeReceivedPacket::NativeReceivedPacket(
    std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeReceivedPacket::~NativeReceivedPacket() = default;
const std::unique_ptr<NativeReceivedPacket::State>& NativeReceivedPacket::state()
    const noexcept {
  return state_;
}

std::unique_ptr<NativeSimulatedNetwork> new_simulated_network(
    const NativeManualClock& clock) noexcept {
  auto state = std::make_shared<NativeSimulatedNetwork::State>(&clock);
  if (!state->thread || !state->thread->Start()) {
    return nullptr;
  }
  return std::make_unique<NativeSimulatedNetwork>(std::move(state));
}

std::unique_ptr<NativeNetworkEndpoint> register_network_endpoint(
    const NativeSimulatedNetwork& network,
    rust::Slice<const std::uint8_t> ip_bytes,
    std::uint8_t& error) noexcept {
  const auto ip = ToIp(ip_bytes);
  if (!ip || ip->IsNil()) {
    error = kInvalidAddress;
    return nullptr;
  }
  const auto& state = network.state();
  std::lock_guard lock(state->mutex);
  const auto found = state->endpoints.find(*ip);
  if (found != state->endpoints.end() && !found->second.expired()) {
    error = kAddressInUse;
    return nullptr;
  }
  auto endpoint =
      std::make_shared<NativeNetworkEndpoint::State>(state, *ip);
  state->endpoints[*ip] = endpoint;
  error = kOk;
  return std::make_unique<NativeNetworkEndpoint>(std::move(endpoint));
}

void close_network_endpoint(NativeNetworkEndpoint& endpoint) noexcept {
  CloseEndpoint(endpoint.state());
}

std::unique_ptr<NativeNetworkManagerProvider> new_network_manager_provider(
    const NativeNetworkEndpoint& endpoint) noexcept {
  if (EndpointClosed(endpoint.state())) {
    return nullptr;
  }
  return std::make_unique<NativeNetworkManagerProvider>(endpoint.state());
}

bool network_manager_provider_is_valid(
    const NativeNetworkManagerProvider& provider) noexcept {
  const auto& endpoint = provider.endpoint();
  if (EndpointClosed(endpoint)) {
    return false;
  }
  bool valid = false;
  endpoint->network->thread->BlockingCall([&] {
    auto manager = provider.Create();
    manager->StartUpdating();
    valid = manager->GetNetworks().size() == 1;
    manager->StopUpdating();
  });
  return valid;
}

std::unique_ptr<NativePacketSocketFactoryProvider>
new_packet_socket_factory_provider(
    const NativeNetworkEndpoint& endpoint) noexcept {
  if (EndpointClosed(endpoint.state())) {
    return nullptr;
  }
  return std::make_unique<NativePacketSocketFactoryProvider>(endpoint.state());
}

bool packet_socket_factory_supports_udp(
    const NativePacketSocketFactoryProvider& provider) noexcept {
  return !EndpointClosed(provider.endpoint());
}
bool packet_socket_factory_supports_tcp(
    const NativePacketSocketFactoryProvider&) noexcept {
  return false;
}
bool packet_socket_factory_supports_dns(
    const NativePacketSocketFactoryProvider&) noexcept {
  return false;
}

std::unique_ptr<NativeSimulatedUdpSocket> create_simulated_udp_socket(
    const NativePacketSocketFactoryProvider& provider,
    std::uint16_t port,
    std::uint8_t& error) noexcept {
  const auto& endpoint = provider.endpoint();
  if (EndpointClosed(endpoint)) {
    error = kEndpointClosed;
    return nullptr;
  }
  auto state = std::make_unique<NativeSimulatedUdpSocket::State>();
  state->thread = endpoint->network->thread.get();
  state->thread->BlockingCall([&] {
    auto factory = provider.Create();
    webrtc::Environment environment = webrtc::CreateEnvironment();
    state->socket = factory->CreateUdpSocket(
        environment, webrtc::SocketAddress(endpoint->ip, port), 0, 0);
    if (state->socket) {
      auto* received_state = state.get();
      state->socket->RegisterReceivedPacketCallback(
          [received_state](webrtc::AsyncPacketSocket*,
                           const webrtc::ReceivedIpPacket& packet) {
            ReceivedData received{packet.source_address(),
                                  std::vector<std::uint8_t>(
                                      packet.payload().begin(),
                                      packet.payload().end())};
            std::lock_guard lock(received_state->mutex);
            received_state->received.push_back(std::move(received));
          });
    }
  });
  if (!state->socket) {
    error = port == 0 ? kNativeConstructionFailed : kAddressInUse;
    return nullptr;
  }
  error = kOk;
  return std::make_unique<NativeSimulatedUdpSocket>(std::move(state));
}

rust::Vec<std::uint8_t> simulated_udp_local_ip(
    const NativeSimulatedUdpSocket& socket) noexcept {
  webrtc::IPAddress ip;
  socket.state()->thread->BlockingCall(
      [&] { ip = socket.state()->socket->GetLocalAddress().ipaddr(); });
  return FromIp(ip);
}

std::uint16_t simulated_udp_local_port(
    const NativeSimulatedUdpSocket& socket) noexcept {
  std::uint16_t port = 0;
  socket.state()->thread->BlockingCall(
      [&] { port = socket.state()->socket->GetLocalAddress().port(); });
  return port;
}

bool simulated_udp_send_to(const NativeSimulatedUdpSocket& socket,
                           rust::Slice<const std::uint8_t> destination_ip,
                           std::uint16_t destination_port,
                           rust::Vec<std::uint8_t> payload,
                           std::uint8_t& error) noexcept {
  const auto ip = ToIp(destination_ip);
  if (!ip || ip->IsNil()) {
    error = kInvalidAddress;
    return false;
  }
  if (destination_port == 0) {
    error = kInvalidPort;
    return false;
  }
  bool sent = false;
  socket.state()->thread->BlockingCall([&] {
    webrtc::AsyncSocketPacketOptions options;
    sent = socket.state()->socket->SendTo(
               payload.data(), payload.size(),
               webrtc::SocketAddress(*ip, destination_port), options) >= 0;
  });
  error = sent ? kOk : kSocketClosed;
  return sent;
}

std::unique_ptr<NativeReceivedPacket> simulated_udp_take_received(
    const NativeSimulatedUdpSocket& socket) noexcept {
  std::lock_guard lock(socket.state()->mutex);
  if (socket.state()->received.empty()) {
    return nullptr;
  }
  ReceivedData packet = std::move(socket.state()->received.front());
  socket.state()->received.pop_front();
  return std::make_unique<NativeReceivedPacket>(
      std::make_unique<NativeReceivedPacket::State>(std::move(packet)));
}

void close_simulated_udp_socket(NativeSimulatedUdpSocket& socket) noexcept {
  if (!socket.state() || !socket.state()->thread || !socket.state()->socket) {
    return;
  }
  socket.state()->thread->BlockingCall([&] { socket.state()->socket->Close(); });
  std::lock_guard lock(socket.state()->mutex);
  socket.state()->received.clear();
}

std::unique_ptr<NativeOutboundPacket> take_outbound_packet(
    const NativeSimulatedNetwork& network) noexcept {
  const auto& state = network.state();
  std::lock_guard lock(state->mutex);
  while (!state->outbound.empty()) {
    const std::uint64_t id = state->outbound.front();
    state->outbound.pop_front();
    const auto packet = state->pending.find(id);
    if (packet != state->pending.end()) {
      return std::make_unique<NativeOutboundPacket>(
          std::make_unique<NativeOutboundPacket::State>(packet->second));
    }
  }
  return nullptr;
}

std::uint64_t outbound_packet_id(const NativeOutboundPacket& packet) noexcept {
  return packet.state()->packet.id;
}
rust::Vec<std::uint8_t> outbound_packet_source_ip(
    const NativeOutboundPacket& packet) noexcept {
  return FromIp(packet.state()->packet.source.ipaddr());
}
std::uint16_t outbound_packet_source_port(
    const NativeOutboundPacket& packet) noexcept {
  return packet.state()->packet.source.port();
}
rust::Vec<std::uint8_t> outbound_packet_destination_ip(
    const NativeOutboundPacket& packet) noexcept {
  return FromIp(packet.state()->packet.destination.ipaddr());
}
std::uint16_t outbound_packet_destination_port(
    const NativeOutboundPacket& packet) noexcept {
  return packet.state()->packet.destination.port();
}
rust::Vec<std::uint8_t> outbound_packet_payload(
    const NativeOutboundPacket& packet) noexcept {
  return FromBytes(packet.state()->packet.payload);
}
std::int64_t outbound_packet_deadline_us(
    const NativeOutboundPacket& packet) noexcept {
  return packet.state()->packet.deadline_us;
}

bool deliver_outbound_packet(const NativeSimulatedNetwork& network,
                             std::uint64_t packet_id,
                             bool keep_pending,
                             std::uint8_t& error) noexcept {
  PacketData packet;
  std::shared_ptr<SocketRegistration> registration;
  {
    const auto& state = network.state();
    std::lock_guard lock(state->mutex);
    const auto found = state->pending.find(packet_id);
    if (found == state->pending.end()) {
      error = kPacketNotFound;
      return false;
    }
    packet = found->second;
    if (!keep_pending) {
      state->pending.erase(found);
    }
    const auto destination = state->sockets.find(packet.destination);
    if (destination != state->sockets.end()) {
      registration = destination->second.lock();
    }
  }
  if (!registration) {
    error = kDestinationUnavailable;
    return false;
  }
  bool delivered = false;
  webrtc::Thread* thread;
  {
    std::lock_guard lock(registration->mutex);
    thread = registration->thread;
  }
  if (thread != nullptr) {
    thread->BlockingCall([&] {
      SimulatedAsyncPacketSocket* socket;
      {
        std::lock_guard lock(registration->mutex);
        socket = registration->socket;
      }
      delivered = socket != nullptr && socket->Receive(packet);
    });
  }
  error = delivered ? kOk : kDestinationUnavailable;
  return delivered;
}

bool drop_outbound_packet(const NativeSimulatedNetwork& network,
                          std::uint64_t packet_id,
                          std::uint8_t& error) noexcept {
  const auto& state = network.state();
  std::lock_guard lock(state->mutex);
  if (state->pending.erase(packet_id) == 0) {
    error = kPacketNotFound;
    return false;
  }
  error = kOk;
  return true;
}

rust::Vec<std::uint8_t> received_packet_source_ip(
    const NativeReceivedPacket& packet) noexcept {
  return FromIp(packet.state()->packet.source.ipaddr());
}
std::uint16_t received_packet_source_port(
    const NativeReceivedPacket& packet) noexcept {
  return packet.state()->packet.source.port();
}
rust::Vec<std::uint8_t> received_packet_payload(
    const NativeReceivedPacket& packet) noexcept {
  return FromBytes(packet.state()->packet.payload);
}

}  // namespace pulsebeam::webrtc_sys
