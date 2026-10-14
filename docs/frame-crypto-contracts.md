# Legacy frame encryptor/decryptor contracts

This slice is source-wired against SDK revision
`ba469aa2093ba950066258ca0a59a6fbd1295582`. It is not behavioral
qualification. Offline Rust integration checks and existing-configuration core
C++ object compilation have passed in the scopes recorded in
`plans/completion.md`. Matching-configuration artifact builds, linked runtime
tests and sanitizers remain unqualified. These interfaces are distinct from SFrame key
management and frame transformation.

## Native member ledger

| Pinned SDK contract | Owning Rust path |
| --- | --- |
| `FrameEncryptorInterface::GetMaxCiphertextByteSize` | `FrameEncryptor::max_ciphertext_byte_size`; `FrameEncryptorProvider::max_ciphertext_byte_size` |
| `FrameEncryptorInterface::Encrypt` | `FrameEncryptor::encrypt`; `FrameEncryptorProvider::encrypt` |
| `FrameDecryptorInterface::GetMaxPlaintextByteSize` | `FrameDecryptor::max_plaintext_byte_size`; `FrameDecryptorProvider::max_plaintext_byte_size` |
| `FrameDecryptorInterface::Decrypt` | `FrameDecryptor::decrypt`; `FrameDecryptorProvider::decrypt` |
| `FrameDecryptorInterface::Status` | Open `FrameDecryptionStatus`, including raw signed-int metadata |
| `FrameDecryptorInterface::Result` construction, copying, const fields and `IsOk` | Actual owning SDK `FrameDecryptionResult`, `clone`, field getters and `is_ok`; Rust `assign`/setters reconstruct a complete SDK result, preserving the untouched field |
| `RtpSenderInterface::SetFrameEncryptor`, `GetFrameEncryptor` | `RtpSender::set_frame_encryptor`, `frame_encryptor` |
| `RtpReceiverInterface::SetFrameDecryptor`, `GetFrameDecryptor` | `RtpReceiver::set_frame_decryptor`, `frame_decryptor` |

Local receivers construct their permanently receiver-bound decryptor through
`RtpReceiver::create_frame_decryptor`. Encryptors are constructible directly
through `FrameEncryptor::from_provider`. `ProductionSession` exposes creation,
installation, native getter identity, both operations, size queries, failure
queries and release through opaque `FrameEncryptorId` and `FrameDecryptorId`.

## Callback and ownership contracts

The native adapters are actual SDK refcounted interfaces. They retain owned
`Send + Sync + 'static` providers through SDK references, not through a borrowed
caller pointer. Dropping a Rust alias or releasing an actor ID does not remove
an installed provider. Getter results retain the actual installed interface;
identity comparison uses the SDK interface pointer, not a copied configuration.
An actor getter reuses an existing ID for the same native resource, or admits a
new retained ID after the previous alias was released.

SDK callbacks execute synchronously on native media paths and may overlap with
application operations. They do not run through an extra actor executor.
Providers must not synchronously wait for or reenter their owning peer/actor.
Provider destruction can occur on SDK or application threads. With
`panic=unwind`, callback and final-provider destructor panics are contained.
Panic payloads are deliberately forgotten because their own Drop can panic;
this is a bounded leak per caught panic, not normal provider retention.
`panic=abort`, process aborts and ordinary allocation failure are not catchable.

Each callback gets borrowed input/additional-data/CSRC views and separate,
zero-filled mutable Rust scratch. Rust never forms a slice over uninitialized
SDK output. Video can decrypt in-place: the adapter copies the validated Rust
prefix into SDK output only after the Rust callback and its input loans have
ended. This is initialization, not a promise of secret-memory zeroization.

Encryptors support Send/Sync because their closed producer graph contains only
these thread-safe adapters, with no retained peer or SDK sequence. A local
decryptor retains its origin peer and exact native receiver, stays non-Send and
non-Sync, and cannot attach to a different receiver even after detachment.
`ProductionSession` owns those local handles internally under its existing
move-only, Send/non-Sync mutable-owner contract. Releasing the origin receiver ID
does not change the decryptor's retained native receiver identity.

Failure queries currently downcast SDK getter results to these adapters. This is
sound only for the audited closed producer graph: public constructors/setters
admit these adapters exclusively, the native getter returns the stored pointer,
and this pin's SFrame creation does not produce legacy frame cryptors. Any
future native injection or automatic SDK producer must revisit this invariant
before being exposed. There is no global registry or synthetic getter result.

## Exact domains and consumption guards

Encryption preserves all signed-int return codes. Decryption preserves the
entire signed-int status domain and uses the SDK's `IsOk`, not a Rust substitute.
The SDK tells providers not to return Unknown, but the audited consumers only
compare/store statuses; Unknown and other raw values do not create a safety
hazard and are not normalized. Owned result values preserve the entire size_t
metadata domain, including `usize::MAX`. MediaType, SSRC, CSRCs and arbitrary
input/additional-data bytes are forwarded unchanged.

Both provider operations validate `bytes_written <= output_capacity`, including
failure statuses: audio send/receive consumers trust this count, and video
senders resize their SDK buffer. Invalid counts poison the provider and yield a
failed operation with zero written bytes. Safe valid failures keep their exact
native code/status and count; the binding does not turn them into success.

Only video maximum plaintext capacity is constrained to encrypted frame size:
`video/buffered_frame_decryptor.cc::DecryptFrame` otherwise executes
`RTC_CHECK_LE`. An invalid result records
`FrameCryptoFailure::VideoPlaintextCapacity`, returns zero capacity to avoid the
CHECK, and makes decryption fail. There is no arbitrary encryptor/audio size cap
or saturating repair of provider results. SDK byte-buffer growth selects at
least the requested allocation size even if its optional headroom wraps.

Callback panics and unsafe output record the first typed `FrameCryptoFailure`.
Subsequent callback entry fails rather than synthesizing success. Already
in-flight concurrent callbacks may finish. Ordinary native allocation failure
remains outside this diagnostic contract.

Standalone operation methods allocate their own native output span. Before
entering its noexcept allocation they reject a capacity above the actual native
`std::vector<uint8_t>::max_size()`, which would otherwise throw length_error and
terminate. `FrameCryptoOperationError::OutputCapacityTooLarge` is a pre-dispatch
binding error, not an invented SDK result or provider poison. This does not cap
size-query metadata or pretend to make feasible allocation sizes infallible.
Actor lifecycle errors remain distinct `FrameCryptoOperationError::Binding`.

## Written, unrun fixtures

`tests/frame_crypto.rs` covers SDK result domains/copy/IsOk, raw statuses,
refcounted alias retention and final drop, exact callback input, initialized
prefixes, nonzero return codes, typed poison, capacity rejection without poison,
receiver-origin rejection after detach, restored actor getter IDs, migration,
and provider-destructor unwind containment. `tests/support/frame_crypto.rs`
contains only a reversible test cipher, not a production cryptographic scheme.

The first fixture in `tests/production_session.rs` now requires native frame
encryption on both peers and installs caller encryptor/decryptor providers. It
releases the actor aliases before sending, observes real SDK media callbacks on
SDK threads, checks recovered Opus bytes plus existing source/capture metadata,
and checks final provider drops after migrated actor shutdown. It has not run.
Controlled paths, pending-work teardown, negative on-wire behavior and matching
core/native sanitizer gates remain qualification work, not asserted passes.

## Native SFrame key interfaces, not an implemented cipher pipeline

`SframeMode`, all five `SframeCipherSuite` constants and unknown signed
enum values round-trip through a native owning `SframeEncrypterInit`. Its
construction, assignment-copy and mutation touch the SDK aggregate. It is an
independent Send/Sync metadata value, not tied to a peer or installed cryptor.

Sender/receiver creation calls the real SDK methods. The pinned stock SDK first
calls its owning transceiver's `TryToEnableSframe`, which can change the
negotiation flag and request negotiation, then returns UNSUPPORTED_OPERATION
with "Sframe encrypter not yet implemented" or "Sframe decrypter not yet
implemented". After negotiation has disabled SFrame it instead returns the
original INVALID_MODIFICATION error. The binding preserves these errors and
side effects, not an invented successful product. SFrame key handles retain
their peer, use SDK refcounted clone aliases, and expose the native
`SetEncryptionKey`, `AddDecryptionKey`, `RemoveDecryptionKey` error-returning
methods; keys and uint64 IDs are not interpreted by Rust. The interfaces have
no public encrypt/decrypt data method. At this pin, stock creation cannot yield
a successful key handle, so key operations are wired but lack an executable
stock success producer.

The callback captures a raw transceiver pointer. Peer retention alone is not a
lifetime proof: completed negotiation and rollback can remove transceivers
from the SDK's owning list. Normal sender/receiver wrappers therefore retain
the exact SDK transceiver, including handles made by legacy graph getters.
Queued TrackRemoved receiver arrivals capture that owner from the exact SDK
receiver relation while it is still present, before later negotiation removal.
This is retained native ownership, not an SSRC/ID join or a hidden registry.
On rollback the owner may have been removed before OnRemoveTrack; for a
receiver-only handle without an owner, creation searches the actual live SDK
graph and holds a matched transceiver across dispatch. If ownership cannot be
established, it returns the distinct `SframeCreateError::DetachedOwner`
without invoking a potentially dangling callback. Plan B has no such owner
callback, does not call its unsupported GetTransceivers method, and preserves
the SDK INTERNAL_ERROR. Ordinary peer Close does not itself erase the owner
graph; binding creation does not replace the SDK's post-close error semantics.

Key calls and final SDK key-interface release are marshaled onto the retained
signaling role. Root key handles are local-only. ProductionSession owns them
captive, exposes typed IDs, rejects foreign-session IDs before dispatch, and
preserves its Send/non-Sync migration contract without making leaf handles
Send. Successful handles would retain their transceiver even after removal.

`tests/sframe.rs` and the negotiated-disabled fixture in
`tests/production_session.rs` are written, unrun source fixtures. They cover
complete init enum domains/copy/migration, stock native errors and negotiation
side effects, actor foreign-ID rejection and final migrated shutdown,
negotiated-disabled failure, and stopped-transceiver removal with surviving
endpoint/queued receiver handles. There is no claim of on-wire SFrame
encryption, successful stock key calls, or sanitizer evidence. Frame
transformation remains a separate implementation and qualification obligation.
