// Real SDK broadcaster dispatch into the production collector. This proves
// callback copying/retention, not end-to-end source-view wants installation.
#include <bit>
#include <cmath>
#include <cstdlib>
#include <memory>
#include <thread>

#include "api/video/i420_buffer.h"
#include "media/base/video_broadcaster.h"
#include "pulsebeam-webrtc-sys/native/video_sink_queue.h"

namespace {
void Require(bool value) {
  if (!value) std::abort();
}
}  // namespace

void VideoSinkQueueRegression() {
  using pulsebeam::webrtc_sys::FrameSink;
  auto collector = std::make_unique<FrameSink>();
  webrtc::VideoBroadcaster broadcaster;
  webrtc::VideoTrackSourceConstraints constraints;
  constraints.max_fps = 0.0;
  broadcaster.ProcessConstraints(constraints);
  constraints.max_fps = 999.0;

  webrtc::VideoSinkWants wants;
  wants.rotation_applied = true;
  // SDK replays its stored constraints only when this sink is first added.
  broadcaster.AddOrUpdateSink(collector.get(), wants);
  auto event = collector->TakeEvent();
  Require(event.valid && event.kind == 1);
  Require(!event.has_min_fps && event.has_max_fps && event.max_fps == 0.0);
  Require(!collector->TakeEvent().valid);

  constexpr std::uint64_t nan_bits = 0x7ff8000000000123ULL;
  constraints.min_fps = -0.0;
  constraints.max_fps = std::bit_cast<double>(nan_bits);
  broadcaster.ProcessConstraints(constraints);
  constraints.min_fps.reset();
  constraints.max_fps.reset();
  event = collector->TakeEvent();
  Require(event.valid && event.kind == 1 && event.has_min_fps && event.has_max_fps);
  Require(event.min_fps == 0.0 && std::signbit(event.min_fps));
  Require(std::bit_cast<std::uint64_t>(event.max_fps) == nan_bits);
  broadcaster.ProcessConstraints(constraints);
  event = collector->TakeEvent();
  Require(event.valid && event.kind == 1 && !event.has_min_fps && !event.has_max_fps);

  auto buffer = webrtc::I420Buffer::Create(4, 4);
  webrtc::I420Buffer::SetBlack(buffer.get());
  auto frame = webrtc::VideoFrame::Builder()
                   .set_video_frame_buffer(buffer)
                   .set_rotation(webrtc::kVideoRotation_90)
                   .set_timestamp_us(12345)
                   .build();
  broadcaster.OnFrame(frame);
  Require(!collector->Take());
  event = collector->TakeEvent();
  Require(event.valid && event.kind == 0);
  Require(collector->DroppedFrames() == 0 && collector->DroppedEvents() == 0);

  wants.rotation_applied = false;
  broadcaster.AddOrUpdateSink(collector.get(), wants);
  Require(!collector->TakeEvent().valid);  // Updates do not replay constraints.
  broadcaster.OnFrame(frame);
  auto returned_frame = collector->Take();
  Require(returned_frame != nullptr);
  // Existing four-frame retention remains independent of control callbacks.
  for (int i = 0; i < 6; ++i) broadcaster.OnFrame(frame);
  Require(collector->DroppedFrames() == 2);
  for (int i = 0; i < 4; ++i) Require(collector->Take() != nullptr);
  Require(!collector->Take());

  // An eight-event FIFO keeps newest notifications and exact optional values.
  for (int i = 0; i < 10; ++i) {
    constraints.min_fps = i + 0.25;
    broadcaster.ProcessConstraints(constraints);
  }
  Require(collector->DroppedEvents() == 2 && collector->DroppedFrames() == 2);
  for (int i = 2; i < 10; ++i) {
    event = collector->TakeEvent();
    Require(event.valid && event.kind == 1 && event.has_min_fps);
    Require(event.min_fps == i + 0.25 && !event.has_max_fps);
  }
  Require(!collector->TakeEvent().valid);
  broadcaster.OnDiscardedFrame();
  broadcaster.OnFrame(frame);
  collector->Deactivate();
  collector->Deactivate();
  Require(!collector->Take());
  broadcaster.OnDiscardedFrame();
  broadcaster.ProcessConstraints(constraints);
  broadcaster.OnFrame(frame);
  event = collector->TakeEvent();
  Require(event.valid && event.kind == 0);  // Accepted before close, not erased.
  Require(!collector->TakeEvent().valid);
  Require(collector->DroppedEvents() == 2 && collector->DroppedFrames() == 2);
  broadcaster.RemoveSink(collector.get());
  collector.reset();
  // A returned frame owns its native buffer beyond collector destruction.
  Require(pulsebeam::webrtc_sys::native_video_frame_width(*returned_frame) == 4);
  Require(pulsebeam::webrtc_sys::native_video_frame_height(*returned_frame) == 4);
  Require(pulsebeam::webrtc_sys::native_video_frame_timestamp_us(*returned_frame) == 12345);
  Require(pulsebeam::webrtc_sys::native_video_frame_rotation(*returned_frame) == 90);

  // The production deactivation/remove order races native notifications without
  // retaining callback storage or releasing the target before removal returns.
  collector = std::make_unique<FrameSink>();
  broadcaster.AddOrUpdateSink(collector.get(), wants);
  std::thread callbacks([&] {
    for (int i = 0; i < 1000; ++i) {
      broadcaster.OnDiscardedFrame();
      broadcaster.OnFrame(frame);
    }
  });
  collector->Deactivate();
  broadcaster.RemoveSink(collector.get());
  collector.reset();
  callbacks.join();
  // No callbacks remain registered after RemoveSink, including control events.
  broadcaster.OnDiscardedFrame();
  broadcaster.ProcessConstraints(constraints);
  broadcaster.OnFrame(frame);
}
