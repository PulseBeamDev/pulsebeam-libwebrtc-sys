#pragma once

#include <cstddef>
#include <cstdint>
#include <memory>

#include "api/scoped_refptr.h"
#include "rust/cxx.h"

namespace webrtc {
class RtpSenderInterface;
class RtpReceiverInterface;
class RtpTransceiverInterface;
}  // namespace webrtc

namespace pulsebeam::webrtc_sys {

struct RustFrameEncryptorProvider;
struct RustFrameDecryptorProvider;
struct FfiFrameCryptoOutput;
struct FfiFrameCryptoFailure;
std::size_t frame_crypto_max_output_capacity() noexcept;
class NativeFrameEncryptor final {
 public:
  struct State;
  explicit NativeFrameEncryptor(std::unique_ptr<State> state) noexcept;
  ~NativeFrameEncryptor();
  const std::unique_ptr<State>& state() const noexcept;
 private:
  std::unique_ptr<State> state_;
};
class NativeFrameDecryptor final {
 public:
  struct State;
  explicit NativeFrameDecryptor(std::unique_ptr<State> state) noexcept;
  ~NativeFrameDecryptor();
  const std::unique_ptr<State>& state() const noexcept;
 private:
  std::unique_ptr<State> state_;
};
class NativePeerConnectionFactory;
class NativePeerConnection;
class NativeRtpCapabilities;
class NativeRtcError;
class NativeVideoFrame;
struct FfiSenderParameters;
struct FfiCodecFormat;
struct FfiVideoCodecCapability;
struct FfiEncodedVideoFrame;
struct FfiRtpHeaderExtension;

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

class NativeSframeEncrypterInit;
class NativeRtcError;
class NativePeerConnection;
struct FfiSframeEncrypterCreation;
struct FfiSframeDecrypterCreation;
class NativeSframeEncrypter final {
 public:
  struct State;
  explicit NativeSframeEncrypter(std::unique_ptr<State> state) noexcept;
  ~NativeSframeEncrypter();
  const std::unique_ptr<State>& state() const noexcept;
 private:
  std::unique_ptr<State> state_;
};
class NativeSframeDecrypter final {
 public:
  struct State;
  explicit NativeSframeDecrypter(std::unique_ptr<State> state) noexcept;
  ~NativeSframeDecrypter();
  const std::unique_ptr<State>& state() const noexcept;
 private:
  std::unique_ptr<State> state_;
};
struct FfiRtpByteString;
class NativeRtpSender;
class NativeRtpReceiver;
FfiSframeEncrypterCreation rtp_sender_create_sframe(const NativeRtpSender& sender, const NativePeerConnection& peer, const NativeSframeEncrypterInit& init) noexcept;
FfiSframeDecrypterCreation rtp_receiver_create_sframe(const NativeRtpReceiver& receiver, const NativePeerConnection& peer, std::int32_t suite) noexcept;
std::unique_ptr<NativeSframeEncrypter> sframe_encrypter_clone(const NativeSframeEncrypter& value) noexcept;
std::unique_ptr<NativeSframeDecrypter> sframe_decrypter_clone(const NativeSframeDecrypter& value) noexcept;
std::unique_ptr<NativeRtcError> sframe_set_encryption_key(const NativeSframeEncrypter& value, std::uint64_t id, rust::Slice<const std::uint8_t> key) noexcept;
std::unique_ptr<NativeRtcError> sframe_add_decryption_key(const NativeSframeDecrypter& value, std::uint64_t id, rust::Slice<const std::uint8_t> key) noexcept;
std::unique_ptr<NativeRtcError> sframe_remove_decryption_key(const NativeSframeDecrypter& value, std::uint64_t id) noexcept;

class NativeRtpSender final {
 public:
  struct State;
  explicit NativeRtpSender(std::unique_ptr<State> state) noexcept;
  ~NativeRtpSender();
  const std::unique_ptr<State>& state() const noexcept;
  webrtc::scoped_refptr<webrtc::RtpSenderInterface> sender() const noexcept;
  webrtc::scoped_refptr<webrtc::RtpTransceiverInterface> sframe_owner() const noexcept;

 private:
  std::unique_ptr<State> state_;
};


class NativeRtpReceiver;
struct FfiRtpPacketEvent;
class NativeRtpSenderObserver final {
 public:
  struct State;
  explicit NativeRtpSenderObserver(std::unique_ptr<State> state) noexcept;
  ~NativeRtpSenderObserver();
  const std::unique_ptr<State>& state() const noexcept;
 private:
  std::unique_ptr<State> state_;
};
std::unique_ptr<NativeRtpSenderObserver> rtp_sender_observer(const NativeRtpSender& value) noexcept;
std::size_t rtp_sender_observer_identity(const NativeRtpSenderObserver& value) noexcept;
bool rtp_sender_observer_register(const NativeRtpSenderObserver& value, std::size_t capacity) noexcept;
void rtp_sender_observer_unregister(const NativeRtpSenderObserver& value) noexcept;
FfiRtpPacketEvent rtp_sender_observer_take(const NativeRtpSenderObserver& value) noexcept;
std::uint64_t rtp_sender_observer_take_dropped(const NativeRtpSenderObserver& value) noexcept;
class NativeRtpReceiverObserver final {
 public:
  struct State;
  explicit NativeRtpReceiverObserver(std::unique_ptr<State> state) noexcept;
  ~NativeRtpReceiverObserver();
  const std::unique_ptr<State>& state() const noexcept;
 private:
  std::unique_ptr<State> state_;
};
std::unique_ptr<NativeRtpReceiverObserver> rtp_receiver_observer(const NativeRtpReceiver& value) noexcept;
std::size_t rtp_receiver_observer_identity(const NativeRtpReceiverObserver& value) noexcept;
bool rtp_receiver_observer_register(const NativeRtpReceiverObserver& value, std::size_t capacity) noexcept;
void rtp_receiver_observer_unregister(const NativeRtpReceiverObserver& value) noexcept;
FfiRtpPacketEvent rtp_receiver_observer_take(const NativeRtpReceiverObserver& value) noexcept;
std::uint64_t rtp_receiver_observer_take_dropped(const NativeRtpReceiverObserver& value) noexcept;

class NativeDtmfSender final {
 public:
  struct State;
  explicit NativeDtmfSender(std::unique_ptr<State> state) noexcept;
  ~NativeDtmfSender();
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
  webrtc::scoped_refptr<webrtc::RtpTransceiverInterface> sframe_owner() const noexcept;

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
    webrtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver,
    webrtc::scoped_refptr<webrtc::RtpTransceiverInterface> owner = nullptr) noexcept;
std::unique_ptr<NativeRtpSender> rtp_transceiver_sender(
    const NativeRtpTransceiver& transceiver) noexcept;
std::unique_ptr<NativeRtpReceiver> rtp_transceiver_receiver(
    const NativeRtpTransceiver& transceiver) noexcept;
std::uint8_t rtp_transceiver_direction(
    const NativeRtpTransceiver& transceiver) noexcept;
std::int8_t rtp_transceiver_current_direction(
    const NativeRtpTransceiver& transceiver) noexcept;
std::int32_t rtp_transceiver_direction_value(
    const NativeRtpTransceiver& transceiver) noexcept;
bool rtp_transceiver_current_direction_value(
    const NativeRtpTransceiver& transceiver, std::int32_t& direction) noexcept;
std::unique_ptr<NativeRtcError> rtp_transceiver_set_direction_value(
    const NativeRtpTransceiver& transceiver, std::int32_t direction,
    bool& safe) noexcept;
std::unique_ptr<NativeRtcError> rtp_transceiver_stop_standard(
    const NativeRtpTransceiver& transceiver) noexcept;
bool rtp_transceiver_stopped(
    const NativeRtpTransceiver& transceiver) noexcept;
std::int32_t rtp_transceiver_media_type(
    const NativeRtpTransceiver& transceiver) noexcept;
bool rtp_transceiver_stopping(const NativeRtpTransceiver& transceiver) noexcept;
bool rtp_transceiver_receptive(const NativeRtpTransceiver& transceiver) noexcept;
bool rtp_transceiver_fired_direction(
    const NativeRtpTransceiver& transceiver, std::int32_t& direction) noexcept;
bool rtp_transceiver_sframe_enabled(
    const NativeRtpTransceiver& transceiver, bool& enabled) noexcept;
std::unique_ptr<NativeRtpCapabilities> rtp_transceiver_codec_preferences_value(
    const NativeRtpTransceiver& transceiver) noexcept;
std::unique_ptr<NativeRtpCapabilities> rtp_transceiver_header_capabilities_value(
    const NativeRtpTransceiver& transceiver, bool negotiated) noexcept;
std::unique_ptr<NativeRtcError> rtp_transceiver_set_codec_preferences_value(
    const NativeRtpTransceiver& transceiver,
    const NativeRtpCapabilities& values) noexcept;
std::unique_ptr<NativeRtcError> rtp_transceiver_set_header_capabilities_value(
    const NativeRtpTransceiver& transceiver,
    const NativeRtpCapabilities& values) noexcept;
rust::Vec<FfiRtpHeaderExtension> rtp_transceiver_header_extensions(
    const NativeRtpTransceiver& transceiver, bool negotiated) noexcept;
bool rtp_transceiver_set_header_extensions(
    const NativeRtpTransceiver& transceiver,
    rust::Slice<const FfiRtpHeaderExtension> extensions,
    std::uint8_t& error_type, rust::String& error) noexcept;
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
struct FfiDtmfToneChange;
std::size_t rtp_sender_identity(const NativeRtpSender& sender) noexcept;
std::unique_ptr<NativeRtpSender> rtp_sender_copy(const NativeRtpSender& sender) noexcept;
bool rtp_sender_same_resource(const NativeRtpSender& first, const NativeRtpSender& second) noexcept;
bool rtp_receiver_same_resource(const NativeRtpReceiver& first, const NativeRtpReceiver& second) noexcept;
bool rtp_transceiver_same_resource(const NativeRtpTransceiver& first, const NativeRtpTransceiver& second) noexcept;
std::size_t dtmf_sender_identity(const NativeDtmfSender& sender) noexcept;
std::unique_ptr<NativeDtmfSender> rtp_sender_dtmf(const NativeRtpSender& sender, const NativePeerConnection& peer) noexcept;
bool dtmf_sender_can_insert(const NativeDtmfSender& sender) noexcept;
std::uint8_t dtmf_sender_insert(const NativeDtmfSender& sender, rust::Str tones,
    std::int32_t duration, std::int32_t gap, bool has_comma_delay,
    std::int32_t comma_delay) noexcept;
rust::String dtmf_sender_tones(const NativeDtmfSender& sender) noexcept;
std::int32_t dtmf_sender_duration(const NativeDtmfSender& sender) noexcept;
std::int32_t dtmf_sender_inter_tone_gap(const NativeDtmfSender& sender) noexcept;
std::int32_t dtmf_sender_comma_delay(const NativeDtmfSender& sender) noexcept;
bool dtmf_sender_register_observer(const NativeDtmfSender& sender, std::size_t capacity) noexcept;
void dtmf_sender_unregister_observer(const NativeDtmfSender& sender) noexcept;
FfiDtmfToneChange dtmf_sender_take_tone_change(const NativeDtmfSender& sender) noexcept;
std::uint64_t dtmf_sender_take_dropped(const NativeDtmfSender& sender) noexcept;
class NativeDtlsTransport;
std::unique_ptr<NativeDtlsTransport> rtp_sender_dtls(
    const NativeRtpSender& sender, const NativePeerConnection& peer) noexcept;
std::unique_ptr<NativeDtlsTransport> rtp_receiver_dtls(
    const NativeRtpReceiver& receiver, const NativePeerConnection& peer) noexcept;
std::unique_ptr<NativeFrameEncryptor> frame_encryptor_new(rust::Box<RustFrameEncryptorProvider> provider) noexcept;
std::unique_ptr<NativeFrameEncryptor> frame_encryptor_clone(const NativeFrameEncryptor& value) noexcept;
bool frame_encryptor_same(const NativeFrameEncryptor& lhs, const NativeFrameEncryptor& rhs) noexcept;
FfiFrameCryptoFailure frame_encryptor_failure(const NativeFrameEncryptor& value) noexcept;
std::size_t frame_encryptor_max_size(const NativeFrameEncryptor& value, std::int32_t media, std::size_t size) noexcept;
FfiFrameCryptoOutput frame_encryptor_encrypt(const NativeFrameEncryptor& value, std::int32_t media, std::uint32_t ssrc,
    rust::Slice<const std::uint8_t> additional, rust::Slice<const std::uint8_t> input, std::size_t capacity) noexcept;
std::unique_ptr<NativeFrameDecryptor> frame_decryptor_new(const NativeRtpReceiver& receiver, rust::Box<RustFrameDecryptorProvider> provider) noexcept;
std::unique_ptr<NativeFrameDecryptor> frame_decryptor_clone(const NativeFrameDecryptor& value) noexcept;
bool frame_decryptor_same(const NativeFrameDecryptor& lhs, const NativeFrameDecryptor& rhs) noexcept;
FfiFrameCryptoFailure frame_decryptor_failure(const NativeFrameDecryptor& value) noexcept;
std::size_t frame_decryptor_max_size(const NativeFrameDecryptor& value, std::int32_t media, std::size_t size) noexcept;
FfiFrameCryptoOutput frame_decryptor_decrypt(const NativeFrameDecryptor& value, std::int32_t media, rust::Slice<const std::uint32_t> csrcs,
    rust::Slice<const std::uint8_t> additional, rust::Slice<const std::uint8_t> input, std::size_t capacity) noexcept;
void rtp_sender_set_frame_encryptor(const NativeRtpSender& sender, const NativeFrameEncryptor* value) noexcept;
std::unique_ptr<NativeFrameEncryptor> rtp_sender_frame_encryptor(const NativeRtpSender& sender) noexcept;
bool rtp_receiver_set_frame_decryptor(const NativeRtpReceiver& receiver, const NativeFrameDecryptor* value) noexcept;
std::unique_ptr<NativeFrameDecryptor> rtp_receiver_frame_decryptor(const NativeRtpReceiver& receiver) noexcept;
bool media_type_to_string(std::int32_t kind, rust::String& name) noexcept;
std::uint32_t rtp_sender_ssrc(const NativeRtpSender& sender) noexcept;
std::int32_t rtp_sender_media_type(const NativeRtpSender& sender) noexcept;
rust::Vec<rust::String> rtp_sender_stream_ids(const NativeRtpSender& sender) noexcept;
rust::Vec<FfiRtpByteString> rtp_sender_stream_id_bytes(const NativeRtpSender& sender) noexcept;
void rtp_sender_set_streams(const NativeRtpSender& sender,
    rust::Slice<const rust::String> ids) noexcept;
rust::String rtp_sender_id(const NativeRtpSender& sender) noexcept;
std::unique_ptr<NativeRtcError> rtp_sender_generate_keyframe(
    const NativeRtpSender& sender, const NativePeerConnection& peer,
    rust::Slice<const FfiRtpByteString> rids) noexcept;
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
std::int32_t rtp_receiver_media_type(const NativeRtpReceiver& receiver) noexcept;
rust::Vec<rust::String> rtp_receiver_stream_ids(const NativeRtpReceiver& receiver) noexcept;
rust::String rtp_receiver_id(const NativeRtpReceiver& receiver) noexcept;
std::unique_ptr<NativeVideoTrack> rtp_receiver_track(
    const NativeRtpReceiver& receiver) noexcept;

}  // namespace pulsebeam::webrtc_sys
