#include "pulsebeam-webrtc-sys/native/peer.h"

#include <array>
#include <atomic>
#include <climits>
#include <cstdint>
#include <deque>
#include <mutex>
#include <optional>
#include <set>
#include <string>
#include <type_traits>
#include <unordered_map>
#include <utility>
#include <vector>

#include <openssl/pem.h>
#include <openssl/x509.h>
#include <openssl/x509_vfy.h>
#include <openssl/x509v3.h>

#include "api/create_modular_peer_connection_factory.h"
#if defined(PULSEBEAM_WEBRTC_NATIVE_AUDIO)
#include "api/audio/create_audio_device_module.h"
#endif
#include "api/audio/builtin_audio_processing_builder.h"
#include "api/audio_codecs/builtin_audio_decoder_factory.h"
#include "api/audio_codecs/builtin_audio_encoder_factory.h"
#include "api/enable_media.h"
#include "api/jsep.h"
#include "api/make_ref_counted.h"
#include "api/peer_connection_interface.h"
#include "api/rtc_error.h"
#include "api/scoped_refptr.h"
#include "api/stats/rtc_stats_collector_callback.h"
#include "api/stats/rtc_stats_report.h"
#include "api/stats/rtcstats_objects.h"
#include "api/video_codecs/video_decoder_factory.h"
#include "api/video_codecs/video_encoder_factory.h"
#include "pulsebeam-webrtc-sys/native/codec.h"
#include "pulsebeam-webrtc-sys/native/data_channel.h"
#include "pulsebeam-webrtc-sys/native/execution.h"
#include "pulsebeam-webrtc-sys/native/network.h"
#include "pulsebeam-webrtc-sys/native/video.h"
#include "modules/audio_device/include/audio_device_default.h"
#include "pulsebeam-webrtc-sys/src/lib.rs.h"
#include "rtc_base/buffer.h"
#include "rtc_base/ssl_certificate.h"
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
  kDataChannel = 9,
  kTrack = 10,
  kTrackRemoved = 11,
  kStatsReport = 12,
};

constexpr std::uint8_t kClosedError = 255;

class HeadlessAudioDevice
    : public webrtc::webrtc_impl::AudioDeviceModuleDefault<
          webrtc::AudioDeviceModule> {
 public:
  int32_t RegisterAudioCallback(webrtc::AudioTransport* callback) override {
    transport_.store(callback);
    return 0;
  }
  int32_t StartPlayout() override {
    playing_.store(true);
    return 0;
  }
  int32_t StopPlayout() override {
    playing_.store(false);
    return 0;
  }
  bool Playing() const override { return playing_.load(); }
  bool Pump() const {
    auto* transport = transport_.load();
    if (!playing_.load() || !transport) return false;
    std::array<std::int16_t, 480> samples{};
    int64_t elapsed_ms = 0;
    int64_t ntp_ms = 0;
    transport->PullRenderData(16, 48000, 1, 480, samples.data(),
                              &elapsed_ms, &ntp_ms);
    return true;
  }

 private:
  std::atomic<webrtc::AudioTransport*> transport_{nullptr};
  std::atomic<bool> playing_{false};
};

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

struct NotifyOnExit {
  std::shared_ptr<ReadinessSignal> readiness;
  ~NotifyOnExit() { SignalReadiness(readiness); }
};

struct EventState {
  explicit EventState(std::shared_ptr<ReadinessSignal> signal)
      : readiness(std::move(signal)) {}

  bool Begin(std::uint64_t operation_id) {
    NotifyOnExit notify{readiness};
    std::lock_guard lock(mutex);
    if (closed) {
      events.push_back(ClosedOperation(operation_id));
      return false;
    }
    return pending.insert(operation_id).second;
  }

  bool BeginStats(std::uint64_t operation_id) {
    std::lock_guard lock(mutex);
    if (closed || stats_operation_id != 0) {
      return false;
    }
    stats_operation_id = operation_id;
    pending.insert(operation_id);
    return true;
  }

  void Complete(FfiPeerEvent event) {
    NotifyOnExit notify{readiness};
    std::lock_guard lock(mutex);
    if (pending.erase(event.operation_id) != 0) {
      events.push_back(std::move(event));
    }
  }

  void Push(FfiPeerEvent event) {
    NotifyOnExit notify{readiness};
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
    if (event.operation_id != 0 && event.operation_id == stats_operation_id) {
      stats_operation_id = 0;
    }
    return event;
  }

  void AddDataChannel(
      webrtc::scoped_refptr<webrtc::DataChannelInterface> channel) {
    NotifyOnExit notify{readiness};
    std::lock_guard lock(mutex);
    if (closed) {
      return;
    }
    const std::uint64_t arrival_id = next_data_channel_id++;
    data_channels.emplace(arrival_id, std::move(channel));
    FfiPeerEvent event;
    event.kind = kDataChannel;
    event.operation_id = arrival_id;
    events.push_back(std::move(event));
  }

  webrtc::scoped_refptr<webrtc::DataChannelInterface> TakeDataChannel(
      std::uint64_t arrival_id) {
    std::lock_guard lock(mutex);
    auto found = data_channels.find(arrival_id);
    if (found == data_channels.end()) {
      return nullptr;
    }
    auto channel = std::move(found->second);
    data_channels.erase(found);
    return channel;
  }

  void AddTransceiver(
      webrtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver) {
    NotifyOnExit notify{readiness};
    std::lock_guard lock(mutex);
    if (closed) {
      return;
    }
    const std::uint64_t arrival_id = next_media_id++;
    transceivers.emplace(arrival_id, std::move(transceiver));
    FfiPeerEvent event;
    event.kind = kTrack;
    event.operation_id = arrival_id;
    events.push_back(std::move(event));
  }

  webrtc::scoped_refptr<webrtc::RtpTransceiverInterface> TakeTransceiver(
      std::uint64_t arrival_id) {
    std::lock_guard lock(mutex);
    auto found = transceivers.find(arrival_id);
    if (found == transceivers.end()) {
      return nullptr;
    }
    auto transceiver = std::move(found->second);
    transceivers.erase(found);
    return transceiver;
  }

  void RemoveReceiver(
      webrtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver) {
    NotifyOnExit notify{readiness};
    std::lock_guard lock(mutex);
    if (closed) {
      return;
    }
    const std::uint64_t arrival_id = next_media_id++;
    receivers.emplace(arrival_id, std::move(receiver));
    FfiPeerEvent event;
    event.kind = kTrackRemoved;
    event.operation_id = arrival_id;
    events.push_back(std::move(event));
  }

  webrtc::scoped_refptr<webrtc::RtpReceiverInterface> TakeReceiver(
      std::uint64_t arrival_id) {
    std::lock_guard lock(mutex);
    auto found = receivers.find(arrival_id);
    if (found == receivers.end()) {
      return nullptr;
    }
    auto receiver = std::move(found->second);
    receivers.erase(found);
    return receiver;
  }

  void Close() {
    NotifyOnExit notify{readiness};
    std::unordered_map<
        std::uint64_t,
        webrtc::scoped_refptr<webrtc::DataChannelInterface>> abandoned_channels;
    std::unordered_map<
        std::uint64_t,
        webrtc::scoped_refptr<webrtc::RtpTransceiverInterface>>
        abandoned_transceivers;
    std::unordered_map<std::uint64_t,
                       webrtc::scoped_refptr<webrtc::RtpReceiverInterface>>
        abandoned_receivers;
    {
      std::lock_guard lock(mutex);
      if (closed) {
        return;
      }
      closed = true;
      for (std::uint64_t operation_id : pending) {
        events.push_back(ClosedOperation(operation_id));
      }
      pending.clear();
      abandoned_channels.swap(data_channels);
      abandoned_transceivers.swap(transceivers);
      abandoned_receivers.swap(receivers);
      FfiPeerEvent event;
      event.kind = kClosed;
      events.push_back(std::move(event));
    }
  }

  const std::shared_ptr<ReadinessSignal> readiness;
  std::mutex mutex;
  std::deque<FfiPeerEvent> events;
  std::set<std::uint64_t> pending;
  std::uint64_t stats_operation_id = 0;
  std::unordered_map<std::uint64_t,
                     webrtc::scoped_refptr<webrtc::DataChannelInterface>>
      data_channels;
  std::uint64_t next_data_channel_id = 1;
  std::unordered_map<
      std::uint64_t,
      webrtc::scoped_refptr<webrtc::RtpTransceiverInterface>> transceivers;
  std::unordered_map<std::uint64_t,
                     webrtc::scoped_refptr<webrtc::RtpReceiverInterface>>
      receivers;
  std::uint64_t next_media_id = 1;
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
      webrtc::scoped_refptr<webrtc::DataChannelInterface> channel) override {
    state_->AddDataChannel(std::move(channel));
  }

  void OnTrack(webrtc::scoped_refptr<webrtc::RtpTransceiverInterface>
                   transceiver) override {
    state_->AddTransceiver(std::move(transceiver));
  }

  void OnRemoveTrack(
      webrtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver) override {
    state_->RemoveReceiver(std::move(receiver));
  }

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

template <typename T>
void AddStatsField(rust::Vec<FfiStatsField>& fields, const char* name,
                   const std::optional<T>& value) {
  if (!value) {
    return;
  }
  FfiStatsField field;
  field.name = name;
  if constexpr (std::is_same_v<T, std::string>) {
    field.kind = 1;
    field.text = *value;
  } else if constexpr (std::is_floating_point_v<T>) {
    field.kind = 4;
    field.decimal = *value;
  } else if constexpr (std::is_same_v<T, bool>) {
    field.kind = 5;
    field.flag = *value;
  } else if constexpr (std::is_signed_v<T>) {
    field.kind = 3;
    field.signed_value = *value;
  } else {
    field.kind = 2;
    field.unsigned_value = *value;
  }
  fields.push_back(std::move(field));
}

FfiStatsRecord NewStatsRecord(const webrtc::RTCStats& stats,
                              std::uint8_t kind) {
  FfiStatsRecord record;
  record.id = std::string(stats.id());
  record.kind = kind;
  record.timestamp_us = stats.timestamp().us();
  return record;
}

class StatsObserver : public webrtc::RTCStatsCollectorCallback {
 public:
  StatsObserver(std::shared_ptr<EventState> state,
                std::uint64_t operation_id)
      : state_(std::move(state)), operation_id_(operation_id) {}

  void OnStatsDelivered(
      const webrtc::scoped_refptr<const webrtc::RTCStatsReport>& report)
      override {
    FfiPeerEvent event;
    event.kind = kStatsReport;
    event.operation_id = operation_id_;
    event.stats_timestamp_us = report->timestamp().us_or(-1);
    // Never retain an unbounded upstream report in the Rust event queue.
    bool overflow = false;
    auto add = [&](FfiStatsRecord record) {
      if (event.stats_records.size() == 256) {
        overflow = true;
      } else {
        event.stats_records.push_back(std::move(record));
      }
    };
#define COPY(member) AddStatsField(record.fields, #member, stat->member)
    for (const auto* stat :
         report->GetStatsOfType<webrtc::RTCIceCandidatePairStats>()) {
      auto record = NewStatsRecord(*stat, 1);
      COPY(transport_id); COPY(local_candidate_id); COPY(remote_candidate_id);
      COPY(state); COPY(nominated); COPY(packets_sent); COPY(packets_received);
      COPY(bytes_sent); COPY(bytes_received); COPY(current_round_trip_time);
      COPY(available_outgoing_bitrate); COPY(available_incoming_bitrate);
      add(std::move(record));
    }
    for (const auto* stat : report->GetStatsOfType<webrtc::RTCTransportStats>()) {
      auto record = NewStatsRecord(*stat, 2);
      COPY(selected_candidate_pair_id); COPY(ice_state); COPY(dtls_state);
      COPY(packets_sent); COPY(packets_received); COPY(bytes_sent);
      COPY(bytes_received);
      add(std::move(record));
    }
    for (const auto* stat :
         report->GetStatsOfType<webrtc::RTCInboundRtpStreamStats>()) {
      auto record = NewStatsRecord(*stat, 3);
      COPY(kind); COPY(ssrc); COPY(transport_id); COPY(mid);
      COPY(packets_received); COPY(packets_lost); COPY(bytes_received);
      COPY(jitter); COPY(frames_received); COPY(frames_decoded);
      add(std::move(record));
    }
    for (const auto* stat :
         report->GetStatsOfType<webrtc::RTCOutboundRtpStreamStats>()) {
      auto record = NewStatsRecord(*stat, 4);
      COPY(kind); COPY(ssrc); COPY(transport_id); COPY(mid); COPY(rid);
      COPY(packets_sent); COPY(bytes_sent); COPY(target_bitrate);
      COPY(frames_encoded); COPY(frames_sent);
      add(std::move(record));
    }
    for (const auto* stat :
         report->GetStatsOfType<webrtc::RTCDataChannelStats>()) {
      auto record = NewStatsRecord(*stat, 5);
      COPY(label); COPY(protocol); COPY(state); COPY(data_channel_identifier);
      COPY(messages_sent); COPY(messages_received); COPY(bytes_sent);
      COPY(bytes_received);
      add(std::move(record));
    }
#undef COPY
    if (overflow) {
      state_->Complete(ErrorEvent(
          operation_id_, webrtc::RTCErrorType::RESOURCE_EXHAUSTED,
          "stats report exceeds the 256-record limit"));
    } else {
      state_->Complete(std::move(event));
    }
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
  if (*parsed_type == webrtc::SdpType::kRollback && !sdp.empty()) {
    error.description = "rollback must not contain SDP";
    return nullptr;
  }
  return webrtc::CreateSessionDescription(
      *parsed_type, std::string_view(sdp.data(), sdp.size()), &error);
}

}  // namespace

struct NativePeerConnectionFactory::State {
  std::shared_ptr<ReadinessSignal> readiness;
  webrtc::scoped_refptr<HeadlessAudioDevice> headless_audio_device;
  webrtc::scoped_refptr<webrtc::AudioDeviceModule> audio_device;
  webrtc::scoped_refptr<webrtc::PeerConnectionFactoryInterface> factory;
  webrtc::Thread* signaling_thread = nullptr;
  webrtc::Thread* worker_thread = nullptr;
};

struct NativePeerConnection::State {
  webrtc::scoped_refptr<HeadlessAudioDevice> headless_audio_device;
  bool native_audio_enabled = false;
  webrtc::scoped_refptr<webrtc::PeerConnectionInterface> peer;
  std::shared_ptr<EventState> events;
  std::unique_ptr<PeerObserver> observer;
  webrtc::Thread* signaling_thread = nullptr;
  webrtc::Thread* worker_thread = nullptr;
  std::set<std::string> encoded_receiver_ids;
  std::set<std::string> audio_receiver_ids;
  bool closed = false;
};

NativePeerConnectionFactory::NativePeerConnectionFactory(
    std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativePeerConnectionFactory::~NativePeerConnectionFactory() {
  if (state_ && state_->factory && state_->signaling_thread) {
    state_->signaling_thread->BlockingCall([this] { state_->factory = nullptr; });
  }
  if (state_ && state_->audio_device && state_->worker_thread) {
    // PulseAudio's ADM must also be destroyed on its construction thread.
    state_->worker_thread->BlockingCall([this] { state_->audio_device = nullptr; });
  }
}
const std::unique_ptr<NativePeerConnectionFactory::State>&
NativePeerConnectionFactory::state() const noexcept {
  return state_;
}
webrtc::scoped_refptr<webrtc::PeerConnectionFactoryInterface>
NativePeerConnectionFactory::factory() const noexcept {
  return state_->factory;
}
webrtc::Thread* NativePeerConnectionFactory::signaling_thread() const noexcept {
  return state_->signaling_thread;
}
webrtc::scoped_refptr<webrtc::AudioDeviceModule>
NativePeerConnectionFactory::audio_device() const noexcept {
  return state_->audio_device;
}

std::shared_ptr<ReadinessSignal>
NativePeerConnectionFactory::readiness() const noexcept {
  return state_->readiness;
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
webrtc::scoped_refptr<webrtc::PeerConnectionInterface>
NativePeerConnection::peer() const noexcept {
  return state_->peer;
}
webrtc::Thread* NativePeerConnection::signaling_thread() const noexcept {
  return state_->signaling_thread;
}
webrtc::Thread* NativePeerConnection::worker_thread() const noexcept {
  return state_->worker_thread;
}
std::shared_ptr<ReadinessSignal>
NativePeerConnection::readiness() const noexcept {
  return state_->events->readiness;
}
bool NativePeerConnection::reserve_encoded_receiver(
    const std::string& id) const noexcept {
  return !state_->closed && state_->encoded_receiver_ids.insert(id).second;
}
bool NativePeerConnection::reserve_audio_receiver(
    const std::string& id) const noexcept {
  return !state_->closed && state_->audio_receiver_ids.insert(id).second;
}
void NativePeerConnection::release_audio_receiver(
    const std::string& id) const noexcept {
  state_->audio_receiver_ids.erase(id);
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
    bool native_audio,
    const NativeReadiness* readiness,
    const FfiAudioProcessingConfig& processing,
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
  if ((video_encoder == nullptr) != (video_decoder == nullptr)) {
    error = "video encoder and decoder factories must be supplied together";
    return nullptr;
  }
  // Core and default peers stay headless. Native ADM construction is an
  // explicit opt-in and is never linked by the core bridge.
  webrtc::scoped_refptr<HeadlessAudioDevice> headless_audio_device;
  webrtc::scoped_refptr<webrtc::AudioDeviceModule> platform_audio_device;
  if (native_audio) {
#if defined(PULSEBEAM_WEBRTC_NATIVE_AUDIO)
    dependencies.worker_thread->BlockingCall([&] {
      platform_audio_device = webrtc::CreateAudioDeviceModule(
          *dependencies.env, webrtc::AudioDeviceModule::kPlatformDefaultAudio);
    });
    if (!platform_audio_device) {
      error = "native audio device module unavailable";
      return nullptr;
    }
    // WebRtcVoiceEngine::Init calls adm_helpers::Init on the worker and
    // RTC_CHECKs the result. Fail gracefully before constructing a peer on
    // hosts without a usable sound service instead of aborting the process.
    bool audio_initialized = false;
    dependencies.worker_thread->BlockingCall([&] {
      audio_initialized = platform_audio_device->Init() == 0;
    });
    if (!audio_initialized) {
      // The PulseAudio backend checks destructor thread affinity as well.
      dependencies.worker_thread->BlockingCall([&] {
        platform_audio_device = nullptr;
      });
      error = "native audio device module initialization failed";
      return nullptr;
    }
    dependencies.adm = platform_audio_device;
#else
    error = "native audio requires a native artifact";
    return nullptr;
#endif
  } else {
    headless_audio_device = webrtc::make_ref_counted<HeadlessAudioDevice>();
    dependencies.adm = headless_audio_device;
  }
  dependencies.audio_encoder_factory =
      audio_encoder ? audio_encoder->factory()
                    : webrtc::CreateBuiltinAudioEncoderFactory();
  dependencies.audio_decoder_factory =
      audio_decoder ? audio_decoder->factory()
                    : webrtc::CreateBuiltinAudioDecoderFactory();
  if (video_encoder) {
    dependencies.video_encoder_factory =
        std::make_unique<BorrowedVideoEncoderFactory>(*video_encoder);
  }
  if (video_decoder) {
    dependencies.video_decoder_factory =
        std::make_unique<BorrowedVideoDecoderFactory>(*video_decoder);
  }
  if (processing.enabled) {
    if (processing.noise_suppression > 4 || processing.gain_control > 4) {
      error = "invalid audio processing configuration";
      return nullptr;
    }
    webrtc::AudioProcessing::Config config;
    config.echo_canceller.enabled = processing.echo_cancellation;
    config.noise_suppression.enabled = processing.noise_suppression != 0;
    if (processing.noise_suppression != 0) {
      config.noise_suppression.level = static_cast<
          webrtc::AudioProcessing::Config::NoiseSuppression::Level>(
              processing.noise_suppression - 1);
    }
    config.gain_controller1.enabled = processing.gain_control >= 1 &&
                                      processing.gain_control <= 3;
    if (config.gain_controller1.enabled) {
      config.gain_controller1.mode = static_cast<
          webrtc::AudioProcessing::Config::GainController1::Mode>(
              processing.gain_control - 1);
    }
    config.gain_controller2.enabled = processing.gain_control == 4;
    if (config.gain_controller2.enabled) {
      config.gain_controller2.adaptive_digital.enabled = true;
    }
    dependencies.audio_processing_builder =
        std::make_unique<webrtc::BuiltinAudioProcessingBuilder>(config);
  }
  webrtc::EnableMedia(dependencies);
  auto factory =
      webrtc::CreateModularPeerConnectionFactory(std::move(dependencies));
  if (!factory) {
    if (platform_audio_device) {
      worker_thread.thread()->BlockingCall([&] { platform_audio_device = nullptr; });
    }
    error = "CreateModularPeerConnectionFactory failed";
    return nullptr;
  }
  auto state = std::make_unique<NativePeerConnectionFactory::State>();
  state->readiness = readiness ? readiness->signal() : nullptr;
  state->headless_audio_device = std::move(headless_audio_device);
  state->audio_device = std::move(platform_audio_device);
  state->factory = std::move(factory);
  state->signaling_thread = signaling_thread.thread();
  state->worker_thread = worker_thread.thread();
  return std::make_unique<NativePeerConnectionFactory>(std::move(state));
}

FfiAudioProcessingState factory_audio_processing_state(
    const NativePeerConnectionFactory& factory) noexcept {
  FfiAudioProcessingState result{};
  if (!factory.factory() || !factory.state()->worker_thread) return result;
  const auto encode = [](const std::optional<bool>& value) -> std::int8_t {
    return value ? (*value ? 1 : 0) : -1;
  };
  factory.state()->worker_thread->BlockingCall([&] {
    const auto state = factory.factory()->GetAudioProcessingState();
    result.has_module = state.has_audio_processing_module;
    const auto fill = [&](const webrtc::AudioProcessingComponentState& component,
                          std::int8_t& software, bool& available,
                          std::int8_t& platform, std::uint8_t& effective) {
      software = encode(component.software_active);
      available = component.platform_available;
      platform = encode(component.platform_active);
      effective = static_cast<std::uint8_t>(component.effective);
    };
    fill(state.echo_cancellation, result.echo_software,
         result.echo_platform_available, result.echo_platform,
         result.echo_effective);
    fill(state.noise_suppression, result.noise_software,
         result.noise_platform_available, result.noise_platform,
         result.noise_effective);
    fill(state.auto_gain_control, result.gain_software,
         result.gain_platform_available, result.gain_platform,
         result.gain_effective);
    fill(state.high_pass_filter, result.highpass_software,
         result.highpass_platform_available, result.highpass_platform,
         result.highpass_effective);
  });
  return result;
}

bool factory_audio_devices(const NativePeerConnectionFactory& factory,
                           bool recording, rust::Vec<FfiAudioDevice>& devices,
                           rust::String& error) noexcept {
  auto adm = factory.audio_device();
  if (!adm || !factory.state()->worker_thread) {
    error = "device enumeration requires a native-audio factory";
    return false;
  }
  bool ok = false;
  factory.state()->worker_thread->BlockingCall([&] {
    const int count = recording ? adm->RecordingDevices() : adm->PlayoutDevices();
    if (count < 0) { error = "audio device enumeration failed"; return; }
    for (int index = 0; index < count; ++index) {
      char name[webrtc::kAdmMaxDeviceNameSize] = {};
      char guid[webrtc::kAdmMaxGuidSize] = {};
      const int status = recording
          ? adm->RecordingDeviceName(index, name, guid)
          : adm->PlayoutDeviceName(index, name, guid);
      if (status != 0) {
        devices.clear();
        error = "audio device name lookup failed";
        return;
      }
      FfiAudioDevice device{};
      device.index = static_cast<std::uint16_t>(index);
      device.name = name;
      device.id = guid;
      devices.push_back(std::move(device));
    }
    ok = true;
  });
  return ok;
}
bool factory_select_audio_device(const NativePeerConnectionFactory& factory,
                                 bool recording, std::uint16_t index,
                                 rust::String& error) noexcept {
  auto adm = factory.audio_device();
  if (!adm || !factory.state()->worker_thread) {
    error = "device selection requires a native-audio factory";
    return false;
  }
  bool ok = false;
  factory.state()->worker_thread->BlockingCall([&] {
    const int count = recording ? adm->RecordingDevices() : adm->PlayoutDevices();
    if (count < 0 || index >= count) {
      error = "audio device index is not currently available";
      return;
    }
    ok = (recording ? adm->SetRecordingDevice(index)
                    : adm->SetPlayoutDevice(index)) == 0;
    if (!ok) error = "audio device selection failed or is currently active";
  });
  return ok;
}

// A peer-scoped additional trust anchor for TURN/TLS. The upstream adapter
// still enforces hostname matching after chain verification. Default WebRTC
// roots remain trusted, so this widens trust only when explicitly configured.
class TurnCaVerifier final : public webrtc::SSLCertificateVerifier {
 public:
  explicit TurnCaVerifier(X509* ca) : ca_(ca, X509_free) {}

  bool VerifyChain(const webrtc::SSLCertChain& chain) override {
    if (chain.GetSize() == 0) {
      return false;
    }
    std::unique_ptr<X509_STORE, decltype(&X509_STORE_free)> store(
        X509_STORE_new(), X509_STORE_free);
    std::unique_ptr<X509_STORE_CTX, decltype(&X509_STORE_CTX_free)> context(
        X509_STORE_CTX_new(), X509_STORE_CTX_free);
    std::unique_ptr<STACK_OF(X509), void (*)(STACK_OF(X509)*)> intermediates(
        sk_X509_new_null(), [](STACK_OF(X509)* stack) { sk_X509_free(stack); });
    if (!store || !context || !intermediates ||
        X509_STORE_add_cert(store.get(), ca_.get()) != 1) {
      return false;
    }
    std::vector<std::unique_ptr<X509, decltype(&X509_free)>> certificates;
    for (std::size_t i = 0; i < chain.GetSize(); ++i) {
      webrtc::Buffer der;
      chain.Get(i).ToDER(&der);
      const unsigned char* data = der.data();
      certificates.emplace_back(d2i_X509(nullptr, &data, der.size()), X509_free);
      if (!certificates.back()) {
        return false;
      }
      if (i > 0 && sk_X509_push(intermediates.get(), certificates.back().get()) == 0) {
        return false;
      }
    }
    const bool verified =
        X509_STORE_CTX_init(context.get(), store.get(), certificates.front().get(),
                            intermediates.get()) == 1 &&
        X509_STORE_CTX_set_purpose(context.get(), X509_PURPOSE_SSL_SERVER) == 1 &&
        X509_verify_cert(context.get()) == 1;
    context.reset();  // Release references before the certificate chain.
    return verified;
  }

 private:
  std::unique_ptr<X509, decltype(&X509_free)> ca_;
};

std::unique_ptr<NativePeerConnection> create_peer_connection(
    const NativePeerConnectionFactory& factory,
    std::uint16_t ice_candidate_pool_size,
    bool always_negotiate_data_channels,
    rust::Slice<const FfiIceServer> ice_servers,
    bool relay_only,
    rust::Str turn_tls_ca_pem,
    rust::String& error) noexcept {
  auto events = std::make_shared<EventState>(factory.readiness());
  auto observer = std::make_unique<PeerObserver>(events);
  webrtc::PeerConnectionDependencies dependencies(observer.get());
  if (!turn_tls_ca_pem.empty()) {
    if (turn_tls_ca_pem.size() > INT_MAX) {
      error = "invalid TURN TLS CA certificate";
      return nullptr;
    }
    const std::string pem(turn_tls_ca_pem);
    std::unique_ptr<BIO, decltype(&BIO_free)> bio(
        BIO_new_mem_buf(pem.data(), static_cast<int>(pem.size())), BIO_free);
    std::unique_ptr<X509, decltype(&X509_free)> ca(
        bio ? PEM_read_bio_X509(bio.get(), nullptr, nullptr, nullptr) : nullptr,
        X509_free);
    if (!ca || X509_check_ca(ca.get()) <= 0) {
      error = "invalid TURN TLS CA certificate";
      return nullptr;
    }
    dependencies.tls_cert_verifier =
        std::make_unique<TurnCaVerifier>(ca.release());
  }
  webrtc::PeerConnectionInterface::RTCConfiguration configuration;
  configuration.ice_candidate_pool_size = ice_candidate_pool_size;
  configuration.always_negotiate_data_channels =
      always_negotiate_data_channels;
  configuration.type = relay_only
                           ? webrtc::PeerConnectionInterface::kRelay
                           : webrtc::PeerConnectionInterface::kAll;
  for (const auto& entry : ice_servers) {
    webrtc::PeerConnectionInterface::IceServer server;
    server.urls.push_back(std::string(entry.url));
    server.username = std::string(entry.username);
    server.password = std::string(entry.password);
    configuration.servers.push_back(std::move(server));
  }
  auto result = factory.state()->factory->CreatePeerConnectionOrError(
      configuration, std::move(dependencies));
  if (!result.ok()) {
    error = result.error().message();
    return nullptr;
  }
  auto state = std::make_unique<NativePeerConnection::State>();
  state->headless_audio_device = factory.state()->headless_audio_device;
  state->native_audio_enabled = factory.state()->audio_device != nullptr;
  state->peer = result.MoveValue();
  state->events = std::move(events);
  state->observer = std::move(observer);
  state->signaling_thread = factory.state()->signaling_thread;
  state->worker_thread = factory.state()->worker_thread;
  return std::make_unique<NativePeerConnection>(std::move(state));
}

bool pump_headless_audio(const NativePeerConnection& peer) noexcept {
  return peer.state()->headless_audio_device &&
         peer.state()->headless_audio_device->Pump();
}

void peer_create_offer(const NativePeerConnection& peer,
                       std::uint64_t operation_id,
                       bool ice_restart) noexcept {
  const auto& state = peer.state();
  if (!state->events->Begin(operation_id)) {
    return;
  }
  auto observer =
      webrtc::make_ref_counted<CreateDescriptionObserver>(state->events,
                                                          operation_id);
  webrtc::PeerConnectionInterface::RTCOfferAnswerOptions options;
  options.ice_restart = ice_restart;
  state->peer->CreateOffer(observer.get(), options);
}

bool peer_request_stats(const NativePeerConnection& peer,
                        std::uint64_t operation_id) noexcept {
  const auto& state = peer.state();
  if (!state->events->BeginStats(operation_id)) {
    return false;
  }
  state->peer->GetStats(
      webrtc::make_ref_counted<StatsObserver>(state->events, operation_id).get());
  return true;
}

bool peer_set_bitrate(const NativePeerConnection& peer, std::int32_t minimum,
                      std::int32_t start, std::int32_t maximum,
                      std::uint8_t& error_type, rust::String& message) noexcept {
  webrtc::BitrateSettings settings;
  if (minimum >= 0) settings.min_bitrate_bps = minimum;
  if (start >= 0) settings.start_bitrate_bps = start;
  if (maximum >= 0) settings.max_bitrate_bps = maximum;
  const auto result = peer.state()->peer->SetBitrate(settings);
  error_type = static_cast<std::uint8_t>(result.type());
  message = result.message();
  return result.ok();
}

std::uint8_t peer_descriptions(
    const NativePeerConnection& peer,
    rust::Vec<FfiDescriptionSnapshot>& descriptions) noexcept {
  const auto& state = peer.state();
  std::uint8_t status = 0;
  state->signaling_thread->BlockingCall([&] {
    if (state->closed) {
      status = 1;
      return;
    }
    auto copy = [&](std::uint8_t slot,
                    const webrtc::SessionDescriptionInterface* description) {
      if (!description) {
        return true;
      }
      std::string sdp;
      if (!description->ToString(&sdp)) {
        return false;
      }
      FfiDescriptionSnapshot snapshot;
      snapshot.slot = slot;
      snapshot.kind = static_cast<std::uint8_t>(description->GetType());
      snapshot.sdp = sdp;
      descriptions.push_back(std::move(snapshot));
      return true;
    };
    if (!copy(0, state->peer->current_local_description()) ||
        !copy(1, state->peer->current_remote_description()) ||
        !copy(2, state->peer->pending_local_description()) ||
        !copy(3, state->peer->pending_remote_description())) {
      descriptions.clear();
      status = 2;
    }
  });
  return status;
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
  // This overload bypasses the upstream proxy. Keep initiation as well as
  // completion on the retained signaling sequence after caller migration.
  state->signaling_thread->BlockingCall([&] {
    state->peer->SetLocalDescription(
        std::move(description),
        webrtc::make_ref_counted<SetLocalObserver>(state->events, operation_id));
  });
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
  state->signaling_thread->BlockingCall([&] {
    state->peer->SetRemoteDescription(
        std::move(description),
        webrtc::make_ref_counted<SetRemoteObserver>(state->events, operation_id));
  });
}

void peer_reject_controlled_media(const NativePeerConnection& peer,
                                  std::uint64_t operation_id) noexcept {
  const auto& events = peer.state()->events;
  if (!events->Begin(operation_id)) {
    return;
  }
  events->Complete(ErrorEvent(
      operation_id, webrtc::RTCErrorType::UNSUPPORTED_OPERATION,
      "controlled audio/video requires threaded codec queues"));
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

std::unique_ptr<NativeDataChannel> peer_take_data_channel(
    const NativePeerConnection& peer,
    std::uint64_t arrival_id) noexcept {
  auto channel = peer.state()->events->TakeDataChannel(arrival_id);
  return wrap_data_channel(std::move(channel), peer.peer(),
                           peer.signaling_thread(), peer.readiness());
}

std::unique_ptr<NativeRtpTransceiver> peer_take_transceiver(
    const NativePeerConnection& peer, std::uint64_t arrival_id) noexcept {
  return wrap_rtp_transceiver(peer.state()->events->TakeTransceiver(arrival_id));
}

std::unique_ptr<NativeRtpReceiver> peer_take_receiver(
    const NativePeerConnection& peer, std::uint64_t arrival_id) noexcept {
  return wrap_rtp_receiver(peer.state()->events->TakeReceiver(arrival_id));
}

bool peer_set_native_audio_enabled(const NativePeerConnection& peer,
                                   bool recording, bool enabled,
                                   rust::String& error) noexcept {
  const auto& state = peer.state();
  if (!state || !state->native_audio_enabled) {
    error = "native audio is not enabled for this factory";
    return false;
  }
  bool ok = false;
  state->signaling_thread->BlockingCall([&] {
    if (state->closed || !state->peer) {
      error = "peer connection is closed";
      return;
    }
    // WebRTC owns the ADM start/stop decisions and only starts a device when
    // corresponding send/receive streams exist. Do not call ADM directly.
    if (recording) {
      state->peer->SetAudioRecording(enabled);
    } else {
      state->peer->SetAudioPlayout(enabled);
    }
    ok = true;
  });
  return ok;
}

bool close_peer_connection(const NativePeerConnection& peer) noexcept {
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
