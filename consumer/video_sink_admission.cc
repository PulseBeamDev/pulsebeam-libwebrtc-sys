// Production-owned source, real SDK track/source delegation and broadcaster.
// This is admission/retirement evidence, not a claim about encoder geometry.
#include <atomic>
#include <cstdlib>
#include <limits>
#include <numeric>
#include <thread>

#include "api/make_ref_counted.h"
#include "api/video/i420_buffer.h"
#include "pc/video_track.h"
#include "pulsebeam-webrtc-sys/native/raw_video_source.h"
#include "rtc_base/thread.h"

namespace {
void Require(bool value) {
  if (!value) std::abort();
}
class Sink final : public webrtc::VideoSinkInterface<webrtc::VideoFrame> {
 public:
  void OnFrame(const webrtc::VideoFrame& frame) override {
    const auto* buffer = frame.video_frame_buffer()->GetI420();
    Require(buffer != nullptr);
    y.store(buffer->DataY()[0]);
    ++frames;
  }
  void OnDiscardedFrame() override { ++discarded; }
  std::atomic<int> frames{0};
  std::atomic<int> discarded{0};
  std::atomic<int> y{0};
};
webrtc::VideoSinkWants Wants(int alignment) {
  webrtc::VideoSinkWants wants;
  wants.resolution_alignment = alignment;
  return wants;
}
}  // namespace

void VideoSinkAdmissionRegression() {
  using pulsebeam::webrtc_sys::PushVideoSource;
  using pulsebeam::webrtc_sys::SinkAlignmentBound;
  // The guard is exact for non-power-of-two and signed SDK alignments too.
  SinkAlignmentBound bound;
  Require(bound.Include(0) && bound.Include(-3) && bound.Include(5));
  Require(bound.Include(7));
  Require(!bound.Include(std::numeric_limits<int>::min()));

  webrtc::AutoThread caller;
  auto worker = webrtc::Thread::Create();
  Require(worker->Start());
  auto source = webrtc::make_ref_counted<PushVideoSource>();
  auto track = webrtc::VideoTrack::Create("admission", source, worker.get());
  Sink first, second, zero;
  auto& broadcaster = source->broadcaster();

  worker->BlockingCall([&] {
    auto a = Wants(46337);
    a.is_active = false;
    a.requested_resolution.emplace(4, 4);
    track->AddOrUpdateSink(&first, a);
    Require(broadcaster.WantsForSink(&first)->resolution_alignment == 46337);
    // The SDK currently filters both requests out, but later activation or
    // removal can expose the overflowing nonzero LCM. Reject before track AND
    // source mutation for a public app operation scoped on the worker.
    auto b = Wants(46349);
    b.requested_resolution.emplace(4, 4);
    bool invoked = false;
    Require(!broadcaster.AdmitAndApply(&second, b, [&] {
      invoked = true;
      track->AddOrUpdateSink(&second, b);
    }));
    Require(!invoked && !broadcaster.WantsForSink(&second));
    Require(broadcaster.RejectedRequests() == 1);

    // An opaque rejected NEW SDK track registration changes its track list,
    // but creates no source pair. Later native track removal must be safe.
    track->AddOrUpdateSink(&second, b);
    Require(broadcaster.RejectedRequests() == 2);
    Require(!broadcaster.WantsForSink(&second));
    track->RemoveSink(&second);

    track->AddOrUpdateSink(&second, Wants(1));
    // App preflight also preserves an existing pair on refusal.
    Require(!broadcaster.AdmitAndApply(&second, b, [&] {
      std::abort();
    }));
    Require(broadcaster.RejectedRequests() == 3);
    Require(broadcaster.WantsForSink(&second)->resolution_alignment == 1);
    // For an opaque UPDATE, native track bookkeeping has already changed:
    // retire the old source pair rather than retain stale black/geometry wants.
    track->AddOrUpdateSink(&second, b);
    Require(broadcaster.RejectedRequests() == 4);
    Require(!broadcaster.WantsForSink(&second));
    Require(first.discarded == 0 && second.discarded == 0);
    // A later valid SDK update can register again, without a shadow tombstone.
    track->AddOrUpdateSink(&second, Wants(1));
    Require(broadcaster.WantsForSink(&second)->resolution_alignment == 1);
    track->RemoveSink(&first);
    track->RemoveSink(&second);
    Require(!broadcaster.WantsForSink(&first));
    Require(!broadcaster.WantsForSink(&second));

    source->AddOrUpdateSink(&first, Wants(std::numeric_limits<int>::max()));
    source->AddOrUpdateSink(&zero, Wants(0));
    Require(broadcaster.wants().resolution_alignment == 0);
    source->AddOrUpdateSink(&second, Wants(2));
    Require(broadcaster.RejectedRequests() == 5);
    Require(!broadcaster.WantsForSink(&second));
    source->RemoveSink(&zero);
    Require(broadcaster.wants().resolution_alignment == std::numeric_limits<int>::max());
    source->RemoveSink(&first);
    source->AddOrUpdateSink(&second, Wants(std::numeric_limits<int>::min()));
    Require(broadcaster.RejectedRequests() == 6);
    Require(!broadcaster.WantsForSink(&second));
    source->RemoveSink(&second);  // refused NEW requests may be removed normally

    // Preserve every untouched SDK field, including optional aggregates and
    // a non-power-of-two alignment. Query the REAL pair, not an input cache.
    auto wants = Wants(-3);
    wants.rotation_applied = true;
    wants.black_frames = true;
    wants.max_pixel_count = 123;
    wants.target_pixel_count = 22;
    wants.max_framerate_fps = 17;
    wants.resolutions.emplace_back(4, 3);
    wants.requested_resolution.emplace(5, 7);
    wants.is_active = true;
    wants.aggregates.emplace();
    wants.aggregates->any_active_without_requested_resolution = true;
    source->AddOrUpdateSink(&first, wants);
    const auto actual = broadcaster.WantsForSink(&first);
    Require(actual && actual->resolution_alignment == -3);
    Require(actual->rotation_applied && actual->black_frames);
    Require(actual->max_pixel_count == 123 && actual->target_pixel_count == 22);
    Require(actual->max_framerate_fps == 17 && actual->is_active);
    Require(actual->resolutions.size() == 1);
    Require(actual->resolutions[0].width == 4 && actual->resolutions[0].height == 3);
    Require(actual->requested_resolution->width == 5 && actual->requested_resolution->height == 7);
    Require(actual->aggregates->any_active_without_requested_resolution);
    source->RemoveSink(&first);

    // The same-worker recursive scope permits real track->source delegation.
    // The SDK overwrites caller black_frames with the actual track enabled bit.
    Require(broadcaster.AdmitAndApply(&first, wants, [&] {
      Require(worker->IsCurrent());
      track->AddOrUpdateSink(&first, wants);
    }));
    Require(!broadcaster.WantsForSink(&first)->black_frames);
  });
  // Do not hold the source guard on signaling: set_enabled synchronously
  // delegates to worker and replays the real track's wants to this source.
  Require(track->set_enabled(false));
  Require(broadcaster.WantsForSink(&first)->black_frames);
  Require(broadcaster.RejectedRequests() == 6);

  auto pixels = webrtc::I420Buffer::Create(4, 4);
  pixels->InitializeData();
  pixels->MutableDataY()[0] = 99;
  auto frame = webrtc::VideoFrame::Builder().set_video_frame_buffer(pixels).build();
  source->Push(frame);
  Require(first.frames == 1 && first.y != 99);  // SDK black-frame behavior
  Require(track->set_enabled(true));
  Require(!broadcaster.WantsForSink(&first)->black_frames);
  source->Push(frame);
  Require(first.frames == 2 && first.y == 99);
  worker->BlockingCall([&] { track->RemoveSink(&first); });
  track = nullptr;  // SDK track destructor belongs on signaling

  // Every subset of an admitted group can retire without overflowing native
  // UpdateWants. Include zero so no proof relies on zero masking other pairs.
  Sink group[4];
  const int alignments[] = {0, -3, 5, 7};
  worker->BlockingCall([&] {
    for (unsigned remaining = 0; remaining < 16; ++remaining) {
      for (int i = 0; i < 4; ++i) source->AddOrUpdateSink(&group[i], Wants(alignments[i]));
      for (int i = 0; i < 4; ++i) {
        if (!(remaining & (1u << i))) source->RemoveSink(&group[i]);
      }
      int expected = 1;
      for (int i = 0; i < 4; ++i) {
        if (remaining & (1u << i)) expected = std::lcm(expected, alignments[i]);
      }
      Require(broadcaster.wants().resolution_alignment == expected);
      for (auto& sink : group) source->RemoveSink(&sink);
    }
  });

  // The SDK source API above requires its worker. The BROADCASTER API permits
  // worker/encoder callers; stress that production member directly, without
  // pretending source registration is legal on arbitrary callback threads.
  // Real broadcaster frame dispatch races actual guarded Add/Remove mutations.
  std::thread updates([&] {
    for (int i = 0; i < 500; ++i) {
      broadcaster.AddOrUpdateSink(&first, Wants(3));
      broadcaster.RemoveSink(&first);
    }
  });
  std::thread other_updates([&] {
    for (int i = 0; i < 500; ++i) {
      broadcaster.AddOrUpdateSink(&second, Wants(5));
      broadcaster.RemoveSink(&second);
    }
  });
  std::thread callbacks([&] {
    for (int i = 0; i < 500; ++i) source->Push(frame);
  });
  updates.join();
  other_updates.join();
  callbacks.join();
  broadcaster.RemoveSink(&first);
  broadcaster.RemoveSink(&second);
  const auto before = first.frames.load() + second.frames.load();
  source->Push(frame);
  Require(before == first.frames.load() + second.frames.load());
  Require(broadcaster.RejectedRequests() == 6);
  source = nullptr;
  worker->Stop();
}
