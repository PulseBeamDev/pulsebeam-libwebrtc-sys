#pragma once

#include <cstdint>
#include <memory>
#include <string>

#include "api/scoped_refptr.h"
#include "rust/cxx.h"

namespace webrtc {
class AudioDeviceModule;
class FrameTransformerInterface;
class PeerConnectionFactoryInterface;
class PeerConnectionInterface;
class Thread;
}

namespace pulsebeam::webrtc_sys {

struct FfiIceServer;
struct FfiAudioDevice;
struct FfiAudioProcessingConfig;
struct FfiAudioProcessingState;
struct FfiDescriptionSnapshot;
struct FfiPeerEvent;
class NativeAudioDecoderFactory;
class NativeAudioEncoderFactory;
class NativeDataChannel;
class NativeEnvironment;
class NativeNetworkManagerProvider;
class NativePacketSocketFactoryProvider;
class NativeThread;
class NativeVideoDecoderFactory;
class NativeVideoEncoderFactory;

class NativeReadiness;
class ReadinessSignal;

class NativePeerConnectionFactory final {
 public:
  struct State;

  explicit NativePeerConnectionFactory(std::unique_ptr<State> state) noexcept;
  ~NativePeerConnectionFactory();
  NativePeerConnectionFactory(const NativePeerConnectionFactory&) = delete;
  NativePeerConnectionFactory& operator=(const NativePeerConnectionFactory&) =
      delete;

  const std::unique_ptr<State>& state() const noexcept;
  webrtc::scoped_refptr<webrtc::PeerConnectionFactoryInterface> factory()
      const noexcept;
  webrtc::Thread* signaling_thread() const noexcept;
  webrtc::scoped_refptr<webrtc::AudioDeviceModule> audio_device() const noexcept;
  std::shared_ptr<ReadinessSignal> readiness() const noexcept;

 private:
  std::unique_ptr<State> state_;
};

class NativePeerConnection final {
 public:
  struct State;

  explicit NativePeerConnection(std::unique_ptr<State> state) noexcept;
  ~NativePeerConnection();
  NativePeerConnection(const NativePeerConnection&) = delete;
  NativePeerConnection& operator=(const NativePeerConnection&) = delete;

  const std::unique_ptr<State>& state() const noexcept;
  webrtc::scoped_refptr<webrtc::PeerConnectionInterface> peer() const noexcept;
  webrtc::Thread* signaling_thread() const noexcept;
  webrtc::Thread* worker_thread() const noexcept;
  webrtc::scoped_refptr<webrtc::FrameTransformerInterface> opus_transformer() const noexcept;
  std::shared_ptr<ReadinessSignal> readiness() const noexcept;
  bool reserve_encoded_receiver(const std::string& id) const noexcept;
  bool reserve_audio_receiver(const std::string& id) const noexcept;
  void release_audio_receiver(const std::string& id) const noexcept;

 private:
  std::unique_ptr<State> state_;
};

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
    rust::String& error) noexcept;
FfiAudioProcessingState factory_audio_processing_state(
    const NativePeerConnectionFactory& factory) noexcept;
bool factory_audio_devices(const NativePeerConnectionFactory& factory,
                           bool recording, rust::Vec<FfiAudioDevice>& devices,
                           rust::String& error) noexcept;
bool factory_select_audio_device(const NativePeerConnectionFactory& factory,
                                 bool recording, std::uint16_t index,
                                 rust::String& error) noexcept;

std::unique_ptr<NativePeerConnection> create_peer_connection(
    const NativePeerConnectionFactory& factory,
    std::uint16_t ice_candidate_pool_size,
    bool always_negotiate_data_channels,
    rust::Slice<const FfiIceServer> ice_servers,
    bool relay_only,
    rust::Str turn_tls_ca_pem,
    rust::String& error) noexcept;

void peer_create_offer(const NativePeerConnection& peer,
                       std::uint64_t operation_id,
                       bool ice_restart) noexcept;
void peer_create_answer(const NativePeerConnection& peer,
                        std::uint64_t operation_id) noexcept;
bool peer_request_stats(const NativePeerConnection& peer,
                        std::uint64_t operation_id) noexcept;
bool peer_set_bitrate(const NativePeerConnection& peer, std::int32_t minimum,
                      std::int32_t start, std::int32_t maximum,
                      std::uint8_t& error_type, rust::String& message) noexcept;
std::uint8_t peer_descriptions(
    const NativePeerConnection& peer,
    rust::Vec<FfiDescriptionSnapshot>& descriptions) noexcept;
void peer_set_local_description(const NativePeerConnection& peer,
                                std::uint64_t operation_id,
                                std::uint8_t sdp_type,
                                rust::Str sdp) noexcept;
void peer_set_remote_description(const NativePeerConnection& peer,
                                 std::uint64_t operation_id,
                                 std::uint8_t sdp_type,
                                 rust::Str sdp) noexcept;
// Complete a controlled-mode video operation without entering upstream's
// synchronous receive-stream recreation (which waits on a cooperative queue).
void peer_reject_controlled_media(const NativePeerConnection& peer,
                                  std::uint64_t operation_id) noexcept;
void peer_add_ice_candidate(const NativePeerConnection& peer,
                            std::uint64_t operation_id,
                            rust::Str sdp_mid,
                            std::int32_t sdp_mline_index,
                            rust::Str candidate) noexcept;
FfiPeerEvent peer_take_event(const NativePeerConnection& peer) noexcept;
std::unique_ptr<NativeDataChannel> peer_take_data_channel(
    const NativePeerConnection& peer,
    std::uint64_t arrival_id) noexcept;
bool close_peer_connection(const NativePeerConnection& peer) noexcept;
// Route audio lifecycle through WebRTC AudioState, not directly through ADM.
bool peer_set_native_audio_enabled(const NativePeerConnection& peer,
                                   bool recording, bool enabled,
                                   rust::String& error) noexcept;
// Pull one 10 ms receive block from the headless audio transport. This does
// not access any operating-system audio device.
bool pump_headless_audio(const NativePeerConnection& peer) noexcept;

}  // namespace pulsebeam::webrtc_sys
