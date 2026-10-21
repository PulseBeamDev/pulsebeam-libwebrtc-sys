# Native Call execution resources

`ExecutionResources` supplies execution defaults independently of codec and source
selection. Both `PeerConnectionFactory::builder()` and the common
`Session::with_runtime(runtime, config)` builder accept
`.execution_resources(resources)`. Runtime policy supplies a default only when
no resource configuration was registered.

```rust
use pulsebeam_webrtc_sys::{CodecCoreBudget, ExecutionResources};

let resources = ExecutionResources {
    codec_core_budget: Some(CodecCoreBudget::new(8)?),
};
# Ok::<(), pulsebeam_webrtc_sys::InvalidCodecCoreBudget>(())
```

`CodecCoreBudget` accepts every positive native `int` and rejects zero and negative
values before native resource construction. With no configuration registration,
caller-driven peer factories select one core and threaded factories preserve
native host-core detection. An explicitly registered configuration is never
rewritten, including an absent budget requesting native host detection.

This is a default for native Calls, not a global setting or a process-wide worker
cap. Backends may use fewer cores, impose their own limits or manage workers
independently. Selecting a budget alone does not establish caller-owned execution.

## Native boundary

The pinned runtime-only patch adds `CallConfig::codec_core_budget`. Native Call
construction uses it, when present, instead of `cpu_info::DetectNumberOfCores()`.
The resulting value reaches video send/receive stream construction and native
encoder/decoder settings. No media algorithm, codec selection, RTP behavior,
clock or entropy source is replaced.

The selected SDK `MediaFactory` still creates the real Call. The adapter fills
only an absent CallConfig budget before delegation; an explicit native CallConfig
value takes precedence. Standalone codec initialization does not pass through this
default and retains its caller-provided settings.

`patches/runtime-codec-core-budget.patch` is pinned separately from the other SDK
patches. The producer checks the SDK diff, and artifact consumers check its source
and native-configuration identity. Other target defaults are unchanged. The
qualified binding scope is Linux x86_64 core/native.

## Managed VP8 workers

On desktop Linux x86_64, the managed runtime installs the pinned optional libvpx
worker service for builtin VP8 encoders carrying its exact native environment and
owner. This includes encoders reached through the SDK's cached/simulcast factory
path. Custom encoders and unrelated environments do not acquire that provenance.

The encoder retains its native worker-count and speed-selection policy. Its
available-processor input uses the selected codec resource count before the
original libvpx pool clamp. Persistent encoder/filter entries execute unchanged
on guarded continuation stacks, using actions in the existing clock-wide task
queue kernel. Known encoder semaphores and native spin waits suspend or dispatch
ready work without advancing time. These are not a second executor, a replacement
codec, or a general implementation of arbitrary native blocking waits.

Each stack uses the native pthread default stack size with guard pages. Admission
failure propagates native worker-creation failure and retires partial work rather
than falling back to OS workers. Final worker destruction precedes restoration of
the prior process-global worker/runtime interfaces. Sanitized builds use the ASan
fiber-switch protocol; ASan still emits its standard makecontext/swapcontext
support warning.

Threaded execution preserves the native pthread path. Android and other platforms
are not covered by this continuation backend. Explicit native-only services and
external providers retain their actual execution behavior.

## Supported configurations and limits

The common caller-driven raw-video path includes stock VP8, VP9 profile 0, AV1
8-bit and constrained-baseline H.264 with the default one-core native Call budget.
The pinned VP8 decoder is single-threaded; the scoped VP9, libaom/dav1d and
OpenH264/FFmpeg settings avoid their physical worker pools at that budget.
Builtin VP8 also supports managed persistent workers with an explicit eight-core
budget, without changing codec complexity or forcing a serial encoder.

The focused public-API media cases use 1280×720 input and observe two distinct
decoded frames, full visible I420 pixels, capture/RTP timestamps, logical events
and virtual observation time. They do not establish every profile, layer, worker
configuration, sustained feedback/statistics timeline, or reconfiguration path.
In particular, repeated dav1d configuration ownership and broader explicit
multi-core/host-detected settings remain qualification gaps, not upstream
unsupported results or automatic determinism opt-outs.

Resource defaults do not close every synchronous wait, nested provider or teardown
gap. Native-only device execution, arbitrary provider callbacks and arbitrary
stack/TLS usage are outside these bounded software-worker contracts. See
[caller-driven scheduling](controlled-scheduling.md) for lifecycle and pump rules.

## Stock audio qualification

The shared generic application in `tests/support/managed_pcm.rs` drives stock
Opus, PCMU, PCMA and G722 through capture, negotiated RTP, native NetEq/playout,
reports and externally selected loss. Replay compares actual plaintext encoded
frames and all returned PCM without changing codec algorithms or callback sample
rates. Current core/native and core ASan cases passed, with complete-lifecycle
monitoring and repeated fresh-process/cross-flavor observations. These bounded
primary-format results do not close auxiliary formats, all processing/mixing or
arbitrary NetEq settings. Separate standalone L16 stereo and six-channel Opus
cases qualify direct encode/decode and creator/parsed-frame ownership across
runtimes, without changing peer advertisement or establishing installed NetEq
consumers for those formats. The generic standalone application also qualifies
two simultaneous stock mixer sources at distinct preferred rates, mono/stereo
output and source removal/release, without inventing an environment parameter
for the context-free SDK mixer. See the
[audio runtime contracts](audio-runtime-contracts.md) for exact scope and trace
controls.

## Exercising the media boundary

`tests/support/managed_video.rs` uses the same application procedure for threaded
and managed execution. `tests/managed_{vp8,vp9,av1,h264}.rs` select stock codecs;
`tests/managed_vp8_workers.rs` supplies the explicit eight-core VP8 budget.
The tests drive actual encoding, transport and decoding, not codec option echoes.

For fresh-process comparisons, set `PULSEBEAM_VP8_TRACE_PATH`,
`PULSEBEAM_VP9_TRACE_PATH`, `PULSEBEAM_AV1_TRACE_PATH`, `PULSEBEAM_H264_TRACE_PATH`
or `PULSEBEAM_VP8_WORKERS_TRACE_PATH` to a distinct JSON path for each invocation
of the corresponding already-built executable. The optional
`PULSEBEAM_{VP8,VP9,AV1}_HOST_WORKER_PROBE=1` controls request native host detection;
their worker behavior depends on the selected backend and host, so they must not
be assumed to create physical workers.

The continuous monitor in `tools/check_controlled_lifecycle_trace.py` checks
thread/process creation and real socket syscalls across complete acquisition and
final-drop scopes. Its watchdog detects hangs but never drives media progress.
Replay observations and syscall traces are complementary: neither source review
nor an unchanged worker count alone establishes native media execution or replay.
