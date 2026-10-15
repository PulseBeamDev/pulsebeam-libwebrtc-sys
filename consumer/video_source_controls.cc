// Thin-dispatch evidence: real SDK source/proxy virtual dispatch, exact values
// and existing-worker affinity. The RTP case uses the stock source's callback,
// not a substitute peer or evidence of end-to-end RTCP/keyframe delivery.
#include <bit>
#include <cstdint>
#include <cstdlib>

#include "api/make_ref_counted.h"
#include "media/base/video_broadcaster.h"
#include "pc/video_rtp_track_source.h"
#include "pc/video_track_source_proxy.h"
#include "pulsebeam-webrtc-sys/native/video_source_controls.h"

namespace {
void Require(bool value) {
  if (!value) std::abort();
}

class ObservedSource : public webrtc::VideoTrackSource {
 public:
  explicit ObservedSource(webrtc::Thread& worker)
      : VideoTrackSource(false), worker_(worker) {}
  void RequestRefreshFrame() override {
    Require(worker_.IsCurrent());
    ++refreshes;
  }
  void GenerateKeyFrame() override {
    Require(worker_.IsCurrent());
    ++keyframes;
  }
  void ProcessConstraints(
      const webrtc::VideoTrackSourceConstraints& value) override {
    Require(worker_.IsCurrent());
    constraints = value;
    ++changes;
  }
  unsigned refreshes = 0;
  unsigned keyframes = 0;
  unsigned changes = 0;
  webrtc::VideoTrackSourceConstraints constraints;

 protected:
  webrtc::VideoSourceInterface<webrtc::VideoFrame>* source() override {
    return &broadcaster_;
  }

 private:
  webrtc::Thread& worker_;
  webrtc::VideoBroadcaster broadcaster_;
};

class RemoteCallback final : public webrtc::VideoRtpTrackSource::Callback {
 public:
  explicit RemoteCallback(webrtc::Thread& worker) : worker_(worker) {}
  void OnGenerateKeyFrame() override {
    Require(worker_.IsCurrent());
    ++keyframes;
  }
  void OnEncodedSinkEnabled(bool) override { Require(worker_.IsCurrent()); }
  unsigned keyframes = 0;

 private:
  webrtc::Thread& worker_;
};
}  // namespace

void VideoSourceControlsRegression() {
  using namespace pulsebeam::webrtc_sys;
  webrtc::AutoThread signaling;
  auto worker = webrtc::Thread::Create();
  Require(worker->Start());
  {
    auto source = webrtc::make_ref_counted<ObservedSource>(*worker);
    auto proxy = webrtc::VideoTrackSourceProxy::Create(
        webrtc::Thread::Current(), worker.get(), source);
    for (auto* target : {static_cast<webrtc::VideoTrackSourceInterface*>(source.get()),
                         static_cast<webrtc::VideoTrackSourceInterface*>(proxy.get())}) {
      RequestVideoSourceRefresh(*target, *worker);
      GenerateVideoSourceKeyFrame(*target, *worker);
      webrtc::VideoTrackSourceConstraints value;
      value.min_fps = -0.0;
      constexpr std::uint64_t nan_bits = 0x7ff8000000000123ULL;
      value.max_fps = std::bit_cast<double>(nan_bits);
      ProcessVideoSourceConstraints(*target, *worker, value);
      value.min_fps.reset();
      value.max_fps.reset();
      Require(source->constraints.min_fps.has_value());
      Require(std::bit_cast<std::uint64_t>(*source->constraints.min_fps) ==
              0x8000000000000000ULL);
      Require(source->constraints.max_fps.has_value());
      Require(std::bit_cast<std::uint64_t>(*source->constraints.max_fps) == nan_bits);
      ProcessVideoSourceConstraints(*target, *worker, value);
      Require(!source->constraints.min_fps && !source->constraints.max_fps);
      value.max_fps = 0.0;
      ProcessVideoSourceConstraints(*target, *worker, value);
      Require(!source->constraints.min_fps && source->constraints.max_fps == 0.0);
    }
    Require(source->refreshes == 2 && source->keyframes == 2 && source->changes == 6);

    RemoteCallback callback(*worker);
    auto remote = webrtc::make_ref_counted<webrtc::VideoRtpTrackSource>(&callback);
    auto remote_proxy = webrtc::VideoTrackSourceProxy::Create(
        webrtc::Thread::Current(), worker.get(), remote);
    GenerateVideoSourceKeyFrame(*remote, *worker);
    GenerateVideoSourceKeyFrame(*remote_proxy, *worker);
    Require(callback.keyframes == 2);
    worker->BlockingCall([&] { remote->ClearCallback(); });
    GenerateVideoSourceKeyFrame(*remote_proxy, *worker);
    Require(callback.keyframes == 2);  // SDK cleared callback: no fabricated work.
  }
  worker->Stop();

  // Already-current owner roles execute inline, without a separate pump. This
  // is dispatch evidence, not full controlled-world lifecycle qualification.
  auto caller = webrtc::make_ref_counted<ObservedSource>(*webrtc::Thread::Current());
  RequestVideoSourceRefresh(*caller, *webrtc::Thread::Current());
  GenerateVideoSourceKeyFrame(*caller, *webrtc::Thread::Current());
  ProcessVideoSourceConstraints(*caller, *webrtc::Thread::Current(), {});
  Require(caller->refreshes == 1 && caller->keyframes == 1 && caller->changes == 1);
}
