# Clock contracts and durable bounds

Native authority: the pinned `system_wrappers/include/clock.h`,
`system_wrappers/source/clock.cc`, `rtc_base/time_utils.cc` and
`api/units/timestamp.h`.

## Shared clock ownership

`Clock` is a shared owning handle. Clones and retaining native environments use
the same adapter. Dropping all Rust clock handles does not retire a clock still
owned by a native environment. Reads and provider callbacks may run concurrently
on native engine threads. A `ClockProvider` therefore requires `Send + Sync`.
Callbacks must not wait for or reenter their mutable owner, and provider
destructors must not panic.

`Clock::from_provider` preserves every finite signed current-time result.
Timestamp-to-NTP conversion receives the exact timestamp, including explicit
infinity inputs, and returns all native NTP bits. A callback panic cannot unwind
through native code: failure latches, future reads return the last successful
time (zero before a first success), and future NTP conversion returns invalid
zero. Calls already entered when failure latches may finish.

`SimulatedClock::new` also retains every finite signed starting timestamp and
native atomic wrapping on advancement. Advances landing on an unreadable
infinity sentinel fail without changing time. Negative fractional simulated
timestamp-to-NTP conversion is separately fallible because the SDK's floating
conversion is undefined there. Negative integral seconds and supported era
wrapping remain available.

`Clock::real_time` retains the existing production lease and matched NTP
mapping. The lease requirements concern process-clock isolation and native NTP
calibration. They are not required merely to bound the current-time integer.

## Opt-in enforced bounds

`ClockTimeBounds::new(minimum, maximum)` accepts ordered inclusive
`FiniteTimestamp` endpoints. It rejects reversed endpoints.
`Clock::from_bounded_provider(provider, bounds)` passes every in-range result
through unchanged. An out-of-range result latches observable `has_failed()`
before that result enters native code. Reads then return an in-range fallback.
Before the first success that fallback is zero if in range, otherwise the
endpoint nearest zero. Already-entered callbacks may still publish in-range
results after failure. No callback may publish an out-of-range result.

`SimulatedClock::bounded(initial, bounds)` checks both the starting time and
every wrapped advance. The native adapter checks and mutates under one lock, so
concurrent advancement cannot evade the bound. Rejection leaves time unchanged.
Bounds apply to current time only, not to timestamp-to-NTP conversion inputs.

For example, a bounded simulated clock can be retained in an environment:

```rust
use pulsebeam_webrtc_sys::{
    ClockTimeBounds, Environment, FiniteTimestamp, SimulatedClock, Timestamp,
};

let bounds = ClockTimeBounds::new(
    FiniteTimestamp::from_microseconds(-1_000_000)?,
    FiniteTimestamp::from_microseconds(10_000_000_000)?,
)?;
let clock = SimulatedClock::bounded(Timestamp::zero(), bounds)?;
let environment = Environment::builder().clock_handle(clock.clock()).build()?;
clock.advance_microseconds(1_000)?;
assert_eq!(environment.clock_time_bounds(), bounds);
# Ok::<(), Box<dyn std::error::Error>>(())
```

`Clock::time_bounds()`, `ManualClock::time_bounds()` and
`Environment::clock_time_bounds()` report durable bounds, not a sampled time.
Manual clocks retain their existing `[0, MAX_CONTROLLED_TIME]` range.
Environment copies retain the selected adapter bounds.
The default SDK environment uses the real-time singleton: its current time is
`TimeNanos() / 1000`, with a signed `int64` numerator even when a process
testing clock is selected. Its bounds are therefore
`[INT64_MIN / 1000, INT64_MAX / 1000]`. Existing unrestricted custom and
simulated clocks report the entire finite domain instead.

## Opt-in enforced nondecreasing time

`Clock::from_monotonic_bounded_provider(provider, bounds)` additionally enforces
nondecreasing current time. The first successful time can be negative; it is
not compared with the initial fallback. A backward or out-of-range result, or a
callback panic, latches `has_failed()` before that result reaches native code.
Current-time publications and failure from NTP conversion serialize under a
shared lock. An already-entered current-time callback cannot replace the
latched fallback after failure, including the race before a first negative
success. NTP conversion retains its separate input/output semantics.

`Clock::is_monotonic()` and `Environment::clock_is_monotonic()` expose durable
adapter evidence, not a sampled-time assertion. Environment evidence is checked
against the retained native clock's identity. Manual clocks and leased
`Clock::real_time` have this evidence. Existing provider constructors, rewindable
simulated clocks, and the default SDK environment without a production lease do
not gain it implicitly.

The ordinary Opus packet-loss callback subtracts consecutive native
microsecond timestamps. Its owning product therefore requires nondecreasing
time and a complete bounds width strictly less than `INT64_MAX`. The strict
bound also excludes a resulting native infinity sentinel. A current-time sample
cannot substitute for this retaining proof. Other selected encoders whose
callback is the native no-op do not need that restriction.

## Stock NetEq receive-time proof, implementation in progress

NetEq's queued receive metadata can outlive the insertion call. Checking one
current-time sample would not protect later extraction, especially if another
thread advances the clock. The stock-only Rust proof uses durable bounds
`[L, U]`: for finite receive timestamp `R`, it widens to `i128` and requires

```text
INT64_MIN < L - R
U - R < INT64_MAX
```

These endpoint checks prove that every later native subtraction is finite and
does not overflow or produce an infinity sentinel. Minus infinity means missing
receive time and bypasses the native statistics calculation. Plus infinity is
not safe for that stock calculation. Shared timestamp values and packet-info
round trips still retain both infinities.

Ordinary timestamps pass for the SDK default and manual clock ranges. An
unrestricted caller clock only proves zero or missing receive time; opt-in
enforced bounds allow a broader metadata range. The proof never replaces a
timestamp, removes packet metadata, freezes a clock or patches the SDK.

The Rust NetEq product remains an unpublished draft until its native constructor
and decoder/context guards are connected. These bounds do not establish safety
for every other native consumer or promise unrestricted arithmetic throughout
libwebrtc.

## Qualification status

`tests/clock_bounds.rs`, `tests/monotonic_clock.rs`, and the `src/neteq_safety.rs` unit tests contain bounds,
failure, ownership, concurrent access and subtraction-boundary expectations.
Existing unrestricted-clock tests remain unchanged. Source review is not test
execution. Builds, runtime tests and sanitizer qualification remain deferred.
