// Test-only offline inspection through provided libwebrtc parsers.
// No payload decryption, SRTP authentication, packet rewriting or engine helpers.
#include <algorithm>
#include <charconv>
#include <cstdint>
#include <iostream>
#include <map>
#include <set>
#include <span>
#include <sstream>
#include <string>
#include <vector>

#include "modules/rtp_rtcp/include/rtp_header_extension_map.h"
#include "modules/rtp_rtcp/source/rtp_header_extensions.h"
#include "modules/rtp_rtcp/source/rtp_packet_received.h"
#include "modules/rtp_rtcp/source/rtp_video_layers_allocation_extension.h"

int main() {
  webrtc::RtpHeaderExtensionMap extensions;
  std::map<uint32_t, std::string> streams;
  std::set<uint32_t> published;
  std::set<uint32_t> vla_streams;
  size_t allocations = 0;
  size_t full_allocations = 0;
  size_t maximum_packet = 0;
  bool registered_vla = false;
  std::string line;
  auto fail = [](const char* message) {
    std::cerr << message << '\n';
    return 1;
  };
  while (std::getline(std::cin, line)) {
    std::istringstream input(line);
    std::string kind;
    input >> kind;
    if (kind == "extension") {
      int id;
      std::string uri;
      if (!(input >> id >> uri) || id <= 0 || id > 255)
        return fail("invalid negotiated extension");
      // Other negotiated extensions need not be understood by this probe.
      if (uri == webrtc::RtpVideoLayersAllocationExtension::Uri()) {
        if (!extensions.Register<webrtc::RtpVideoLayersAllocationExtension>(id))
          return fail("VLA registration failed");
        registered_vla = true;
      } else if (uri == webrtc::RtpStreamId::Uri()) {
        if (!extensions.Register<webrtc::RtpStreamId>(id))
          return fail("RID registration failed");
      }
    } else if (kind == "stream") {
      std::string rid;
      uint32_t ssrc;
      if (!(input >> rid >> ssrc) || !streams.emplace(ssrc, rid).second)
        return fail("invalid native stream snapshot");
    } else if (kind == "packet") {
      std::string hex;
      input >> hex;
      if (hex.empty() || hex.size() % 2 != 0)
        return fail("invalid capture hex");
      std::vector<uint8_t> bytes;
      bytes.reserve(hex.size() / 2);
      for (size_t offset = 0; offset < hex.size(); offset += 2) {
        unsigned value;
        auto result = std::from_chars(hex.data() + offset,
                                      hex.data() + offset + 2, value, 16);
        if (result.ec != std::errc{} || result.ptr != hex.data() + offset + 2)
          return fail("invalid capture byte");
        bytes.push_back(static_cast<uint8_t>(value));
      }
      webrtc::RtpPacketReceived packet(&extensions);
      if (!packet.Parse(std::span<const uint8_t>(bytes)))
        continue;  // Actual UDP capture also contains STUN and DTLS.
      auto stream = streams.find(packet.Ssrc());
      if (stream == streams.end())
        continue;  // RTCP, RTX and the opposite endpoint are not primary media.
      published.insert(packet.Ssrc());
      maximum_packet = std::max(maximum_packet, bytes.size());
      // Includes SRTP payload and authentication trailer, not just RTP headers.
      // These fixtures use IPv4 UDP; leave 28 bytes for IP/UDP within 1500.
      if (bytes.size() > 1472)
        return fail("captured primary media exceeded packet-size bound");
      std::string rid;
      if (packet.GetExtension<webrtc::RtpStreamId>(&rid) && rid != stream->second)
        return fail("wire RID disagrees with native SSRC snapshot");
      webrtc::VideoLayersAllocation allocation;
      if (!packet.GetExtension<webrtc::RtpVideoLayersAllocationExtension>(&allocation))
        continue;
      ++allocations;
      vla_streams.insert(packet.Ssrc());
      int expected_index = stream->second == "q" ? 0 : stream->second == "h" ? 1 : 2;
      if (allocation.rtp_stream_index != expected_index)
        return fail("wire VLA stream index disagrees with native RID order");
      std::set<int> allocated;
      for (const auto& layer : allocation.active_spatial_layers) {
        if (layer.spatial_id != 0 || layer.rtp_stream_index < 0 ||
            layer.rtp_stream_index > 2 ||
            layer.target_bitrate_per_temporal_layer.size() != 1 ||
            layer.target_bitrate_per_temporal_layer[0].bps() <= 0)
          return fail("unexpected native simulcast allocation");
        allocated.insert(layer.rtp_stream_index);
      }
      if (allocated == std::set<int>{0, 1, 2})
        ++full_allocations;
    } else {
      return fail("unknown capture record");
    }
  }
  if (!registered_vla || streams.size() != 3 || published.size() != 3 ||
      allocations == 0 || full_allocations == 0)
    return fail("native three-rung wire allocation evidence missing");
  std::cout << "native VLA allocations=" << allocations
            << " full=" << full_allocations << " VLA SSRCs=" << vla_streams.size()
            << " primary SSRCs=" << published.size()
            << " maximum full packet=" << maximum_packet << '\n';
  return 0;
}
