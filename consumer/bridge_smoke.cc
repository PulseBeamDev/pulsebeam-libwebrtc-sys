// This fixture uses the real Rust/CXX runtime through the crate's staticlib.
// Keep it separate from the pure SDK C++ consumer.
void TransformableFrameOwnershipRegression();

int main() {
  TransformableFrameOwnershipRegression();
  return 0;
}
