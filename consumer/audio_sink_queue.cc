// Direct SDK virtual dispatch into the production collector. This is a narrow
// callback-copy regression, not proof that stock calls use the legacy overload.
#include <algorithm>
#include <cstdlib>
#include <limits>
#include <memory>
#include <optional>
#include <vector>

#include "pulsebeam-webrtc-sys/native/audio_sink_queue.h"

namespace {
void Require(bool value) {
  if (!value) std::abort();
}
}  // namespace

void AudioSinkQueueRegression() {
  using pulsebeam::webrtc_sys::ReceivedAudioCollector;
  auto collector = std::make_unique<ReceivedAudioCollector>();
  webrtc::AudioTrackSinkInterface& sdk = *collector;
  Require(sdk.NumPreferredChannels() == -1);  // Unknown, not input geometry.
  const std::vector<std::int16_t> original{17, -29, 43, -61, 79, -101};
  {
    auto borrowed = original;
    sdk.OnData(borrowed.data(), 16, 44100, 2, 3);
    std::fill(borrowed.begin(), borrowed.end(), 12345);
  }
  auto legacy = collector->Take();
  Require(legacy.valid && !legacy.has_capture_time);
  Require(legacy.sample_rate_hz == 44100 && legacy.channels == 2 &&
          legacy.samples_per_channel == 3);
  Require(std::equal(legacy.samples.begin(), legacy.samples.end(),
                     original.begin(), original.end()));
  Require(!collector->Take().valid);

  const std::optional<std::int64_t> capture_times[] = {
      std::nullopt, 0, std::numeric_limits<std::int64_t>::min(),
      std::numeric_limits<std::int64_t>::max()};
  for (auto capture : capture_times) {
    auto borrowed = original;
    sdk.OnData(borrowed.data(), 16, 48000, 1, borrowed.size(), capture);
    std::fill(borrowed.begin(), borrowed.end(), -12345);
    auto frame = collector->Take();
    Require(frame.valid && frame.has_capture_time == capture.has_value());
    if (capture) Require(frame.capture_time_ms == *capture);
    Require(frame.sample_rate_hz == 48000 && frame.channels == 1 &&
            frame.samples_per_channel == original.size());
    Require(std::equal(frame.samples.begin(), frame.samples.end(),
                       original.begin(), original.end()));
  }
  Require(collector->DroppedFrames() == 0);

  // Reject malformed callback metadata before dereferencing this deliberately
  // short buffer. Exercise both overloads without manufacturing valid storage.
  std::int16_t one = 9;
  sdk.OnData(nullptr, 16, 48000, 1, 1);
  sdk.OnData(&one, 8, 48000, 1, 1, 0);
  sdk.OnData(&one, 16, 0, 1, 1);
  sdk.OnData(&one, 16, 48000, 0, 1, std::nullopt);
  sdk.OnData(&one, 16, 48000, 17, 1);
  sdk.OnData(&one, 16, 48000, 1, 0, 0);
  sdk.OnData(&one, 16, 48000, 1,
             webrtc::AudioFrame::kMaxDataSizeSamples + 1);
  Require(!collector->Take().valid && collector->DroppedFrames() == 7);

  // Both overloads feed the same eight-frame queue. Newest frames retain their
  // own timing presence and sample data rather than the callback's storage.
  for (std::int16_t i = 0; i < 10; ++i) {
    one = i;
    if (i % 2) sdk.OnData(&one, 16, 8000, 1, 1, 0);
    else sdk.OnData(&one, 16, 8000, 1, 1);
  }
  one = -1;
  Require(collector->DroppedFrames() == 9);
  for (std::int16_t i = 2; i < 10; ++i) {
    auto frame = collector->Take();
    Require(frame.valid && frame.samples.size() == 1 && frame.samples[0] == i);
    Require(frame.has_capture_time == (i % 2 != 0));
    if (frame.has_capture_time) Require(frame.capture_time_ms == 0);
  }
  Require(!collector->Take().valid);
  sdk.OnData(&one, 16, 8000, 1, 1);
  collector->Deactivate();
  collector->Deactivate();
  Require(!collector->Take().valid);
  sdk.OnData(&one, 16, 8000, 1, 1);
  sdk.OnData(&one, 16, 8000, 1, 1, 0);
  Require(!collector->Take().valid && collector->DroppedFrames() == 9);
  collector.reset();
  // A returned frame also survives final destruction of the callback target.
  Require(std::equal(legacy.samples.begin(), legacy.samples.end(),
                     original.begin(), original.end()));
}
