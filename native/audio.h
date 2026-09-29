#pragma once

#include <cstdint>
#include <memory>

#include "rust/cxx.h"

namespace pulsebeam::webrtc_sys {

class NativePeerConnectionFactory;
class NativePeerConnection;
class NativeRtpTransceiver;
class NativeRtpReceiver;
struct FfiReceivedAudioFrame;
struct FfiEncodedAudioFrame;
struct FfiAudioCodecCapability;
struct FfiCodecFormat;

class NativeAudioSource final {
 public:
  struct State;
  explicit NativeAudioSource(std::unique_ptr<State> state) noexcept;
  ~NativeAudioSource();
  const std::unique_ptr<State>& state() const noexcept;

 private:
  std::unique_ptr<State> state_;
};

class NativeAudioSink final {
 public:
  struct State;
  explicit NativeAudioSink(std::unique_ptr<State> state) noexcept;
  ~NativeAudioSink();
  const std::unique_ptr<State>& state() const noexcept;

 private:
  std::unique_ptr<State> state_;
};

class NativeEncodedAudioSink final {
 public:
  struct State;
  explicit NativeEncodedAudioSink(std::unique_ptr<State> state) noexcept;
  ~NativeEncodedAudioSink();
  const std::unique_ptr<State>& state() const noexcept;

 private:
  std::unique_ptr<State> state_;
};

class NativeAudioTrack final {
 public:
  struct State;
  explicit NativeAudioTrack(std::unique_ptr<State> state) noexcept;
  ~NativeAudioTrack();
  const std::unique_ptr<State>& state() const noexcept;

 private:
  std::unique_ptr<State> state_;
};

std::unique_ptr<NativeAudioSource> create_audio_source(
    const NativePeerConnectionFactory& factory) noexcept;
std::unique_ptr<NativeAudioSource> create_encoded_audio_source(
    const NativePeerConnectionFactory& factory, std::uint8_t channels) noexcept;
rust::Vec<FfiAudioCodecCapability> peer_audio_codec_capabilities(
    const NativePeerConnectionFactory& factory, bool sender) noexcept;
bool rtp_transceiver_set_audio_codec_preferences(
    const NativeRtpTransceiver& transceiver,
    const NativePeerConnectionFactory& factory,
    rust::Slice<const FfiCodecFormat> formats, std::uint8_t& error_type,
    rust::String& error) noexcept;
bool close_audio_source(const NativeAudioSource& source) noexcept;
bool audio_source_push_pcm(const NativeAudioSource& source,
                           rust::Slice<const std::int16_t> samples,
                           std::uint32_t sample_rate_hz,
                           std::uint8_t channels,
                           std::int64_t timestamp_us) noexcept;
bool audio_source_push_opus(const NativeAudioSource& source,
                            rust::Slice<const std::uint8_t> payload,
                            std::uint32_t rtp_timestamp,
                            std::uint32_t samples_per_channel) noexcept;
std::unique_ptr<NativeAudioTrack> create_microphone_track(
    const NativePeerConnectionFactory& factory, rust::Str id) noexcept;
std::unique_ptr<NativeAudioTrack> create_audio_track(
    const NativePeerConnectionFactory& factory, const NativeAudioSource& source,
    rust::Str id) noexcept;
rust::String audio_track_id(const NativeAudioTrack& track) noexcept;
bool audio_track_enabled(const NativeAudioTrack& track) noexcept;
bool audio_track_set_enabled(const NativeAudioTrack& track, bool enabled) noexcept;
bool audio_track_set_processing_options(const NativeAudioTrack& track,
                                        std::uint8_t echo, std::uint8_t noise,
                                        std::uint8_t gain,
                                        rust::String& error) noexcept;
std::unique_ptr<NativeAudioSink> rtp_receiver_attach_audio_sink(
    const NativePeerConnection& peer, const NativeRtpReceiver& receiver) noexcept;
FfiReceivedAudioFrame audio_sink_take_frame(const NativeAudioSink& sink) noexcept;
std::uint64_t audio_sink_dropped_frames(const NativeAudioSink& sink) noexcept;
bool close_audio_sink(const NativeAudioSink& sink) noexcept;
std::unique_ptr<NativeEncodedAudioSink> rtp_receiver_attach_encoded_audio_sink(
    const NativePeerConnection& peer, const NativeRtpReceiver& receiver) noexcept;
FfiEncodedAudioFrame encoded_audio_sink_take_frame(
    const NativeEncodedAudioSink& sink) noexcept;
std::uint64_t encoded_audio_sink_dropped_frames(
    const NativeEncodedAudioSink& sink) noexcept;
bool close_encoded_audio_sink(const NativeEncodedAudioSink& sink) noexcept;
std::unique_ptr<NativeRtpTransceiver> peer_add_audio_transceiver(
    const NativePeerConnection& peer, const NativeAudioTrack& track,
    std::uint8_t direction, std::uint8_t& error_type,
    rust::String& error) noexcept;

}  // namespace pulsebeam::webrtc_sys
