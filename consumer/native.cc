#include <cstdint>
#include <iostream>
#include <memory>

#include "api/audio/create_audio_device_module.h"
#include "api/environment/environment_factory.h"
#include "modules/desktop_capture/desktop_capture_options.h"
#include "modules/desktop_capture/desktop_capturer.h"
#include "modules/video_capture/video_capture_factory.h"

namespace {

struct NativeFactories {
  webrtc::scoped_refptr<webrtc::AudioDeviceModule> audio;
  std::unique_ptr<webrtc::VideoCaptureModule::DeviceInfo> camera;
  std::unique_ptr<webrtc::DesktopCapturer> screen;
  std::unique_ptr<webrtc::DesktopCapturer> window;
};

} // namespace

// The probe deliberately exposes only an opaque handle across this boundary.
// STL values and WebRTC allocations remain in C++, and the matching destroy
// function releases them through the runtime that created them.
extern "C" void *PulsebeamCreateNativeFactories() {
  const webrtc::Environment environment = webrtc::CreateEnvironment();
  auto factories = std::make_unique<NativeFactories>();
  factories->audio = webrtc::CreateAudioDeviceModule(
      environment, webrtc::AudioDeviceModule::kPlatformDefaultAudio);
  factories->camera.reset(webrtc::VideoCaptureFactory::CreateDeviceInfo());
  const auto options =
      webrtc::DesktopCaptureOptions::CreateDefault(environment);
  factories->screen = webrtc::DesktopCapturer::CreateScreenCapturer(options);
  factories->window = webrtc::DesktopCapturer::CreateWindowCapturer(options);
  return factories.release();
}

extern "C" void PulsebeamDestroyNativeFactories(void *handle) {
  delete static_cast<NativeFactories *>(handle);
}

int main() {
  void *factories = PulsebeamCreateNativeFactories();
  if (!factories) {
    std::cerr << "native factory construction failed\n";
    return 1;
  }
  PulsebeamDestroyNativeFactories(factories);
  // A factory can report no usable object on a headless/no-device CI worker.
  // Reaching this point proves that the packaged symbols and dependencies
  // loaded; physical device availability is deliberately not an acceptance
  // condition.
  std::cout << "native media factory symbols loaded (devices not required)\n";
  return 0;
}
