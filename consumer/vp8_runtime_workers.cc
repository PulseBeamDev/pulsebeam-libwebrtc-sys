// Native libvpx runtime-seam consumer, not managed-worker qualification.
#include <cassert>
#include <atomic>
#include <cstdio>
#include <cstring>
#include <cstdint>
#include "vpx/vpx_encoder.h"
#include "vpx/vp8cx.h"
#include "vpx/vpx_decoder.h"
#include "vpx/vp8dx.h"
#include "vpx_util/vpx_thread.h"

namespace {
VPxWorkerInterface original;
std::atomic<unsigned> resets{0}, launches{0}, ends{0};
std::atomic<unsigned> sem_inits{0}, sem_destroys{0}, sem_waits{0}, sem_posts{0};
unsigned fail_at = 0;
int enabled = 0;
void Init(VPxWorker* w) { original.init(w); }
int Reset(VPxWorker* w) {
  const unsigned index = ++resets;
  if (index == fail_at) return 0;
  return original.reset(w);
}
int Sync(VPxWorker* w) { return original.sync(w); }
void Launch(VPxWorker* w) { ++launches; original.launch(w); }
void Execute(VPxWorker* w) { original.execute(w); }
void End(VPxWorker* w) { ++ends; original.end(w); }
int Enabled() { return enabled; }
int SemInit(void*, int, unsigned, int*) { ++sem_inits; return 0; }
int SemWait(void*, int*) { ++sem_waits; return 0; }
int SemPost(void*, int*) { ++sem_posts; return 0; }
int SemDestroy(void*, int*) { ++sem_destroys; return 0; }
void Yield() {}
void Clear() {
  resets = 0; launches = 0; ends = 0;
  sem_inits = 0; sem_destroys = 0; sem_waits = 0; sem_posts = 0;
}
void Run(int selected, unsigned failure, bool observe_waits) {
  Clear(); enabled = selected > 0; fail_at = failure;
  const VPxRuntimeInterface runtime = {Enabled,
      observe_waits ? SemInit : nullptr, observe_waits ? SemWait : nullptr,
      observe_waits ? SemPost : nullptr, observe_waits ? SemDestroy : nullptr,
      observe_waits ? Yield : nullptr, nullptr};
  assert(vpx_set_runtime_interface(selected < 0 ? nullptr : &runtime));
  vpx_codec_ctx_t encoder{};
  vpx_codec_enc_cfg_t config{};
  assert(vpx_codec_enc_config_default(vpx_codec_vp8_cx(), &config, 0) == VPX_CODEC_OK);
  config.g_w = 1280; config.g_h = 720;
  config.g_threads = 3; config.g_lag_in_frames = 0;
  config.g_timebase.num = 1; config.g_timebase.den = 30;
  config.rc_target_bitrate = 1000;
  const auto result = vpx_codec_enc_init(&encoder, vpx_codec_vp8_cx(), &config, 0);
  if (failure) {
    assert(selected && result != VPX_CODEC_OK);
    assert(resets == failure && launches == failure - 1 && ends == failure);
    assert(sem_inits == sem_destroys);
    std::printf("failure=%u native-error=%d waits=%u retired=%u\n", failure,
                int(result), sem_inits.load(), sem_destroys.load());
    if (encoder.priv) vpx_codec_destroy(&encoder);
    return;
  }
  assert(result == VPX_CODEC_OK);
  vpx_codec_ctx_t decoder{};
  vpx_codec_dec_cfg_t decode_config{};
  decode_config.threads = 1;
  assert(vpx_codec_dec_init(&decoder, vpx_codec_vp8_dx(), &decode_config, 0) == VPX_CODEC_OK);
  vpx_image_t input{};
  assert(vpx_img_alloc(&input, VPX_IMG_FMT_I420, config.g_w, config.g_h, 32));
  unsigned decoded = 0;
  uint64_t encoded_bytes = 0;
  uint64_t visible_digest = 14695981039346656037ULL;
  uint64_t bitstream_digest = 14695981039346656037ULL;
  for (unsigned frame = 0; frame < 8; ++frame) {
    for (unsigned plane = 0; plane < 3; ++plane) {
      const unsigned width = plane ? config.g_w / 2 : config.g_w;
      const unsigned height = plane ? config.g_h / 2 : config.g_h;
      for (unsigned y = 0; y < height; ++y)
        for (unsigned x = 0; x < width; ++x)
          input.planes[plane][y * input.stride[plane] + x] =
              static_cast<unsigned char>(16 + ((x * 3 + y * 5 + plane * 51 + frame * 7) % 220));
    }
    assert(vpx_codec_encode(&encoder, &input, frame, 1, 0, VPX_DL_REALTIME) == VPX_CODEC_OK);
    vpx_codec_iter_t iterator = nullptr;
    while (const auto* packet = vpx_codec_get_cx_data(&encoder, &iterator)) {
      if (packet->kind != VPX_CODEC_CX_FRAME_PKT) continue;
      encoded_bytes += packet->data.frame.sz;
      const auto* encoded = static_cast<const uint8_t*>(packet->data.frame.buf);
      for (size_t i = 0; i < packet->data.frame.sz; ++i)
        bitstream_digest = (bitstream_digest ^ encoded[i]) * 1099511628211ULL;
      assert(vpx_codec_decode(&decoder,
          static_cast<const uint8_t*>(packet->data.frame.buf),
          packet->data.frame.sz, nullptr, 0) == VPX_CODEC_OK);
      vpx_codec_iter_t output = nullptr;
      while (const auto* image = vpx_codec_get_frame(&decoder, &output)) {
        assert(image->d_w == config.g_w && image->d_h == config.g_h);
        ++decoded;
        for (unsigned plane = 0; plane < 3; ++plane) {
          const unsigned width = plane ? image->d_w / 2 : image->d_w;
          const unsigned height = plane ? image->d_h / 2 : image->d_h;
          for (unsigned y = 0; y < height; ++y)
            for (unsigned x = 0; x < width; ++x)
              visible_digest = (visible_digest ^ image->planes[plane][y * image->stride[plane] + x]) * 1099511628211ULL;
        }
      }
    }
  }
  assert(decoded == 8 && encoded_bytes > 0);
  vpx_img_free(&input);
  assert(vpx_codec_destroy(&decoder) == VPX_CODEC_OK);
  assert(vpx_codec_destroy(&encoder) == VPX_CODEC_OK);
  assert(resets == (selected > 0 ? 3u : 0u));
  assert(launches == resets && ends == resets);
  if (observe_waits) assert(sem_inits == 6 && sem_destroys == 6 && sem_waits && sem_posts);
  std::printf("selected=%d observe=%d interface-workers=%u frames=%u bytes=%llu visible=%llu bitstream=%llu\n",
              selected, int(observe_waits), resets.load(), decoded,
              static_cast<unsigned long long>(encoded_bytes),
              static_cast<unsigned long long>(visible_digest),
              static_cast<unsigned long long>(bitstream_digest));
}
}  // namespace
int main(int argc, char** argv) {
  std::setvbuf(stdout, nullptr, _IONBF, 0);
  original = *vpx_get_worker_interface();
  const VPxWorkerInterface counted = {Init, Reset, Sync, Launch, Execute, End};
  assert(vpx_set_worker_interface(&counted));
  VPxRuntimeInterface invalid{};
  assert(!vpx_set_runtime_interface(&invalid));
  assert(vpx_set_runtime_interface(nullptr));
  if (argc == 2 && std::strcmp(argv[1], "--native-only") == 0) {
    for (unsigned repeat = 0; repeat < 4; ++repeat) Run(-1, 0, false);
    assert(vpx_set_worker_interface(&original));
    return 0;
  }
  assert(argc == 1);
  // Raw libvpx realtime defaults adapt speed to measured native elapsed time.
  // Digests above are unnormalized observations, not deterministic assertions:
  // even the no-interface path changes output under external timing pressure.
  // Keep codec defaults and the explicit three-worker setting unchanged.
  Run(-1, 0, false);  // No runtime interface installed.
  Run(0, 0, false);   // Disabled interface retains the original pthread path.
  Run(1, 0, false);   // Absent callbacks keep native semaphores.
  Run(1, 0, true);    // Declined callbacks keep native waits.
  for (unsigned failure = 1; failure <= 3; ++failure) Run(1, failure, true);
  assert(vpx_set_runtime_interface(nullptr));
  assert(vpx_set_worker_interface(&original));
  assert(vpx_get_runtime_interface()->enabled == nullptr);
}
