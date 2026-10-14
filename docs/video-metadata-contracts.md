# Encoded video metadata values

## VideoFrameMetadata common fields

`VideoFrameMetadata` owns the actual SDK metadata value. `values` returns an
independent snapshot of its common scalar, optional and ordered list fields;
`set_values` updates those fields without replacing codec-specific storage.
Copy and assignment copy the entire SDK value, not only this snapshot.

Dependencies preserve absence separately from a present empty list. The legacy
`frame_dependencies` query maps absence to an empty list; its setter stores a
present list even when empty. Frame IDs and indices retain their full native
signed domains. Dimensions retain the SDK's full unsigned 16-bit getter/setter
domain despite the SDK's internal signed 16-bit storage.

Frame type and decode-target indication are scoped signed-int SDK enums. Their
metadata wrappers preserve the entire underlying domain. Content type is a
fixed-underlying byte enum and retains all byte values. Rotation and codec are
non-fixed C++ enums: casts are defined only for their representable ranges,
0..=511 and 0..=7 respectively. Binding rejection outside those ranges occurs
before changing any field. Representable unnamed values are not normalized.

These are metadata-value domains, not transmission admission. Native serializers
and stock frame mutation can have narrower consumption contracts. The common
snapshot intentionally does not inspect codec-specific headers.

## Codec-specific header graph

`codec_specifics` copies the complete SDK variant: monostate, VP8, VP9 or H.264.
`set_codec_specifics` replaces that variant without changing codec selection or
common fields. The native metadata setter permits codec/variant disagreement;
the binding preserves it. A consuming frame mutation/packetizer has its own
narrower association and value requirements.

All aggregate members are represented, including three VP9 reference slots,
eight spatial dimension slots and all 255 GOF rows with three reference slots
each. Inactive storage is retained. H.264 NAL order, duplicate values, signed
SPS/PPS IDs and arbitrary byte types are retained. Its scoped packetization mode
accepts the complete signed-int domain. Its non-fixed packetization type accepts
representable values 0..=3, including unnamed 3, and rejects other representations
before mutating metadata.

The header/GOF Rust aggregates are independent complete copies. Their ordinary
`PartialEq` compares physical storage. `sdk_equals` delegates to SDK conditional
equality instead. `VideoFrameMetadata::equals` delegates to the actual owning
SDK value's equality. Native equality consumes only selected prefixes: VP9 SS
fields are ignored without SS data, dimensions are ignored without the
resolution flag, and GOF/reference tails are ignored beyond active counts.

Counts retain their complete native metadata domains. Equality guards only array
reads reached by the SDK's exact short-circuit order. A scalar or earlier element
difference still yields the native false result even if later counts would be
unsafe. Reached overruns produce typed `NativeConsumption` errors. Metadata
construction/snapshots do not apply those consumer-local count restrictions.

Header `init`, GOF `set_structure` and `copy_prefix_from` invoke their native
methods. VP9 Init preserves arrays, resolution flag and GOF. GOF setup preserves
pid_start and unused slots; prefix copy preserves destination pid_start, tails
and unselected reference slots. Unsafe source copy counts are rejected before
changing the target. Temporal modes 0..=3 are representable. Unnamed 3 preserves
the native unchanged result when assertions are disabled; an enabled fatal
DCHECK is reported as `NativeAssertion` before invoking it.

### Initialization proof and integration boundary

Both pinned stock sender population (`call/rtp_payload_params.cc`) and VP9 receive
depacketization (`video_rtp_depacketizer_vp9.cc`) create their variant using
`emplace<RTPVideoHeaderVP9>()` with no arguments. The pinned libc++ construction
value-initializes the aggregate, including its GOF and array tails, before SDK
Init assigns selected defaults. Thus untouched stock tails and GOF pid_start
are initialized. The earlier concern inferred from Init alone does not apply
to these construction paths.

Binding header defaults also value-initialize, then invoke SDK Init. That does
not normalize a supplied stock value. SDK raw structs created through plain
default-initialization can still contain indeterminate members and are not a
safe external ingestion contract. Future transform integration must preserve
stock or fully binding-initialized provenance; it cannot accept arbitrary raw
native values and presume every member is initialized.

Source audits found no actionable defect in the common or header slices.
`tests/video_frame_metadata.rs` covers defaults, option distinction, full
physical storage/copy, native Init/GOF behavior, atomic range/copy rejection and
consumer-specific equality short circuits. `consumer/video_frame_metadata.cc`
adds a shipped-kit construction/copy/GOF initialization regression to C++ smoke.
Both remain unrun. Public frame-transformer installation remains separate from the owning stock
frame interfaces described below; metadata snapshots alone do not establish a
safe live callback-delivery path.

## Complete outer RTP video header

`RtpVideoHeader` owns the actual SDK `RTPVideoHeader`; `values` snapshots every
outer member. Native copy construction and assignment independently retain
generic descriptor vectors, complete codec-specific storage, optional color/HDR,
capture time and instrumentation samples. The value and its owned snapshots have
no execution-role dependency.

`RtpVideoTypeHeader` includes the outer header's legacy-generic picture ID in
addition to monostate, VP8, VP9 and H.264. Codec selection and this variant remain
independent in storage. `RtpVideoGenericDescriptorInfo` retains ordered
dependencies, chain differences, raw decode-target indications, full signed
frame/index domains and all 32 active-target bits. No transmission-count policy
is applied to these metadata snapshots.

`set_values` replaces the entire value. Unrepresentable non-fixed enum values or
codec-specific physical-shape errors are rejected before target mutation. It does
not invoke packetization, reference finding or transport admission.

`as_metadata`, `set_from_metadata` and `from_metadata` invoke the SDK's actual
subset conversions. Only the selected VP8/VP9/H.264 paths require matching
variants, because those paths use `std::get`. Unsupported codecs ignore variant
storage; they do not cause unrelated variant rejection.

The conversion intentionally loses outer-only fields when constructing a fresh
header. Updating an existing header preserves its first/last-packet flags,
playout delay, timing, color, tracking ID, absolute capture time and
instrumentation. Generic descriptor state is reset/recreated by the SDK:
chain differences become empty and active-target bits return to all ones.
An absent frame ID removes the generic descriptor. A present frame ID with
absent metadata dependencies becomes an empty dependency vector, which converts
back to present-empty dependencies. Metadata SSRC/CSRC fields have no storage in
the outer header and are not synthesized into it.

`tests/rtp_video_header.rs` covers full storage, physical tails, independent
copy/assignment, native subset loss/preservation and atomic local rejection.
It is written and unrun. Owning metadata does not establish that arbitrary metadata is safe to submit to
a live frame callback.

Existing encoded audio/video collector time snapshots preserve the native unit
infinity sentinels as `i64::MIN`/`i64::MAX` rather than calling the finite-only
SDK `.us()` conversion on them. Optional absence remains distinct.

## Owning transformable frames

`TransformableMediaFrame` owns an actual stock SDK audio or video frame.
`TransformableAudioFrame` and `TransformableVideoFrame` expose subtype operations
and dereference to the common `TransformableFrame` interface. The safe API does
not construct a replacement frame class or expose the SDK's private Passkey.
The two stock video constructors unconditionally abort at this pin; their Rust
entry points return `UnsupportedVideoConstructor` without entering that path.

Frame payload and metadata ownership are independent of callback-target
ownership. Stock frames contain owning buffers and copied fields. A received
video frame additionally stores its original receiver pointer as provenance;
frame operations and destruction do not dereference it. The native receiver
callback only compares it with its current receiver, whose lifetime is a
separate delivery obligation. Frame owners are move-only `Send`, non-`Sync`.
Reads, exclusive mutation and destruction do not require retaining the original
peer, stream, factory or task queue. This does not permit delivery through an
expired native callback.

`data` loans the native payload until mutation, with Rust borrowing enforcing
that boundary. `set_data` invokes the SDK copying setter. MIME and RID preserve
byte strings and optional presence. Common time queries preserve infinity
sentinels and optional absence. Payload/capture/audio-level setters first check
the stock capability predicate instead of entering its unsupported default
DCHECK. Audio-level clamping and incoming-frame voice-activity effects are the
SDK's own behavior, not binding normalization.

Outgoing audio stores capture time as unsigned milliseconds. Capture-time
mutation invokes the SDK's nearest-millisecond conversion, including small
negative timestamps rounding to zero. It rejects only infinite input, negative
rounded milliseconds and values that cannot safely roundtrip through the native
unsigned `Timestamp::Millis` getter, including that getter's compiled DCHECK
boundary. Guard failure leaves the frame unchanged. The absolute capture query
preserves native uint64 storage and does not reinterpret it as the received
extension's Q32.32 timestamp.

Video metadata mutation checks the reached codec-selected `std::get`, not a
blanket codec/index/count policy. The received-video setter only applies frame
ID/dependencies to its header and does not update its cached `Metadata` value.
Its equality assertion is guarded only when compiled in, including the native
conditional codec-storage reads. The complete independently owned `header`
snapshot therefore remains distinct from `metadata` after receiver mutation.
Success here certifies the local setter, not subsequent packetizer/reference-
finder/serializer consumption. Those live consumers require separate admission.

`sdk_clone` is explicit and delegates to the SDK factory functions. Audio clones
preserve native direction. Video clones always become sender frames, including
receiver inputs, rebuild their header through `FromMetadata`, and use the SDK's
fresh/default capture and presentation fields. Sender video MIME uses its
immutable construction codec, even if the caller later changes header metadata;
a clone instead adopts the current metadata codec. Unknown codec metadata is
retained, with only the fatal MIME name consumption rejected. Capturing an
original frame must not implicitly clone it or change its direction.

The owning-frame API does not by itself install a Rust transformer provider,
register a callback or make held/late frames deliverable. SDK sender/receiver
slots and the existing encoded collectors still require explicit installation
and retirement contracts.

## FrameInstrumentationData

`FrameInstrumentationData` owns an actual pinned SDK value, with no peer, borrowed
frame, callback or execution-role dependency. Native copy construction and
assignment produce independent sample vectors. Rust values support `Send` and
`Sync`; mutation requires exclusive access.

Default sequence index, standard deviation and both error thresholds are zero.
The value is initially non-droppable and its sample vector is empty.

The SDK setters return a boolean without modifying the previous field on
rejection:

- Sequence index accepts 0 through 16383.
- Standard deviation accepts 0 through 40.
- Luma and chroma error thresholds accept 0 through 15.
- Every sample accepts 0 through 255. The SDK imposes no sample-count limit on
  this metadata value.

The floating-point setters use comparison-based SDK validation, which accepts
NaN and preserves signed zero. The binding does not normalize or reject those
SDK-accepted metadata values. This is a metadata contract, not a claim that every
accepted value is safe for all downstream RTP serializers.

`set_sample_values` calls the native span-copy overload.
`set_sample_values_owned` consumes its Rust vector and calls the native
vector-rvalue overload. Both preserve the previous SDK samples on rejection.
`sample_values` returns an independent ordered copy.

`holds_upper_bits` is true exactly when the value is non-droppable and the lower
seven sequence-index bits are zero. `is_sync_only` is true exactly when its
sample vector is empty.

Constructing or mutating this value does not install a corruption-detection
generator, select an RTP extension or cause anything to be transmitted.
