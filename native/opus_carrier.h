#pragma once

#include <array>
#include <cstdint>
#include <cstring>
#include <memory>
#include <mutex>
#include <optional>
#include <span>
#include <unordered_map>

namespace pulsebeam::webrtc_sys::opus_carrier {
// An adapter-private transport across 48 kHz mono/stereo AudioSource sinks.
// It never appears on the wire. Each 10 ms callback carries 960 bytes per channel.
constexpr std::size_t kBytesPerChannelBlock = 960;
constexpr std::size_t kHeaderBytes = 28;
constexpr std::size_t kMaxPayloadBytes = 1200;
constexpr std::size_t kMaxPendingSlots = 24;
constexpr std::array<std::uint8_t, 8> kMagic = {'P','B','O','P','U','S','0','1'};

inline void Store16(std::uint8_t* out, std::uint16_t n) {
  out[0] = n & 255;
  out[1] = n >> 8;
}
inline void Store32(std::uint8_t* out, std::uint32_t n) {
  for (int i = 0; i < 4; ++i) out[i] = (n >> (i * 8)) & 255;
}
inline std::uint16_t Load16(const std::uint8_t* in) {
  return std::uint16_t(in[0]) | (std::uint16_t(in[1]) << 8);
}
inline std::uint32_t Load32(const std::uint8_t* in) {
  return std::uint32_t(in[0]) | (std::uint32_t(in[1]) << 8) |
         (std::uint32_t(in[2]) << 16) | (std::uint32_t(in[3]) << 24);
}
inline std::uint32_t Checksum(std::span<const std::uint8_t> data) {
  std::uint32_t hash = 2166136261u;
  for (auto byte : data) hash = (hash ^ byte) * 16777619u;
  return hash;
}
// The source reserves before dispatch; the encoder acknowledges once per
// packet. Closing the source cancels any slots WebRTC did not consume.
struct Budget {
  std::mutex mutex;
  std::uint32_t next_packet = 0;
  std::size_t slots = 0;
  std::unordered_map<std::uint32_t, std::size_t> pending;
};
struct Registry {
  std::mutex mutex;
  std::uint32_t next_source = 0;
  std::unordered_map<std::uint32_t, std::weak_ptr<Budget>> sources;
};
inline Registry& Budgets() {
  static Registry registry;
  return registry;
}
inline std::uint32_t Register(const std::shared_ptr<Budget>& budget) {
  auto& registry = Budgets();
  std::lock_guard lock(registry.mutex);
  const auto id = ++registry.next_source;
  registry.sources[id] = budget;
  return id;
}
inline void Unregister(std::uint32_t source) {
  auto& registry = Budgets();
  std::lock_guard lock(registry.mutex);
  registry.sources.erase(source);
}
inline std::optional<std::uint32_t> Reserve(Budget& budget, std::size_t slots) {
  std::lock_guard lock(budget.mutex);
  if (slots > kMaxPendingSlots - budget.slots) return std::nullopt;
  const auto id = ++budget.next_packet;
  budget.pending[id] = slots;
  budget.slots += slots;
  return id;
}
inline void Acknowledge(std::uint32_t source, std::uint32_t packet) {
  auto& registry = Budgets();
  std::shared_ptr<Budget> budget;
  {
    std::lock_guard lock(registry.mutex);
    if (auto it = registry.sources.find(source); it != registry.sources.end())
      budget = it->second.lock();
  }
  if (!budget) return;
  std::lock_guard lock(budget->mutex);
  if (auto it = budget->pending.find(packet); it != budget->pending.end()) {
    budget->slots -= it->second;
    budget->pending.erase(it);
  }
}
inline bool HasMagic(std::span<const std::uint8_t> bytes) {
  return bytes.size() >= kMagic.size() &&
         std::memcmp(bytes.data(), kMagic.data(), kMagic.size()) == 0;
}
} // namespace pulsebeam::webrtc_sys::opus_carrier
