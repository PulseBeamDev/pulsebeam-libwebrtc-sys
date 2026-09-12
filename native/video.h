#pragma once

#include <cstdint>
#include <memory>

#include "api/scoped_refptr.h"
#include "rust/cxx.h"

namespace webrtc {
class RtpReceiverInterface;
class RtpTransceiverInterface;
}  // namespace webrtc

namespace pulsebeam::webrtc_sys {

class NativePeerConnectionFactory;
class NativePeerConnection;
class NativeVideoFrame;

class NativeVideoSource final {
 public:
  struct State;
  explicit NativeVideoSource(std::unique_ptr<State> state) noexcept;
  ~NativeVideoSource();
  const std::unique_ptr<State>& state() const noexcept;

 private:
  std::unique_ptr<State> state_;
};

class NativeVideoTrack final {
 public:
  struct State;
  explicit NativeVideoTrack(std::unique_ptr<State> state) noexcept;
  ~NativeVideoTrack();
  const std::unique_ptr<State>& state() const noexcept;

 private:
  std::unique_ptr<State> state_;
};

class NativeVideoSink final {
 public:
  struct State;
  explicit NativeVideoSink(std::unique_ptr<State> state) noexcept;
  ~NativeVideoSink();
  const std::unique_ptr<State>& state() const noexcept;

 private:
  std::unique_ptr<State> state_;
};

class NativeRtpSender final {
 public:
  struct State;
  explicit NativeRtpSender(std::unique_ptr<State> state) noexcept;
  ~NativeRtpSender();
  const std::unique_ptr<State>& state() const noexcept;

 private:
  std::unique_ptr<State> state_;
};

class NativeRtpReceiver final {
 public:
  struct State;
  explicit NativeRtpReceiver(std::unique_ptr<State> state) noexcept;
  ~NativeRtpReceiver();
  const std::unique_ptr<State>& state() const noexcept;

 private:
  std::unique_ptr<State> state_;
};

class NativeRtpTransceiver final {
 public:
  struct State;
  explicit NativeRtpTransceiver(std::unique_ptr<State> state) noexcept;
  ~NativeRtpTransceiver();
  const std::unique_ptr<State>& state() const noexcept;

 private:
  std::unique_ptr<State> state_;
};

std::unique_ptr<NativeVideoSource> create_video_source(
    const NativePeerConnectionFactory& factory) noexcept;
bool close_video_source(const NativeVideoSource& source) noexcept;
std::uint8_t video_source_state(const NativeVideoSource& source) noexcept;
bool video_source_push_frame(const NativeVideoSource& source,
                             rust::Slice<const std::uint8_t> data,
                             std::uint32_t width, std::uint32_t height,
                             std::int64_t timestamp_us,
                             std::uint32_t rtp_timestamp) noexcept;
std::unique_ptr<NativeVideoTrack> create_video_track(
    const NativePeerConnectionFactory& factory,
    const NativeVideoSource& source, rust::Str id) noexcept;
rust::String video_track_id(const NativeVideoTrack& track) noexcept;
bool video_track_enabled(const NativeVideoTrack& track) noexcept;
bool video_track_set_enabled(const NativeVideoTrack& track,
                             bool enabled) noexcept;
std::uint8_t video_track_state(const NativeVideoTrack& track) noexcept;
std::unique_ptr<NativeVideoSink> video_track_attach_sink(
    const NativeVideoTrack& track) noexcept;
std::unique_ptr<NativeVideoFrame> video_sink_take_frame(
    const NativeVideoSink& sink) noexcept;
bool close_video_sink(const NativeVideoSink& sink) noexcept;

std::unique_ptr<NativeRtpTransceiver> peer_add_video_transceiver(
    const NativePeerConnection& peer, const NativeVideoTrack& track,
    std::uint8_t direction, std::uint8_t& error_type,
    rust::String& error) noexcept;
bool peer_remove_track(const NativePeerConnection& peer,
                       const NativeRtpSender& sender,
                       std::uint8_t& error_type, rust::String& error) noexcept;
std::unique_ptr<NativeRtpTransceiver> peer_take_transceiver(
    const NativePeerConnection& peer, std::uint64_t arrival_id) noexcept;
std::unique_ptr<NativeRtpReceiver> peer_take_receiver(
    const NativePeerConnection& peer, std::uint64_t arrival_id) noexcept;
std::unique_ptr<NativeRtpTransceiver> wrap_rtp_transceiver(
    webrtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver) noexcept;
std::unique_ptr<NativeRtpReceiver> wrap_rtp_receiver(
    webrtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver) noexcept;
std::unique_ptr<NativeRtpSender> rtp_transceiver_sender(
    const NativeRtpTransceiver& transceiver) noexcept;
std::unique_ptr<NativeRtpReceiver> rtp_transceiver_receiver(
    const NativeRtpTransceiver& transceiver) noexcept;
std::uint8_t rtp_transceiver_direction(
    const NativeRtpTransceiver& transceiver) noexcept;
std::int8_t rtp_transceiver_current_direction(
    const NativeRtpTransceiver& transceiver) noexcept;
bool rtp_transceiver_stopped(
    const NativeRtpTransceiver& transceiver) noexcept;
bool rtp_transceiver_set_direction(const NativeRtpTransceiver& transceiver,
                                   std::uint8_t direction,
                                   std::uint8_t& error_type,
                                   rust::String& error) noexcept;
rust::String rtp_sender_id(const NativeRtpSender& sender) noexcept;
std::unique_ptr<NativeVideoTrack> rtp_sender_track(
    const NativeRtpSender& sender) noexcept;
rust::String rtp_receiver_id(const NativeRtpReceiver& receiver) noexcept;
std::unique_ptr<NativeVideoTrack> rtp_receiver_track(
    const NativeRtpReceiver& receiver) noexcept;

}  // namespace pulsebeam::webrtc_sys
