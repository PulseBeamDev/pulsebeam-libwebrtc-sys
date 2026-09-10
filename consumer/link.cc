#include "api/environment/environment_factory.h"
#include "rtc_base/crypto_random.h"

int main() {
  const webrtc::Environment env = webrtc::CreateEnvironment();
  return env.clock().CurrentTime().IsFinite() && webrtc::CreateRandomId64() != 0
             ? 0
             : 1;
}
