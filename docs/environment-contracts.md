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

## Environment access and lifetime

Without an override, native environment defaults are retained. The default deprecated process-global trial source is not replaced with an empty map. Its native copies continue to use the pinned process-global lookup semantics and registration checks. The binding does not add a process-global setter.

`Environment::field_trials()` and `PeerConnectionFactory::field_trials()` return owned `FieldTrialsSnapshot` views. Copies call the actual native view's `CreateCopy`, not a cached Rust configuration echo. Snapshots are `Send + Sync`, retain their provider dependencies, and can outlive the environment or factory. Custom provider registrations last through all retaining environment/factory copies; snapshots own their own copies.

`ProductionSession::new_with_field_trials` configures the ordinary actor constructor before factory creation; `field_trials` reads its effective environment after actor migration. Existing constructors and configuration literals are unchanged. The corresponding L1T3 and simulcast constructors also have field-trial variants, preserving their existing encoded-input validation and setup choices. Actor shutdown rejects further session queries, but previously returned independent snapshots remain readable.

`Environment::close()` releases the native environment first, then that handle's
Rust clock/queue and randomness leases. Repeated close is harmless. Independent
environment copies and retaining products keep their own dependencies until
closed or finally retired; closing an original handle does not revoke them.

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

Clock ownership, current-time bounds, and opt-in bounded provider/simulated-clock
behavior are documented in [clock contracts](clock-contracts.md). Environment
bounds are durable across copies, not obtained by sampling the clock.

## Evidence and remaining work

`tests/field_trials.rs` covers native parser validation, open values, edit/merge semantics, native prefix queries, concurrent owned snapshots, custom copy/retention/drop behavior, panic containment, actor migration/shutdown, and changed native crypto selection under cooperative execution with unchanged native thread count. The native `WebRTC-EnableDtlsPqc` trial changes `KeyExchangeGroups::update` output, demonstrating a real consumer effect rather than setter readback.

`tests/environment_task_queue.rs` covers SDK factory delegates, retained provider
and randomness resources, byte-exact names and priorities, provider rejection,
reinstallation, production admission/exclusion and final release, original
cooperative creator identity and shared deadlines,
explicit pumping, lifecycle-yield isolation, default threaded execution, and an
actual installed peer event-log creation context. The execution unit regression
also exercises self-reposting public work across an explicit native lifecycle
yield through a reinstalled utility. Matching Linux x86-64 qualification passed:
75 integration tests and 5 execution unit tests on core, 45 integration tests on
native, and 50 integration tests plus 5 execution unit tests under ASan with leak
detection. These gates qualify the named utility/lifecycle paths, not the full
provider slice.

Event-log utilities and caller factories are now in-progress adapter work, documented in `docs/event-log-contracts.md`. They are not yet qualified. Remaining creation/provider dependencies and full environment/application-facing parity are incomplete.
