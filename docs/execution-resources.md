# Native Call execution resources

`ExecutionResources` supplies execution defaults independently of codec and source
selection. Both `PeerConnectionFactory::builder()` and
`ProductionSession::builder(config)` accept `.execution_resources(resources)`.
The selection is not specific to real or simulated execution.

```rust
use pulsebeam_webrtc_sys::{CodecCoreBudget, ExecutionResources};

let resources = ExecutionResources {
    codec_core_budget: Some(CodecCoreBudget::new(1)?),
};
# Ok::<(), pulsebeam_webrtc_sys::InvalidCodecCoreBudget>(())
```

`CodecCoreBudget` accepts every positive native `int` and rejects zero and negative
values without constructing native resources. An absent budget preserves native
host-core detection. This is a default for native Calls, not a global setting or a
process-wide worker cap. Backends may use fewer cores, impose their own limits or
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

The resource setting does not remove the existing controlled-media admission
restrictions, turn `ProductionSession` into a common real/sim owner, pump queues,
close synchronous-wait or teardown gaps, or make arbitrary caller/native/hardware
providers deterministic. Those require their own execution and ownership support.
Existing mode-specific constructors are not a substitute for that common path.

The actual Call consumer fixture in `tests/video.rs` records encoder and decoder
initialization during negotiated raw-video transport with caller codec probes.
It distinguishes the absent native default from explicit budgets without using
an option readback echo. Those probes do not qualify stock codec backend threading,
compression quality or deterministic replay. The latter need stock-media,
continuous lifecycle and replay checks with matching artifacts.
