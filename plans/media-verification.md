# Media qualification and completion evidence

Historical qualification: 2026-10-15, Linux x86_64 headless core, before the main rebase.

## Rebase onto main

The rebase integrates `main` at `a59e624`. The evidence below qualifies the
pre-rebase media candidate only, not the combined audio/connectivity/media source.
The old ordinary/ASAN archives remain unchanged and are not relabeled as matching
the rebased source. Source-level checks and native adapter compilation are rerun
for the integration; a fresh full runtime/ASAN qualification is separate.

Main's byte-oriented general `MediaStream`/`AudioProcessor` APIs remain intact.
Typed local containers and audio-track processor aliases now have explicit
`LocalMediaStream`/`AudioTrackProcessor` types and captive
`SessionLocalMediaStreamId`/`SessionAudioTrackProcessorId` IDs. Colliding actor
stream operations use `local_stream_*`, `local_add_media_stream_track`,
`local_remove_media_stream_track` and `local_release_media_stream` names.
The complete owning `AudioSourceOptions` value is shared by both source paths.
PCM publishing retains main's optional per-track processing argument, audio
receive aliases share the single-sink invariant, and deferred callback-thread
cleanup moves every new media resource rather than dropping it in place. Generic received audio references retain
factory roles without introducing a `PeerInner` cycle; typed sender aliases
recover exact known source owners across the general and typed paths.

Integration core C++ adapter compilation passed in verifier `e7be12f4`
(`/tmp/pi-verifier-vl35rb/output.log`), using the unchanged pinned SDK checkout,
CXX 1.0.200 and an isolated `.work/rebase/` stage. This is compile evidence,
not a rebuilt published artifact or a runtime/ASAN qualification. Parser fixture
manifests and embedded provenance identities are synchronized with the integrated
source without relabeling the old runtime archives.

Final integration source gates passed: `just check` (`2bdbf4cd`,
`/tmp/pi-verifier-C1pehN/output.log`), offline Rust all-target checks for core and
`native` (`d3604c28`, `/tmp/pi-verifier-UjQeHm/output.log`), staged whitespace
and pristine pinned-SDK checks (`49c89bfd`, `/tmp/pi-verifier-Pu17hx/output.log`).
The control-plane run unsets global `CARGO_NET_OFFLINE` so fake transfer/cache
tests can exercise their online branch; its Cargo invocations remain offline.

Authority: `plans/parallel-slices/02-media.md`. The original handoff is retained
unchanged as session context. This ledger replaces the earlier unqualified
implementation checklist; historical passes do not certify changed source.

## Scope and supported outcome

The owning media implementation is complete and qualified within the pinned
native boundaries documented in `docs/media-native-limitations.md`:

- Complete owning encoder/decoder values, selected factories/products, both
  query/decode overload families, settings, rates, FEC, feedback, selectors,
  encoded/raw buffers, metadata, registration-specific callbacks and observers.
- Local and captive ProductionSession routes preserve exact statuses, optional
  values, native creation failure, callback retirement and resource ownership.
- Full sender parameter transactions and transceiver initialization preflight
  reached consumer arithmetic before mutation, while preserving native sync/
  async errors, effective state and non-atomic publication/rollback semantics.
- Installed live transformer composition, supported owning mutations and caller
  crypto retain actual native frames/providers through replacement and teardown.
- Raw PCM/video cross real negotiated calls with meaningful contents/metadata,
  source/sink replacement, retained aliases and migrated actor teardown.
- Mixed H264 profile pairs and two independent L1T3 sources sharing a truthful
  temporal-fraction capability retain payload/source identity through native
  packetization, receive reassembly, replacement, retirement and controlled replay.

Different temporal-fraction capabilities for two same-format L1T3 sources are
**not** claimed. Before initialization, the SDK factory/info query has no causal
sender/source token. The user explicitly selected **Accept documented limit**
through the structured decision. No creation-order inference, format tags, false
shared capability, SDK patch or changed SDK pin was introduced. The approved
plan itself was not rewritten. Stock simultaneous three-rung reception and an
implemented SFrame cipher pipeline also remain unclaimed native limitations.

## Final native artifact identity

- SDK: `ba469aa2093ba950066258ca0a59a6fbd1295582`, source state `pristine`.
- depot_tools: `ed9c87f6f12f6b87210e7025d4a36a5a72a2ccd4`.
- CXX: `1.0.200`.
- Compiler: clang 23, revision `20b6ec66967ac2a8f932863c1abf251e5b17a843`.
- C++20, bundled libc++/libc++abi, Debian Bullseye sysroot, glibc 2.31 minimum.
- Native floating point: `-fno-fast-math -ffp-contract=off`.
- GN enables protobuf, Opus, VP8/VP9, dav1d and H264; core disables internal
  audio devices, X11 and PipeWire. Full definitions/GN inputs are in each kit's
  `build.txt` and manifest. Physical device execution is not certified.

| Input | SHA-256 |
| --- | --- |
| Rust bridge source | `be304d0403886855d8e0521b57a9594c42c8ced0bc871372ef298f750879429b` |
| Generated CXX header | `67af7bc0c47c5577cc9655c86164747e6423d1ea9bdf9e6ded483536d6de29e5` |
| Generated CXX source | `f47e9ee345427e4f634ea0f109f8afc22e6e684b63f1f88a2586dafe6ccde61b` |
| Native adapter | `192fdbf45563ea17a62473e745aeba6b753d3751a2e901935878e8988c2fc32f` |
| Ordinary native configuration | `3bf4b8e3e803f928450b3c39c380a1da671b180422bb04ad7ce26e172d32fa29` |
| ASAN native configuration | `c56e37c785597316bba182e4c10e683fe6c87cd26118f14545b9f1fa56d347cd` |

Artifacts are local, ignored build outputs, not repository payloads:

| Mode | Archive | Archive SHA-256 |
| --- | --- | --- |
| Ordinary | `.work/media-final-dist/webrtc-core-linux-x86_64.tar.gz` | `c507600fb47e732aa820eb078926ddead5225774e505ffb3594413e9443558a2` |
| ASAN | `.work/asan/dist/webrtc-core-linux-x86_64.tar.gz` | `07e3fc6821bdd45fd2154441adc58901c4689e8cd55f3f370f120d4423ae4d15` |

Matching extracted kits are `.work/package/webrtc-core-linux-x86_64` and
`.work/asan/package/webrtc-core-linux-x86_64`. Both repository manifest fixtures
carry the ordinary artifact's bridge, adapter and native configuration identity.

## Final executed gates

The final functional source candidate was
`c410466211417d9a8f6cc2194246536bce610c00:b81d9d70513389932147ef72385fdb4feac3ed8e1f411f4b3bd5639a777944ba`.
Only this ledger and manifest configuration-identity synchronization were changed
after its runtime gates. Artifact-support/control-plane and both provenance checks
passed again for that metadata-only closeout in
`verifier:717e1119-3354-4e42-9d5d-67b9380c286e`. Mechanical `cargo fmt` changes were included before all
final gates; they did not change bridge/native artifact inputs.

| Gate | Final evidence | Result |
| --- | --- | --- |
| Ordinary export | `verifier:7ee2dc90-1ee5-4251-a4eb-b898719f8058` | Pass |
| ASAN export | `verifier:baa93c2d-bafa-464c-837d-842c8292a834` | Pass |
| Ordinary all targets and isolated randomness/frame lifetime | `verifier:e2544dff-a17d-4145-a9dc-4e182e21ff97` | Pass |
| ASAN all targets and isolated randomness/frame lifetime | `verifier:d9e9b739-ed36-4599-a486-11a2e6c0f14e` | Pass, leak detection enabled |
| Ordinary C++ bridge executable | `verifier:f9737fab-8596-4140-af24-c10e88e2cb71` | Pass |
| ASAN C++ bridge executable | `verifier:17314367-10d4-479c-9397-1d9b8880f7a6` | Pass, leak detection enabled |
| Ordinary native VLA, fresh replay and continuous lifecycle | `verifier:41e675ff-1ace-42f2-baf5-be2c92389ce7` | Pass |
| ASAN native VLA, fresh replay and continuous lifecycle | `verifier:bbfb3b3f-05f8-4fee-8c04-be2738e18d23` | Pass |
| Repository control plane, Python tests, formatting, doctests, artifact support | `verifier:d00ba87d-9f20-495a-a732-fc83f609ffea` | Pass |
| Final CXX/native provenance in both kits | `verifier:cfc005ef-cbf6-4ee7-8c17-1ac9011950a7` | Pass |
| Both SDK source-state and whitespace gates | `verifier:bfa238bb-6c4d-4b16-95f4-3846d79e2b8e` | Pass |
| Configured rust-analyzer and clangd | Final source diagnostics | Zero errors; existing unused-include/deprecated-public-API hints remain |

All-target execution passes **567 tests per mode**. The normally ignored
process-global randomness/frame-lifetime fixture passes separately, alone, in
both modes. The normally ignored native VLA parser fixture also passes explicitly
in both modes against a probe built from the matching SDK kit. The real globally
addressed IPv6 TURN fixture remains ignored because that environmental capability
is not certified; controlled IPv4/IPv6 media is exercised independently.

Each mode compares meaningful VP8/Opus and mixed-H264 canonical traces across two
fresh processes, in addition to same-process comparisons. Each continuous strace
checks **56 complete lifecycles**, including acquisition, real native work,
rejected setup and destruction, with zero thread/helper-process creation or real
socket syscalls inside the marked scopes. ASAN leak detection is enabled for
runtime, VLA, replay and C++ execution. Only the separate ptrace/strace invocation
uses `detect_leaks=0`, because LeakSanitizer cannot run under ptrace.

A runtime delivery timeout was emitted for the control-plane job. Its retained
receipt and eventual completion both report `status=passed`, exit 0, duration
107002 ms. It was not a failed command and was not rerun to recover output.

## Public behavior to fixture evidence

| Contract family | Representative executed fixtures |
| --- | --- |
| PCM frames, source/sink/stream views, layout/options and real call/migration | `audio`, `audio_frame_values`, `pcm_views`, `pcm_handoff`, `media_stream`, `audio_source_views`, `audio_source_options`, `production_raw_media` |
| Raw video buffer/frame metadata, writable initialization/freeze, wants and source output | `video`, `video_source_views`, `video_frame_metadata`, `video_metadata`, `production_raw_media`, `tests/support/video_sink_wants.rs` |
| Full selected video factories/products/values/output/feedback/FEC/selectors | `video_factory_values`, `video_factory_optionality`, `video_codec_values`, `video_encoder_owned`, `video_decoder_owned`, `video_decoder_product`, `video_encoder_selector`, `video` |
| Fresh native parameter transactions and actor/captive resources | `sender_parameter_transactions`, `production_sender`, `production_session`, `video_encoder_owned` |
| Installed transformation, crypto, supported native ownership/retirement | `transformable_frames`, `frame_crypto`, `crypto_configuration`, `sframe`, `encoded_audio`, `video` |
| Concurrent identity, profile pairs, replacement and same/fresh replay | `controlled_media`, `controlled_driver`, `tests/support/mixed_h264.rs`, `production_raw_media` |
| Controlled acquisition/rejection/final queued destruction | Continuous lifecycle traces plus `execution`, `controlled_driver`, `controlled_media` |

The pinned limitations document is part of this reconciliation, not a claim that
unsupported native operations passed. Standalone value checks are complemented by
installed/native/actor delivery and teardown evidence; factory identity and launch
receipts are not treated as output proof.

## Repairs made during final qualification

- Real encoder Release/reinitialization and callback epochs; nullable output and
  exact creation/query failures, without failed-product success sentinels.
- Transformer self-dispatch, SCTP getter/transport teardown affinity and monitor
  userdata destruction on the real SDK network role.
- Explicit ManualClock evidence versus an unproven default global clock, native
  environment identity/capability retention and dependency retirement on close.
- Native audio admission before the L16 parser's narrowing DCHECK, while retaining
  ordinary unsupported results and separate carrier behavior.
- Standard panic-payload disposal at callback boundaries without invoking unknown
  payload destructors; failure observers and disabled registrations remain intact.
- Shared per-peer permission factory wiring into the public SDK BasicPortAllocator
  as well as transport control. The pinned default allocator omits that dependency.
  The same SDK manager/socket resources and provider lifetime are retained; no
  SDK patch, invented permission callback, route change or timeout workaround.
- Continuously fed impaired/replacement media fixtures, correct HDR wire layout,
  immutable field-trial cache expectations, byte-safe SDP negotiation and native
  RTCError C-string semantics. No required media case was conditionally skipped.
- Shell fixture compiler coverage now checks the exact native/generated source
  inventory rather than an obsolete constant count. Repository-required Rust
  formatting is mechanical and separated from feature changes where possible.

## Closeout

No SDK checkout patch, dependency/runtime patch, substitute engine, physical-device
claim, guessed source identity or fabricated SFrame pipeline is part of this
candidate. The user-supplied `plans/media-handoff.md` remains unchanged and is not
a deliverable commit. Final commits preserve the qualified functional source;
post-qualification changes are limited to evidence and configuration metadata.
