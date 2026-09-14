set shell := ["bash", "-euo", "pipefail", "-c"]
set windows-shell := ["C:/Program Files/Git/bin/bash.exe", "-euo", "pipefail", "-c"]

root := justfile_directory()
work := env_var_or_default("WEBRTC_WORK", root + "/.work")
dist := env_var_or_default("WEBRTC_DIST", root + "/dist")

# Upgrade WebRTC by changing this commit. Change depot_tools only when required.
webrtc_url := "https://github.com/webrtc-sdk/webrtc.git"
webrtc_commit := "ba469aa2093ba950066258ca0a59a6fbd1295582"
webrtc_core_ios_patch := root + "/patches/core-ios-remove-framework-objc.patch"
webrtc_core_ios_patch_sha256 := "c05d3e629c6c59f621e0c89be1a26fce31ee6625a9763d4a3d9f492f80454037"
webrtc_core_ios_build_gn_sha256 := "ed533d68269e01a70f7c40567099cd9f7fd5c52c66ea8f8ef609d52c966c9a69"
depot_tools_url := "https://chromium.googlesource.com/chromium/tools/depot_tools.git"
depot_tools_commit := "ed9c87f6f12f6b87210e7025d4a36a5a72a2ccd4"
default: check

# Fast, offline validation of the complete repository-owned control plane.
check:
    #!/usr/bin/env bash
    set -euo pipefail
    cd "{{ root }}"
    test "$(just --list --unsorted | sed -n 's/^    \([^ _][^ ]*\).*/\1/p' | grep -v '^default$' | sort)" = $'build\ncheck\nrefresh-cxx\nverify-artifact'
    grep -Fq '  source-pin-adapter-check:' .github/workflows/upgrade-rehearsal.yml
    test "$(find consumer -type f | wc -l)" -eq 3
    grep -Fq 'rtc_use_h264=false' Justfile
    grep -Fq 'rtc_build_libvpx=true' Justfile
    grep -Fq 'rtc_include_dav1d_in_internal_decoder_factory=true' Justfile
    grep -Eq '^set windows-shell := \["C:/Program Files/Git/bin/bash\.exe"' Justfile
    grep -Fq 'install-sysroot.py" --arch=arm64' Justfile
    grep -Eq '^[[:space:]]+while IFS= read -r root_label; do roots\+=' Justfile
    grep -Fq 'CreateModularPeerConnectionFactory' consumer/smoke.cc
    grep -Fq 'SetRandomGenerator' consumer/smoke.cc
    python3 tools/cxx_import.py verify
    python3 -m unittest tests/test_cxx_import.py
    python3 -m unittest tests/test_justfile_input_retrieval.py
    python3 -m unittest tests/test_cross_target_runtime_policy.py
    python3 -m unittest tests/test_linux_asan_static_closure.py
    python3 -m unittest tests/test_cxx_provenance.py
    python3 -m unittest tests/test_consumer_metadata.py
    python3 -m unittest tests/test_artifact_lock.py
    python3 -m unittest tests/test_release_audit.py
    python3 -m unittest tests/test_tracked_payloads.py
    python3 tools/check_tracked_payloads.py
    python3 tools/write_artifact_manifest.py --help >/dev/null
    python3 tools/write_artifact_lock.py --help >/dev/null
    python3 -m tools.audit_release --help >/dev/null
    ! grep -E '^[[:space:]]*(- )?uses:' .github/workflows/*.yml | grep -Ev '@[0-9a-f]{40}([[:space:]#]|$)'
    cargo fmt --check
    CARGO_HOME="{{ work }}/cargo-home" PULSEBEAM_WEBRTC_SYS_SKIP_LINK=1 cargo test --doc --locked --offline
    CARGO_HOME="{{ work }}/cargo-home" PULSEBEAM_WEBRTC_SYS_SKIP_LINK=1 cargo test --test artifact_support --locked --offline
    echo 'source/control-plane check passed; native proof requires: just verify-artifact <archive> <sha256>'
    git diff --check

# Verify one untracked, checksum-pinned core Linux x86_64 artifact with all native proofs.
verify-artifact archive='' sha256='':
    #!/usr/bin/env bash
    set -euo pipefail
    cd "{{ root }}"
    test -n "{{ archive }}" || { echo 'core linux-x86_64 artifact path is required' >&2; exit 1; }
    test -n "{{ sha256 }}" || { echo 'core linux-x86_64 expected SHA-256 is required' >&2; exit 1; }
    [[ "{{ sha256 }}" =~ ^[0-9a-f]{64}$ ]] || { echo "core linux-x86_64 expected SHA-256 is invalid: {{ sha256 }}" >&2; exit 1; }
    test -f "{{ archive }}" || { echo "missing core linux-x86_64 artifact: {{ archive }}" >&2; exit 1; }
    test ! -L "{{ archive }}" || { echo "core linux-x86_64 artifact must be a regular file: {{ archive }}" >&2; exit 1; }
    relative=$(realpath --relative-to="$PWD" "{{ archive }}")
    ! git ls-files --error-unmatch -- "$relative" >/dev/null 2>&1 || { echo "core linux-x86_64 artifact is tracked and rejected: $relative" >&2; exit 1; }
    actual=$(sha256sum "{{ archive }}" | awk '{print $1}')
    test "$actual" = "{{ sha256 }}" || { echo "core linux-x86_64 checksum mismatch: expected={{ sha256 }} actual=$actual" >&2; exit 1; }
    snapshot=$(mktemp); trap 'rm -f "$snapshot"' EXIT
    cp -- "{{ archive }}" "$snapshot"
    just --justfile "{{ root }}/Justfile" _runtime-test core linux-x86_64 "$snapshot"
    CARGO_HOME="{{ work }}/cargo-home" python3 tools/rust_only_consumer.py --artifact "$snapshot" --sha256 "{{ sha256 }}"

# Download, checksum, and mechanically refresh the pinned Rust-only CXX import.
refresh-cxx:
    python3 "{{ root }}/tools/cxx_import.py" refresh

# Synchronize, build, export, archive, and compile/link-check one complete crate artifact.
build flavor target:
    #!/usr/bin/env bash
    set -euo pipefail
    justfile="{{ root }}/Justfile"
    just --justfile "$justfile" _validate-flavor "{{ flavor }}"
    just --justfile "$justfile" _validate-target "{{ target }}"
    just --justfile "$justfile" _validate-host "{{ target }}"
    just --justfile "$justfile" _prerequisites "{{ target }}"
    just --justfile "$justfile" _sync "{{ flavor }}" "{{ target }}"
    just --justfile "$justfile" _target-dependencies "{{ target }}"
    just --justfile "$justfile" _configure "{{ flavor }}" "{{ target }}"
    just --justfile "$justfile" _apple-host-protoc "{{ flavor }}" "{{ target }}"
    just --justfile "$justfile" _compile "{{ flavor }}" "{{ target }}"
    just --justfile "$justfile" _export "{{ flavor }}" "{{ target }}"

_validate-flavor flavor:
    @case "{{ flavor }}" in core|native) ;; *) echo "unsupported flavor: {{ flavor }}" >&2; exit 1;; esac

_validate-target target:
    @case "{{ target }}" in linux-x86_64|linux-arm64|windows-x86_64|macos-x86_64|macos-arm64|android-x86_64|android-arm64-v8a|ios-arm64|ios-simulator-arm64) ;; *) echo "unsupported target: {{ target }}" >&2; exit 1;; esac

_validate-host target:
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{ target }}:$(uname -s)" in
      linux-*:Linux|windows-*:*MINGW*|windows-*:*MSYS*|windows-*:*CYGWIN*|macos-*:Darwin|ios-*:Darwin|android-*:Linux|android-*:Darwin) ;;
      *) echo "unsupported build host for {{ target }}" >&2; exit 1 ;;
    esac

_prerequisites target:
    #!/usr/bin/env bash
    set -euo pipefail
    for tool in git tar python3; do
      command -v "$tool" >/dev/null || { echo "missing prerequisite: $tool" >&2; exit 1; }
    done
    command -v cargo >/dev/null || { echo 'missing prerequisite: cargo' >&2; exit 1; }
    case "{{ target }}" in
      windows-*)
        command -v cl.exe >/dev/null || { echo 'Windows builds require an MSVC developer shell' >&2; exit 1; }
        command -v cmd.exe >/dev/null || { echo 'Windows builds require cmd.exe' >&2; exit 1; }
        command -v cygpath >/dev/null || { echo 'Windows builds require Git Bash cygpath' >&2; exit 1; }
        ;;
      macos-*|ios-*) command -v xcrun >/dev/null || { echo 'Apple builds require Xcode command-line tools' >&2; exit 1; } ;;
    esac

# Git Bash owns checkout manipulation. Convert only when handing a path to a
# native Windows executable, so a workspace name containing spaces stays data.
_native-path path:
    #!/usr/bin/env bash
    set -euo pipefail
    case "$(uname -s)" in *MINGW*|*MSYS*|*CYGWIN*) cygpath --absolute --windows "{{ path }}";; *) printf '%s\n' "{{ path }}";; esac

_source-state flavor target:
    #!/usr/bin/env bash
    set -euo pipefail
    src="{{ work }}/checkout/src"; patch="{{ webrtc_core_ios_patch }}"; expected=pristine
    case "{{ flavor }}:{{ target }}" in core:ios-arm64|core:ios-simulator-arm64) expected=applied;; esac
    test "$(git -C "$src" rev-parse HEAD)" = "{{ webrtc_commit }}" || { echo "source state failed: target={{ target }} flavor={{ flavor }} invariant=revision expected={{ webrtc_commit }}" >&2; exit 1; }
    test -f "$patch" || { echo "source state failed: target={{ target }} flavor={{ flavor }} invariant=patch expected={{ webrtc_core_ios_patch_sha256 }} actual=missing" >&2; exit 1; }
    actual_patch=$(shasum -a 256 "$patch" | awk '{print $1}')
    test "$actual_patch" = "{{ webrtc_core_ios_patch_sha256 }}" || { echo "source state failed: target={{ target }} flavor={{ flavor }} invariant=patch-digest expected={{ webrtc_core_ios_patch_sha256 }} actual=$actual_patch" >&2; exit 1; }
    test -z "$(git -C "$src" ls-files --others --exclude-standard)" || { echo "source state failed: target={{ target }} flavor={{ flavor }} invariant=checkout-cleanliness expected=$expected actual=untracked-files" >&2; exit 1; }
    actual_build=$(shasum -a 256 "$src/BUILD.gn" | awk '{print $1}')
    if test "$actual_build" = "{{ webrtc_core_ios_build_gn_sha256 }}"; then
      test "$(git -C "$src" diff --name-only)" = BUILD.gn && git -C "$src" diff --cached --quiet || { echo "source state failed: target={{ target }} flavor={{ flavor }} invariant=checkout-cleanliness expected=$expected actual=unexpected-modification" >&2; exit 1; }
      actual=applied
    else
      git -C "$src" diff --quiet && git -C "$src" diff --cached --quiet || { echo "source state failed: target={{ target }} flavor={{ flavor }} invariant=checkout-cleanliness expected=$expected actual=unexpected-modification" >&2; exit 1; }
      git -C "$src" apply --check "$patch" || { echo "source state failed: target={{ target }} flavor={{ flavor }} invariant=patch-applicability expected=$expected actual=unexpected" >&2; exit 1; }
      actual=pristine
    fi
    if test "$actual" = "$expected"; then exit 0; fi
    rm -rf "{{ work }}/package/webrtc-{{ flavor }}-{{ target }}"
    rm -f "{{ dist }}/webrtc-{{ flavor }}-{{ target }}.tar.gz"
    if test "$actual" = pristine; then git -C "$src" apply "$patch"; else git -C "$src" apply --reverse "$patch"; fi
    just --justfile "{{ root }}/Justfile" _source-state "{{ flavor }}" "{{ target }}"

_sync flavor target:
    #!/usr/bin/env bash
    set -euo pipefail
    depot="{{ work }}/depot_tools"; checkout="{{ work }}/checkout"; src="$checkout/src"
    retry_acquisition() {
      local input="$1" pin="$2" attempt; shift 2
      for attempt in 1 2 3; do
        if "$@"; then return 0; fi
        if test "$attempt" -lt 3; then
          echo "retrying pinned input=$input pin=$pin attempt=$attempt/3 failure=upstream-acquisition" >&2
          sleep "$attempt"
        fi
      done
      echo "pinned input retrieval failed input=$input pin=$pin attempts=3 failure=upstream-acquisition-exhausted" >&2
      return 1
    }
    target_os=''; sync_key=desktop; case "{{ target }}" in android-*) target_os="target_os = ['android']"; sync_key=android;; ios-*) target_os="target_os = ['ios']"; sync_key=ios;; esac
    mkdir -p "{{ work }}"
    if test ! -d "$depot/.git"; then git init -q "$depot"; git -C "$depot" remote add origin "{{ depot_tools_url }}"; fi
    if test "$(git -C "$depot" rev-parse HEAD 2>/dev/null || true)" != "{{ depot_tools_commit }}"; then
      retry_acquisition depot_tools "{{ depot_tools_commit }}" git -C "$depot" fetch --depth=1 origin "{{ depot_tools_commit }}"
      git -C "$depot" checkout --detach --force FETCH_HEAD
      test "$(git -C "$depot" rev-parse HEAD)" = "{{ depot_tools_commit }}" || { echo "pinned input validation failed input=depot_tools pin={{ depot_tools_commit }} failure=invalid-checkout" >&2; exit 1; }
    fi
    if test "{{ target }}" = windows-x86_64; then
      depot_native=$(just --justfile "{{ root }}/Justfile" _native-path "$depot")
      checkout_native=$(just --justfile "{{ root }}/Justfile" _native-path "$checkout")
      selected_platform=$(cat "$depot/.cipd_client_platform" 2>/dev/null || printf '%s' windows-amd64)
      test "$selected_platform" = windows-amd64 && grep -Eq '^windows-amd64[[:space:]]+sha256[[:space:]]+[0-9a-f]{64}$' "$depot/cipd_client_version.digests" || { echo "CIPD bootstrap unsupported: host=windows-amd64 selected=$selected_platform depot_tools_commit={{ depot_tools_commit }}" >&2; exit 1; }
      MSYS2_ARG_CONV_EXCL='*' cmd.exe /d /s /c "\"$depot_native\\cipd.bat\" version" || { echo "CIPD bootstrap failed: host=windows-amd64 depot_tools_commit={{ depot_tools_commit }}" >&2; exit 1; }
    fi
    if test "$(cat "$checkout/.pulsebeam-sync-target" 2>/dev/null || true)" = "$sync_key" && test "$(git -C "$src" rev-parse HEAD 2>/dev/null || true)" = "{{ webrtc_commit }}"; then
      just --justfile "{{ root }}/Justfile" _source-state "{{ flavor }}" "{{ target }}"
      exit 0
    fi
    mkdir -p "$checkout"
    if test -d "$src"; then just --justfile "{{ root }}/Justfile" _source-state "{{ flavor }}" "{{ target }}"; fi
    printf "solutions = [{'name': 'src', 'url': '{{ webrtc_url }}', 'deps_file': 'DEPS', 'managed': False, 'custom_deps': {}, 'custom_vars': {}}]\n%s\n" "$target_os" > "$checkout/.gclient"
    if test "{{ target }}" = windows-x86_64; then
      vpython_native=$(just --justfile "{{ root }}/Justfile" _native-path "{{ work }}/vpython")
      retry_acquisition webrtc_sync "{{ webrtc_commit }}" env MSYS2_ARG_CONV_EXCL='*' cmd.exe /d /s /c "cd /d \"$checkout_native\" && set \"DEPOT_TOOLS_UPDATE=0\" && set \"GCLIENT_PY3=1\" && set \"VPYTHON_VIRTUALENV_ROOT=$vpython_native\" && call \"$depot_native\\gclient.bat\" sync --no-history --shallow --nohooks --force --revision src@{{ webrtc_commit }}"
    else
      export PATH="$depot:$PATH" DEPOT_TOOLS_UPDATE=0 GCLIENT_PY3=1 VPYTHON_VIRTUALENV_ROOT="{{ work }}/vpython"
      cd "$checkout"
      retry_acquisition webrtc_sync "{{ webrtc_commit }}" gclient sync --no-history --shallow --nohooks --force --revision "src@{{ webrtc_commit }}"
    fi
    actual_webrtc=$(git -C "$src" rev-parse HEAD 2>/dev/null || printf '%s' missing)
    test "$actual_webrtc" = "{{ webrtc_commit }}" || { echo "pinned input validation failed input=webrtc pin={{ webrtc_commit }} expected={{ webrtc_commit }} actual=$actual_webrtc failure=invalid-checkout" >&2; exit 1; }
    if test "{{ target }}" = windows-x86_64; then
      retry_acquisition webrtc_hooks "{{ webrtc_commit }}" env MSYS2_ARG_CONV_EXCL='*' cmd.exe /d /s /c "cd /d \"$checkout_native\" && set \"DEPOT_TOOLS_UPDATE=0\" && set \"GCLIENT_PY3=1\" && set \"VPYTHON_VIRTUALENV_ROOT=$vpython_native\" && call \"$depot_native\\gclient.bat\" runhooks"
    else
      retry_acquisition webrtc_hooks "{{ webrtc_commit }}" gclient runhooks
    fi
    printf '%s\n' "$sync_key" > "$checkout/.pulsebeam-sync-target"
    just --justfile "{{ root }}/Justfile" _source-state "{{ flavor }}" "{{ target }}"

_target-dependencies target:
    #!/usr/bin/env bash
    set -euo pipefail
    src="{{ work }}/checkout/src"
    if test "{{ target }}" = linux-arm64; then
      for attempt in 1 2 3; do
        if python3 "$src/build/linux/sysroot_scripts/install-sysroot.py" --arch=arm64; then exit 0; fi
        if test "$attempt" -lt 3; then
          echo "retrying pinned input=linux-arm64-sysroot pin={{ webrtc_commit }} attempt=$attempt/3 failure=upstream-acquisition" >&2
          sleep "$attempt"
        fi
      done
      echo "pinned input retrieval failed input=linux-arm64-sysroot pin={{ webrtc_commit }} attempts=3 failure=upstream-acquisition-exhausted" >&2
      exit 1
    fi

_gn-args flavor target:
    #!/usr/bin/env bash
    set -euo pipefail
    common='is_debug=false is_component_build=false use_rtti=false rtc_include_tests=false rtc_build_examples=false rtc_build_tools=false rtc_use_h264=false rtc_include_opus=true rtc_build_opus=true rtc_build_libvpx=true rtc_libvpx_build_vp9=true rtc_include_builtin_audio_codecs=true rtc_include_dav1d_in_internal_decoder_factory=true rtc_enable_protobuf=false symbol_level=0 use_siso=false treat_warnings_as_errors=false'
    case "${PULSEBEAM_WEBRTC_SANITIZER:-}" in '') ;; address) common+=' is_asan=true dcheck_always_on=true';; *) echo 'PULSEBEAM_WEBRTC_SANITIZER must be empty or address' >&2; exit 1;; esac
    case "{{ flavor }}" in core) common+=' rtc_include_internal_audio_device=false';; native) common+=' rtc_include_internal_audio_device=true';; esac
    case "{{ target }}" in
      linux-x86_64) platform='target_os="linux" target_cpu="x64" use_sysroot=true target_sysroot="//build/linux/debian_bullseye_amd64-sysroot" use_custom_libcxx=true' ;;
      linux-arm64) platform='target_os="linux" target_cpu="arm64" use_sysroot=true target_sysroot="//build/linux/debian_bullseye_arm64-sysroot" use_custom_libcxx=true' ;;
      windows-x86_64) platform='target_os="win" target_cpu="x64" is_clang=true use_lld=true' ;;
      macos-x86_64) platform='target_os="mac" target_cpu="x64" mac_deployment_target="12.0" use_lld=true use_custom_libcxx=false' ;;
      macos-arm64) platform='target_os="mac" target_cpu="arm64" mac_deployment_target="12.0" use_lld=true use_custom_libcxx=false' ;;
      android-x86_64) platform='target_os="android" target_cpu="x64" android_ndk_api_level=26 default_min_sdk_version=26 android_static_analysis="off"' ;;
      android-arm64-v8a) platform='target_os="android" target_cpu="arm64" android_ndk_api_level=26 default_min_sdk_version=26 android_static_analysis="off"' ;;
      ios-arm64) platform='target_os="ios" target_cpu="arm64" target_environment="device" ios_deployment_target="18.0" ios_enable_code_signing=false use_lld=true' ;;
      ios-simulator-arm64) platform='target_os="ios" target_cpu="arm64" target_environment="simulator" ios_deployment_target="18.0" ios_enable_code_signing=false use_lld=true' ;;
    esac
    case "{{ flavor }}:{{ target }}" in core:linux-*) platform+=' rtc_use_x11=false rtc_use_pipewire=false';; native:linux-*) platform+=' rtc_use_x11=true rtc_use_pipewire=true rtc_link_pipewire=false rtc_include_pulse_audio=true';; native:android-*) platform+=' rtc_enable_android_aaudio=true';; esac
    printf '%s %s\n' "$common" "$platform"

_configure flavor target:
    #!/usr/bin/env bash
    set -euo pipefail
    src="{{ work }}/checkout/src"; out="{{ work }}/out/{{ flavor }}/{{ target }}"
    just --justfile "{{ root }}/Justfile" _source-state "{{ flavor }}" "{{ target }}"
    args=$(just --justfile "{{ root }}/Justfile" _gn-args "{{ flavor }}" "{{ target }}")
    mkdir -p "$out"
    native_src="$src"; native_out="$out"
    if test "{{ target }}" = windows-x86_64; then native_src=$(just --justfile "{{ root }}/Justfile" _native-path "$src"); native_out=$(just --justfile "{{ root }}/Justfile" _native-path "$out"); fi
    "$(just --justfile "{{ root }}/Justfile" _gn)" gen "$native_out" --root="$native_src" --args="$args"
    printf '%s\n' "$args" > "$out/pulsebeam-gn-args.txt"

# Build and validate the host-only protobuf bootstrap before compiling an Apple
# target. GN resolves this target through host_toolchain, independently of the
# target SDK and CPU.
_apple-host-protoc flavor target:
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{ target }}" in macos-*|ios-*) ;; *) exit 0;; esac
    src="{{ work }}/checkout/src"; out="{{ work }}/out/{{ flavor }}/{{ target }}"; ar="$src/third_party/llvm-build/Release+Asserts/bin/llvm-ar"
    case "$(uname -m)" in x86_64) host_arch=x86_64;; arm64) host_arch=arm64;; *) echo "unsupported Apple host architecture: $(uname -m)" >&2; exit 1;; esac
    "$src/third_party/ninja/ninja" -C "$out" protoc
    protoc=$(find "$out" -type f -name protoc -perm -111 -print -quit); support=$(find "$out" -type f -path '*/obj/third_party/protobuf/libprotoc_lib.a' -print -quit)
    test -x "$protoc" || { echo "missing host protoc: $protoc" >&2; exit 1; }
    file "$protoc" | grep -Eq "Mach-O.*${host_arch}" || { echo "host protoc architecture mismatch: expected=$host_arch actual=$(file "$protoc")" >&2; exit 1; }
    test -f "$support" || { echo "missing host protoc support archive: $support" >&2; exit 1; }
    support_members=$("$ar" t "$support") || { echo "unreadable host protoc support archive: $support" >&2; exit 1; }
    ! grep -Fqx '__.SYMDEF' <<< "$support_members" || { echo "invalid host protoc support archive member: $support:__.SYMDEF" >&2; exit 1; }
    support_member=$(head -n 1 <<< "$support_members"); test -n "$support_member" || { echo "empty host protoc support archive: $support" >&2; exit 1; }
    support_format=$("$ar" p "$support" "$support_member" | file -) || { echo "unreadable host protoc support member: $support:$support_member" >&2; exit 1; }
    grep -Eq "Mach-O.*${host_arch}" <<< "$support_format" || { echo "host protoc support architecture mismatch: expected=$host_arch actual=$support_format" >&2; exit 1; }

_roots flavor target:
    #!/usr/bin/env bash
    set -euo pipefail
    printf '%s\n' //:webrtc //api/video_codecs:builtin_video_encoder_factory //api/video_codecs:builtin_video_decoder_factory
    if test "{{ flavor }}" = native; then case "{{ target }}" in android-*) printf '%s\n' //sdk/android:native_api;; ios-*) printf '%s\n' //sdk:framework_objc;; macos-*) printf '%s\n' //sdk:mac_framework_objc;; esac; fi

_compile flavor target:
    #!/usr/bin/env bash
    set -euo pipefail
    src="{{ work }}/checkout/src"; out="{{ work }}/out/{{ flavor }}/{{ target }}"
    roots=()
    while IFS= read -r root_label; do roots+=("${root_label#//}"); done < <(just --justfile "{{ root }}/Justfile" _roots "{{ flavor }}" "{{ target }}")
    if test "{{ target }}" = windows-x86_64; then src=$(just --justfile "{{ root }}/Justfile" _native-path "$src"); out=$(just --justfile "{{ root }}/Justfile" _native-path "$out"); fi
    "$src/third_party/ninja/ninja" -C "$out" "${roots[@]}"
    if [[ "{{ target }}" = linux-* || "{{ target }}" = android-* ]]; then "$src/third_party/ninja/ninja" -C "$out" libc++ libc++abi; fi
    if [[ "{{ target }}" = android-* ]]; then "$src/third_party/ninja/ninja" -C "$out" buildtools/third_party/libunwind:libunwind; fi

_export-predicate-failed flavor target invariant expected actual:
    #!/usr/bin/env bash
    printf 'export predicate failed: target=%s flavor=%s invariant=%s expected=%s actual=%s\n' "{{ target }}" "{{ flavor }}" "{{ invariant }}" "{{ expected }}" "{{ actual }}" >&2
    exit 1

_export-static-closure flavor target src out src_native out_native archives objects:
    #!/usr/bin/env bash
    set -euo pipefail
    gn=$(just --justfile "{{ root }}/Justfile" _gn)
    base=$(mktemp); extra=$(mktemp); trap 'rm -f "$base" "$extra"' EXIT
    { printf '%s\n' //:webrtc; "$gn" desc --root="{{ src_native }}" "{{ out_native }}" //:webrtc deps --all; } | LC_ALL=C sort -u > "$base"
    while read -r root_label; do { printf '%s\n' "$root_label"; "$gn" desc --root="{{ src_native }}" "{{ out_native }}" "$root_label" deps --all; }; done < <(just --justfile "{{ root }}/Justfile" _roots "{{ flavor }}" "{{ target }}") | LC_ALL=C sort -u > "$extra"
    gn_outputs() {
      local label="$1" output
      if ! output=$("$gn" desc --root="{{ src_native }}" "{{ out_native }}" "$label" outputs 2>&1); then
        just --justfile "{{ root }}/Justfile" _export-predicate-failed "{{ flavor }}" "{{ target }}" static-closure "GN outputs for $label" "GN output query failed for $label: $output"
      fi
      printf '%s\n' "$output"
    }
    static_archives() { awk '/\.(a|lib)$/ && $0 !~ /libprotoc_lib\.(a|lib)$/ {print}'; }
    required_static_archives() {
      local label="$1" output archives
      output=$(gn_outputs "$label")
      archives=$(printf '%s\n' "$output" | static_archives)
      test -n "$archives" || just --justfile "{{ root }}/Justfile" _export-predicate-failed "{{ flavor }}" "{{ target }}" static-closure "a static archive output for $label" "no matching .a or .lib output for $label"
      printf '%s\n' "$archives"
    }
    { gn_outputs //:webrtc; while read -r label; do gn_outputs "$label"; done < <(comm -23 "$extra" "$base"); } | static_archives | LC_ALL=C sort -u > "{{ archives }}"
    if [[ "{{ target }}" = linux-* || "{{ target }}" = android-* ]]; then
      required_static_archives //buildtools/third_party/libc++ >> "{{ archives }}"
      if test "${PULSEBEAM_WEBRTC_SANITIZER:-}" = address; then
        label=//buildtools/third_party/libc++abi
        if ! source_output=$("$gn" desc --root="{{ src_native }}" "{{ out_native }}" "$label" sources 2>&1); then
          just --justfile "{{ root }}/Justfile" _export-predicate-failed "{{ flavor }}" "{{ target }}" static-closure "GN sources for $label" "GN source query failed for $label: $source_output"
        fi
        object_dir="{{ out }}/obj/buildtools/third_party/libc++abi"
        if test -d "$object_dir"; then find "$object_dir" -type f -name '*.o' -print | LC_ALL=C sort -u >> "{{ objects }}"; fi
        test -s "{{ objects }}" || just --justfile "{{ root }}/Justfile" _export-predicate-failed "{{ flavor }}" "{{ target }}" static-closure "compiled object outputs for $label" 'no libc++abi object output'
      else
        required_static_archives //buildtools/third_party/libc++abi >> "{{ archives }}"
      fi
    fi
    test -s "{{ archives }}" || just --justfile "{{ root }}/Justfile" _export-predicate-failed "{{ flavor }}" "{{ target }}" static-closure 'at least one static archive output' 'no matching .a or .lib output'

_export flavor target:
    #!/usr/bin/env bash
    set -euo pipefail
    src="{{ work }}/checkout/src"; out="{{ work }}/out/{{ flavor }}/{{ target }}"; root_native=$(just --justfile "{{ root }}/Justfile" _native-path "{{ root }}"); src_native=$(just --justfile "{{ root }}/Justfile" _native-path "$src"); out_native=$(just --justfile "{{ root }}/Justfile" _native-path "$out"); cxx_version=$(python3 "$root_native/tools/cxx_import.py" version); cxx_abi=${cxx_version##*.}
    stage="{{ work }}/package/webrtc-{{ flavor }}-{{ target }}"; archive="{{ dist }}/webrtc-{{ flavor }}-{{ target }}.tar.gz"
    gn=$(just --justfile "{{ root }}/Justfile" _gn); ar=$(just --justfile "{{ root }}/Justfile" _native-path "$src/third_party/llvm-build/Release+Asserts/bin/llvm-ar")
    rm -rf "$stage"; mkdir -p "$stage/include" "$stage/lib" "$stage/LICENSES" "{{ dist }}"
    archives=$(mktemp); objects=$(mktemp); definitions_file=$(mktemp); toolchain_file=$(mktemp); trap 'rm -f "$archives" "$objects" "$definitions_file" "$toolchain_file"' EXIT
    just --justfile "{{ root }}/Justfile" _export-static-closure "{{ flavor }}" "{{ target }}" "$src" "$out" "$src_native" "$out_native" "$archives" "$objects"
    if [[ "{{ target }}" = android-* ]]; then find "$out/obj/buildtools/third_party/libunwind/libunwind" -name '*.o' -print > "$objects"; fi
    roots=(api audio call common_audio common_video experiments logging media modules net p2p pc rtc_base sdk/objc/base system_wrappers video)
    for source_root in "${roots[@]}"; do find "$src/$source_root" -type f \( -name '*.h' -o -name '*.hh' -o -name '*.hpp' -o -name '*.inc' \) -print; done | sed "s#^$src/##" | LC_ALL=C sort -u | tar -C "$src" -T - -cf - | tar -C "$stage/include" -xf -
    cp -R "$src/third_party/abseil-cpp/absl" "$stage/include/"; cp -R "$src/third_party/libyuv/include/." "$stage/include/"
    if test -d "$out/gen"; then (cd "$out/gen" && find . -type f \( -name '*.h' -o -name '*.inc' \) -print0 | tar --null -T - -cf -) | tar -C "$stage/include" -xf -; fi
    if [[ "{{ target }}" = linux-* || "{{ target }}" = android-* ]]; then mkdir -p "$stage/include/c++/v1"; cp -R "$src/third_party/libc++/src/include/." "$stage/include/c++/v1/"; cp "$src/buildtools/third_party/libc++/__config_site" "$src/buildtools/third_party/libc++/__assertion_handler" "$stage/include/c++/v1/"; fi
    "$gn" desc --root="$src_native" "$out_native" //:webrtc defines --all | sed 's/^/-D/' > "$definitions_file"
    just --justfile "{{ root }}/Justfile" _bridge-objects "{{ flavor }}" "{{ target }}" "$stage" "$definitions_file" >> "$objects"
    library="$stage/lib/$(case "{{ target }}" in windows-*) printf webrtc.lib;; *) printf libwebrtc.a;; esac)"
    library_native=$(just --justfile "{{ root }}/Justfile" _native-path "$library")
    { printf 'CREATE %s\n' "$library_native"; while read -r input; do test -f "$input" || { echo "missing archive: $input" >&2; exit 1; }; printf 'ADDLIB %s\n' "$(just --justfile "{{ root }}/Justfile" _native-path "$input")"; done < "$archives"; while read -r input; do test -f "$input" || { echo "missing object: $input" >&2; exit 1; }; printf 'ADDMOD %s\n' "$(just --justfile "{{ root }}/Justfile" _native-path "$input")"; done < "$objects"; printf 'SAVE\nEND\n'; } | "$ar" -M
    members=$("$ar" t "$library_native") || just --justfile "{{ root }}/Justfile" _export-predicate-failed "{{ flavor }}" "{{ target }}" archive-membership "a readable member list for $library" 'llvm-ar could not list archive members'
    for member in bridge execution network codec peer data_channel video probe cxx; do grep -E "(^|/)${member}\\.(o|obj)$" <<< "$members" >/dev/null || just --justfile "{{ root }}/Justfile" _export-predicate-failed "{{ flavor }}" "{{ target }}" archive-membership "${member}.o or ${member}.obj in $library" "member ${member} is absent"; done
    bridge_symbol="pulsebeam\$webrtc_sys\$cxxbridge1\$$cxx_abi\$bridge_identity"
    nm=$(just --justfile "{{ root }}/Justfile" _native-path "$src/third_party/llvm-build/Release+Asserts/bin/llvm-nm")
    nm_output=$("$nm" "$library_native") || just --justfile "{{ root }}/Justfile" _export-predicate-failed "{{ flavor }}" "{{ target }}" bridge-identity "$bridge_symbol in $library" 'llvm-nm could not inspect archive symbols'
    grep -F "$bridge_symbol" <<< "$nm_output" >/dev/null || just --justfile "{{ root }}/Justfile" _export-predicate-failed "{{ flavor }}" "{{ target }}" bridge-identity "$bridge_symbol in $library" 'bridge identity symbol is absent'
    export PATH="{{ work }}/depot_tools:$PATH" DEPOT_TOOLS_UPDATE=0 VPYTHON_VIRTUALENV_ROOT="{{ work }}/vpython"
    license_targets=(); while IFS= read -r root_label; do license_targets+=(--target "$root_label"); done < <(just --justfile "{{ root }}/Justfile" _roots "{{ flavor }}" "{{ target }}")
    stage_native=$(just --justfile "{{ root }}/Justfile" _native-path "$stage")
    if test "{{ target }}" = windows-x86_64; then depot_native=$(just --justfile "{{ root }}/Justfile" _native-path "{{ work }}/depot_tools"); MSYS2_ARG_CONV_EXCL='*' cmd.exe /d /s /c "\"$depot_native\\vpython3.bat\" \"$src_native\\tools_webrtc\\libs\\generate_licenses.py\" ${license_targets[*]} \"$stage_native\\LICENSES\" \"$out_native\""; else vpython3 "$src/tools_webrtc/libs/generate_licenses.py" "${license_targets[@]}" "$stage/LICENSES" "$out"; fi
    cp "$src/LICENSE" "$stage/LICENSES/WEBRTC-BSD.txt"; test ! -f "$src/PATENTS" || cp "$src/PATENTS" "$stage/LICENSES/"; cp "{{ root }}/LICENSE" "$stage/LICENSES/REPOSITORY-APACHE-2.0.txt"
    cp "{{ root }}/vendor/cxx/LICENSE-APACHE" "$stage/LICENSES/CXX-APACHE-2.0.txt"; cp "{{ root }}/vendor/cxx/LICENSE-MIT" "$stage/LICENSES/CXX-MIT.txt"
    just --justfile "{{ root }}/Justfile" _link-flags "{{ flavor }}" "{{ target }}" > "$stage/link.txt"
    just --justfile "{{ root }}/Justfile" _toolchain "{{ target }}" > "$toolchain_file"
    definitions=$(tr '\n' ' ' < "$definitions_file")
    just --justfile "{{ root }}/Justfile" _source-state "{{ flavor }}" "{{ target }}"
    source_state=pristine; case "{{ flavor }}:{{ target }}" in core:ios-arm64|core:ios-simulator-arm64) source_state=applied;; esac
    { printf 'webrtc_repository=%s\nwebrtc_commit=%s\nwebrtc_core_ios_patch_sha256=%s\nwebrtc_source_state=%s\ndepot_tools_commit=%s\nflavor=%s\ntarget=%s\ntoolchain=' '{{ webrtc_url }}' '{{ webrtc_commit }}' '{{ webrtc_core_ios_patch_sha256 }}' "$source_state" '{{ depot_tools_commit }}' '{{ flavor }}' '{{ target }}'; cat "$toolchain_file"; printf 'gn_args='; cat "$out/pulsebeam-gn-args.txt"; printf 'cxx_defines=%s\n' "$definitions"; } > "$stage/build.txt"
    python3 "$root_native/tools/write_artifact_manifest.py" --flavor "{{ flavor }}" --target "{{ target }}" --bridge-identity pulsebeam-webrtc-sys-bridge-v2 --source-repository "{{ webrtc_url }}" --source-revision "{{ webrtc_commit }}" --source-patch-sha256 "{{ webrtc_core_ios_patch_sha256 }}" --source-state "$source_state" --depot-tools-repository "{{ depot_tools_url }}" --depot-tools-revision "{{ depot_tools_commit }}" --bridge-source "$root_native/src/lib.rs" --generated-header "$stage_native/include/pulsebeam-webrtc-sys/src/lib.rs.h" --generated-source "$(just --justfile "{{ root }}/Justfile" _native-path "{{ work }}/bridge/{{ flavor }}/{{ target }}/lib.rs.cc")" --toolchain-file "$(just --justfile "{{ root }}/Justfile" _native-path "$toolchain_file")" --gn-args-file "$out_native/pulsebeam-gn-args.txt" --defines-file "$(just --justfile "{{ root }}/Justfile" _native-path "$definitions_file")" --licenses "$stage_native/LICENSES" --output "$stage_native/manifest.json"
    members=(include lib link.txt LICENSES build.txt manifest.json)
    rm -f "$archive"; tar -C "$stage" -czf "$archive" "${members[@]}"
    verify=$(mktemp -d); trap 'rm -rf "$verify"; rm -f "$archives" "$objects" "$definitions_file" "$toolchain_file"' EXIT; tar -C "$verify" -xzf "$archive"
    python3 "$root_native/tools/cxx_provenance.py" "$(just --justfile "{{ root }}/Justfile" _native-path "$verify")"
    just --justfile "{{ root }}/Justfile" _cpp-smoke "{{ flavor }}" "{{ target }}" "$verify"
    just --justfile "{{ root }}/Justfile" _rust-smoke "{{ flavor }}" "{{ target }}" "$verify"
    just --justfile "{{ root }}/Justfile" _source-state "{{ flavor }}" "{{ target }}"

_bridge-objects flavor target stage definitions_file:
    #!/usr/bin/env bash
    set -euo pipefail
    src="{{ work }}/checkout/src"; bridge="{{ work }}/bridge/{{ flavor }}/{{ target }}"; root_native=$(just --justfile "{{ root }}/Justfile" _native-path "{{ root }}"); src_native=$(just --justfile "{{ root }}/Justfile" _native-path "$src"); bridge_native=$(just --justfile "{{ root }}/Justfile" _native-path "$bridge"); stage_native=$(just --justfile "{{ root }}/Justfile" _native-path "{{ stage }}")
    generator=$(CARGO_HOME="$(just --justfile "{{ root }}/Justfile" _native-path "{{ work }}/cargo-home")" python3 "$root_native/tools/cxx_import.py" install-generator --root "$(just --justfile "{{ root }}/Justfile" _native-path "{{ work }}/cxxbridge-tools")")
    rm -rf "$bridge"; mkdir -p "$bridge/obj" "{{ stage }}/include/rust" "{{ stage }}/include/pulsebeam-webrtc-sys/src" "{{ stage }}/include/pulsebeam-webrtc-sys/native"
    cp "{{ root }}/native/probe.h" "{{ root }}/native/execution.h" "{{ root }}/native/network.h" "{{ root }}/native/codec.h" "{{ root }}/native/peer.h" "{{ root }}/native/data_channel.h" "{{ root }}/native/video.h" "{{ stage }}/include/pulsebeam-webrtc-sys/native/"
    cp "{{ root }}/vendor/cxx/include/cxx.h" "{{ stage }}/include/rust/cxx.h"
    "$generator" "$root_native/src/lib.rs" --header > "{{ stage }}/include/pulsebeam-webrtc-sys/src/lib.rs.h"
    "$generator" "$root_native/src/lib.rs" > "$bridge/lib.rs.cc"
    "$generator" "$root_native/src/lib.rs" --header > "$bridge/lib.rs.h.repeat"
    "$generator" "$root_native/src/lib.rs" > "$bridge/lib.rs.cc.repeat"
    cmp "{{ stage }}/include/pulsebeam-webrtc-sys/src/lib.rs.h" "$bridge/lib.rs.h.repeat"
    cmp "$bridge/lib.rs.cc" "$bridge/lib.rs.cc.repeat"
    rm "$bridge/lib.rs.h.repeat" "$bridge/lib.rs.cc.repeat"
    defs=(); while IFS= read -r definition; do defs+=("$definition"); done < "{{ definitions_file }}"
    include=(-I"{{ stage }}/include")
    case "{{ target }}" in
      linux-x86_64) cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang++"; args=(--target=x86_64-linux-gnu --sysroot="$src/build/linux/debian_bullseye_amd64-sysroot" -std=c++20 -fno-exceptions -fno-rtti -Wno-nullability-completeness -nostdinc++ -isystem "{{ stage }}/include/c++/v1" -pthread); suffix=o;;
      linux-arm64) cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang++"; args=(--target=aarch64-linux-gnu --sysroot="$src/build/linux/debian_bullseye_arm64-sysroot" -std=c++20 -fno-exceptions -fno-rtti -Wno-nullability-completeness -nostdinc++ -isystem "{{ stage }}/include/c++/v1" -pthread); suffix=o;;
      android-x86_64|android-arm64-v8a) prebuilt=$(find "$src/third_party/android_toolchain/ndk/toolchains/llvm/prebuilt" -mindepth 1 -maxdepth 1 -type d | head -1); triple=$(case "{{ target }}" in android-x86_64) printf x86_64;; *) printf aarch64;; esac); cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang++"; args=(--target="${triple}-linux-android26" --sysroot="$prebuilt/sysroot" -std=c++20 -fno-exceptions -fno-rtti -Wno-nullability-completeness -nostdinc++ -isystem "{{ stage }}/include/c++/v1"); suffix=o;;
      macos-x86_64|macos-arm64)
        case "{{ target }}" in macos-x86_64) arch=x86_64;; *) arch=arm64;; esac
        sdk_path=$(xcrun --sdk macosx --show-sdk-path)
        cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang++"
        args=(-arch "$arch" -isysroot "$sdk_path" -mmacosx-version-min=12.0 -std=c++20 -fno-exceptions -fno-rtti -Wno-nullability-completeness)
        suffix=o;;
      ios-arm64|ios-simulator-arm64)
        case "{{ target }}" in ios-arm64) sdk=iphoneos; minimum=-miphoneos-version-min=18.0;; *) sdk=iphonesimulator; minimum=-mios-simulator-version-min=18.0;; esac
        sdk_path=$(xcrun --sdk "$sdk" --show-sdk-path)
        cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang++"
        args=(-arch arm64 -isysroot "$sdk_path" "$minimum" -std=c++20 -fno-exceptions -fno-rtti -Wno-nullability-completeness)
        suffix=o;;
      windows-x86_64) cxx="$src_native\\third_party\\llvm-build\\Release+Asserts\\bin\\clang-cl"; include=(-I"$stage_native/include"); args=(/std:c++20 /GR- /EHs-c- /MT -Wno-nullability-completeness); suffix=obj;;
    esac
    if test "{{ target }}" = windows-x86_64; then
      MSYS2_ARG_CONV_EXCL='*' "$cxx" "${args[@]}" "${include[@]}" "${defs[@]}" /c "$bridge_native\\lib.rs.cc" "/Fo$bridge_native\\obj\\bridge.$suffix"
      MSYS2_ARG_CONV_EXCL='*' "$cxx" "${args[@]}" "${include[@]}" "${defs[@]}" /c "$root_native\\native\\execution.cc" "/Fo$bridge_native\\obj\\execution.$suffix"
      MSYS2_ARG_CONV_EXCL='*' "$cxx" "${args[@]}" "${include[@]}" "${defs[@]}" /c "$root_native\\native\\network.cc" "/Fo$bridge_native\\obj\\network.$suffix"
      MSYS2_ARG_CONV_EXCL='*' "$cxx" "${args[@]}" "${include[@]}" "${defs[@]}" /c "$root_native\\native\\codec.cc" "/Fo$bridge_native\\obj\\codec.$suffix"
      MSYS2_ARG_CONV_EXCL='*' "$cxx" "${args[@]}" "${include[@]}" "${defs[@]}" /c "$root_native\\native\\peer.cc" "/Fo$bridge_native\\obj\\peer.$suffix"
      MSYS2_ARG_CONV_EXCL='*' "$cxx" "${args[@]}" "${include[@]}" "${defs[@]}" /c "$root_native\\native\\data_channel.cc" "/Fo$bridge_native\\obj\\data_channel.$suffix"
      MSYS2_ARG_CONV_EXCL='*' "$cxx" "${args[@]}" "${include[@]}" "${defs[@]}" /c "$root_native\\native\\video.cc" "/Fo$bridge_native\\obj\\video.$suffix"
      MSYS2_ARG_CONV_EXCL='*' "$cxx" "${args[@]}" "${include[@]}" "${defs[@]}" /c "$root_native\\native\\probe.cc" "/Fo$bridge_native\\obj\\probe.$suffix"
      MSYS2_ARG_CONV_EXCL='*' "$cxx" "${args[@]}" "${include[@]}" "${defs[@]}" /c "$root_native\\vendor\\cxx\\src\\cxx.cc" "/Fo$bridge_native\\obj\\cxx.$suffix"
    else
      "$cxx" "${args[@]}" "${include[@]}" "${defs[@]}" -c "$bridge/lib.rs.cc" -o "$bridge/obj/bridge.$suffix"
      "$cxx" "${args[@]}" "${include[@]}" "${defs[@]}" -c "{{ root }}/native/execution.cc" -o "$bridge/obj/execution.$suffix"
      "$cxx" "${args[@]}" "${include[@]}" "${defs[@]}" -c "{{ root }}/native/network.cc" -o "$bridge/obj/network.$suffix"
      "$cxx" "${args[@]}" "${include[@]}" "${defs[@]}" -c "{{ root }}/native/codec.cc" -o "$bridge/obj/codec.$suffix"
      "$cxx" "${args[@]}" "${include[@]}" "${defs[@]}" -c "{{ root }}/native/peer.cc" -o "$bridge/obj/peer.$suffix"
      "$cxx" "${args[@]}" "${include[@]}" "${defs[@]}" -c "{{ root }}/native/data_channel.cc" -o "$bridge/obj/data_channel.$suffix"
      "$cxx" "${args[@]}" "${include[@]}" "${defs[@]}" -c "{{ root }}/native/video.cc" -o "$bridge/obj/video.$suffix"
      "$cxx" "${args[@]}" "${include[@]}" "${defs[@]}" -c "{{ root }}/native/probe.cc" -o "$bridge/obj/probe.$suffix"
      "$cxx" "${args[@]}" "${include[@]}" "${defs[@]}" -c "{{ root }}/vendor/cxx/src/cxx.cc" -o "$bridge/obj/cxx.$suffix"
    fi
    if test "{{ target }}" = windows-x86_64; then "$src_native\\third_party\\llvm-build\\Release+Asserts\\bin\\llvm-readobj" --file-headers "$bridge_native\\obj\\bridge.obj" | grep -F 'Format: COFF-x86-64' >/dev/null; fi
    printf '%s\n' "$bridge/obj/bridge.$suffix" "$bridge/obj/execution.$suffix" "$bridge/obj/network.$suffix" "$bridge/obj/codec.$suffix" "$bridge/obj/peer.$suffix" "$bridge/obj/data_channel.$suffix" "$bridge/obj/video.$suffix" "$bridge/obj/probe.$suffix" "$bridge/obj/cxx.$suffix"

_link-flags flavor target:
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{ target }}" in
      linux-x86_64) flags='-fuse-ld=lld -nostdlib++ -pthread -ldl -lrt -lm'; test "{{ flavor }}" = core || flags+=' -lX11 -lgio-2.0 -lglib-2.0 -lgobject-2.0 -lXcomposite -lXdamage -lXext -lXfixes -lXrandr -lXrender -lXtst -lgbm -ldrm';;
      linux-arm64) flags='-fuse-ld=lld --rtlib=compiler-rt -nostdlib++ -pthread -ldl -lrt -lm'; test "{{ flavor }}" = core || flags+=' -lX11 -lgio-2.0 -lglib-2.0 -lgobject-2.0 -lXcomposite -lXdamage -lXext -lXfixes -lXrandr -lXrender -lXtst -lgbm -ldrm';;
      windows-*) flags='advapi32.lib bcrypt.lib crypt32.lib d3d11.lib dmoguids.lib dwmapi.lib dxgi.lib iphlpapi.lib msdmo.lib ole32.lib oleaut32.lib secur32.lib shcore.lib strmiids.lib user32.lib winmm.lib wmcodecdspuuid.lib ws2_32.lib';;
      macos-*) flags='-framework Foundation -framework AppKit -framework ApplicationServices -framework CoreAudio -framework CoreFoundation -framework CoreGraphics -framework CoreMedia -framework CoreVideo -framework AudioToolbox -framework AVFoundation -framework IOKit -framework IOSurface -framework OpenGL -framework VideoToolbox -weak_framework ScreenCaptureKit';;
      android-*) flags='-fuse-ld=lld -nostdlib++ --unwindlib=none -llog -landroid -lGLESv2 -lOpenSLES -ldl -lm'; test "{{ flavor }}" = core || flags+=' -laaudio';;
      ios-*) flags='-framework Foundation -framework CoreFoundation -framework CoreGraphics -framework CoreMedia -framework CoreVideo -framework AudioToolbox -framework AVFoundation -framework VideoToolbox -framework UIKit';;
    esac
    printf '%s\n' "$flags"

_cpp-smoke flavor target kit:
    #!/usr/bin/env bash
    set -euo pipefail
    src="{{ work }}/checkout/src"; root_native=$(just --justfile "{{ root }}/Justfile" _native-path "{{ root }}"); src_native=$(just --justfile "{{ root }}/Justfile" _native-path "$src"); kit_native=$(just --justfile "{{ root }}/Justfile" _native-path "{{ kit }}"); read -r -a link <<< "$(just --justfile "{{ root }}/Justfile" _link-flags "{{ flavor }}" "{{ target }}")"
    read -r -a exported_defines <<< "$(sed -n 's/^cxx_defines=//p' "{{ kit }}/build.txt")"
    defs=(-std=c++20 -fno-exceptions -fno-rtti -Wno-nullability-completeness "${exported_defines[@]}")
    case "{{ target }}" in
      linux-x86_64) cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang++"; args=(--target=x86_64-linux-gnu --sysroot="$src/build/linux/debian_bullseye_amd64-sysroot" -nostdinc++ -isystem "{{ kit }}/include/c++/v1" -pthread);;
      linux-arm64) cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang++"; args=(--target=aarch64-linux-gnu --sysroot="$src/build/linux/debian_bullseye_arm64-sysroot" -nostdinc++ -isystem "{{ kit }}/include/c++/v1" -pthread);;
      android-x86_64|android-arm64-v8a) prebuilt=$(find "$src/third_party/android_toolchain/ndk/toolchains/llvm/prebuilt" -mindepth 1 -maxdepth 1 -type d | head -1); cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang++"; triple=$(case "{{ target }}" in android-x86_64) printf x86_64;; *) printf aarch64;; esac); args=(--target="${triple}-linux-android26" --sysroot="$prebuilt/sysroot" -nostdinc++ -isystem "{{ kit }}/include/c++/v1");;
      macos-x86_64|macos-arm64) cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang++"; arch=$(case "{{ target }}" in macos-x86_64) printf x86_64;; *) printf arm64;; esac); args=(-arch "$arch" -isysroot "$(xcrun --sdk macosx --show-sdk-path)" -mmacosx-version-min=12.0 -DWEBRTC_POSIX -DWEBRTC_MAC);;
      ios-arm64|ios-simulator-arm64) cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang++"; sdk=$(case "{{ target }}" in ios-arm64) printf iphoneos;; *) printf iphonesimulator;; esac); minimum=$(case "{{ target }}" in ios-arm64) printf '%s' -miphoneos-version-min=18.0;; *) printf '%s' -mios-simulator-version-min=18.0;; esac); args=(-arch arm64 -isysroot "$(xcrun --sdk "$sdk" --show-sdk-path)" "$minimum" -DWEBRTC_POSIX -DWEBRTC_IOS -DWEBRTC_MAC);;
      windows-x86_64) MSYS2_ARG_CONV_EXCL='*' "$src_native\\third_party\\llvm-build\\Release+Asserts\\bin\\clang-cl" /std:c++20 /GR- /EHs-c- /MT "${exported_defines[@]}" /I"$kit_native\\include" "$root_native\\consumer\\smoke.cc" "$kit_native\\lib\\webrtc.lib" "${link[@]}" "/Fe:$kit_native\\smoke.exe"; exit;;
    esac
    "$cxx" "${defs[@]}" "${args[@]}" -I"{{ kit }}/include" "{{ root }}/consumer/smoke.cc" "{{ kit }}/lib/libwebrtc.a" "${link[@]}" -o "{{ kit }}/smoke"

_rust-smoke flavor target kit:
    #!/usr/bin/env bash
    set -euo pipefail
    src="{{ work }}/checkout/src"; kit_native=$(just --justfile "{{ root }}/Justfile" _native-path "{{ kit }}"); cargo_home_native=$(just --justfile "{{ root }}/Justfile" _native-path "{{ work }}/cargo-home"); smoke_native=$(just --justfile "{{ root }}/Justfile" _native-path "{{ work }}/rust-smoke/{{ flavor }}/{{ target }}")
    case "{{ target }}" in
      linux-x86_64) cargo_target=x86_64-unknown-linux-gnu; cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang"; rustflags=(-C link-arg=--target=x86_64-linux-gnu -C "link-arg=--sysroot=$src/build/linux/debian_bullseye_amd64-sysroot" -C link-arg=-fuse-ld=lld);;
      linux-arm64) cargo_target=aarch64-unknown-linux-gnu; cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang"; rustflags=(-C link-arg=--target=aarch64-linux-gnu -C "link-arg=--sysroot=$src/build/linux/debian_bullseye_arm64-sysroot" -C link-arg=-fuse-ld=lld -C link-arg=--rtlib=compiler-rt);;
      android-x86_64|android-arm64-v8a) prebuilt=$(find "$src/third_party/android_toolchain/ndk/toolchains/llvm/prebuilt" -mindepth 1 -maxdepth 1 -type d | head -1); case "{{ target }}" in android-x86_64) cargo_target=x86_64-linux-android; triple=x86_64;; *) cargo_target=aarch64-linux-android; triple=aarch64;; esac; cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang"; rustflags=(-C "link-arg=--target=${triple}-linux-android26" -C "link-arg=--sysroot=$prebuilt/sysroot" -C link-arg=-fuse-ld=lld -C link-arg=--unwindlib=none);;
      macos-x86_64|macos-arm64) case "{{ target }}" in macos-x86_64) cargo_target=x86_64-apple-darwin; arch=x86_64;; *) cargo_target=aarch64-apple-darwin; arch=arm64;; esac; cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang"; rustflags=(-C "link-arg=-arch" -C "link-arg=$arch" -C "link-arg=-isysroot" -C "link-arg=$(xcrun --sdk macosx --show-sdk-path)" -C link-arg=-mmacosx-version-min=12.0);;
      ios-arm64|ios-simulator-arm64) case "{{ target }}" in ios-arm64) cargo_target=aarch64-apple-ios; sdk=iphoneos; minimum=-miphoneos-version-min=18.0;; *) cargo_target=aarch64-apple-ios-sim; sdk=iphonesimulator; minimum=-mios-simulator-version-min=18.0;; esac; cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang"; rustflags=(-C link-arg=-arch -C link-arg=arm64 -C link-arg=-isysroot -C "link-arg=$(xcrun --sdk "$sdk" --show-sdk-path)" -C "link-arg=$minimum");;
      windows-x86_64) cargo_target=x86_64-pc-windows-msvc; cxx=''; rustflags=(-C target-feature=+crt-static);;
    esac
    if test "${PULSEBEAM_WEBRTC_SANITIZER:-}" = address; then rustflags+=(-C link-arg=-fsanitize=address); fi
    linker_env="CARGO_TARGET_$(printf '%s' "$cargo_target" | tr '[:lower:]-' '[:upper:]_')_LINKER"
    command=(env RUSTFLAGS="${rustflags[*]}" CARGO_HOME="$cargo_home_native" CARGO_TARGET_DIR="$smoke_native" PULSEBEAM_WEBRTC_SYS_ARTIFACT_DIR="$kit_native")
    test -z "$cxx" || command+=("$linker_env=$cxx")
    features=(); test "{{ flavor }}" = core || features=(--features native)
    "${command[@]}" cargo test --locked --target "$cargo_target" "${features[@]}" --test identity --no-run

# Execute the complete Rust runtime suite against an extracted host-native artifact.
_runtime-test flavor target archive:
    #!/usr/bin/env bash
    set -euo pipefail
    just --justfile "{{ root }}/Justfile" _validate-flavor "{{ flavor }}"
    case "{{ target }}:$(uname -s):$(uname -m)" in
      linux-x86_64:Linux:x86_64|windows-x86_64:*MINGW*:x86_64|windows-x86_64:*MSYS*:x86_64|macos-x86_64:Darwin:x86_64|macos-arm64:Darwin:arm64) ;;
      *) echo "{{ target }} is not native to this runtime host" >&2; exit 1;;
    esac
    test -f "{{ archive }}" || { echo "missing runtime artifact: {{ archive }}" >&2; exit 1; }
    artifact=$(mktemp -d); trap 'rm -rf "$artifact"' EXIT
    tar -C "$artifact" -xzf "{{ archive }}"
    features=(); test "{{ flavor }}" = core || features=(--features native)
    rustflags=(); if test "${PULSEBEAM_WEBRTC_SANITIZER:-}" = address; then rustflags=(-C link-arg=-fsanitize=address); fi
    cargo_home_native=$(just --justfile "{{ root }}/Justfile" _native-path "{{ work }}/cargo-home"); runtime_native=$(just --justfile "{{ root }}/Justfile" _native-path "{{ work }}/runtime/{{ flavor }}/{{ target }}"); artifact_native=$(just --justfile "{{ root }}/Justfile" _native-path "$artifact")
    RUSTFLAGS="${rustflags[*]}" CARGO_HOME="$cargo_home_native" CARGO_TARGET_DIR="$runtime_native" PULSEBEAM_WEBRTC_SYS_ARTIFACT_DIR="$artifact_native" cargo test --locked "${features[@]}" --tests -- --test-threads=1

_gn:
    @case "$(uname -s)" in Linux) path='{{ work }}/checkout/src/buildtools/linux64/gn';; Darwin) path='{{ work }}/checkout/src/buildtools/mac/gn';; *) path='{{ work }}/checkout/src/buildtools/win/gn.exe';; esac; test -x "$path" || { echo 'pinned GN is missing' >&2; exit 1; }; just --justfile '{{ root }}/Justfile' _native-path "$path"

_toolchain target:
    #!/usr/bin/env bash
    set -euo pipefail
    src="{{ work }}/checkout/src"
    case "{{ target }}" in windows-*) src=$(just --justfile "{{ root }}/Justfile" _native-path "$src"); printf 'MSVC %s; Windows SDK %s; ' "$(cl.exe 2>&1 | sed -n '1p')" "${WindowsSDKVersion:-unknown}"; MSYS2_ARG_CONV_EXCL='*' "$src\\third_party\\llvm-build\\Release+Asserts\\bin\\clang-cl" --version | head -1;; macos-*|ios-*) printf 'Xcode %s; ' "$(xcodebuild -version | tr '\n' ' ')"; "$src/third_party/llvm-build/Release+Asserts/bin/clang++" --version | head -1;; android-*) printf 'Android NDK API 26; '; "$src/third_party/llvm-build/Release+Asserts/bin/clang++" --version | head -1;; *) "$src/third_party/llvm-build/Release+Asserts/bin/clang++" --version | head -1;; esac
