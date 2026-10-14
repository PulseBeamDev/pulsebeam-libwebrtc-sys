# Persistent standalone caller SDK audio factories

## Public owners and scope

`AudioEncoderFactoryRegistration::sdk_factory` and
`AudioDecoderFactoryRegistration::sdk_factory` create owning
`CallerAudioEncoderFactory` and `CallerAudioDecoderFactory` handles. Each handle
retains one actual refcounted SDK-derived factory and the Rust registration;
clones share that same native instance. Two calls to sdk_factory create distinct
SDK owners sharing the original mutable registration. Construction invokes no
provider callbacks. Registration retirement does not retire a live SDK owner.

These types are intentionally distinct from selected `AudioEncoderFactory` and
`AudioDecoderFactory` dependencies. Peer and production builders do not accept
them, and the native wrappers expose no SDK factory accessor for installation.
#37 mutable Query consumer admission, #38 caller Encode failure and #39 caller
decoder output/compatibility admission remain unresolved. Persistent ownership
is not permission to install an unsafe consumer or caller codec.

The original per-operation `_via_sdk` registration methods remain available;
they still create temporary SDK adapters per operation. Direct Rust registration
methods remain independent serialized dispatch, not native factory virtuals.

## Dispatch, serialization and request identity

All persistent handle methods perform their named actual SDK virtual operation:
encoder discovery, mutable Query and independent Create; decoder discovery,
support query and both distinct Create overloads. There is no discovery/Query
cache, format parser whitelist, implicit support query before Create, metadata
repair or custom-factory fallback.

The handle mutex serializes access to its native instance and request pointer.
Before acquiring that mutex, a thread-local scope keyed by the registration
refuses same-registration SDK reentry, including through a different SDK handle.
Direct provider callback reentry is also checked before the native mutex. Waiting
entries recheck the registration's panic-disabled state after acquiring it. The
SDK-operation scope is separate from callback activation: the real virtual call
still enters the Rust provider once through the existing callback mutex.
Callbacks/destructors must not synchronously wait for or reenter their owning
actor/factory. Application-owned strong reference cycles remain caller-owned.

Each allowed operation owns a fresh request and outcome receipt. The native
CallScope borrows that request only for synchronous dispatch and restores the
previous source pointer before returning to Rust. It cannot retain the current
environment or receipt for the next operation. The SDK factory's durable Rust
owner retains the registration, not last-call capability or creation inputs.
Raw C++ interop callers must provide the same owner serialization and matching
request identity/lifetime; the private Rust owner enforces them for safe APIs.

## Inputs and existing native products

Encoder Create passes a copy of the actual SDK Options value, preserving the full
signed payload field and optional native codec-pair identity. Both decoder Create
overloads preserve their identity and absent versus actual optional pair. Create
receives the actual supplied SDK Environment, with matching Rust-only clock,
queue and randomness leases retained in the owning callback context. Different
operations can supply different environments without inheriting prior inputs.

Provider-returned products are still existing owning native `AudioEncoder` or
`AudioDecoder` handles. Caller standalone encoder/decoder/frame products are not
accepted by these native Create contracts. Returned products must be uniquely
transferable; outstanding Rust owners or native decoder frames refuse handoff.
No mutable DSP backend is cloned or detached from its original owner.

The actual SDK envelope owns the complete transferred product, guarded state,
environment and Rust leases. The returned Rust wrapper resolves through that
privately constructed envelope's direct origin, preserving backend kind, mutex,
Opus guards and decoder frame-counter lifetime. Product provenance is not inferred
from request/discovery names or advertised codec metadata. Native one-shot and
persistent drivers share the same private wrap helpers, not a replacement codec.
Returned products/parsed frames can outlive factory and registration retirement.

## Failure, lifetime and evidence

Ordinary unsupported Query/None, false support and null Create remain native
ordinary dispositions. Callback Err/panic, reentry, disabled state and failed
unique transfer are typed per-operation failures, not empty/false/null success.
A prior failure receipt cannot contaminate another operation's actual result.
The shared first-failure observer remains independent and can outlive the factory;
only callback panic disables later provider dispatch. Ordinary provider errors
and rejected reentry do not reset codec state or disable successful later calls.

The SDK owner is destroyed before the retaining Rust registration. Registration
provider destruction is contained by its existing first-failure observer; a
single destructor panic is recorded. Double panic, abort and OOM are not made
recoverable by this boundary. No published factory identity or arbitrary codec
association token is invented.

`tests/audio_factory_sdk_handles.rs` supplies real PCMU products/payloads,
registration retirement/migration, raw mutable metadata, successive actual
environments/options and exact codec-pair retention, overload preservation,
product/frame lifetime, None/error/panic distinctions, same- and cross-handle
reentry and concurrent independent receipts across distinct SDK owners sharing
one registration. Existing one-shot/forwarding fixtures remain unchanged.

Scoped source review found no concrete ABI/lifetime/deadlock/contract defect.
Initial fixture review found no concrete API/PCM assumption defect and identified
pair-identity and distinct-handle concurrency coverage gaps; source fixtures now
cover both. All fixtures are unrun. Static review and formatting are not
compiler, runtime, ABI, matching-artifact or installed-consumer qualification.
