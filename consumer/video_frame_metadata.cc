#include "api/video/video_frame_metadata.h"
#include "modules/rtp_rtcp/source/active_decode_targets_helper.h"
#include "modules/rtp_rtcp/source/rtp_video_header.h"

#include <array>
#include <limits>

static bool MetadataFrameIdResetRegression() {
  const std::array<int, 1> protected_by_chain{0};
  const std::array<int, 1> chain_diffs{1};
  for (const auto frame_id : {std::numeric_limits<int64_t>::min(),
                             std::numeric_limits<int64_t>::max()}) {
    const int64_t prior_id = frame_id < 0 ? 17 : -17;
    webrtc::ActiveDecodeTargetsHelper tracker;
    tracker.OnFrame(protected_by_chain, std::bitset<32>(1), true,
                    prior_id, chain_diffs);
    const auto prior_chains = tracker.ActiveChainsBitmask();
    webrtc::RTPVideoHeader header;
    header.generic.emplace();
    header.generic->chain_diffs.assign(chain_diffs.begin(), chain_diffs.end());
    auto metadata = header.GetAsMetadata();
    metadata.SetFrameType(webrtc::VideoFrameType::kVideoFrameDelta);
    metadata.SetFrameId(frame_id);
    header.SetFromMetadata(metadata);
    if (!header.generic || header.generic->frame_id != frame_id ||
        !header.generic->chain_diffs.empty() ||
        !header.generic->active_decode_targets.all())
      return false;
    // Caller mutation resets chains. This must return before subtracting the
    // private prior ID: both chosen pairs would overflow if that path ran.
    tracker.OnFrame(protected_by_chain, header.generic->active_decode_targets,
                    false, frame_id, header.generic->chain_diffs);
    if (tracker.ActiveChainsBitmask() != prior_chains)
      return false;
    // The empty-chain branch also leaves the prior frame ID unchanged.
    tracker.OnFrame(protected_by_chain, std::bitset<32>(1), false,
                    prior_id + 1, chain_diffs);
  }
  return true;
}

// Match both stock producer construction sites exactly: variant emplace with no
// arguments value-initializes this aggregate before its partial Init method.
// This gate must run against the shipped kit, not default-initialized raw
// structs.
bool VideoFrameMetadataInitializationRegression() {
  webrtc::RTPVideoHeaderCodecSpecifics specifics;
  auto &vp9 = specifics.emplace<webrtc::RTPVideoHeaderVP9>();
  vp9.InitRTPVideoHeaderVP9();
  if (vp9.spatial_layer_resolution_present || vp9.gof.num_frames_in_gof != 0 ||
      vp9.gof.pid_start != 0)
    return false;
  for (std::size_t i = 0; i < webrtc::kMaxVp9RefPics; ++i)
    if (vp9.pid_diff[i] != 0 || vp9.ref_picture_id[i] != 0)
      return false;
  for (std::size_t i = 0; i < webrtc::kMaxVp9NumberOfSpatialLayers; ++i)
    if (vp9.width[i] != 0 || vp9.height[i] != 0)
      return false;
  for (std::size_t i = 0; i < webrtc::kMaxVp9FramesInGof; ++i) {
    if (vp9.gof.temporal_idx[i] != 0 || vp9.gof.temporal_up_switch[i] ||
        vp9.gof.num_ref_pics[i] != 0)
      return false;
    for (std::size_t p = 0; p < webrtc::kMaxVp9RefPics; ++p)
      if (vp9.gof.pid_diff[i][p] != 0)
        return false;
  }
  vp9.ss_data_available = true;
  vp9.gof.SetGofInfoVP9(webrtc::kTemporalStructureMode3);
  vp9.gof.pid_start = 65535;
  vp9.width[7] = 60123;
  vp9.pid_diff[2] = 217;
  vp9.InitRTPVideoHeaderVP9();
  if (vp9.width[7] != 60123 || vp9.pid_diff[2] != 217 ||
      vp9.gof.pid_start != 65535 || vp9.gof.num_frames_in_gof != 4)
    return false;
  vp9.ss_data_available = true;
  webrtc::VideoFrameMetadata metadata;
  metadata.SetCodec(webrtc::kVideoCodecVP9);
  metadata.SetRTPVideoHeaderCodecSpecifics(specifics);
  auto copy = metadata;
  if (metadata != copy)
    return false;
  auto copied_specifics = copy.GetRTPVideoHeaderCodecSpecifics();
  auto &copied_vp9 = std::get<webrtc::RTPVideoHeaderVP9>(copied_specifics);
  if (copied_vp9.width[7] != 60123 || copied_vp9.pid_diff[2] != 217 ||
      copied_vp9.gof.pid_start != 65535 || copied_vp9.gof.temporal_idx[3] != 2)
    return false;
  webrtc::GofInfoVP9 target{};
  target.pid_start = 123;
  target.pid_diff[0][2] = 177;
  target.temporal_idx[254] = 211;
  target.CopyGofInfoVP9(copied_vp9.gof);
  return target.pid_start == 123 && target.pid_diff[0][2] == 177 &&
         target.temporal_idx[254] == 211 && target.num_frames_in_gof == 4 &&
         MetadataFrameIdResetRegression();
}
