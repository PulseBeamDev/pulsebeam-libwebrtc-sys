# Pinned native SDK inventory and mapping

The native SDK scope is defined by `native-api.md` in the checkout pinned by
`webrtc_commit` in `Justfile`. It is **not** the subset of APIs already bound by
Rust, the headers copied into an artifact, or a browser-oriented feature list.

`tools/sdk_coverage.py` reads both tables in that document: `api/` is recursive;
the listed legacy directories are not. New headers in those directories enter
the inventory without an accompanying Rust change. Existing Linux camera and
desktop-capture capabilities outside those directories are retained separately
in [`native-sdk-mapping.json`](native-sdk-mapping.json), with their real native
consumer roots. This does not turn every implementation header under the capture
modules into an independent binding requirement.

## Inventory boundaries

The tool uses the pinned Clang ExtractAPI frontend, not a regular-expression
parser for C++. Each active declaration has a native source location, header
SHA-256, Clang USR, declaration fragments and referenced native type identities.
Overloads have different identities. Public fields, enumeration cases, aliases,
templates, inline functions and macros are retained. Private/protected members
and their descendants are excluded, as are SDK `internal`/`impl` names in either
snake_case or CamelCase. Names such as `SimpleStringBuilder` are not excluded by
substring matching.

Test-only header exclusion uses explicit GN `sources`/`public` ownership and
`testonly` metadata. A non-test owner wins over a test owner. A header with no
owner is **not** assumed test-only. In particular, some headers under `api/test`
have non-test consumers. The metadata graph enables test targets so that fixtures
hidden by `rtc_include_tests=false` can still be classified; it does not change
the qualified core/native artifact configurations.

Other header dispositions require concrete source evidence. For example, the
SDK explicitly rejects `rtc_base/win32.h` on non-Windows builds, and builds the
legacy JNI utilities only under `is_android`. A missing include, generated
header, object dependency, disabled build option or wrapper is not evidence of
upstream unavailability. Parser failures remain unresolved headers and make the
inventory command fail after retaining the diagnostics.

Both Linux x86_64 artifact profiles are extracted using their actual public
C++ definitions, native configuration identities, sysroot and C++ headers.
Header-owner include directories come from GN. Declaration presence establishes
neither an implementation in the archive nor correct provider installation.

ExtractAPI describes active preprocessor configurations. The inventory also
retains all header conditional-directive locations and the configuration macros
documented by `native-api.md`. An inactive branch or a missing declaration must
not be silently interpreted as an upstream-unavailable feature. Configuration
capabilities and necessary transitive value/interface types require review in
addition to the active-declaration list. References to external or specialized
native types remain visible in the mapping report; template instantiations are
not automatically separate binding products. Third-party APIs are not independent
products, but a necessary native consumer input cannot be discarded as third-party.

## Generating the inventory

Use a pinned, configured Linux x86_64 work directory with both exported flavors.
Create a separate GN metadata directory from the core configuration, enabling
`rtc_include_tests` and `rtc_include_internal_audio_device` there. The latter is
needed by upstream test targets that depend on `create_audio_device_module`.
Keep the artifact build directories unchanged.

For the default `.work` layout:

```sh
mkdir -p .work/sdk-coverage/metadata
cp .work/out/core/linux-x86_64/args.gn .work/sdk-coverage/metadata/args.gn
# In the copied args.gn only:
#   rtc_include_tests = true
#   rtc_include_internal_audio_device = true
(cd .work/checkout/src && buildtools/linux64/gn gen \
  "$OLDPWD/.work/sdk-coverage/metadata")

# Public network-emulation and neural-echo creator headers consume these
# generated types/build flags. This does not add their implementations to
# the exported artifact's link closure.
for flavor in core native; do
  .work/checkout/src/third_party/ninja/ninja \
    -C ".work/out/$flavor/linux-x86_64" \
    gen/api/test/network_emulation/network_config_schedule.pb.h \
    gen/third_party/tflite/buildflags.h
done

python3 tools/sdk_coverage.py inventory \
  --metadata-build .work/sdk-coverage/metadata \
  --output .work/sdk-coverage/inventory.json
python3 tools/sdk_coverage.py check \
  --inventory .work/sdk-coverage/inventory.json \
  --output .work/sdk-coverage/mapping-report.json
```

`--work` (or `WEBRTC_WORK`) relocates the standard work layout. `--jobs` bounds
concurrent frontend processes. Inventory and report outputs belong in the work
directory, not in the source tree. They can be regenerated from the pinned source,
matching artifacts, GN metadata and the durable mapping.

## Mapping rules

`native-sdk-mapping.json` uses exact declaration identities, not broad wildcards
that could silently cover a newly added overload. Each declaration entry records:

- the `core`/`native` profiles in which it is declared;
- its Rust entry points and actual native installation/consumer paths;
- distinct native/default/ownership/error semantics;
- focused verification source anchors;
- `implemented` source wiring or an explicitly `partial` mapping with remaining gaps.

Source anchors must match exactly once and remain relative to either this
repository or the pinned SDK. Unknown/duplicate mappings, changed native header
bytes, missing consumers, stale anchors and wrong profile claims fail validation.
The report always lists unmapped declarations. A partial typed-track wrapper does
not satisfy an arbitrary native `MediaStreamTrackInterface` overload.

`configuration_mappings` accounts for the compile-time configuration capabilities
listed by `native-api.md`; it requires the same Rust, installation and verification
paths. Non-Linux header dispositions retain explicit source evidence. Existing
capture capabilities have separate source/consumer/test references rather than
being silently dropped because their roots are outside the SDK directory tables.

`check --require-mapped` additionally rejects unmapped or partial active
declarations and unaccounted configuration macros. It is a structural mapping
gate, **not** proof of functional SDK coverage. A valid source anchor or a test
file's existence does not prove execution, callback fidelity, provider consumption,
artifact implementation, ownership safety, deterministic progress, or successful
runtime qualification. Those require the corresponding matching native-boundary
and public Rust consumer checks. The focused [capability matrix](capability-matrix.md)
is supplementary evidence, not the complete declaration denominator.
