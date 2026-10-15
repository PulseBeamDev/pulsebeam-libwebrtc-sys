// This fixture uses the real Rust/CXX runtime through the crate's staticlib.
// Keep it separate from the pure SDK C++ consumer.
void TransformableFrameOwnershipRegression();
void AudioSinkQueueRegression();
void VideoSinkQueueRegression();

int main() {
  TransformableFrameOwnershipRegression();
  AudioSinkQueueRegression();
  VideoSinkQueueRegression();
  return 0;
}
