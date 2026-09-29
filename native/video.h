#pragma once

#include <cstddef>
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
struct FfiSenderParameters;
struct FfiCodecFormat;
struct FfiVideoCodecCapability;
struct FfiEncodedVideoFrame;

class NativeScreenCapture final {
 public:
  struct State;
  explicit NativeScreenCapture(std::unique_ptr<State> state) noexcept;
  ~NativeScreenCapture();
  const std::unique_ptr<State>& state() const noexcept;
 private:
  std::unique_ptr<State> state_;
};

class NativeCamera final {
 public:
  struct State;
  explicit NativeCamera(std::unique_ptr<State> state) noexcept;
  ~NativeCamera();
  const std::unique_ptr<State>& state() const noexcept;
 private:
  std::unique_ptr<State> state_;
};

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

class NativeEncodedVideoSink final {
 public:
  struct State;
  explicit NativeEncodedVideoSink(std::unique_ptr<State> state) noexcept;
  ~NativeEncodedVideoSink();
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
  webrtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver() const noexcept;

 private:
  std::unique_ptr<State> state_;
};

class NativeTransceiverList final {
 public:
  struct State;
  explicit NativeTransceiverList(std::unique_ptr<State> state) noexcept;
  ~NativeTransceiverList();
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

struct FfiCameraDevice;
struct FfiCameraFormat;
bool camera_formats(rust::Str device_id,
                    rust::Vec<FfiCameraFormat>& formats,
                    rust::String& error) noexcept;
struct FfiScreenSource;
bool screen_sources(bool windows, rust::Vec<FfiScreenSource>& screens,
                    rust::String& error) noexcept;
std::unique_ptr<NativeScreenCapture> open_screen(
    const NativeVideoSource& source, std::int64_t screen_id, bool window,
    rust::String& error) noexcept;
bool screen_capture_next_frame(const NativeScreenCapture& capture) noexcept;
std::uint8_t screen_capture_status(const NativeScreenCapture& capture) noexcept;
std::uint64_t screen_capture_failed_frames(const NativeScreenCapture& capture) noexcept;
bool close_screen(const NativeScreenCapture& capture) noexcept;
bool camera_devices(rust::Vec<FfiCameraDevice>& devices,
                    rust::String& error) noexcept;
std::unique_ptr<NativeCamera> open_camera(const NativeVideoSource& source,
                                           rust::Str device_id,
                                           std::uint32_t width,
                                           std::uint32_t height,
                                           std::uint32_t fps,
                                           rust::String& error) noexcept;
// Starting, streaming, stalled, closed, or capture stopped unexpectedly.
std::uint8_t camera_capture_status(const NativeCamera& camera,
                                   std::uint64_t stale_after_ms) noexcept;
bool close_camera(const NativeCamera& camera) noexcept;

std::unique_ptr<NativeVideoSource> create_video_source(
    const NativePeerConnectionFactory& factory) noexcept;
bool close_video_source(const NativeVideoSource& source) noexcept;
std::uint8_t video_source_state(const NativeVideoSource& source) noexcept;
bool video_source_push_encoded_trigger(
    const NativeVideoSource& source, std::uint32_t width,
    std::uint32_t height, std::int64_t timestamp_us,
    std::int64_t token) noexcept;
bool video_source_push_frame(const NativeVideoSource& source,
                             rust::Slice<const std::uint8_t> data,
                             std::uint32_t width, std::uint32_t height,
                             std::int64_t timestamp_us,
                             std::uint32_t rtp_timestamp,
                             std::uint16_t rotation) noexcept;
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
std::uint64_t video_sink_dropped_frames(const NativeVideoSink& sink) noexcept;
bool close_video_sink(const NativeVideoSink& sink) noexcept;

std::unique_ptr<NativeTransceiverList> peer_audio_transceivers(
    const NativePeerConnection& peer) noexcept;
std::unique_ptr<NativeTransceiverList> peer_video_transceivers(
    const NativePeerConnection& peer) noexcept;
rust::Vec<FfiVideoCodecCapability> peer_video_codec_capabilities(
    const NativePeerConnectionFactory& factory, bool sender) noexcept;
std::size_t transceiver_list_len(const NativeTransceiverList& list) noexcept;
std::unique_ptr<NativeRtpTransceiver> transceiver_list_at(
    const NativeTransceiverList& list, std::size_t index) noexcept;
std::unique_ptr<NativeRtpTransceiver> peer_add_video_transceiver(
    const NativePeerConnection& peer, const NativeVideoTrack& track,
    std::uint8_t direction, rust::Slice<const rust::String> rids,
    std::uint8_t& error_type, rust::String& error) noexcept;
bool peer_remove_track(const NativePeerConnection& peer,
                       const NativeRtpSender& sender,
                       std::uint8_t& error_type, rust::String& error) noexcept;
std::unique_ptr<NativeRtpTransceiver> peer_take_transceiver(
    const NativePeerConnection& peer, std::uint64_t arrival_id) noexcept;
std::unique_ptr<NativeRtpReceiver> peer_take_receiver(
    const NativePeerConnection& peer, std::uint64_t arrival_id) noexcept;
std::unique_ptr<NativeRtpTransceiver> wrap_rtp_transceiver(
    webrtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver) noexcept;
bool rtp_receiver_request_keyframe(
    const NativePeerConnection& peer, const NativeRtpReceiver& receiver) noexcept;
std::unique_ptr<NativeEncodedVideoSink> rtp_receiver_attach_encoded_video_sink(
    const NativePeerConnection& peer, const NativeRtpReceiver& receiver) noexcept;
FfiEncodedVideoFrame encoded_video_sink_take_frame(
    const NativeEncodedVideoSink& sink) noexcept;
std::uint64_t encoded_video_sink_dropped_frames(
    const NativeEncodedVideoSink& sink) noexcept;
bool close_encoded_video_sink(const NativeEncodedVideoSink& sink) noexcept;

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
bool rtp_transceiver_mid(const NativeRtpTransceiver& transceiver,
                         rust::String& mid) noexcept;
bool rtp_transceiver_set_video_codec_preferences(
    const NativeRtpTransceiver& transceiver, const NativePeerConnectionFactory& factory,
    rust::Slice<const FfiCodecFormat> formats, std::uint8_t& error_type,
    rust::String& error) noexcept;
bool rtp_transceiver_stop(const NativeRtpTransceiver& transceiver,
                          std::uint8_t& error_type,
                          rust::String& error) noexcept;
bool rtp_transceiver_set_direction(const NativeRtpTransceiver& transceiver,
                                   std::uint8_t direction,
                                   std::uint8_t& error_type,
                                   rust::String& error) noexcept;
rust::String rtp_sender_id(const NativeRtpSender& sender) noexcept;
bool rtp_sender_request_keyframe(const NativeRtpSender& sender,
                                 const NativePeerConnection& peer,
                                 rust::Slice<const rust::String> rids,
                                 std::uint8_t& error_type,
                                 rust::String& error) noexcept;
bool rtp_sender_get_parameters(const NativeRtpSender& sender,
                               FfiSenderParameters& parameters) noexcept;
bool rtp_sender_set_parameters(const NativeRtpSender& sender,
                               const FfiSenderParameters& parameters,
                               std::uint8_t& error_type,
                               rust::String& error) noexcept;
std::unique_ptr<NativeVideoTrack> rtp_sender_track(
    const NativeRtpSender& sender) noexcept;
bool rtp_sender_set_video_track(const NativeRtpSender& sender,
                                const NativeVideoTrack& track) noexcept;
bool rtp_sender_clear_track(const NativeRtpSender& sender) noexcept;
rust::String rtp_receiver_id(const NativeRtpReceiver& receiver) noexcept;
std::unique_ptr<NativeVideoTrack> rtp_receiver_track(
    const NativeRtpReceiver& receiver) noexcept;

}  // namespace pulsebeam::webrtc_sys
