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
#include "api/crypto/crypto_options.h"
#if defined(PULSEBEAM_WEBRTC_NATIVE_AUDIO)
#include "api/audio/create_audio_device_module.h"
#endif
#include "api/audio/builtin_audio_processing_builder.h"
#include "api/audio_codecs/builtin_audio_decoder_factory.h"
#include "api/audio_codecs/builtin_audio_encoder_factory.h"
#include "api/frame_transformer_interface.h"
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
#include "rtc_base/ssl_adapter.h"
#include "rtc_base/ssl_fingerprint.h"
#include "rtc_base/rtc_certificate.h"
#include "rtc_base/rtc_certificate_generator.h"

#ifndef OPENSSL_IS_BORINGSSL
#error "Shared immutable certificate resources require the qualified BoringSSL backend"
#endif
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

webrtc::PeerConnectionInterface::RTCOfferAnswerOptions OfferAnswerOptions(
    const FfiOfferAnswerOptions& input) {
  webrtc::PeerConnectionInterface::RTCOfferAnswerOptions result;
  result.offer_to_receive_video = input.receive_video;
  result.offer_to_receive_audio = input.receive_audio;
  result.voice_activity_detection = input.voice_activity_detection;
  result.ice_restart = input.ice_restart;
  result.use_rtp_mux = input.use_rtp_mux;
  result.raw_packetization_for_video = input.raw_packetization_for_video;
  result.num_simulcast_layers = input.simulcast_layers;
  result.use_obsolete_sctp_sdp = input.use_obsolete_sctp_sdp;
  return result;
}

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

  // 0 accepted, 1 closed, 2 exhausted, 3 previous stats not consumed,
  // 4 another native SDP callback pending.
  // Queued terminal outcomes retain their admission until Take observes them.
  std::uint8_t Begin(std::uint64_t operation_id, bool sdp = false) {
    std::lock_guard lock(mutex);
    if (!closed && sdp && sdp_operation_id != 0) return 4;
    const auto status = BeginLocked(operation_id);
    if (status == 0 && sdp) sdp_operation_id = operation_id;
    return status;
  }

  std::uint8_t BeginStats(std::uint64_t operation_id) {
    std::lock_guard lock(mutex);
    if (closed) return 1;
    if (stats_operation_id != 0) return 3;
    const auto status = BeginLocked(operation_id);
    if (status == 0) stats_operation_id = operation_id;
    return status;
  }

  std::uint8_t BeginLocked(std::uint64_t operation_id) {
    if (closed) return 1;
    if (admitted.size() >= 64 || !admitted.insert(operation_id).second) return 2;
    pending.insert(operation_id);
    return 0;
  }

  void Complete(FfiPeerEvent event) {
    NotifyOnExit notify{readiness};
    std::lock_guard lock(mutex);
    if (pending.erase(event.operation_id) != 0) {
      if (event.operation_id == sdp_operation_id) sdp_operation_id = 0;
      events.push_back(std::move(event));
    }
  }

  static bool Advisory(std::uint8_t kind) {
    return kind >= kConnectionState && kind <= kNegotiationNeeded;
  }
  static std::size_t ControlBytes(const FfiPeerEvent& event) {
    return event.sdp.size() + event.sdp_mid.size() + event.candidate.size() +
           event.address.size() + event.url.size() + event.message.size();
  }
  bool PushControlLocked(FfiPeerEvent event) {
    if (Advisory(event.kind)) {
      // Replace the callback-time value, and put that value at its actual
      // observation position. Do not reinterpret an old queued notification.
      auto it = std::find_if(events.begin(), events.end(), [&](const auto& old) {
        return old.kind == event.kind;
      });
      if (it != events.end()) {
        events.erase(it);
        ++coalesced_advisories;
      }
    } else {
      const auto bytes = ControlBytes(event);
      if (retained_controls >= 128 || bytes > 256 * 1024 - retained_control_bytes) {
        ++dropped_controls;
        return false;
      }
      ++retained_controls;
      retained_control_bytes += bytes;
    }
    events.push_back(std::move(event));
    return true;
  }
  void Push(FfiPeerEvent event) {
    NotifyOnExit notify{readiness};
    std::lock_guard lock(mutex);
    if (!closed) PushControlLocked(std::move(event));
  }
  bool AdmitArrivalLocked(FfiPeerEvent event) {
    if (data_channels.size() + transceivers.size() + receivers.size() >= 128) {
      ++dropped_controls;
      return false;
    }
    return PushControlLocked(std::move(event));
  }
  FfiPeerEventObservation Observation() {
    std::lock_guard lock(mutex);
    FfiPeerEventObservation result;
    result.dropped_control_events = dropped_controls;
    result.coalesced_advisories = coalesced_advisories;
    result.retained_control_events = retained_controls;
    for (const auto& event : events)
      if (Advisory(event.kind)) ++result.retained_control_events;
    result.admitted_operations = admitted.size();
    result.pending_operations = pending.size();
    return result;
  }

  FfiPeerEvent Take() {
    std::lock_guard lock(mutex);
    if (events.empty()) {
      return EmptyEvent();
    }
    FfiPeerEvent event = std::move(events.front());
    events.pop_front();
    if (!Advisory(event.kind) && event.kind != kOperationComplete &&
        event.kind != kStatsReport && event.kind != kClosed) {
      --retained_controls;
      retained_control_bytes -= ControlBytes(event);
    }
    if (event.kind == kOperationComplete || event.kind == kStatsReport) {
      admitted.erase(event.operation_id);
      if (event.operation_id == stats_operation_id) stats_operation_id = 0;
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
    FfiPeerEvent event;
    event.kind = kDataChannel;
    event.operation_id = arrival_id;
    if (AdmitArrivalLocked(std::move(event)))
      data_channels.emplace(arrival_id, std::move(channel));
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
    FfiPeerEvent event;
    event.kind = kTrack;
    event.operation_id = arrival_id;
    if (AdmitArrivalLocked(std::move(event)))
      transceivers.emplace(arrival_id, std::move(transceiver));
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
    FfiPeerEvent event;
    event.kind = kTrackRemoved;
    event.operation_id = arrival_id;
    if (AdmitArrivalLocked(std::move(event)))
      receivers.emplace(arrival_id, std::move(receiver));
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
      // Arrival handles are discarded below. Do not leave events referring
      // to those handles, or expose new control deliveries through closed peer.
      std::erase_if(events, [](const auto& event) {
        return event.kind != kOperationComplete && event.kind != kStatsReport;
      });
      retained_controls = 0;
      retained_control_bytes = 0;
      for (std::uint64_t operation_id : pending) {
        events.push_back(ClosedOperation(operation_id));
      }
      pending.clear();
      sdp_operation_id = 0;
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
  std::size_t retained_controls = 0, retained_control_bytes = 0;
  std::uint64_t dropped_controls = 0, coalesced_advisories = 0;
  std::set<std::uint64_t> pending;
  std::set<std::uint64_t> admitted;
  std::uint64_t stats_operation_id = 0;
  // Avoid queuing additional SDP callbacks behind an unfinished native create
  // operation: native close can discard those callback wrappers before invocation.
  std::uint64_t sdp_operation_id = 0;
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
  webrtc::scoped_refptr<webrtc::FrameTransformerInterface> opus_transformer;
  std::shared_ptr<ReadinessSignal> readiness;
  webrtc::scoped_refptr<HeadlessAudioDevice> headless_audio_device;
  webrtc::scoped_refptr<webrtc::AudioDeviceModule> audio_device;
  webrtc::scoped_refptr<webrtc::PeerConnectionFactoryInterface> factory;
  webrtc::Thread* signaling_thread = nullptr;
  webrtc::Thread* worker_thread = nullptr;
};

struct NativePeerConnection::State {
  webrtc::scoped_refptr<webrtc::FrameTransformerInterface> opus_transformer;
  webrtc::scoped_refptr<HeadlessAudioDevice> headless_audio_device;
  bool native_audio_enabled = false;
  std::string turn_tls_ca_pem;
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
webrtc::scoped_refptr<webrtc::FrameTransformerInterface>
NativePeerConnection::opus_transformer() const noexcept {
  return state_->opus_transformer;
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
  state->opus_transformer = audio_encoder ? audio_encoder->opus_transformer() : nullptr;
  state->factory = std::move(factory);
  state->signaling_thread = signaling_thread.thread();
  state->worker_thread = worker_thread.thread();
  return std::make_unique<NativePeerConnectionFactory>(std::move(state));
}

static webrtc::CryptoOptions NativeCryptoOptions(const FfiCryptoOptions& options) {
  webrtc::CryptoOptions native;
  native.srtp.enable_gcm_crypto_suites = options.enable_gcm_crypto_suites;
  native.srtp.prefer_gcm_crypto_suites = options.prefer_gcm_crypto_suites;
  native.srtp.enable_aes128_sha1_32_crypto_cipher =
      options.enable_aes128_sha1_32_crypto_cipher;
  native.srtp.enable_aes128_sha1_80_crypto_cipher =
      options.enable_aes128_sha1_80_crypto_cipher;
  native.srtp.enable_encrypted_rtp_header_extensions =
      options.enable_encrypted_rtp_header_extensions;
  native.srtp.cryptex_policy =
      static_cast<webrtc::CryptoOptions::Srtp::CryptexPolicy>(options.cryptex_policy);
  native.sframe.require_frame_encryption = options.require_frame_encryption;
  native.ephemeral_key_exchange_cipher_groups.SetEnabled(
      std::vector<std::uint16_t>(options.key_exchange_groups.begin(),
                                 options.key_exchange_groups.end()));
  return native;
}

static FfiCryptoOptions CryptoOptionsSnapshot(const webrtc::CryptoOptions& native) {
  FfiCryptoOptions result{};
  result.enable_gcm_crypto_suites = native.srtp.enable_gcm_crypto_suites;
  result.prefer_gcm_crypto_suites = native.srtp.prefer_gcm_crypto_suites;
  result.enable_aes128_sha1_32_crypto_cipher =
      native.srtp.enable_aes128_sha1_32_crypto_cipher;
  result.enable_aes128_sha1_80_crypto_cipher =
      native.srtp.enable_aes128_sha1_80_crypto_cipher;
  result.enable_encrypted_rtp_header_extensions =
      native.srtp.enable_encrypted_rtp_header_extensions;
  result.cryptex_policy = static_cast<std::uint8_t>(native.srtp.cryptex_policy);
  result.require_frame_encryption = native.sframe.require_frame_encryption;
  for (auto group : native.ephemeral_key_exchange_cipher_groups.GetEnabled()) {
    result.key_exchange_groups.push_back(group);
  }
  return result;
}

FfiCryptoOptions crypto_default_options() noexcept {
  return CryptoOptionsSnapshot(webrtc::CryptoOptions());
}

bool crypto_supported_srtp_suites(const FfiCryptoOptions& options,
                                rust::Vec<std::uint16_t>& suites) noexcept {
  suites.clear();
  // The native query has RTC_CHECK(!crypto_suites.empty()). Never let an
  // expressible invalid value terminate a safe Rust consumer.
  if (!options.enable_gcm_crypto_suites &&
      !options.enable_aes128_sha1_32_crypto_cipher &&
      !options.enable_aes128_sha1_80_crypto_cipher) {
    return false;
  }
  for (auto suite : NativeCryptoOptions(options).GetSupportedDtlsSrtpCryptoSuites()) {
    suites.push_back(static_cast<std::uint16_t>(suite));
  }
  return true;
}

rust::Vec<std::uint16_t> crypto_supported_key_exchange_groups() noexcept {
  rust::Vec<std::uint16_t> groups;
  for (auto group : webrtc::CryptoOptions::EphemeralKeyExchangeCipherGroups::GetSupported()) {
    groups.push_back(group);
  }
  return groups;
}

bool crypto_key_exchange_group_name(std::uint16_t group,
                                   rust::String& name) noexcept {
  const auto native =
      webrtc::CryptoOptions::EphemeralKeyExchangeCipherGroups::GetName(group);
  if (!native) return false;
  name = *native;
  return true;
}

FfiCryptoOptions crypto_update_key_exchange_groups(
    const NativeEnvironment& environment, const FfiCryptoOptions& options,
    rust::Slice<const std::uint16_t> disabled) noexcept {
  auto native = NativeCryptoOptions(options);
  const auto env = environment.environment();
  const std::vector<std::uint16_t> disabled_groups(disabled.begin(), disabled.end());
  native.ephemeral_key_exchange_cipher_groups.Update(&env.field_trials(),
                                                     &disabled_groups);
  return CryptoOptionsSnapshot(native);
}

namespace {

webrtc::KeyParams NativeCertificateKeyParams(
    const FfiCertificateKeyParams& params) {
  if (params.kind == 0) {
    return webrtc::KeyParams::RSA(static_cast<int>(params.modulus_bits),
                                 static_cast<int>(params.public_exponent));
  }
  return webrtc::KeyParams::ECDSA(static_cast<webrtc::ECCurve>(params.curve));
}

bool CertificateSslReady() {
  static const bool initialized = webrtc::InitializeSSL();
  return initialized;
}

std::shared_ptr<NativeCertificate> RetainCertificate(
    webrtc::scoped_refptr<webrtc::RTCCertificate> certificate) {
  if (!certificate) return nullptr;
  return std::make_shared<NativeCertificate>(std::move(certificate));
}

}  // namespace

NativeCertificate::NativeCertificate(
    webrtc::scoped_refptr<webrtc::RTCCertificate> certificate) noexcept
    : certificate_(std::move(certificate)) {}
NativeCertificate::~NativeCertificate() = default;
const webrtc::scoped_refptr<webrtc::RTCCertificate>&
NativeCertificate::certificate() const noexcept {
  return certificate_;
}

FfiCertificateKeyParams certificate_default_key_params(bool rsa) noexcept {
  const webrtc::KeyParams params(
      rsa ? webrtc::KT_RSA : webrtc::KT_DEFAULT);
  FfiCertificateKeyParams result{};
  result.kind = params.type() == webrtc::KT_RSA ? 0 : 1;
  if (result.kind == 0) {
    result.modulus_bits = params.rsa_params().mod_size;
    result.public_exponent = params.rsa_params().pub_exp;
  } else {
    result.curve = static_cast<std::uint8_t>(params.ec_curve());
  }
  return result;
}

bool certificate_key_params_valid(
    const FfiCertificateKeyParams& params) noexcept {
  return params.kind <= 1 && NativeCertificateKeyParams(params).IsValid();
}

std::shared_ptr<NativeCertificate> certificate_generate(
    const FfiCertificateKeyParams& params, bool has_lifetime,
    std::uint64_t lifetime_ms) noexcept {
  if (!certificate_key_params_valid(params) || !CertificateSslReady()) return nullptr;
  return RetainCertificate(webrtc::RTCCertificateGenerator::GenerateCertificate(
      NativeCertificateKeyParams(params),
      has_lifetime ? std::optional<std::uint64_t>(lifetime_ms) : std::nullopt));
}

std::shared_ptr<NativeCertificate> certificate_from_pem(
    rust::Str private_key, rust::Str certificate, bool chain) noexcept {
  if (!CertificateSslReady()) return nullptr;
  const std::string key(private_key);
  const std::string cert(certificate);
  if (!chain) {
    return RetainCertificate(webrtc::RTCCertificate::FromPEM(
        webrtc::RTCCertificatePEM(key, cert)));
  }
  auto identity = webrtc::SSLIdentity::CreateFromPEMChainStrings(key, cert);
  if (!identity) return nullptr;
  return RetainCertificate(webrtc::RTCCertificate::Create(std::move(identity)));
}

FfiCertificatePem certificate_to_pem(
    const NativeCertificate& certificate) noexcept {
  const auto pem = certificate.certificate()->ToPEM();
  FfiCertificatePem result{};
  result.private_key = pem.private_key();
  result.certificate = pem.certificate();
  return result;
}

std::uint64_t certificate_expires(
    const NativeCertificate& certificate) noexcept {
  return certificate.certificate()->Expires();
}

bool certificate_has_expired(const NativeCertificate& certificate,
                             std::uint64_t now) noexcept {
  return certificate.certificate()->HasExpired(now);
}

rust::Vec<FfiCertificateData> certificate_chain(
    const NativeCertificate& certificate) noexcept {
  rust::Vec<FfiCertificateData> result;
  const auto& chain = certificate.certificate()->GetSSLCertificateChain();
  for (std::size_t index = 0; index < chain.GetSize(); ++index) {
    const auto& cert = chain.Get(index);
    FfiCertificateData data{};
    data.pem = cert.ToPEMString();
    webrtc::Buffer der;
    cert.ToDER(&der);
    for (auto byte : der) data.der.push_back(byte);
    data.expiration_unix_seconds = cert.CertificateExpirationTime();
    std::string algorithm;
    data.has_signature_digest_algorithm =
        cert.GetSignatureDigestAlgorithm(&algorithm);
    data.signature_digest_algorithm = algorithm;
    result.push_back(std::move(data));
  }
  return result;
}

FfiCertificateFingerprint certificate_fingerprint(
    const NativeCertificate& certificate, rust::Str algorithm) noexcept {
  FfiCertificateFingerprint result{};
  auto fingerprint = webrtc::SSLFingerprint::Create(
      std::string(algorithm), certificate.certificate()->GetSSLCertificate());
  if (!fingerprint) return result;
  result.valid = true;
  result.algorithm = fingerprint->algorithm;
  result.value = fingerprint->GetRfc4572Fingerprint();
  for (auto byte : fingerprint->digest) result.digest.push_back(byte);
  return result;
}

bool certificate_equal(const NativeCertificate& a,
                       const NativeCertificate& b) noexcept {
  return *a.certificate() == *b.certificate();
}

bool certificate_same_resource(const NativeCertificate& a,
                               const NativeCertificate& b) noexcept {
  return a.certificate().get() == b.certificate().get();
}

void factory_set_options(const NativePeerConnectionFactory& factory,
                         const FfiFactoryOptions& options) noexcept {
  webrtc::PeerConnectionFactoryInterface::Options native;
  native.disable_encryption = options.disable_encryption;
  native.disable_network_monitor = options.disable_network_monitor;
  native.network_ignore_mask = options.network_ignore_mask;
  native.ssl_max_version =
      static_cast<webrtc::SSLProtocolVersion>(options.ssl_max_version);
  factory.signaling_thread()->BlockingCall([&] {
    factory.factory()->SetOptions(native);
  });
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


struct NativePeerConfiguration::State {
  webrtc::PeerConnectionInterface::RTCConfiguration configuration;
  std::string turn_tls_ca_pem;
};
NativePeerConfiguration::NativePeerConfiguration(std::unique_ptr<State> state) noexcept
    : state_(std::move(state)) {}
NativePeerConfiguration::~NativePeerConfiguration() = default;
const std::unique_ptr<NativePeerConfiguration::State>& NativePeerConfiguration::state() const noexcept { return state_; }

static std::optional<int> OptionalInt(const FfiOptionalI32& v) {
  return v.present ? std::optional<int>(v.value) : std::nullopt;
}
static FfiOptionalI32 OptionalSnapshot(const std::optional<int>& v) {
  return {v.has_value(), v.value_or(0)};
}
static void ApplyPeerOptions(webrtc::PeerConnectionInterface::RTCConfiguration& c, const FfiPeerConnectionOptions& v) {
  c.bundle_policy = static_cast<decltype(c.bundle_policy)>(v.bundle_policy);
  c.rtcp_mux_policy = static_cast<decltype(c.rtcp_mux_policy)>(v.rtcp_mux_policy);
  c.disable_ipv6_on_wifi = v.disable_ipv6_on_wifi;
  c.max_ipv6_networks = v.max_ipv6_networks;
  c.disable_link_local_networks = v.disable_link_local_networks;
  c.screencast_min_bitrate = OptionalInt(v.screencast_min_bitrate);
  c.tcp_candidate_policy = static_cast<decltype(c.tcp_candidate_policy)>(v.tcp_candidate_policy);
  c.candidate_network_policy = static_cast<decltype(c.candidate_network_policy)>(v.candidate_network_policy);
  c.audio_jitter_buffer_max_packets = v.audio_jitter_buffer_max_packets;
  c.audio_jitter_buffer_fast_accelerate = v.audio_jitter_buffer_fast_accelerate;
  c.audio_jitter_buffer_min_delay_ms = v.audio_jitter_buffer_min_delay_ms;
  c.ice_connection_receiving_timeout = v.ice_connection_receiving_timeout;
  c.ice_backup_candidate_pair_ping_interval = v.ice_backup_candidate_pair_ping_interval;
  c.continual_gathering_policy = static_cast<decltype(c.continual_gathering_policy)>(v.continual_gathering_policy);
  c.prioritize_most_likely_ice_candidate_pairs = v.prioritize_most_likely_ice_candidate_pairs;
  c.prune_turn_ports = v.prune_turn_ports;
  c.turn_port_prune_policy = static_cast<decltype(c.turn_port_prune_policy)>(v.turn_port_prune_policy);
  c.presume_writable_when_fully_relayed = v.presume_writable_when_fully_relayed;
  c.enable_ice_renomination = v.enable_ice_renomination;
  c.redetermine_role_on_ice_restart = v.redetermine_role_on_ice_restart;
  c.surface_ice_candidates_on_ice_transport_type_changed = v.surface_ice_candidates_on_ice_transport_type_changed;
  c.ice_check_interval_strong_connectivity = OptionalInt(v.ice_check_interval_strong_connectivity);
  c.ice_check_interval_weak_connectivity = OptionalInt(v.ice_check_interval_weak_connectivity);
  c.ice_check_min_interval = OptionalInt(v.ice_check_min_interval);
  c.ice_unwritable_timeout = OptionalInt(v.ice_unwritable_timeout);
  c.ice_unwritable_min_checks = OptionalInt(v.ice_unwritable_min_checks);
  c.ice_inactive_timeout = OptionalInt(v.ice_inactive_timeout);
  c.stun_candidate_keepalive_interval = OptionalInt(v.stun_candidate_keepalive_interval);
  c.stable_writable_connection_ping_interval_ms = OptionalInt(v.stable_writable_connection_ping_interval_ms);
  c.report_usage_pattern_delay_ms = OptionalInt(v.report_usage_pattern_delay_ms);
  if (v.network_preference.present) c.network_preference = static_cast<webrtc::AdapterType>(v.network_preference.value);
  c.sdp_semantics = static_cast<decltype(c.sdp_semantics)>(v.sdp_semantics);
  c.crypto_options = NativeCryptoOptions(v.crypto_options);
  c.offer_extmap_allow_mixed = v.offer_extmap_allow_mixed;
  c.turn_logging_id = std::string(v.turn_logging_id);
  c.enable_implicit_rollback = v.enable_implicit_rollback;
  c.vpn_preference = static_cast<decltype(c.vpn_preference)>(v.vpn_preference);
  for (const auto& entry : v.vpn_list) {
    webrtc::IPAddress address;
    if (webrtc::IPFromString(std::string(entry.address), &address))
      c.vpn_list.emplace_back(address, entry.prefix_length);
  }
  c.port_allocator_config.min_port = v.port_allocator.min_port;
  c.port_allocator_config.max_port = v.port_allocator.max_port;
  c.port_allocator_config.flags = v.port_allocator.flags;
  if (v.pacer_burst_interval.present) {
    const auto us = v.pacer_burst_interval.value;
    c.pacer_burst_interval = us == INT64_MAX ? webrtc::TimeDelta::PlusInfinity() :
      us == INT64_MIN ? webrtc::TimeDelta::MinusInfinity() : webrtc::TimeDelta::Micros(us);
  }
  c.enable_any_address_ports = v.enable_any_address_ports;
  c.max_sctp_streams = v.max_sctp_streams;
  c.enable_sctp_snap = v.enable_sctp_snap;
  c.set_dscp(v.dscp);
  c.set_stats_timestamp_with_environment_clock(v.stats_timestamp_with_environment_clock);
  c.set_cpu_adaptation(v.cpu_adaptation);
  c.set_suspend_below_min_bitrate(v.suspend_below_min_bitrate);
  c.set_prerenderer_smoothing(v.prerenderer_smoothing);
  c.set_experiment_cpu_load_estimator(v.experiment_cpu_load_estimator);
  c.set_audio_rtcp_report_interval_ms(v.audio_rtcp_report_interval_ms);
  c.set_video_rtcp_report_interval_ms(v.video_rtcp_report_interval_ms);
}
static FfiPeerConnectionOptions PeerOptionsSnapshot(const webrtc::PeerConnectionInterface::RTCConfiguration& c) {
  FfiPeerConnectionOptions v{};
  v.bundle_policy = static_cast<std::uint8_t>(c.bundle_policy);
  v.rtcp_mux_policy = static_cast<std::uint8_t>(c.rtcp_mux_policy);
  v.disable_ipv6_on_wifi = c.disable_ipv6_on_wifi;
  v.max_ipv6_networks = c.max_ipv6_networks;
  v.disable_link_local_networks = c.disable_link_local_networks;
  v.screencast_min_bitrate = OptionalSnapshot(c.screencast_min_bitrate);
  v.tcp_candidate_policy = static_cast<std::uint8_t>(c.tcp_candidate_policy);
  v.candidate_network_policy = static_cast<std::uint8_t>(c.candidate_network_policy);
  v.audio_jitter_buffer_max_packets = c.audio_jitter_buffer_max_packets;
  v.audio_jitter_buffer_fast_accelerate = c.audio_jitter_buffer_fast_accelerate;
  v.audio_jitter_buffer_min_delay_ms = c.audio_jitter_buffer_min_delay_ms;
  v.ice_connection_receiving_timeout = c.ice_connection_receiving_timeout;
  v.ice_backup_candidate_pair_ping_interval = c.ice_backup_candidate_pair_ping_interval;
  v.continual_gathering_policy = static_cast<std::uint8_t>(c.continual_gathering_policy);
  v.prioritize_most_likely_ice_candidate_pairs = c.prioritize_most_likely_ice_candidate_pairs;
  v.prune_turn_ports = c.prune_turn_ports;
  v.turn_port_prune_policy = static_cast<std::uint8_t>(c.turn_port_prune_policy);
  v.presume_writable_when_fully_relayed = c.presume_writable_when_fully_relayed;
  v.enable_ice_renomination = c.enable_ice_renomination;
  v.redetermine_role_on_ice_restart = c.redetermine_role_on_ice_restart;
  v.surface_ice_candidates_on_ice_transport_type_changed = c.surface_ice_candidates_on_ice_transport_type_changed;
  v.ice_check_interval_strong_connectivity = OptionalSnapshot(c.ice_check_interval_strong_connectivity);
  v.ice_check_interval_weak_connectivity = OptionalSnapshot(c.ice_check_interval_weak_connectivity);
  v.ice_check_min_interval = OptionalSnapshot(c.ice_check_min_interval);
  v.ice_unwritable_timeout = OptionalSnapshot(c.ice_unwritable_timeout);
  v.ice_unwritable_min_checks = OptionalSnapshot(c.ice_unwritable_min_checks);
  v.ice_inactive_timeout = OptionalSnapshot(c.ice_inactive_timeout);
  v.stun_candidate_keepalive_interval = OptionalSnapshot(c.stun_candidate_keepalive_interval);
  v.stable_writable_connection_ping_interval_ms = OptionalSnapshot(c.stable_writable_connection_ping_interval_ms);
  v.report_usage_pattern_delay_ms = OptionalSnapshot(c.report_usage_pattern_delay_ms);
  v.network_preference = {c.network_preference.has_value(), c.network_preference ? static_cast<std::int32_t>(*c.network_preference) : 0};
  v.sdp_semantics = static_cast<std::uint8_t>(c.sdp_semantics);
  v.crypto_options = CryptoOptionsSnapshot(c.crypto_options);
  v.offer_extmap_allow_mixed = c.offer_extmap_allow_mixed;
  v.turn_logging_id = c.turn_logging_id;
  v.enable_implicit_rollback = c.enable_implicit_rollback;
  v.vpn_preference = static_cast<std::uint8_t>(c.vpn_preference);
  for (const auto& entry : c.vpn_list) {
    FfiNetworkMask mask{}; mask.address = entry.address().ToString();
    mask.prefix_length = entry.prefix_length(); v.vpn_list.push_back(std::move(mask));
  }
  v.port_allocator = {c.port_allocator_config.min_port, c.port_allocator_config.max_port, c.port_allocator_config.flags};
  v.pacer_burst_interval.present = c.pacer_burst_interval.has_value();
  if (c.pacer_burst_interval) v.pacer_burst_interval.value = c.pacer_burst_interval->IsPlusInfinity() ? INT64_MAX :
    c.pacer_burst_interval->IsMinusInfinity() ? INT64_MIN : c.pacer_burst_interval->us();
  v.enable_any_address_ports = c.enable_any_address_ports;
  v.max_sctp_streams = c.max_sctp_streams;
  v.enable_sctp_snap = c.enable_sctp_snap;
  v.dscp = c.dscp();
  v.stats_timestamp_with_environment_clock = c.stats_timestamp_with_environment_clock();
  v.cpu_adaptation = c.cpu_adaptation();
  v.suspend_below_min_bitrate = c.suspend_below_min_bitrate();
  v.prerenderer_smoothing = c.prerenderer_smoothing();
  v.experiment_cpu_load_estimator = c.experiment_cpu_load_estimator();
  v.audio_rtcp_report_interval_ms = c.audio_rtcp_report_interval_ms();
  v.video_rtcp_report_interval_ms = c.video_rtcp_report_interval_ms();
  return v;
}
static webrtc::PeerConnectionInterface::IceServer NativeIceServer(const FfiIceServer& v) {
  webrtc::PeerConnectionInterface::IceServer c;
  c.uri = std::string(v.uri);
  for (const auto& url : v.urls) c.urls.push_back(std::string(url));
  c.username = std::string(v.username); c.password = std::string(v.password);
  c.tls_cert_policy = static_cast<webrtc::PeerConnectionInterface::TlsCertPolicy>(v.tls_certificate_policy);
  c.hostname = std::string(v.hostname);
  for (const auto& protocol : v.tls_alpn_protocols) c.tls_alpn_protocols.push_back(std::string(protocol));
  for (const auto& curve : v.tls_elliptic_curves) c.tls_elliptic_curves.push_back(std::string(curve));
  return c;
}
static FfiIceServer IceServerSnapshot(const webrtc::PeerConnectionInterface::IceServer& c) {
  FfiIceServer v{};
  v.uri = c.uri;
  for (const auto& url : c.urls) v.urls.push_back(url);
  v.username = c.username; v.password = c.password;
  v.tls_certificate_policy = static_cast<std::uint8_t>(c.tls_cert_policy);
  v.hostname = c.hostname;
  for (const auto& protocol : c.tls_alpn_protocols) v.tls_alpn_protocols.push_back(protocol);
  for (const auto& curve : c.tls_elliptic_curves) v.tls_elliptic_curves.push_back(curve);
  return v;
}
FfiPeerConfiguration peer_configuration_values(const NativePeerConfiguration& native) noexcept {
  const auto& c = native.state()->configuration;
  FfiPeerConfiguration v{};
  v.ice_candidate_pool_size = c.ice_candidate_pool_size;
  v.always_negotiate_data_channels = c.always_negotiate_data_channels;
  for (const auto& server : c.servers) v.ice_servers.push_back(IceServerSnapshot(server));
  v.ice_transport_policy = static_cast<std::uint8_t>(c.type);
  v.turn_tls_ca_pem = native.state()->turn_tls_ca_pem;
  v.options = PeerOptionsSnapshot(c);
  return v;
}
FfiPeerConfiguration peer_configuration_default(bool aggressive) noexcept {
  auto state = std::make_unique<NativePeerConfiguration::State>();
  state->configuration = webrtc::PeerConnectionInterface::RTCConfiguration(
      aggressive ? webrtc::PeerConnectionInterface::RTCConfigurationType::kAggressive :
                   webrtc::PeerConnectionInterface::RTCConfigurationType::kSafe);
  return peer_configuration_values(NativePeerConfiguration(std::move(state)));
}
std::unique_ptr<NativePeerConfiguration> peer_configuration_new(const FfiPeerConfiguration& v) noexcept {
  auto state = std::make_unique<NativePeerConfiguration::State>();
  auto& c = state->configuration;
  c.ice_candidate_pool_size = v.ice_candidate_pool_size;
  c.always_negotiate_data_channels = v.always_negotiate_data_channels;
  c.type = static_cast<webrtc::PeerConnectionInterface::IceTransportsType>(v.ice_transport_policy);
  for (const auto& server : v.ice_servers) c.servers.push_back(NativeIceServer(server));
  state->turn_tls_ca_pem = std::string(v.turn_tls_ca_pem);
  ApplyPeerOptions(c, v.options);
  return std::make_unique<NativePeerConfiguration>(std::move(state));
}
void peer_configuration_add_certificate(NativePeerConfiguration& native, const NativeCertificate& certificate) noexcept {
  native.state()->configuration.certificates.push_back(certificate.certificate());
}
std::size_t peer_configuration_certificate_count(const NativePeerConfiguration& native) noexcept {
  return native.state()->configuration.certificates.size();
}
std::shared_ptr<NativeCertificate> peer_configuration_certificate(const NativePeerConfiguration& native, std::size_t index) noexcept {
  const auto& certificates = native.state()->configuration.certificates;
  return index < certificates.size() ? std::make_shared<NativeCertificate>(certificates[index]) : nullptr;
}
std::unique_ptr<NativePeerConfiguration> peer_get_configuration(const NativePeerConnection& peer) noexcept {
  auto state = std::make_unique<NativePeerConfiguration::State>();
  state->configuration = peer.state()->peer->GetConfiguration();
  state->turn_tls_ca_pem = peer.state()->turn_tls_ca_pem;
  return std::make_unique<NativePeerConfiguration>(std::move(state));
}
std::uint8_t peer_set_configuration(const NativePeerConnection& peer, const NativePeerConfiguration& native, rust::String& error) noexcept {
  if (peer.state()->closed) { error = "peer is closed"; return 255; }
  if (peer.state()->turn_tls_ca_pem != native.state()->turn_tls_ca_pem) {
    error = "TURN TLS CA is a creation-time dependency"; return 7;
  }
  auto result = peer.state()->peer->SetConfiguration(native.state()->configuration);
  if (!result.ok()) error = result.message();
  return static_cast<std::uint8_t>(result.type());
}

std::unique_ptr<NativePeerConnection> create_peer_connection(
    const NativePeerConnectionFactory& factory,
    const NativePeerConfiguration& native,
    std::uint8_t& error_kind,
    rust::String& error) noexcept {
  const auto& configuration = native.state()->configuration;
  const auto& turn_tls_ca_pem = native.state()->turn_tls_ca_pem;
  // PortAllocator creation only DCHECKs this native range before allocating
  // pooled sessions. Match SetConfiguration's INVALID_RANGE result rather
  // than triggering a debug abort or unbounded invalid allocation work.
  if (configuration.ice_candidate_pool_size < 0 ||
      configuration.ice_candidate_pool_size > static_cast<int>(UINT16_MAX)) {
    error_kind = static_cast<std::uint8_t>(webrtc::RTCErrorType::INVALID_RANGE);
    error = "ICE candidate pool size must be in the native range 0..=65535";
    return nullptr;
  }
  error_kind = static_cast<std::uint8_t>(webrtc::RTCErrorType::INVALID_PARAMETER);
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
  auto result = factory.state()->factory->CreatePeerConnectionOrError(
      configuration, std::move(dependencies));
  if (!result.ok()) {
    error_kind = static_cast<std::uint8_t>(result.error().type());
    error = result.error().message();
    return nullptr;
  }
  auto state = std::make_unique<NativePeerConnection::State>();
  state->turn_tls_ca_pem = turn_tls_ca_pem;
  state->opus_transformer = factory.state()->opus_transformer;
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

std::uint8_t peer_create_offer(const NativePeerConnection& peer,
                       std::uint64_t operation_id,
                       const FfiOfferAnswerOptions& options) noexcept {
  const auto& state = peer.state();
  const auto status = state->events->Begin(operation_id, true);
  if (status != 0) return status;
  auto observer =
      webrtc::make_ref_counted<CreateDescriptionObserver>(state->events,
                                                          operation_id);
  state->peer->CreateOffer(observer.get(), OfferAnswerOptions(options));
  return 0;
}

std::uint8_t peer_request_stats(const NativePeerConnection& peer,
                        std::uint64_t operation_id) noexcept {
  const auto& state = peer.state();
  const auto status = state->events->BeginStats(operation_id);
  if (status != 0) return status;
  state->peer->GetStats(
      webrtc::make_ref_counted<StatsObserver>(state->events, operation_id).get());
  return 0;
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

std::uint8_t peer_create_answer(const NativePeerConnection& peer,
                        std::uint64_t operation_id,
                        const FfiOfferAnswerOptions& options) noexcept {
  const auto& state = peer.state();
  const auto status = state->events->Begin(operation_id, true);
  if (status != 0) return status;
  auto observer =
      webrtc::make_ref_counted<CreateDescriptionObserver>(state->events,
                                                          operation_id);
  state->peer->CreateAnswer(observer.get(), OfferAnswerOptions(options));
  return 0;
}

bool peer_restart_ice(const NativePeerConnection& peer) noexcept {
  const auto& state = peer.state();
  return state->signaling_thread->BlockingCall([&] {
    if (state->closed) return false;
    state->peer->RestartIce();
    return true;
  });
}

bool peer_should_fire_negotiation_needed(const NativePeerConnection& peer,
                                         std::uint32_t event_id) noexcept {
  return peer.state()->peer->ShouldFireNegotiationNeededEvent(event_id);
}

std::uint8_t peer_set_local_description_auto(const NativePeerConnection& peer,
    std::uint64_t operation_id, bool allow_media) noexcept {
  const auto& state = peer.state();
  const auto status = state->events->Begin(operation_id, true);
  if (status != 0) return status;
  state->signaling_thread->BlockingCall([&] {
    if (!allow_media && (!state->peer->GetSenders().empty() ||
                         !state->peer->GetReceivers().empty())) {
      state->events->Complete(ErrorEvent(
          operation_id, webrtc::RTCErrorType::UNSUPPORTED_OPERATION,
          "controlled audio/video requires admitted cooperative media dependencies"));
      return;
    }
    state->peer->SetLocalDescription(
        webrtc::make_ref_counted<SetLocalObserver>(state->events, operation_id));
  });
  return 0;
}

std::uint8_t peer_modify_ice_candidate(const NativePeerConnection& peer,
    bool remove, rust::Str sdp_mid, std::int32_t sdp_mline_index,
    rust::Str candidate_sdp, rust::String& message) noexcept {
  const auto& state = peer.state();
  return state->signaling_thread->BlockingCall([&]() -> std::uint8_t {
    if (state->closed) return 1;
    webrtc::SdpParseError parse_error;
    auto candidate = webrtc::IceCandidate::Create(
        std::string_view(sdp_mid.data(), sdp_mid.size()), sdp_mline_index,
        std::string_view(candidate_sdp.data(), candidate_sdp.size()), &parse_error);
    if (!candidate) {
      message = parse_error.description;
      return 2;
    }
    const bool accepted = remove ? state->peer->RemoveIceCandidate(candidate.get())
                                 : state->peer->AddIceCandidate(candidate.get());
    return accepted ? 0 : 3;
  });
}

std::uint8_t peer_set_local_description(const NativePeerConnection& peer,
                                std::uint64_t operation_id,
                                std::uint8_t sdp_type,
                                rust::Str sdp) noexcept {
  const auto& state = peer.state();
  const auto status = state->events->Begin(operation_id, true);
  if (status != 0) return status;
  webrtc::SdpParseError parse_error;
  auto description = ParseDescription(sdp_type, sdp, parse_error);
  if (!description) {
    state->events->Complete(ErrorEvent(
        operation_id, webrtc::RTCErrorType::SYNTAX_ERROR,
        parse_error.description));
    return 0;
  }
  // This overload bypasses the upstream proxy. Keep initiation as well as
  // completion on the retained signaling sequence after caller migration.
  state->signaling_thread->BlockingCall([&] {
    state->peer->SetLocalDescription(
        std::move(description),
        webrtc::make_ref_counted<SetLocalObserver>(state->events, operation_id));
  });
  return 0;
}

std::uint8_t peer_set_remote_description(const NativePeerConnection& peer,
                                 std::uint64_t operation_id,
                                 std::uint8_t sdp_type,
                                 rust::Str sdp) noexcept {
  const auto& state = peer.state();
  const auto status = state->events->Begin(operation_id, true);
  if (status != 0) return status;
  webrtc::SdpParseError parse_error;
  auto description = ParseDescription(sdp_type, sdp, parse_error);
  if (!description) {
    state->events->Complete(ErrorEvent(
        operation_id, webrtc::RTCErrorType::SYNTAX_ERROR,
        parse_error.description));
    return 0;
  }
  state->signaling_thread->BlockingCall([&] {
    state->peer->SetRemoteDescription(
        std::move(description),
        webrtc::make_ref_counted<SetRemoteObserver>(state->events, operation_id));
  });
  return 0;
}

std::uint8_t peer_reject_controlled_media(const NativePeerConnection& peer,
                                  std::uint64_t operation_id) noexcept {
  const auto& events = peer.state()->events;
  const auto status = events->Begin(operation_id);
  if (status != 0) return status;
  events->Complete(ErrorEvent(
      operation_id, webrtc::RTCErrorType::UNSUPPORTED_OPERATION,
      "controlled audio/video requires threaded codec queues"));
  return 0;
}

std::uint8_t peer_add_ice_candidate(const NativePeerConnection& peer,
                            std::uint64_t operation_id,
                            rust::Str sdp_mid,
                            std::int32_t sdp_mline_index,
                            rust::Str candidate_sdp) noexcept {
  const auto& state = peer.state();
  const auto status = state->events->Begin(operation_id);
  if (status != 0) return status;
  webrtc::SdpParseError parse_error;
  auto candidate = webrtc::IceCandidate::Create(
      std::string_view(sdp_mid.data(), sdp_mid.size()), sdp_mline_index,
      std::string_view(candidate_sdp.data(), candidate_sdp.size()),
      &parse_error);
  if (!candidate) {
    state->events->Complete(ErrorEvent(
        operation_id, webrtc::RTCErrorType::SYNTAX_ERROR,
        parse_error.description));
    return 0;
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
  return 0;
}

FfiPeerEvent peer_take_event(const NativePeerConnection& peer) noexcept {
  return peer.state()->events->Take();
}
FfiPeerEventObservation peer_event_observation(const NativePeerConnection& peer) noexcept {
  return peer.state()->events->Observation();
}
// Binding-only unit hook: exercise the provided observer callback methods,
// not a transport/media claim or a simulated native protocol implementation.
bool test_peer_control_observations(const NativePeerConnection& peer,
                                   std::uint32_t message_bytes,
                                   bool arrivals) noexcept {
  PeerObserver observer(peer.state()->events);
  for (int i = 0; i < 200; ++i) {
    observer.OnSignalingChange(i % 2 ?
        webrtc::PeerConnectionInterface::kHaveLocalOffer :
        webrtc::PeerConnectionInterface::kStable);
  }
  if (arrivals) {
    auto channel = peer.peer()->CreateDataChannelOrError("retention-unit", nullptr);
    if (!channel.ok()) return false;
    for (int i = 0; i < 200; ++i) observer.OnDataChannel(channel.value());
  } else {
    const std::string message(message_bytes, 'x');
    for (int i = 0; i < 200; ++i)
      observer.OnIceCandidateError("unit", i, "stun:unit", 701, message);
  }
  return true;
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
