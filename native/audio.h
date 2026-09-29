#pragma once

#include <cstdint>
#include <memory>

#include "rust/cxx.h"

namespace pulsebeam::webrtc_sys {

class NativePeerConnectionFactory;
class NativePeerConnection;
class NativeRtpTransceiver;

class NativeAudioSource final {
 public:
  struct State;
  explicit NativeAudioSource(std::unique_ptr<State> state) noexcept;
  ~NativeAudioSource();
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
bool close_audio_source(const NativeAudioSource& source) noexcept;
bool audio_source_push_pcm(const NativeAudioSource& source,
                           rust::Slice<const std::int16_t> samples,
                           std::uint32_t sample_rate_hz,
                           std::uint8_t channels,
                           std::int64_t timestamp_us) noexcept;
std::unique_ptr<NativeAudioTrack> create_audio_track(
    const NativePeerConnectionFactory& factory, const NativeAudioSource& source,
    rust::Str id) noexcept;
rust::String audio_track_id(const NativeAudioTrack& track) noexcept;
bool audio_track_enabled(const NativeAudioTrack& track) noexcept;
bool audio_track_set_enabled(const NativeAudioTrack& track, bool enabled) noexcept;
std::unique_ptr<NativeRtpTransceiver> peer_add_audio_transceiver(
    const NativePeerConnection& peer, const NativeAudioTrack& track,
    std::uint8_t direction, std::uint8_t& error_type,
    rust::String& error) noexcept;

}  // namespace pulsebeam::webrtc_sys
