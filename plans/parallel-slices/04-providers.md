# Execution, network providers, observability and capture

Status: APPROVED

## Motivation

Deliver one independently accepted area of the native WebRTC application surface
without waiting for another plan or repeating completed baseline foundations.

## Outcome

Applications select owning execution, network/security, event-log, congestion/FEC/bitrate and opt-in Linux video-capture components that actually participate in native creation and media behavior, with truthful failures and lifecycle ownership.

## Behavior

### Use a caller-selected execution environment

- Preserve exact field-trial/default/optional selection, relative/absolute scheduling, environment/clock identity and retained resource lifetimes.
- Selected providers affect actual native creation and role-thread behavior. Absence may select native defaults; rejection is not permission to substitute them.
- Retain Rust and native dependent resources, including randomness leases, through asynchronous callbacks, migration and final release.
- Controlled selections remain caller-owned and sequence-bound, with explicit rejection of incompatible production providers and no hidden time advance, worker or real-time wait.

### Create peers with selected network and security providers

- Expose applicable owning provider factory/product/creation contexts, address/certificate/error values and role-thread choices.
- Asynchronous completion/cancellation and native affinity preserve exact native outcomes, including truthful provider/hardware/network absence.
- Selected custom providers are not silently replaced with stock products; native stock selection only follows an absent optional choice.
- Pending completion and dependent identities remain bounded and valid through peer close, actor migration and native-role destruction.

### Record actual events with an installed caller log

- Expose applicable creation/output/event/value operations, subtype fields and RTP parser/extension configuration.
- Selected logs receive actual native events; output/factory rejection, optional fields and errors remain truthful.
- No borrowed event data escapes its validity. Concurrent calls, stop, migration and queued callback retirement retain the required resources.

### Adapt calls with selected congestion and FEC components

- Expose applicable factory/product/value/feedback operations with actual creation environment and durable clock identity.
- Selected components consume real feedback and generate native-supported effects; native metric order, optional/default selection and errors remain exact.
- Native arithmetic, including probe/rate/interval/FEC calculations, remains libwebrtc-owned. Bindings preserve native-supported inputs rather than preflight private algorithms. Callback loans and required non-null products retain Rust/CXX boundary and lifetime safety; rejection must not become fabricated success.
- Callbacks/resources remain valid under reentry, aliases, concurrent feedback and actor migration/retirement.

### Use opt-in Linux video capture

- Expose applicable Linux capture/source/sink options and operations only under explicit native selection.
- Preserve actual geometry/timing, provider/hardware availability, native errors and dependent source/track/capture identity.
- Headless core has no physical capture side effects. Controlled execution explicitly rejects incompatible capture rather than using production fallback.
- Replacement, stale frames, callbacks and actor/session migrated close cannot access invalid resources.

## Public contract scope

The SDK header roots below are relative to the pinned source checkout, not another
planning document. The operation boundaries define the complete public scope.

### Use a caller-selected execution environment

- `Environment`/`EnvironmentFactory` in `api/environment/environment.h`, `api/environment/environment_factory.h`; `Clock` in `system_wrappers/include/clock.h`; trials in `api/field_trials.h`, `api/field_trials_view.h`.
- `TaskQueueBase`/`TaskQueueFactory` in `api/task_queue/task_queue_base.h`, `api/task_queue/task_queue_factory.h`, and `Metronome` in `api/metronome/metronome.h`, with native time/unit values.

Full public creation/input/lookup/clock/scheduling/cancellation/resource operations for these roots are in scope. Event-log provider implementation is included here; generalized controlled mixed-media replay is outside this plan.

### Create peers with selected network and security providers

- Network manager/monitor factories/products in `rtc_base/network.h`, `rtc_base/network_monitor.h`, `rtc_base/network_monitor_factory.h`; sockets in `api/packet_socket_factory.h`, `rtc_base/async_packet_socket.h`.
- DNS in `api/async_dns_resolver.h`, permission in `api/local_network_access_permission.h`, certificate generation in `rtc_base/rtc_certificate_generator.h`, and TLS/certificate verification values in `rtc_base/ssl_certificate.h`, `rtc_base/ssl_adapter.h`.

Public provider construction/input/operation/completion/cancellation/member contracts and required native address/security values are included. Protocol/private socket internals are not new public APIs.

### Record actual events with an installed caller log

- `RtcEventLog`, `RtcEventLogFactoryInterface`, `RtcEvent` in `api/rtc_event_log/rtc_event_log.h`, `api/rtc_event_log/rtc_event_log_factory_interface.h`, `api/rtc_event_log/rtc_event.h`; output in `api/rtc_event_log_output.h`.
- Concrete public event subtypes under `logging/rtc_event_log/events/`, including retained RTP/RTCP/configuration/feedback/native media records and their public field values.

Full public event-log factory/product/output and concrete event subtype members are covered, including supported RTP parser/extension configuration. Private serializer internals are not separate APIs.

### Adapt calls with selected congestion and FEC components

- `NetworkControllerInterface`/factory and message/value roots in `api/transport/network_control.h`, `api/transport/network_types.h`; predictor roots in `api/network_state_predictor.h`.
- FEC factory/product/callback roots in `api/fec_controller.h` and video bitrate allocator factory/product/values in `api/video/video_bitrate_allocator_factory.h`.

All public factory/product/feedback/control/protection members and required rate/time/probe/value types are covered. Unrelated congestion algorithms and generalized source/profile installation are excluded.

### Use opt-in Linux video capture

- `VideoCaptureModule`/factory/device/options values in `modules/video_capture/video_capture.h`, `modules/video_capture/video_capture_factory.h`.
- `DesktopCapturer`, source-list/delegated control, frame/options contracts in `modules/desktop_capture/desktop_capturer.h`, `modules/desktop_capture/delegated_source_list_controller.h`, `modules/desktop_capture/desktop_frame.h`, `modules/desktop_capture/desktop_capture_options.h`.

All applicable Linux public capture factory/product/control/observation/frame operations are included. Backend internals, other-platform behavior and generalized codec/source profiles are excluded.

Every public member in these operation boundaries and its transitive owning
inputs/outputs/callbacks is accounted for, including applicable experimental or
deprecated overloads. Each is available from the baseline or delivered here with
native outcome evidence or an explicit limitation permitted by this contract.
Missing members and configuration echoes cannot pass completeness. This does not
require a separate test for each member or expose private unrelated SDK internals.

## Invariants

- Owning typed public APIs preserve valid native-supported input, legitimate defaults, optional values, open protocol tokens and native errors. CXX/raw pointers/unsafe adapters stay private; generic configuration bags are not parity.
- No fabricated success, payload/metadata synthesis, silent normalization, guessed identity join, hidden registry or stock substitution. Native packetization, transport, feedback, reassembly and codec behavior remain engine-owned.
- Trust libwebrtc for its internal numeric arithmetic. Do not add shadow state, numeric quotas, conservative native-arithmetic admission profiles or field-trial rejection to preflight private SDK algorithms. Preserve direct FFI/unit representation requirements and Rust/CXX ownership, callback, buffer-validity and teardown protections. Unsupported is acceptable only for a genuinely unsupported pinned native operation, not missing binding work.
- Retain every relevant native and Rust dependency through asynchronous callbacks and final destruction. Applicable public builder/endpoint and ProductionSession paths are part of this outcome, not a later sibling integration task.
- ProductionSession remains move-only Send, non-Sync, single mutable-owner and usable after caller-thread migration. Controlled execution, when relevant, remains explicitly caller-owned and sequence-bound without hidden progress or production fallback.

## Constraints and baseline

Source checkpoint: `52b4d80d9f5ebe18064515c952ec8e6382d789a0`. Plan-only
commit `042579b` adds no feature implementation. Native SDK pin:
`ba469aa2093ba950066258ca0a59a6fbd1295582`. Existing public values, owning wrappers,
provider scaffolding, media APIs and controlled-world infrastructure are inputs,
not work to restart. Existing qualification covers only its named candidates and
behaviors; source presence is not installed-runtime proof.

Coverage includes the named application-facing public members and transitive
contracts needed to construct, implement, use and observe them, including applicable
experimental/deprecated APIs. Private/test-only internals and unrelated native
algorithms are excluded.

Scope is Linux x86_64: headless core and explicitly opted-in native device
behavior where relevant. Preserve existing packaging contracts. No new SDK or
dependency runtime patch, substitute engine or public unsafe escape hatch.
Public APIs may change when necessary for a simple, owning, typed contract.

This Markdown contains the complete slice contract. No other slice or shared
planning document is required. All required changes and safety/installation proof
belong to this outcome or use available frozen-baseline capabilities. Completion
and verification cannot require a sibling's changes. Final cross-slice composition
is separate and does not waive this slice's own actual-consumer evidence.

A changed native/bridge/configuration candidate requires a matching artifact with
known SDK/compiler/FP/GN/protobuf/source-manifest identity. Do not reuse or relabel
a historical archive as proof of changed native code.

## Non-goals

- Reimplementing existing clocks/schedulers, private networking/DSP/congestion algorithms and hidden production device/network fallback in controlled mode.
- Generalized mixed-source media/replay, new caller codec products and audio processing/device implementations; available baseline calls and controlled primitives remain usable.
- Other-platform expansion, downstream migration/simulator integration, real-SFU interoperability, publication and patent/license clearance.
- Internal architecture, files, worktrees, fixtures and routine case enumeration are builder choices, not semantic requirements.

## Verification

Evidence must observe stable public behavior/state and reject plausible incorrect
implementations. Reuse existing faithful evidence where applicable; choose the
narrowest mechanism that proves the risk. A real native/continuous lifecycle
boundary is required only where isolated values or mocks cannot faithfully show it.

### Use a caller-selected execution environment

| Material risk | Sufficient observable evidence |
| --- | --- |
| Selected values ignored or incompatible fallback | Observe actual creation, native clock reads and scheduled work under selected providers, including supported defaults and rejected controlled combinations. |
| Scheduling/lease/role failures | Observe deadlines/cancellation, panic/reentry/concurrency and final retained-resource release through public behavior. |

### Create peers with selected network and security providers

| Material risk | Sufficient observable evidence |
| --- | --- |
| Setter-only injection or fake networking | Observe actual candidate/transport creation using selected providers and consuming their completion or rejection through public APIs. |
| Pending callback lifetime/role failures | Retain aliases and pending DNS/permission/certificate/TLS work through cancellation, migrated close and final drop. |

### Record actual events with an installed caller log

| Material risk | Sufficient observable evidence |
| --- | --- |
| Standalone event values mistaken for installed logging | Observe actual peer events and configured RTP fields at the selected public output boundary. |
| Field loss or callback lifetime failures | Exercise rejection, output stop, concurrent callbacks, retained aliases and migrated teardown with owning event data. |

### Adapt calls with selected congestion and FEC components

| Material risk | Sufficient observable evidence |
| --- | --- |
| Standalone injection or invented control success | Observe actual packet/feedback/rate activity reaching selected providers and native control/protection results. |
| Feedback/value forwarding or callback ownership failures | Observe faithful native-supported values and outcomes, direct FFI conversion requirements, and alias/concurrency/retirement behavior on the selected environment. Internal SDK arithmetic is upstream responsibility. |

### Use opt-in Linux video capture

| Material risk | Sufficient observable evidence |
| --- | --- |
| Synthetic capture or incorrect capability claims | Observe actual capture with available hardware or supported provider boundaries; distinguish absent hardware from binding absence and verify headless no-device behavior. |
| Stale frames or capture lifetime errors | Exercise capture replacement, failures, retained aliases and migrated close/drop with pending work. |

Applicable buffer/callback/destruction hazards require matching-runtime safety
evidence. Standalone values, setter echoes, unexecuted fixtures and launch
receipts cannot certify actual installation. Hardware absence is distinct from
binding absence and cannot certify physical capture/device execution.

## Deferred

Unrelated capabilities and final integrated composition. A later material change
to this contract requires renewed human approval.
