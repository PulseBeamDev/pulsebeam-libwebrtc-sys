# DTMF sender contracts

Source-wired against the pinned SDK; matching artifacts, runtime fixtures, documentation compilation and sanitizers remain deferred. This is not a qualification claim.

## Owning access and native calls

`RtpSender::dtmf_sender()` returns `Some(DtmfSender)` for the native audio sender's DTMF proxy and `None` for a native video sender. The handle retains its native sender and its peer/factory origin. It is local to the owner sequence, not a new autonomous executor or a `Send`/`Sync` handle. Repeated getters and clones share one handle through the peer's weak sender-ID cache. The weak cache does not keep abandoned handles alive. IDs originate from native RTP senders within one peer, not caller-chosen cross-peer identities.

The native `GetDtmfSender` proxy marshals capability, insertion, getters and observer registration/unregistration on the signaling thread. The bridge does not call a bare signaling-affine DtmfSender implementation from the Rust owner thread. `CanInsertDtmf`, remaining tones, duration, inter-tone gap and comma delay are forwarded without invented values. Availability of the DTMF object is not negotiated ability to send telephone events.

The insertion methods accept exact `i32` native millisecond values. `insert_tones` invokes the native three-argument overload and its 2000 ms default comma delay; `insert_tones_with_comma_delay` invokes the four-argument overload without replacing caller-supplied values. The pinned implementation accepts duration 40..6000 ms and gaps of at least 30 ms. Its interface comments' stricter 70/50 ms bounds are not the implemented domain. Native-invalid inputs are delegated unchanged and return `Ok(false)` without changing the queued tones or timing. For delay-safe inputs, unavailable capability also returns the native `Ok(false)`.

Successful native insertion replaces the previous queued tone task. Native processing skips unrecognized characters, preserves original case in notifications, treats comma as a delay, and produces an empty-tone completion notification. No timestamp argument or invented timestamp accompanies this API.

## Retained signed-delay proof

For a non-comma recognized tone the pinned asynchronous consumer executes `tone_gap += duration_` in signed `int`. Before forwarding an otherwise native-valid insertion that can produce such a tone, the adapter checks `gap <= INT_MAX - duration`. An unsafe sum returns `Err(DtmfInsertError::DelayOverflow)` before calling native insertion, not a fabricated native `false`. This guard may reject that unsafe insertion even if capability is currently absent: a separate proxy capability probe would not prove capability at the later insertion call.

No upper gap cap is imposed on empty, unrecognized-only or comma-only input because that sum is not consumed. Invalid native duration/gap/comma-delay combinations are rejected by the native function before the asynchronous arithmetic and retain their native `false` result. Accepted parameters and tone input remain immutable for a queued task except when replaced through another guarded insertion. This retains the arithmetic proof at its later asynchronous consumption; checking only a stale mutable Rust snapshot would not.

## Observer ownership and queued events

There is one native nonowning observer per sender. `register_observer(NonZeroUsize)` installs the shared bridge state with an explicit bounded event capacity. A second registration returns false rather than displacing another owning handle's observer. Unregistration and final destruction synchronously use the native signaling proxy before the bridge state can be destroyed. Explicit re-registration clears the previous registration's queue and drop count.

The pinned implementation invokes both native overloads in order for every tone and completion: the modern `(tone, remaining_tones)` callback, then the deprecated `(tone)` callback. Both are queued as distinct native-origin events. `DtmfToneChange::remaining_tones` is `Some`, including an empty string, for the modern callback, and `None` for the legacy callback. This distinction does not infer a missing buffer or pretend two native callbacks are one. A caller wanting only modern notifications can filter on `Some`.

Native callback delivery copies data into the mutex-protected queue. It never invokes Rust user code on the signaling thread. Polling dequeues owning strings. On capacity exhaustion the oldest event is removed and a saturating drop count is incremented. `take_dropped_tone_changes` returns and resets that count. Explicit unregister prevents future native callback entry but does not fabricate completion or erase already-owned events.

## Deferred fixtures

`tests/audio.rs` contains source fixtures for shared observer registration, initial native getters, capability absence, native-invalid input preservation, consumed signed-sum boundaries versus unused large gaps, negotiated minimum native duration/gaps, lowercase/comma/unrecognized-tone behavior, both callback payloads, completion, default comma delay, bounded queue loss and unregister. `tests/video.rs` contains native video DTMF absence. These additions have not been run. Controlled-mode timing, replacement/cancellation, last-owner destruction during pending work and matching core/native sanitizer qualification remain necessary in the later execution phase.
