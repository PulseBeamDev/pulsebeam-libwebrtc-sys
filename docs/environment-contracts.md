# Environment and field-trial contracts

Native authority: pinned WebRTC `api/environment/environment{,_factory}.h`, `api/field_trials{,_view}.h`, and `api/environment/deprecated_global_field_trials.h`.

## Owned values and providers

`FieldTrials::parse` validates native slash-delimited configuration with the pinned parser. Empty configurations and repeated identical entries are valid; incomplete separators, empty names/groups, and conflicting duplicates are errors. `set` rejects empty names and slash-containing names/groups. An empty group removes an entry. `merge` gives the other values precedence. Invalid edits leave values unchanged.

Trial names and groups are open strings. No binding allowlist is imposed, including for unregistered application keys. Native meanings are version-specific and not stable policy APIs. Missing keys return the native empty string. Enabled/disabled queries use the native case-sensitive `Enabled`/`Disabled` prefixes, including groups with parameter suffixes.

`Environment::builder().field_trials(values_or_provider)` takes owned values or a `FieldTrialsView` implementation. An `Arc` provider can be shared when needed. Registered providers are immutable and must implement `Send + Sync`; their lookups can run concurrently on any native engine thread. `create_copy` must produce an independently owned immutable view with the same mapping. It may share immutable storage, but must not borrow application stack buffers or need reentrant access to a mutable actor session. Lookups are synchronous and should not block.

The native adapter owns the Rust provider and implements both native virtual operations, including `CreateCopy`. Panics cannot unwind into native frames: lookup panics produce an empty group, and copy panics produce an empty provider. This is failure containment, not a successful provider result or a recommended way to reject a query. There is no fallible native lookup result.

Adapter-owned provider destruction and task cancellation use the shared
[provider panic containment policy](provider-panic-contracts.md), including safe
handling of unknown panic payloads. Ordinary string panics are recommended.

## Native event subtype codecs

`RtcEvent::encode_batch` calls the concrete SDK subtype encoder and returns its
binary outer event tag, byte length and payload. It borrows input events only
for that synchronous call. Type mismatches are rejected where the native field
extractors would abort. End-log encoding only reads common timestamps, and the
SDK's stub encoders retain their empty-output behavior.

`RtcEventParsedRecords::parse_payload` takes the payload without the outer tag
or byte length, plus the native batched flag. Its owning vectors or SSRC-keyed
map append native records, including partial records when the returned
`RtcEventLogParseStatus` is false. Existing output prefixes remain intact.
Timestamp infinities in partial records are preserved by `Timestamp`; finite
queries on those values return a time-value error instead of entering a native
finite-only accessor. Parsed timestamps retain native millisecond precision.
The begin-log UTC value is available on `LoggedStartEvent`, not on the unencoded
`RtcEventBeginLog` payload, whose UTC field has no public getter.

The pinned SDK implements begin/end log, ALR state, audio playout and delay-based
BWE subtype codecs. The other declared subtype codecs return empty encoded bytes
and native `Not Implemented` parse statuses; they do not produce parsed records.
NetEq minimum-delay events have no public subtype codec. These limitations do
not affect installed native event logging or the owning unencoded event APIs.
Decoded milliseconds that would overflow the SDK's microsecond conversion are
rejected before subtype parsing, without changing output or fabricating a native
parse status. Other safe parse failures retain the actual SDK status and partial
output.

## Reusable native factory composition

`EnvironmentFactory::new()` owns an initially empty native factory. Each
`create()` resolves absent utilities through the SDK, including a fresh default
task-queue factory, rather than copying a cached default environment.
`from_environment(&environment)` uses the SDK's environment-derived constructor
and preserves the selected clock, queue factory and logger. Factory clones share
installed utilities, while later setters affect only that factory's branch.

`set_field_trials`, `set_clock`, `set_clock_handle`, `set_task_queue_factory` and
`set_event_log` install owning typed inputs. Replacing a selection does not
prematurely release dependencies of earlier utilities still owned by SDK storage.
`set_event_log_product` consumes an existing `RtcEventLogHandle` without
recreating or stopping its native logger. It can transfer an active or pending
product: existing stop observers and installed SDK/utility aliases coordinate
through the same admission state. Stock controls bind on the first accepted
control sequence and retain that affinity after transfer; log delivery remains
thread-safe. The SDK kill-switch null product requires no production executor
and is valid with cooperative queues.

`set_randomness` retains the explicit Rust installation without reseeding it.
Created environments and copied trial snapshots retain the SDK storage and Rust
input branch through native destruction, independently of factory close. A
selected randomness lease also follows the shared logger through off-queue stock
retirement, even if that logger was created before the lease was selected. This
lease guard contains no environment back-reference. The lifetime root delegates trial lookup and `CreateCopy` to the actual native view;
it does not synthesize trial values or replace the selected clock, queues or log.

Factory `create` validates cooperative creator affinity and requires an explicitly
selected matching clock. It does not infer a clock from queues. The convenience
`Environment::builder()` retains its existing cooperative-clock inference.
Creating an environment adds no queue, executor or clock advancement. Queue
products still require explicit creation. `close` releases only that factory;
subsequent create/set calls report `Closed`.

`ProductionSession::builder(config).environment_factory(&factory)` snapshots
these owning selections before native actor construction. Individual actor
clock, queue, trial, logger and randomness selections override the snapshot.
Cooperative or unqualified queues cannot enter the production actor. Factory
selection and its native creation contexts remain valid after actor migration.

## Environment access and lifetime

Without an override, native environment defaults are retained. The default deprecated process-global trial source is not replaced with an empty map. Its native copies continue to use the pinned process-global lookup semantics and registration checks. The binding does not add a process-global setter.

`Environment::field_trials()` and `PeerConnectionFactory::field_trials()` return owned `FieldTrialsSnapshot` views. Copies call the actual native view's `CreateCopy`, not a cached Rust configuration echo. Snapshots are `Send + Sync`, retain their provider dependencies, and can outlive the environment or factory. Custom provider registrations last through all retaining environment/factory copies; snapshots own their own copies.

`ProductionSession::new_with_field_trials` configures the ordinary actor constructor before factory creation; `field_trials` reads its effective environment after actor migration. Existing constructors and configuration literals are unchanged. The corresponding L1T3 and simulcast constructors also have field-trial variants, preserving their existing encoded-input validation and setup choices. Actor shutdown rejects further session queries, but previously returned independent snapshots remain readable.

`Environment::close()` releases the native environment first, then that handle's
Rust clock/queue and randomness leases. Repeated close is harmless. Independent
environment copies and retaining products keep their own dependencies until
closed or finally retired; closing an original handle does not revoke them.

`RandomnessLease` clones retain the same exclusive process-global installation,
without reseeding it. `ProductionSession::builder(config).randomness(&lease)`
retains that installation in the actual factory environment. It remains alive
through native creation callbacks, actor migration and independently retained
creation contexts or utility views. Dropping the original lease does not revoke
those owners. This selection does not add a controlled clock or scheduler.

Field-trial configuration also works with manual-clock cooperative environments. It adds no task queues, autonomous threads, sockets, or time advancement. Provider lookup work still has the provider's own execution contract; a caller provider must not introduce autonomous work when used in controlled execution.

`Environment::clock()` returns an owning `Clock` delegate to the exact SDK
utility. It retains the SDK storage and Rust-only leases independently of the
environment handle, and can itself be installed into another environment.
Matching native identity preserves selected provider failure diagnostics, bounds
and monotonicity evidence, including in actual provider creation contexts. A
closed handle returns `EnvironmentClockError::Closed`. The SDK's default system
clock acquires a production lease for its safe NTP mapping and rejects testing
clock coexistence; manual/custom clocks do not gain autonomous progress.
SDK copies are not a universal ownership proof for arbitrary raw-pointer `Set`
utilities. The exposed construction paths install owning adapters, and the pinned
peer factory transfers each selected logger into its environment storage.

`Environment::task_queue_factory()` returns an owning `TaskQueueFactory` delegate
to the exact SDK utility, not a clone of the registration substituted for the
utility. It retains SDK storage and Rust-only leases through factory and queue
retirement, and can itself be installed into another environment. Reinstallation
delegates to the original utility and does not fork its scheduler. Public queues
remain caller-owned: native lifecycle yields do not run their pending tasks.
Explicit pumps preserve the selected scheduler, deadlines, current-queue identity,
provider failure diagnostics and original cooperative creator sequence, even if
the utility handle was obtained on another thread. No clock advancement, worker
or real-time wait is added for cooperative utilities. Production utility handles
reserve production execution through final SDK/Rust resource retirement, including
reinstalled SDK factory copies. Non-cooperative public queues also reserve it
through native queue deletion even when created from a bare registration. Neither
can be admitted while controlled process hooks are installed; utility acquisition
reports `ProductionRuntimeUnavailable`, and bare queue creation fails native
construction before invoking a provider. A closed handle returns
`BuildEnvironmentError::Closed`. Internally imported SDK contexts must match their
selected utility identity before inheriting scheduler capabilities; a context
without that ownership proof returns `UnqualifiedEnvironment`, rather than
assuming a cooperative or production backend. All public construction and
matching provider-creation paths supply this proof.

`Environment::event_log()` returns an owning view of the exact installed SDK
logger. SDK aliases coordinate asynchronous stop and dependent-resource
retirement as described in [event-log contracts](event-log-contracts.md#exact-environment-utility).

Clock ownership, current-time bounds, and opt-in bounded provider/simulated-clock
behavior are documented in [clock contracts](clock-contracts.md). Environment
bounds are durable across copies, not obtained by sampling the clock.
