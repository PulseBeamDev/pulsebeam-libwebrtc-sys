#include <cstdlib>
#include <limits>
#include <memory>
#include <thread>

#include "api/frame_transformer_interface.h"
#include "api/make_ref_counted.h"
#include "api/task_queue/default_task_queue_factory.h"
#include "api/video/encoded_image.h"
#include "audio/channel_send_frame_transformer_delegate.h"
#include "modules/rtp_rtcp/source/frame_object.h"
#include "modules/rtp_rtcp/source/rtp_sender_video_frame_transformer_delegate.h"
#include "modules/rtp_rtcp/source/rtp_video_stream_receiver_frame_transformer_delegate.h"
#include "pulsebeam-webrtc-sys/native/frame_transformer.h"
#include "pulsebeam-webrtc-sys/native/video_metadata.h"
#include "pulsebeam-webrtc-sys/src/lib.rs.h"
#include "system_wrappers/include/clock.h"

namespace {
namespace bridge = pulsebeam::webrtc_sys;
void Require(bool condition) {
  if (!condition)
    std::abort();
}

// Acquire real stock frame classes through their real SDK construction paths.
// This does not manufacture a passkey frame or emulate a peer/codec. Delivery
// is deliberately not tested here: no captured frame goes back to a callback.
class Capture : public webrtc::FrameTransformerInterface {
public:
  std::unique_ptr<webrtc::TransformableFrameInterface> frame;
  void Transform(
      std::unique_ptr<webrtc::TransformableFrameInterface> value) override {
    frame = std::move(value);
  }
};
std::unique_ptr<bridge::NativeTransformableFrame> AcquireAudio() {
  auto capture = webrtc::make_ref_counted<Capture>();
  auto delegate =
      webrtc::make_ref_counted<webrtc::ChannelSendFrameTransformerDelegate>(
          webrtc::ChannelSendFrameTransformerDelegate::SendFrameCallback(),
          capture, nullptr);
  delegate->Init();
  const std::uint8_t payload[] = {1, 2, 3};
  delegate->Transform(webrtc::AudioFrameType::kAudioFrameSpeech, 111, 123,
                      payload, sizeof(payload), 10, 7, "audio/opus", 27,
                      {11, 22});
  delegate->Reset();
  delegate = nullptr;
  Require(capture->frame != nullptr);
  return std::make_unique<bridge::NativeTransformableFrame>(
      std::move(capture->frame), true);
}
std::unique_ptr<bridge::NativeTransformableFrame> AcquireVideo() {
  auto capture = webrtc::make_ref_counted<Capture>();
  auto queues = webrtc::CreateDefaultTaskQueueFactory();
  auto delegate =
      webrtc::make_ref_counted<webrtc::RTPSenderVideoFrameTransformerDelegate>(
          nullptr, capture, 77, "r0", queues.get());
  delegate->Init();
  const std::uint8_t payload[] = {4, 5, 6};
  webrtc::EncodedImage image;
  image.SetEncodedData(
      webrtc::EncodedImageBuffer::Create(payload, sizeof(payload)));
  image.set_frame_type(webrtc::VideoFrameType::kVideoFrameKey);
  webrtc::RTPVideoHeader header{};
  header.codec = webrtc::kVideoCodecVP8;
  header.frame_type = webrtc::VideoFrameType::kVideoFrameKey;
  header.video_type_header.emplace<webrtc::RTPVideoHeaderVP8>()
      .InitRTPVideoHeaderVP8();
  header.generic.emplace().frame_id = 123;
  Require(delegate->TransformFrame(96, webrtc::kVideoCodecVP8, 456, image,
                                   header, webrtc::TimeDelta::Millis(10),
                                   {33}));
  delegate->Reset();
  delegate = nullptr;
  queues = nullptr;
  Require(capture->frame != nullptr);
  return std::make_unique<bridge::NativeTransformableFrame>(
      std::move(capture->frame), false, webrtc::kVideoCodecVP8);
}
std::unique_ptr<bridge::NativeTransformableFrame> AcquireReceiverVideo() {
  auto capture = webrtc::make_ref_counted<Capture>();
  webrtc::SimulatedClock clock(1000);
  auto delegate = webrtc::make_ref_counted<
      webrtc::RtpVideoStreamReceiverFrameTransformerDelegate>(
      nullptr, &clock, capture, nullptr, 88);
  delegate->Init();
  const std::uint8_t payload[] = {7, 8, 9};
  webrtc::RTPVideoHeader header{};
  header.codec = webrtc::kVideoCodecVP8;
  header.frame_type = webrtc::VideoFrameType::kVideoFrameKey;
  header.video_type_header.emplace<webrtc::RTPVideoHeaderVP8>()
      .InitRTPVideoHeaderVP8();
  header.generic.emplace().frame_id = 321;
  auto frame = std::make_unique<webrtc::RtpFrameObject>(
      1, 2, true, 0, webrtc::Timestamp::Micros(1000),
      webrtc::Timestamp::Micros(2000), 789, 0, webrtc::VideoSendTiming(), 96,
      webrtc::kVideoCodecVP8, webrtc::kVideoRotation_0,
      webrtc::VideoContentType::UNSPECIFIED, header, std::nullopt, std::nullopt,
      webrtc::RtpPacketInfos(),
      webrtc::EncodedImageBuffer::Create(payload, sizeof(payload)));
  delegate->TransformFrame(std::move(frame));
  delegate->Reset();
  delegate = nullptr;
  Require(capture->frame != nullptr);
  return std::make_unique<bridge::NativeTransformableFrame>(
      std::move(capture->frame), false, webrtc::kVideoCodecVP8);
}
void AudioRegression(bridge::NativeTransformableFrame &frame) {
  Require(bridge::transformable_frame_data(frame).size() == 3);
  Require(bridge::transformable_frame_payload_type(frame) == 111);
  Require(bridge::transformable_frame_ssrc(frame) == 7);
  Require(bridge::transformable_frame_direction(frame) == 2);
  auto time = bridge::transformable_frame_time(frame, 3);
  Require(time.status == 0 && time.present && time.bits == 10000);
  Require(bridge::transformable_frame_set_capture_time(frame, true, -500) == 0);
  Require(bridge::transformable_frame_time(frame, 3).bits == 0);
  Require(bridge::transformable_frame_set_capture_time(frame, true, -501) == 3);
  Require(bridge::transformable_frame_time(frame, 3).bits == 0);
  Require(bridge::transformable_frame_set_capture_time(frame, true,
                                                       INT64_MAX) == 3);
  Require(bridge::transformable_frame_set_capture_time(frame, true,
                                                       INT64_MAX - 1) == 3);
  constexpr auto boundary = (INT64_MAX / 1000) * 1000;
#if RTC_DCHECK_IS_ON
  Require(bridge::transformable_frame_set_capture_time(frame, true, boundary) ==
          3);
#else
  Require(bridge::transformable_frame_set_capture_time(frame, true, boundary) ==
          0);
  Require(bridge::transformable_frame_time(frame, 3).bits == boundary);
#endif
  Require(bridge::transformable_frame_set_capture_time(frame, true, 1500) == 0);
  Require(bridge::transformable_frame_time(frame, 3).bits == 2000);
  Require(bridge::transformable_audio_set_level(frame, true, 255));
  Require(bridge::transformable_audio_fields(frame).level == 127);
  const auto sources = bridge::transformable_audio_csrcs(frame);
  Require(sources.size() == 2 && sources[0] == 11 && sources[1] == 22);
  Require(bridge::transformable_frame_set_payload_type(frame, 255));
  std::int32_t status = -1;
  auto clone = bridge::transformable_frame_clone(frame, status);
  Require(status == 0 && clone &&
          bridge::transformable_frame_payload_type(*clone) == 255);
  Require(bridge::transformable_frame_direction(*clone) == 2);
  Require(bridge::transformable_frame_set_capture_time(frame, false, 0) == 0);
  Require(!bridge::transformable_frame_time(frame, 3).present);
  Require(bridge::transformable_frame_time(*clone, 3).bits == 2000);
}
void VideoRegression(bridge::NativeTransformableFrame &frame) {
  Require(bridge::transformable_video_is_key(frame));
  Require(bridge::transformable_frame_set_payload_type(frame, 42) == false);
  Require(bridge::transformable_frame_set_capture_time(frame, false, 0) == 1);
  bool present = false;
  auto rid = bridge::transformable_video_rid(frame, present);
  Require(present && rid.size() == 2 && rid[0] == 'r' && rid[1] == '0');
  std::int32_t status = -1;
  auto metadata = bridge::transformable_video_metadata(frame, status);
  Require(status == 0 && metadata);
  auto incompatible = *metadata;
  incompatible.value().SetCodec(webrtc::kVideoCodecH264);
  Require(bridge::transformable_video_set_metadata(frame, incompatible) == 4);
  Require(bridge::transformable_video_header(frame)->value().codec ==
          webrtc::kVideoCodecVP8);
  metadata->value().SetFrameId(INT64_MIN);
  Require(bridge::transformable_video_set_metadata(frame, *metadata) == 0);
  Require(bridge::transformable_video_header(frame)
              ->value()
              .generic->chain_diffs.empty());
  auto clone = bridge::transformable_frame_clone(frame, status);
  Require(status == 0 && clone);
  Require(bridge::transformable_video_metadata(*clone, status)
              ->value()
              .GetFrameId() == INT64_MIN);
  Require(bridge::transformable_frame_direction(*clone) == 2);
  // Sender MIME follows its immutable codec, not caller-mutated header codec.
  metadata->value().SetCodec(static_cast<webrtc::VideoCodecType>(6));
  Require(bridge::transformable_video_set_metadata(frame, *metadata) == 0);
  Require(!bridge::transformable_frame_mime(frame, status).empty() &&
          status == 0);
  auto unnamed = bridge::transformable_frame_clone(frame, status);
  Require(unnamed && status == 0);
  Require(bridge::transformable_frame_mime(*unnamed, status).empty() &&
          status == 2);
}
void ReceiverVideoRegression(bridge::NativeTransformableFrame &frame) {
  Require(bridge::transformable_frame_direction(frame) == 1);
  std::int32_t status = -1;
  auto metadata = bridge::transformable_video_metadata(frame, status);
  Require(metadata && status == 0 && metadata->value().GetFrameId() == 321);
  metadata->value().SetFrameId(INT64_MAX);
  Require(bridge::transformable_video_set_metadata(frame, *metadata) == 0);
  Require(
      bridge::transformable_video_header(frame)->value().generic->frame_id ==
      INT64_MAX);
  Require(bridge::transformable_video_metadata(frame, status)
              ->value()
              .GetFrameId() == 321);
  // The SDK clone intentionally uses cached Metadata, not the changed header,
  // and changes receiver direction into sender direction.
  auto clone = bridge::transformable_frame_clone(frame, status);
  Require(clone && status == 0 &&
          bridge::transformable_frame_direction(*clone) == 2);
  Require(bridge::transformable_video_metadata(*clone, status)
              ->value()
              .GetFrameId() == 321);
  metadata->value().SetWidth(999);
#if RTC_DCHECK_IS_ON
  Require(bridge::transformable_video_set_metadata(frame, *metadata) == 5);
#else
  Require(bridge::transformable_video_set_metadata(frame, *metadata) == 0);
#endif
  Require(bridge::transformable_video_header(frame)->value().width != 999);
}
} // namespace

void TransformableFrameOwnershipRegression() {
  auto audio = AcquireAudio();
  auto video = AcquireVideo();
  auto receiver = AcquireReceiverVideo();
  // All construction delegates and their real video task queue are already
  // destroyed. Reads, exclusive mutation, explicit SDK clones and final drop
  // now occur on another application thread, without endpoint/role retention.
  std::thread owner([audio = std::move(audio), video = std::move(video),
                     receiver = std::move(receiver)]() mutable {
    AudioRegression(*audio);
    VideoRegression(*video);
    ReceiverVideoRegression(*receiver);
    audio.reset();
    video.reset();
    receiver.reset();
  });
  owner.join();
}
