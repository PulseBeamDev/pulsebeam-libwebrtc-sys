# Native Call execution resources

`ExecutionResources` supplies execution defaults independently of codec and source
selection. Both `PeerConnectionFactory::builder()` and the common
`Session::with_runtime(runtime, config)` builder accept
`.execution_resources(resources)`. Runtime policy supplies a default only when
no resource configuration was registered.

```rust
use pulsebeam_webrtc_sys::{CodecCoreBudget, ExecutionResources};

let resources = ExecutionResources {
    codec_core_budget: Some(CodecCoreBudget::new(1)?),
};
# Ok::<(), pulsebeam_webrtc_sys::InvalidCodecCoreBudget>(())
```

`CodecCoreBudget` accepts every positive native `int` and rejects zero and negative
values without constructing native resources. With no configuration registration,
caller-driven peer factories select one core and threaded factories preserve
native host-core detection. An explicitly registered configuration is never
rewritten, including an explicit absent budget requesting native host detection.
This is a default for native Calls, not a global setting or a process-wide worker cap. Backends may use fewer cores, impose their own limits or
manage workers independently. It is not proof that all workers are caller-owned.

## Native boundary

The pinned runtime-only patch adds `CallConfig::codec_core_budget`. Native Call
construction uses it, when present, instead of `cpu_info::DetectNumberOfCores()`.
The same resulting value reaches video send and receive stream construction and
their native encoder and decoder settings. No media algorithm, codec selection,
RTP behavior, clock or entropy source is replaced.

The factory's selected SDK `MediaFactory` still creates the real Call. The adapter
fills only an absent CallConfig budget before delegation; an explicit native
CallConfig value takes precedence. Standalone encoder/decoder initialization does
not pass through this default and retains its caller-provided settings.

`patches/runtime-codec-core-budget.patch` is pinned separately from the existing
SDK patches. The producer rejects a changed patch or a different SDK diff in
`call/call.cc` / `call/call_config.h`. Its digest is recorded in build metadata,
source identity and the native-configuration identity, and checked by artifact
validation. Default behavior on other targets is unchanged; the qualified binding
scope remains Linux x86_64 core/native.

## Boundaries

`Session<R>` already provides common construction and the former media-profile
admission flag is removed. Resource defaults do not select media implementations,
pump queues, close every synchronous-wait/teardown gap or qualify every backend.
Explicit budgets greater than one and explicit host detection remain scheduling
qualification gaps for managed core workflows, not automatic determinism opt-outs
or upstream-unavailable cases. Standalone codec settings remain unchanged.

In the pinned Linux VP8 implementation, the encoder's native `NumberOfThreads`
returns one for a one-core setting at every resolution; host-derived counts can
select multiple workers above VGA. Libvpx creates encoding/filter pthreads only
for its multithreaded path. The VP8 decoder independently fixes its thread count
to one. This concrete dependency motivates the default policy. Source review and a
resource-selector unit test alone are not qualification.

`tests/managed_vp8.rs` now exercises two actual native Calls with stock VP8,
caller-supplied 1280×720 input, native transport and native decoding. With no
resource registration, both Linux flavors produced the first full-resolution
decoded frame and replayed its complete visible I420 pixels, RTP/capture
timestamps, logical connection events and virtual observation time. Repeated
runs inside each process and serialized observations from two fresh processes
matched. Continuous syscall traces from acquisition through final native drop
covered eight default-resource lifecycles in total, with zero thread/process
creation or real socket syscalls. Core ASan with leak detection also passed.

The fixture's optional `PULSEBEAM_VP8_HOST_WORKER_PROBE=1` negative control
explicitly registers an absent budget. On the qualification host (20 detected
cores), the same application completed with three successful native worker
creation calls per flavor; the lifecycle checker correctly rejected that trace.
This verifies that the default-resource observation was not just a low-resolution
or inactive-encoder artifact. Host/multiworker configuration is still accepted
faithfully, not qualified for managed scheduling.

The same application and outer progress harness now live in
`tests/support/managed_video.rs`; `tests/managed_vp8.rs` keeps its original test
entry point. `tests/managed_vp9.rs` selects the stock VP9 profile-0 capability as
an explicit media preference, not a runtime-specific codec admission rule.
The pinned VP9 encoder maps a one-core setting to `g_threads=1`; the decoder
caps its resolution-derived thread count at one. Libvpx enables row-MT but
executes the final worker inline, creating pthreads only for preceding workers.
The scoped row/tile/filter/frame-parallel paths therefore do not create physical
workers with stock one-core settings. Explicit settings remain unchanged.

The VP9 fixture qualified the same first-frame observations at 1280×720 on both
Linux flavors: complete visible pixels, capture/RTP timestamps, logical events
and virtual observation time matched within and across fresh processes. Eight
VP9 lifecycles had no thread/process creation or real socket syscalls. Its
`PULSEBEAM_VP9_HOST_WORKER_PROBE=1` negative control completed the same media
procedure with four successful native worker creations per flavor, correctly
rejected by the monitor. The shared-harness refactor also requalified eight VP8
lifecycles and produced byte-identical JSON observations to the committed VP8
baseline. Both VPx fixtures passed core ASan with leak detection.

`tests/managed_av1.rs` uses the same unmodified application/harness with stock
libaom encoding and dav1d decoding of 1280×720 8-bit input. The pinned encoder
maps one core to `g_threads=1`; row-MT stays synchronous and worker 0 is never
reset/launched as a pthread. The decoder maps one core to `n_threads=1` with
`max_frame_delay=1`; dav1d derives one task/frame context and creates pthreads
only when the task-context count exceeds one. These scoped source conditions
are backed by actual first-frame media, not only option readbacks.

On both Linux flavors, the AV1 first decoded frame's complete visible pixels,
capture/RTP timestamps, logical events and virtual observation time matched
within and across fresh processes. Eight complete AV1 lifecycles had no
thread/process creation or real socket syscalls. The optional
`PULSEBEAM_AV1_HOST_WORKER_PROBE=1` explicitly selected native host detection;
the same media procedure completed with 23 successful native worker creations
per flavor, correctly rejected by the monitor. Core AV1 ASan/leak detection
and existing VPx regressions passed. No SDK/bridge, media algorithm, profile
admission rule or new runtime hook was needed for this stock one-core case.

`tests/managed_h264.rs` applies the same procedure to stock constrained-baseline
H.264 (`profile-level-id=42e01f`, packetization mode 1) with OpenH264 encoding
and FFmpeg decoding. WebRTC's stock encoder requests one thread and one slice.
OpenH264 clamps its final thread count to the slice count and calls its
multithread resource initializer only when that final count exceeds one, so its
shared task pool is unreachable in this configuration. WebRTC's FFmpeg adapter
forces `thread_count=1`; FFmpeg disables active slice/frame threading before
worker initialization. The SDK encoder-thread-limit experiment cannot override
the fixed one-slice clamp in this binding path.

Both Linux flavors produced the stock H.264 first decoded frame with complete
visible pixels, capture/RTP timestamps, logical events and virtual observation
time matching within and across fresh processes. Eight default-resource H.264
lifecycles had no thread/process creation or real socket syscalls. Unlike the
VPx/AV1 diagnostic controls, explicit host detection remains single-threaded for
this stock one-slice configuration: two additional host-resource lifecycles had
no worker/socket calls and produced JSON identical to the default. This is
expected source-backed behavior, not evidence that explicit host resources are
generally managed. Core ASan/leak detection passed for all four codec cases.

### Post-first-frame scheduling and teardown

The shared fixture now retains the first two frames returned by the raw sink,
without duplicate filtering. It asserts increasing decoded-frame timestamps,
distinct RTP timestamps, full-resolution visible pixels and at least 33 ms of
virtual media time before closing both peers. The observation contains both
frames' complete visible I420 data and timestamps, logical connection events,
absolute virtual observation time and media elapsed time. SDK frame timestamps
are retained unchanged; elapsed time is measured separately using the injected
world clock, not by comparing SDK timestamps with that clock.

This extends the four stock codec cases beyond their earlier first-frame
checkpoints. Both Linux flavors replayed the two-frame observations within and
across fresh processes. Continuous traces covered 32 complete default-resource
lifecycles, including close/shutdown and final native drop, without thread/process
creation or real socket syscalls. All four core ASan/leak cases also passed.

The pinned receive path posts decode and post-decode work to SDK task queues.
Its `VideoReceiveStream2::Stop` posts decoder shutdown to the decode queue and
then performs `Event::Wait(kForever)`. The fixture reaches this synchronous
teardown after post-first-frame progress using only caller-owned execution,
packet delivery and virtual-time advancement. It does not establish arbitrary
close ordering or every nested wait. In particular, two frames do not establish
that the periodic encoder watchdog, feedback, retransmission or reconfiguration
paths have run. No new wait hook, helper executor or native patch was added.

For fresh-process comparison, set `PULSEBEAM_VP8_TRACE_PATH`,
`PULSEBEAM_VP9_TRACE_PATH`, `PULSEBEAM_AV1_TRACE_PATH` or
`PULSEBEAM_H264_TRACE_PATH` to a separate JSON path for each invocation of the
corresponding already-built test executable. Run it under the
continuous monitor described in `tools/check_controlled_lifecycle_trace.py`,
then check each default-resource trace with `--minimum-scopes 2`. The optional
host probe intentionally fails that monitor and must be evaluated separately.
JSON file writes occur after all native lifecycle scopes.

This evidence covers the first two decoded frames and their listed observations,
not all encoded bytes, RTP/statistics timelines, long-running sustained media,
software codecs beyond these stock VP8/VP9/AV1/H.264 cases or complete
replay/lifecycle closure. Explicit settings/profiles beyond these cases also
remain unqualified. H.264 multi-slice and experimental parallel configurations
are not exposed by this stock case. Repeated dav1d configuration ownership and
reconfiguration lifecycle were not exercised by the two-frame AV1 case and
remain specific qualification gaps.
Those obligations remain open.

The actual Call consumer fixture in `tests/video.rs` records encoder and decoder
initialization during negotiated raw-video transport with caller codec probes.
It distinguishes the absent native default from explicit budgets without using
an option readback echo. Those probes do not qualify stock codec backend threading,
compression quality or deterministic replay. The latter need stock-media,
continuous lifecycle and replay checks with matching artifacts.
