// This fixture uses the real Rust/CXX runtime through the crate's staticlib.
// Keep it separate from the pure SDK C++ consumer.
void TransformableFrameOwnershipRegression();
void AudioSinkQueueRegression();
void VideoSinkQueueRegression();
void VideoSourceControlsRegression();
void VideoSinkAdmissionRegression();

int main() {
  TransformableFrameOwnershipRegression();
  AudioSinkQueueRegression();
  VideoSinkQueueRegression();
  VideoSourceControlsRegression();
  VideoSinkAdmissionRegression();
  return 0;
}
