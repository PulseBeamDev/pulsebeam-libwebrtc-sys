# Provider panic containment

With `panic=unwind`, native provider adapters contain a single user callback or
adapter-owned provider destruction panic. This covers clocks, trials, task queues,
network monitors, local-network permission, certificate generation, TLS
verification, DNS, event logs, network control/prediction, FEC and video bitrate
allocation. It does not turn a rejected callback into a successful operation or
select a stock replacement. Each adapter retains its documented failure latch,
rejection, or native-required failure value. Cleanup remains attempted.

The private shared containment helper frees known `String` and `&'static str`
panic payloads and intentionally forgets unknown payloads. Retrying the drop of
an unknown payload inside another catch is insufficient: its destructor and field
cleanup can both panic, aborting before that catch can recover. Arbitrary user
panic payloads and resources captured by them are therefore not guaranteed to be
released. Applications should use ordinary string panics and non-panicking
provider destructors.

Generated-CXX Rust owners guard final provider release, not just operational
callbacks such as queue deletion or monitor stop. Cancelled Rust task captures
are also released under a guard. This prevents a single user destructor panic
from unwinding through the native Rust-box drop trampoline. A caller's unrelated
last `Arc` drop remains ordinary application-owned Rust destruction.

As in ordinary Rust, this cannot contain `panic=abort`, a process-aborting panic
hook, or a double panic during a user's own destructor/field cleanup. Containment
is not a promise to recover arbitrary internally aborting user code.

Public runtime regressions exercise hostile payloads through native clock/trial
calls, native task execution and cancellation, migrated monitor/permission
teardown, pending certificate generation, live controlled DNS products/results,
and the selected verifier in a native TURN/TLS handshake. Normal lifecycle cases
remain separate. These tests do not replace matching-runtime sanitizer evidence
for the complete provider slice.
