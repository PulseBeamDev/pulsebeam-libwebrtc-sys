#include "pulsebeam-webrtc-sys/native/codec.h"
#include "pulsebeam-webrtc-sys/src/lib.rs.h"
#include "api/video/i420_buffer.h"

void diagnostic(webrtc::I420Buffer& buffer) {
  buffer.pulsebeam_missing_sdk_member();
}
