# Standalone stock audio processing

`AudioProcessor::builtin` owns a real builtin WebRTC AudioProcessing module and
retains the exact supplied environment, including its Rust dependencies, until
native module destruction. The module is single-sequence and neither Send nor
Sync. It does not create a peer, activate a physical device or supply a hidden
pump. This standalone API is separate from existing factory/actor capture
processing selection.

## Processing and output

Capture and reverse operations drive the actual i16 and normalized planar-float
SDK overloads. AnalyzeReverseStream consumes render data without returning PCM.
Frame counts use native floor(sample_rate / 100). Caller slices must cover the
entire requested channel-major or interleaved prefix. Extra suffix storage is
never exposed to the SDK. Float channel pointers remain private and synchronous;
no borrowed output or uninitialized storage escapes.

Integer operations use equal input/output layouts with copied source storage
and caller-owned in-place destination. This avoids overlapping memcpy in native
unsupported-format copy branches. Float operations accept independently supplied
native input/output layouts and initialized caller output. No wrapper zeroing or
copy-back invents successful output. Status preserves the exact native code,
including negative warnings. Native error paths may write actual best-effort
silence, copied input or broadcast first-channel data; an error does not promise
an untouched output prefix. Shape/admission failures occur before native entry
and leave destination storage unchanged.

Non-enumerated rates in the native supported range are resampled by the SDK,
not rejected by an artificial native-rate whitelist. Unsupported rates and
channel layouts retain native errors and their associated output behavior.
Negative rates and dimension overflow are refused before native size arithmetic.
Channel counts fit the reached signed native domains. Integer channel counts
also guard stock render mono-downmix accumulation and narrowed divisor hazards.
Normalized float inputs must be finite and in [-1, 1]. Resource exhaustion,
native aborts unrelated to admitted operation inputs, and process-global runtime
mutation are not recoverable allocation or callback errors.

## Settings and safety

`configure_effects` changes echo/noise/gain choices on the existing module using
GetConfig/ApplyConfig; it does not rebuild or substitute a product. Other native
configuration fields remain unchanged. Initialization, current processing rates
and channel counts, output-muted intent, stream delay/key state and analog-level
recommendation invoke the native operations. Delay clamping and warning codes
are SDK behavior; out-of-range analog levels are refused before native gain
assertions.

APM does not read the environment clock on these stock paths, so no additional
clock-range or monotonicity rule is imposed. Trial admission is operation-local:
GC1's reached scanf integer must be representable; AEC numeric parameters must
be safely parsed/narrowed, and the actual adjusted SDK configuration must satisfy
its native validity and suppression mask ordering. The SDK token dispatcher and
private adjustment preflight preserve defaults, ignored tokens, duplicate keys
and percent handling. Preflight never edits trial values or passes a repaired
configuration to the real builtin builder. Unsafe trials return UnsafeFieldTrial
before construction or live ApplyConfig; unrelated effect selections remain
available using the original environment.

This API currently exposes stock construction, effect selection and the named
processing/control operations above. Full AudioProcessing configuration values,
runtime settings/statistics/AEC dump and linear output, processing providers and
builders, and installation of standalone selected products remain outside this
API surface. Existing peer processing selection is not standalone-product
installation or proof of complete pinned AudioProcessing parity.
