#pragma once

#include <cstdint>
#include <memory>

#include "api/data_channel_interface.h"
#include "api/peer_connection_interface.h"
#include "api/scoped_refptr.h"
#include "rust/cxx.h"

namespace webrtc {
class Thread;
}

namespace pulsebeam::webrtc_sys {

struct FfiDataChannelEvent;
class NativePeerConnection;

class NativeDataChannel final {
 public:
  struct State;

  explicit NativeDataChannel(std::unique_ptr<State> state) noexcept;
  ~NativeDataChannel();
  NativeDataChannel(const NativeDataChannel&) = delete;
  NativeDataChannel& operator=(const NativeDataChannel&) = delete;

  const std::unique_ptr<State>& state() const noexcept;

 private:
  std::unique_ptr<State> state_;
};

std::unique_ptr<NativeDataChannel> wrap_data_channel(
    webrtc::scoped_refptr<webrtc::DataChannelInterface> channel,
    webrtc::scoped_refptr<webrtc::PeerConnectionInterface> peer,
    webrtc::Thread* signaling_thread) noexcept;

std::unique_ptr<NativeDataChannel> create_data_channel(
    const NativePeerConnection& peer,
    rust::Str label,
    bool ordered,
    std::int32_t max_retransmit_time_ms,
    std::int32_t max_retransmits,
    rust::Str protocol,
    bool negotiated,
    std::int32_t id,
    std::int8_t priority,
    std::uint8_t& error_type,
    rust::String& error) noexcept;
rust::String data_channel_label(const NativeDataChannel& channel) noexcept;
bool data_channel_ordered(const NativeDataChannel& channel) noexcept;
std::int32_t data_channel_max_retransmit_time_ms(
    const NativeDataChannel& channel) noexcept;
std::int32_t data_channel_max_retransmits(
    const NativeDataChannel& channel) noexcept;
rust::String data_channel_protocol(const NativeDataChannel& channel) noexcept;
bool data_channel_negotiated(const NativeDataChannel& channel) noexcept;
std::int32_t data_channel_id(const NativeDataChannel& channel) noexcept;
std::uint8_t data_channel_priority(const NativeDataChannel& channel) noexcept;
std::uint8_t data_channel_state(const NativeDataChannel& channel) noexcept;
std::uint64_t data_channel_buffered_amount(
    const NativeDataChannel& channel) noexcept;
std::uint8_t data_channel_send(const NativeDataChannel& channel,
                               rust::Slice<const std::uint8_t> data,
                               bool binary) noexcept;
FfiDataChannelEvent data_channel_take_event(
    const NativeDataChannel& channel) noexcept;
bool close_data_channel(const NativeDataChannel& channel) noexcept;

}  // namespace pulsebeam::webrtc_sys
