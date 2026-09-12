#pragma once

#include <cstdint>
#include <memory>

#include "api/scoped_refptr.h"
#include "rust/cxx.h"

namespace webrtc {
class PeerConnectionInterface;
class Thread;
}

namespace pulsebeam::webrtc_sys {

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

class NativePeerConnectionFactory final {
 public:
  struct State;

  explicit NativePeerConnectionFactory(std::unique_ptr<State> state) noexcept;
  ~NativePeerConnectionFactory();
  NativePeerConnectionFactory(const NativePeerConnectionFactory&) = delete;
  NativePeerConnectionFactory& operator=(const NativePeerConnectionFactory&) =
      delete;

  const std::unique_ptr<State>& state() const noexcept;

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
    rust::String& error) noexcept;

std::unique_ptr<NativePeerConnection> create_peer_connection(
    const NativePeerConnectionFactory& factory,
    std::uint16_t ice_candidate_pool_size,
    bool always_negotiate_data_channels,
    rust::String& error) noexcept;

void peer_create_offer(const NativePeerConnection& peer,
                       std::uint64_t operation_id) noexcept;
void peer_create_answer(const NativePeerConnection& peer,
                        std::uint64_t operation_id) noexcept;
void peer_set_local_description(const NativePeerConnection& peer,
                                std::uint64_t operation_id,
                                std::uint8_t sdp_type,
                                rust::Str sdp) noexcept;
void peer_set_remote_description(const NativePeerConnection& peer,
                                 std::uint64_t operation_id,
                                 std::uint8_t sdp_type,
                                 rust::Str sdp) noexcept;
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

}  // namespace pulsebeam::webrtc_sys
