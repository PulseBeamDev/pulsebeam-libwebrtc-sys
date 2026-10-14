#include "pulsebeam-webrtc-sys/native/transport.h"

#include <deque>
#include <limits>
#include <mutex>
#include <string>
#include <utility>

#include "pulsebeam-webrtc-sys/native/peer.h"
#include "pulsebeam-webrtc-sys/native/execution.h"
#include "pulsebeam-webrtc-sys/native/tls.h"
#include "pulsebeam-webrtc-sys/native/rtc_error.h"
#include "absl/strings/str_format.h"
#include "pulsebeam-webrtc-sys/src/lib.rs.h"
#include "api/peer_connection_interface.h"
#include "rtc_base/thread.h"
#include "rtc_base/buffer.h"
#include "p2p/base/ice_transport_internal.h"

namespace pulsebeam::webrtc_sys {
namespace {
template <class Event> struct EventQueue {
  std::shared_ptr<ReadinessSignal> readiness;
  std::mutex mutex;
  std::size_t capacity = 0;
  std::deque<Event> events;
  std::uint64_t dropped = 0;
  void Reset(std::size_t size) {
    std::lock_guard lock(mutex);
    capacity = size;
    events.clear();
    dropped = 0;
  }
  void Push(Event event) {
    struct Notify { std::shared_ptr<ReadinessSignal> signal;
      ~Notify() { SignalReadiness(signal); } } notify{readiness};
    std::lock_guard lock(mutex);
    if (!capacity) return;
    if (events.size() == capacity) {
      events.pop_front();
      if (dropped != std::numeric_limits<std::uint64_t>::max()) ++dropped;
    }
    events.push_back(std::move(event));
  }
  std::uint64_t TakeDropped() {
    std::lock_guard lock(mutex);
    return std::exchange(dropped, 0);
  }
};
}
struct NativeDtlsTransport::State final : webrtc::DtlsTransportObserverInterface {
  webrtc::scoped_refptr<webrtc::DtlsTransportInterface> transport;
  webrtc::Thread* network = nullptr;
  bool registered = false;
  EventQueue<std::unique_ptr<NativeDtlsEvent>> queue;
  ~State() override = default;
  void OnStateChange(webrtc::DtlsTransportInformation info) override {
    auto event = std::make_unique<NativeDtlsEvent>();
    event->information = std::make_unique<NativeDtlsInformation>(std::move(info));
    queue.Push(std::move(event));
  }
  void OnError(webrtc::RTCError error) override {
    auto event = std::make_unique<NativeDtlsEvent>();
    event->error = std::move(error);
    queue.Push(std::move(event));
  }
};
struct NativeSctpTransport::State final : webrtc::SctpTransportObserverInterface {
  webrtc::scoped_refptr<webrtc::SctpTransportInterface> transport;
  webrtc::Thread* network = nullptr;
  bool registered = false;
  EventQueue<std::unique_ptr<NativeSctpInformation>> queue;
  ~State() override = default;
  void OnStateChange(webrtc::SctpTransportInformation info) override {
    // Preserve the callback's actual DTLS ref, not a later transport getter.
    queue.Push(std::make_unique<NativeSctpInformation>(std::move(info)));
  }
};
struct NativeIceTransport::State {
  webrtc::scoped_refptr<webrtc::IceTransportInterface> transport;
  webrtc::Thread* network = nullptr;
};
NativeIceTransport::NativeIceTransport(std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeIceTransport::~NativeIceTransport() {
  auto* network = state_->network;
  network->BlockingCall([this] { state_.reset(); });
}
const std::unique_ptr<NativeIceTransport::State>& NativeIceTransport::state() const noexcept {
  return state_;
}
std::unique_ptr<NativeIceTransport> dtls_transport_ice(const NativeDtlsTransport& transport) noexcept {
  auto& owner = *transport.state();
  auto ice = owner.network->BlockingCall([&] { return owner.transport->ice_transport(); });
  if (!ice) return nullptr;
  auto state = std::make_unique<NativeIceTransport::State>();
  state->transport = std::move(ice);
  state->network = owner.network;
  return std::make_unique<NativeIceTransport>(std::move(state));
}
bool ice_transport_has_internal(const NativeIceTransport& transport) noexcept {
  auto& state = *transport.state();
  return state.network->BlockingCall([&] { return state.transport->internal() != nullptr; });
}
FfiIceInternalValue ice_transport_internal_value(const NativeIceTransport& transport,
                                                std::uint8_t field) noexcept {
  auto& state = *transport.state();
  return state.network->BlockingCall([&] {
    FfiIceInternalValue out{};
    auto* internal = state.transport->internal();
    if (!internal) return out;
    out.available = true;
    switch (field) {
      case 0: out.value = static_cast<std::int32_t>(internal->GetState()); break;
      case 1: out.value = static_cast<std::int32_t>(internal->GetIceTransportState()); break;
      case 2: out.value = internal->component(); break;
      case 3: out.value = static_cast<std::int32_t>(internal->GetIceRole()); break;
      case 4: out.value = static_cast<std::int32_t>(internal->gathering_state()); break;
      case 5: {
        auto rtt = internal->GetRttEstimate();
        out.present = rtt.has_value();
        out.value = rtt.value_or(0);
        break;
      }
      default: out.available = false; break;
    }
    return out;
  });
}
NativeDtlsTransport::NativeDtlsTransport(std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeDtlsTransport::~NativeDtlsTransport() {
  // Unregister and serialize against callbacks before beginning destruction of
  // the observer object (including its vtable), not inside State::~State.
  auto* network = state_->network;
  network->BlockingCall([this] {
    if (state_->registered) state_->transport->UnregisterObserver();
    // Retire queued snapshots and the final native reference on their role.
    // The Rust owner keeps the peer and its role threads alive through here.
    state_.reset();
  });
}
const std::unique_ptr<NativeDtlsTransport::State>& NativeDtlsTransport::state() const noexcept {
  return state_;
}
NativeSctpTransport::NativeSctpTransport(std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativeSctpTransport::~NativeSctpTransport() {
  auto* network = state_->network;
  network->BlockingCall([this] {
    if (state_->registered) state_->transport->UnregisterObserver();
    state_.reset();
  });
}
const std::unique_ptr<NativeSctpTransport::State>& NativeSctpTransport::state() const noexcept {
  return state_;
}
std::unique_ptr<NativeDtlsTransport> wrap_dtls_transport(
    webrtc::scoped_refptr<webrtc::DtlsTransportInterface> transport,
    webrtc::Thread* network,
    std::shared_ptr<ReadinessSignal> readiness) noexcept {
  if (!transport) return nullptr;
  auto state = std::make_unique<NativeDtlsTransport::State>();
  state->queue.readiness = std::move(readiness);
  state->transport = std::move(transport);
  state->network = network;
  return std::make_unique<NativeDtlsTransport>(std::move(state));
}
std::unique_ptr<NativeDtlsTransport> peer_dtls_transport_by_mid(
    const NativePeerConnection& peer, rust::Slice<const std::uint8_t> mid) noexcept {
  auto native = peer.network_thread()->BlockingCall([&] {
    return peer.peer()->LookupDtlsTransportByMid(
        std::string(reinterpret_cast<const char*>(mid.data()), mid.size()));
  });
  return wrap_dtls_transport(std::move(native), peer.network_thread(), peer.readiness());
}
bool ice_transport_same_resource(const NativeIceTransport& first, const NativeIceTransport& second) noexcept {
  return first.state()->transport.get() == second.state()->transport.get();
}
std::unique_ptr<NativeSctpTransport> peer_sctp_transport(const NativePeerConnection& peer) noexcept {
  auto transport = peer.peer()->GetSctpTransport();
  if (!transport) return nullptr;
  auto state = std::make_unique<NativeSctpTransport::State>();
  state->queue.readiness = peer.readiness();
  state->transport = std::move(transport);
  state->network = peer.network_thread();
  return std::make_unique<NativeSctpTransport>(std::move(state));
}
std::uintptr_t dtls_transport_identity(const NativeDtlsTransport& transport) noexcept {
  return reinterpret_cast<std::uintptr_t>(transport.state()->transport.get());
}
std::uintptr_t sctp_transport_identity(const NativeSctpTransport& transport) noexcept {
  return reinterpret_cast<std::uintptr_t>(transport.state()->transport.get());
}
std::unique_ptr<NativeDtlsInformation> dtls_transport_information(
    const NativeDtlsTransport& transport) noexcept {
  return std::make_unique<NativeDtlsInformation>(transport.state()->transport->Information());
}
std::unique_ptr<NativeSctpInformation> sctp_transport_information(
    const NativeSctpTransport& transport) noexcept {
  return std::make_unique<NativeSctpInformation>(transport.state()->transport->Information());
}
std::unique_ptr<NativeDtlsTransport> sctp_transport_dtls(
    const NativeSctpTransport& transport) noexcept {
  auto& state = *transport.state();
  // Unlike Information(), this pinned SCTP accessor does not proxy itself.
  auto dtls = state.network->BlockingCall([&] {
    return state.transport->dtls_transport();
  });
  return wrap_dtls_transport(std::move(dtls), state.network, state.queue.readiness);
}
std::unique_ptr<NativeDtlsTransport> sctp_information_dtls(
    const NativeSctpInformation& information, const NativePeerConnection& peer) noexcept {
  return wrap_dtls_transport(information.value().dtls_transport(), peer.network_thread(), peer.readiness());
}
FfiDtlsInformation dtls_information_fields(const NativeDtlsInformation& information) noexcept {
  const auto& value = information.value();
  FfiDtlsInformation out{};
  out.state = static_cast<std::int32_t>(value.state());
  out.has_role = value.role().has_value();
  if (value.role()) out.role = static_cast<std::int32_t>(*value.role());
  out.has_tls_version = value.tls_version().has_value();
  out.tls_version = value.tls_version().value_or(0);
  out.has_ssl_cipher_suite = value.ssl_cipher_suite().has_value();
  out.ssl_cipher_suite = value.ssl_cipher_suite().value_or(0);
  out.has_srtp_cipher_suite = value.srtp_cipher_suite().has_value();
  out.srtp_cipher_suite = value.srtp_cipher_suite().value_or(0);
  out.has_ssl_group_id = value.ssl_group_id().has_value();
  out.ssl_group_id = value.ssl_group_id().value_or(0);
  out.has_remote_certificates = value.remote_ssl_certificates() != nullptr;
  return out;
}
FfiSctpInformation sctp_information_fields(const NativeSctpInformation& information) noexcept {
  const auto& value = information.value();
  FfiSctpInformation out{};
  out.state = static_cast<std::int32_t>(value.state());
  out.has_dtls_transport = value.dtls_transport() != nullptr;
  out.has_max_message_size = value.MaxMessageSize().has_value();
  out.max_message_size = value.MaxMessageSize().value_or(0);
  out.has_max_channels = value.MaxChannels().has_value();
  out.max_channels = value.MaxChannels().value_or(0);
  return out;
}
rust::Vec<FfiCertificateData> dtls_information_remote_certificates(
    const NativeDtlsInformation& information) noexcept {
  rust::Vec<FfiCertificateData> out;
  const auto* chain = information.value().remote_ssl_certificates();
  if (!chain) return out;
  for (std::size_t index = 0; index < chain->GetSize(); ++index) {
    const auto& certificate = chain->Get(index);
    FfiCertificateData data{};
    data.pem = certificate.ToPEMString();
    webrtc::Buffer der;
    certificate.ToDER(&der);
    for (auto byte : der) data.der.push_back(byte);
    data.expiration_unix_seconds = certificate.CertificateExpirationTime();
    std::string algorithm;
    data.has_signature_digest_algorithm = certificate.GetSignatureDigestAlgorithm(&algorithm);
    data.signature_digest_algorithm = algorithm;
    out.push_back(std::move(data));
  }
  return out;
}
std::unique_ptr<NativeDtlsInformation> dtls_information_default() noexcept {
  return std::make_unique<NativeDtlsInformation>(webrtc::DtlsTransportInformation());
}
std::unique_ptr<NativeDtlsInformation> dtls_information_new(std::int32_t state) noexcept {
  return std::make_unique<NativeDtlsInformation>(
      webrtc::DtlsTransportInformation(static_cast<webrtc::DtlsTransportState>(state)));
}
std::unique_ptr<NativeSctpInformation> sctp_information_default() noexcept {
  return std::make_unique<NativeSctpInformation>(webrtc::SctpTransportInformation());
}
std::unique_ptr<NativeSctpInformation> sctp_information_new(std::int32_t state) noexcept {
  return std::make_unique<NativeSctpInformation>(
      webrtc::SctpTransportInformation(static_cast<webrtc::SctpTransportState>(state)));
}
std::unique_ptr<NativeDtlsInformation> dtls_information_from_parts(
    const FfiDtlsInformation& fields, std::unique_ptr<NativeTlsCertificateChain> chain) noexcept {
  return std::make_unique<NativeDtlsInformation>(webrtc::DtlsTransportInformation(
      static_cast<webrtc::DtlsTransportState>(fields.state),
      fields.has_role ? std::optional<webrtc::DtlsTransportTlsRole>(static_cast<webrtc::DtlsTransportTlsRole>(fields.role)) : std::nullopt,
      fields.has_tls_version ? std::optional<int>(fields.tls_version) : std::nullopt,
      fields.has_ssl_cipher_suite ? std::optional<int>(fields.ssl_cipher_suite) : std::nullopt,
      fields.has_srtp_cipher_suite ? std::optional<int>(fields.srtp_cipher_suite) : std::nullopt,
      chain ? chain->Take() : nullptr,
      fields.has_ssl_group_id ? std::optional<int>(fields.ssl_group_id) : std::nullopt));
}
std::unique_ptr<NativeDtlsInformation> dtls_information_from_legacy_parts(
    const FfiDtlsInformation& fields, std::unique_ptr<NativeTlsCertificateChain> chain) noexcept {
  return std::make_unique<NativeDtlsInformation>(webrtc::DtlsTransportInformation(
      static_cast<webrtc::DtlsTransportState>(fields.state),
      fields.has_tls_version ? std::optional<int>(fields.tls_version) : std::nullopt,
      fields.has_ssl_cipher_suite ? std::optional<int>(fields.ssl_cipher_suite) : std::nullopt,
      fields.has_srtp_cipher_suite ? std::optional<int>(fields.srtp_cipher_suite) : std::nullopt,
      chain ? chain->Take() : nullptr));
}
void dtls_information_assign(NativeDtlsInformation& target, const NativeDtlsInformation& source) noexcept { target.Assign(source); }
void dtls_information_move_assign(NativeDtlsInformation& target, std::unique_ptr<NativeDtlsInformation> source) noexcept { target.MoveAssign(*source); }
std::unique_ptr<NativeTlsCertificateChain> dtls_information_certificate_chain(const NativeDtlsInformation& information) noexcept {
  const auto* chain = information.value().remote_ssl_certificates();
  return chain ? std::make_unique<NativeTlsCertificateChain>(chain->Clone()) : nullptr;
}
std::unique_ptr<NativeSctpInformation> sctp_information_without_dtls(const FfiSctpInformation& fields) noexcept {
  return std::make_unique<NativeSctpInformation>(webrtc::SctpTransportInformation(
      static_cast<webrtc::SctpTransportState>(fields.state), nullptr,
      fields.has_max_message_size ? std::optional<double>(fields.max_message_size) : std::nullopt,
      fields.has_max_channels ? std::optional<int>(fields.max_channels) : std::nullopt));
}
std::unique_ptr<NativeSctpInformation> sctp_information_with_dtls(const FfiSctpInformation& fields, const NativeDtlsTransport& transport) noexcept {
  return std::make_unique<NativeSctpInformation>(webrtc::SctpTransportInformation(
      static_cast<webrtc::SctpTransportState>(fields.state), transport.state()->transport,
      fields.has_max_message_size ? std::optional<double>(fields.max_message_size) : std::nullopt,
      fields.has_max_channels ? std::optional<int>(fields.max_channels) : std::nullopt));
}
void sctp_information_assign(NativeSctpInformation& target, const NativeSctpInformation& source) noexcept { target.Assign(source); }
rust::String sctp_information_string(const NativeSctpInformation& information) noexcept { return absl::StrFormat("%v", information.value()); }
rust::String sctp_state_string(std::int32_t state) noexcept { return absl::StrFormat("%v", static_cast<webrtc::SctpTransportState>(state)); }
std::unique_ptr<NativeDtlsInformation> dtls_information_clone(
    const NativeDtlsInformation& information) noexcept {
  return std::make_unique<NativeDtlsInformation>(information.value());
}
std::unique_ptr<NativeSctpInformation> sctp_information_clone(
    const NativeSctpInformation& information) noexcept {
  return std::make_unique<NativeSctpInformation>(information.value());
}
bool dtls_transport_register_observer(const NativeDtlsTransport& transport,
                                      std::size_t capacity) noexcept {
  auto& state = *transport.state();
  return state.network->BlockingCall([&] {
    if (!capacity || state.registered) return false;
    state.queue.Reset(capacity);
    state.transport->RegisterObserver(&state);
    state.registered = true;
    return true;
  });
}
bool sctp_transport_register_observer(const NativeSctpTransport& transport,
                                      std::size_t capacity) noexcept {
  auto& state = *transport.state();
  return state.network->BlockingCall([&] {
    if (!capacity || state.registered) return false;
    state.queue.Reset(capacity);
    state.transport->RegisterObserver(&state);
    state.registered = true;
    return true;
  });
}
void dtls_transport_unregister_observer(const NativeDtlsTransport& transport) noexcept {
  auto& state = *transport.state();
  state.network->BlockingCall([&] {
    if (!state.registered) return;
    state.transport->UnregisterObserver();
    state.registered = false;
  });
}
void sctp_transport_unregister_observer(const NativeSctpTransport& transport) noexcept {
  auto& state = *transport.state();
  state.network->BlockingCall([&] {
    if (!state.registered) return;
    state.transport->UnregisterObserver();
    state.registered = false;
  });
}
std::unique_ptr<NativeDtlsEvent> dtls_transport_take_event(const NativeDtlsTransport& transport) noexcept {
  auto& queue = transport.state()->queue;
  std::lock_guard lock(queue.mutex);
  if (queue.events.empty()) return nullptr;
  auto event = std::move(queue.events.front());
  queue.events.pop_front();
  return event;
}
std::unique_ptr<NativeSctpInformation> sctp_transport_take_event(const NativeSctpTransport& transport) noexcept {
  auto& queue = transport.state()->queue;
  std::lock_guard lock(queue.mutex);
  if (queue.events.empty()) return nullptr;
  auto event = std::move(queue.events.front());
  queue.events.pop_front();
  return event;
}
std::uint64_t dtls_transport_take_dropped(const NativeDtlsTransport& transport) noexcept {
  return transport.state()->queue.TakeDropped();
}
std::uint64_t sctp_transport_take_dropped(const NativeSctpTransport& transport) noexcept {
  return transport.state()->queue.TakeDropped();
}
std::unique_ptr<NativeDtlsInformation> dtls_event_information(NativeDtlsEvent& event) noexcept {
  return std::move(event.information);
}
std::unique_ptr<NativeRtcError> dtls_event_error(const NativeDtlsEvent& event) noexcept {
  return std::make_unique<NativeRtcError>(event.error);
}
}  // namespace pulsebeam::webrtc_sys
