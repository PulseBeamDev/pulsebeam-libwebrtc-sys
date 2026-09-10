set shell := ["bash", "-euo", "pipefail", "-c"]

root := justfile_directory()
work := env_var_or_default("WEBRTC_WORK", root + "/.work")
dist := env_var_or_default("WEBRTC_DIST", root + "/dist")

# Immutable source/build identity. m150_release is deliberately not used by a recipe.
webrtc_url := "https://github.com/webrtc-sdk/webrtc.git"
webrtc_commit := "ba469aa2093ba950066258ca0a59a6fbd1295582"
depot_tools_url := "https://chromium.googlesource.com/chromium/tools/depot_tools.git"
depot_tools_commit := "ed9c87f6f12f6b87210e7025d4a36a5a72a2ccd4"
milestone := "150"
recipe_revision := "1"
flavor := "core"
artifact_prefix := "pulsebeam-libwebrtc-m" + milestone + "-r" + recipe_revision + "-" + flavor

default: check

# Print the supported core matrix without consulting the network.
list-targets:
    @printf '%s\n' linux-x86_64 linux-arm64 windows-x86_64 macos-arm64 macos-x86_64 android-arm64-v8a android-x86_64 ios-arm64 ios-simulator-arm64

# Quick repository checks. This intentionally does not acquire or build WebRTC.
check:
    #!/usr/bin/env bash
    set -euo pipefail
    cd "{{ root }}"
    test "$(git ls-files | grep -Ev '^(README.md|LICENSE|Justfile|\.gitignore|consumer/)' || true)" = ""
    test "$(just --summary | tr ' ' '\n' | grep -E '^(sync|configure|build|package|verify-core|core|verify-core-matrix|clean)$' | wc -l)" -eq 8
    test "$(just list-targets | wc -l)" -eq 9
    grep -Fq '{{ webrtc_commit }}' Justfile
    grep -Fq 'rtc_use_h264=false' Justfile
    grep -Fq 'rtc_include_internal_audio_device=false' Justfile
    grep -Fq 'is_component_build=false' Justfile
    grep -Fq 'is_debug=false' Justfile
    just gn-args linux-x86_64 | grep -Fq 'use_custom_libcxx=true'
    ! grep -Fq 'system libstdc++' Justfile
    grep -Fq 'CreateModularPeerConnectionFactory' consumer/smoke.cc
    grep -Fq 'SetRandomGenerator' consumer/smoke.cc
    git diff --check

# Validate tools, host compatibility, and the runner disk budget for one target.
prerequisites target:
    #!/usr/bin/env bash
    set -euo pipefail
    just --justfile "{{ root }}/Justfile" _validate-target "{{ target }}"
    for tool in git tar python3; do command -v "$tool" >/dev/null || { echo "missing prerequisite: $tool" >&2; exit 1; }; done
    case "{{ target }}" in
      windows-*) command -v cl.exe >/dev/null || { echo 'Windows core builds require an MSVC developer shell (cl.exe)' >&2; exit 1; } ;;
      macos-*|ios-*) command -v xcrun >/dev/null || { echo 'Apple core builds require Xcode command-line tools' >&2; exit 1; } ;;
      android-*) case "$(uname -s)" in Linux|Darwin) ;; *) echo 'Android builds require a Linux or macOS host' >&2; exit 1;; esac ;;
      linux-*) test "$(uname -s)" = Linux || { echo 'Linux builds require a Linux host' >&2; exit 1; } ;;
    esac
    available_kib=$(df -Pk "{{ root }}" | awk 'NR==2 {print $4}')
    required_kib=${WEBRTC_MIN_FREE_KIB:-36700160}
    test "$available_kib" -ge "$required_kib" || { echo "insufficient disk: ${available_kib} KiB free, ${required_kib} KiB required" >&2; exit 1; }

# Acquire depot_tools at the repository-owned immutable revision.
depot-tools:
    #!/usr/bin/env bash
    set -euo pipefail
    depot="{{ work }}/depot_tools"
    mkdir -p "{{ work }}"
    if test ! -d "$depot/.git"; then
      git init -q "$depot"
      git -C "$depot" remote add origin "{{ depot_tools_url }}"
    fi
    current=$(git -C "$depot" rev-parse HEAD 2>/dev/null || true)
    if test "$current" != "{{ depot_tools_commit }}"; then
      git -C "$depot" fetch --depth=1 origin "{{ depot_tools_commit }}"
      git -C "$depot" checkout --detach --force FETCH_HEAD
    fi
    test "$(git -C "$depot" rev-parse HEAD)" = "{{ depot_tools_commit }}"

# Create/sync a no-history checkout whose source HEAD must equal the immutable pin.
sync target:
    #!/usr/bin/env bash
    set -euo pipefail
    just --justfile "{{ root }}/Justfile" prerequisites "{{ target }}"
    just --justfile "{{ root }}/Justfile" depot-tools
    checkout="{{ work }}/checkout"
    src="$checkout/src"
    if test -d "$src/.git"; then
      test "$(git -C "$src" rev-parse HEAD)" = "{{ webrtc_commit }}" || { echo 'existing source checkout does not match the immutable pin; clean it explicitly before syncing' >&2; exit 1; }
      git -C "$src" diff --quiet && git -C "$src" diff --cached --quiet || { echo 'source checkout is modified; no patches are permitted' >&2; exit 1; }
    fi
    target_os=''
    case "{{ target }}" in android-*) target_os="target_os = ['android']";; ios-*) target_os="target_os = ['ios']";; esac
    mkdir -p "$checkout"
    printf "solutions = [{'name': 'src', 'url': '{{ webrtc_url }}', 'deps_file': 'DEPS', 'managed': False, 'custom_deps': {}, 'custom_vars': {}}]\n%s\n" "$target_os" > "$checkout/.gclient"
    export PATH="{{ work }}/depot_tools:$PATH"
    export DEPOT_TOOLS_UPDATE=0
    export GCLIENT_PY3=1
    export VPYTHON_VIRTUALENV_ROOT="{{ work }}/vpython"
    mkdir -p "{{ work }}/gsutil"
    printf '[GSUtil]\nstate_dir = {{ work }}/gsutil\n' > "{{ work }}/boto.cfg"
    export BOTO_CONFIG="{{ work }}/boto.cfg"
    cd "$checkout"
    gclient sync --no-history --shallow --nohooks --force --revision "src@{{ webrtc_commit }}"
    test "$(git -C "$src" rev-parse HEAD)" = "{{ webrtc_commit }}" || { echo 'source checkout does not match the immutable pin' >&2; exit 1; }
    git -C "$src" diff --quiet && git -C "$src" diff --cached --quiet || { echo 'source checkout is modified; no patches are permitted' >&2; exit 1; }
    gclient runhooks
    test "$(git -C "$src" rev-parse HEAD)" = "{{ webrtc_commit }}"

# Print the complete GN argument contract for a target.
gn-args target:
    #!/usr/bin/env bash
    set -euo pipefail
    just --justfile "{{ root }}/Justfile" _validate-target "{{ target }}"
    common='is_debug=false is_component_build=false rtc_include_tests=false rtc_build_examples=false rtc_build_tools=false rtc_use_h264=false rtc_include_internal_audio_device=false rtc_include_builtin_audio_codecs=true rtc_libvpx_build_vp9=true rtc_include_dav1d_in_internal_decoder_factory=true rtc_enable_protobuf=false symbol_level=0 use_siso=false treat_warnings_as_errors=false'
    case "{{ target }}" in
      linux-x86_64) platform='target_os="linux" target_cpu="x64" use_sysroot=true target_sysroot="//build/linux/debian_bullseye_amd64-sysroot" use_custom_libcxx=true rtc_use_x11=false rtc_use_pipewire=false' ;;
      linux-arm64) platform='target_os="linux" target_cpu="arm64" use_sysroot=true target_sysroot="//build/linux/debian_bullseye_arm64-sysroot" use_custom_libcxx=true rtc_use_x11=false rtc_use_pipewire=false' ;;
      windows-x86_64) platform='target_os="win" target_cpu="x64" is_clang=true use_lld=true' ;;
      macos-arm64) platform='target_os="mac" target_cpu="arm64" mac_sdk_min="12.0" use_lld=true use_custom_libcxx=false' ;;
      macos-x86_64) platform='target_os="mac" target_cpu="x64" mac_sdk_min="12.0" use_lld=true use_custom_libcxx=false' ;;
      android-arm64-v8a) platform='target_os="android" target_cpu="arm64" android_ndk_api_level=26 default_min_sdk_version=26 rtc_enable_android_aaudio=true' ;;
      android-x86_64) platform='target_os="android" target_cpu="x64" android_ndk_api_level=26 default_min_sdk_version=26 rtc_enable_android_aaudio=true' ;;
      ios-arm64) platform='target_os="ios" target_cpu="arm64" target_environment="device" ios_deployment_target="18.0" ios_enable_code_signing=false use_lld=true' ;;
      ios-simulator-arm64) platform='target_os="ios" target_cpu="arm64" target_environment="simulator" ios_deployment_target="18.0" ios_enable_code_signing=false use_lld=true' ;;
    esac
    printf '%s %s\n' "$common" "$platform"

# Generate an optimized core build directory without changing source.
configure target:
    #!/usr/bin/env bash
    set -euo pipefail
    just --justfile "{{ root }}/Justfile" _validate-source
    export PATH="{{ work }}/depot_tools:$PATH"
    export DEPOT_TOOLS_UPDATE=0
    export VPYTHON_VIRTUALENV_ROOT="{{ work }}/vpython"
    src="{{ work }}/checkout/src"
    out="{{ work }}/out/{{ target }}"
    args=$(just --justfile "{{ root }}/Justfile" gn-args "{{ target }}")
    gn=$(just --justfile "{{ root }}/Justfile" _gn-path)
    "$gn" gen "$out" --root="$src" --args="$args"
    printf '%s\n' "$args" > "$out/pulsebeam-gn-args.txt"

# Build upstream's complete static aggregate and its separately linked runtime.
build target:
    #!/usr/bin/env bash
    set -euo pipefail
    just --justfile "{{ root }}/Justfile" _validate-source
    export PATH="{{ work }}/depot_tools:$PATH"
    export DEPOT_TOOLS_UPDATE=0
    src="{{ work }}/checkout/src"
    out="{{ work }}/out/{{ target }}"
    test -f "$out/build.ninja" || { echo "not configured: {{ target }}" >&2; exit 1; }
    targets=(webrtc api/video_codecs:builtin_video_encoder_factory api/video_codecs:builtin_video_decoder_factory api/video_codecs:video_codecs_api)
    case "{{ target }}" in
      linux-*) targets+=(libc++ libc++abi) ;;
    esac
    "$src/third_party/ninja/ninja" -C "$out" "${targets[@]}"

# Package a self-contained link kit and provenance captured from the real build.
package target:
    #!/usr/bin/env bash
    set -euo pipefail
    just --justfile "{{ root }}/Justfile" _validate-source
    src="{{ work }}/checkout/src"
    checkout="{{ work }}/checkout"
    out="{{ work }}/out/{{ target }}"
    stage="{{ work }}/package/{{ artifact_prefix }}-{{ target }}"
    archive="{{ dist }}/{{ artifact_prefix }}-{{ target }}.tar.gz"
    test -f "$out/build.ninja" || { echo "not built: {{ target }}" >&2; exit 1; }
    gn=$(just --justfile "{{ root }}/Justfile" _gn-path)
    library=$("$gn" desc --root="$src" "$out" //:webrtc outputs)
    test -n "$library" && test "$(wc -l <<< "$library")" -eq 1 || { echo '//:webrtc must produce exactly one complete static archive' >&2; exit 1; }
    test -f "$library" || { echo 'complete WebRTC archive was not produced' >&2; exit 1; }
    rm -rf "$stage"
    mkdir -p "$stage/include" "$stage/lib" "$stage/licenses" "$stage/metadata" "{{ dist }}"
    llvm_ar="$src/third_party/llvm-build/Release+Asserts/bin/llvm-ar"
    package_archive() {
      local input=$1 output=$2
      if test "$(head -c 7 "$input")" = '!<thin>'; then
        printf 'CREATE %s\nADDLIB %s\nSAVE\nEND\n' "$output" "$input" | "$llvm_ar" -M
      else
        cp "$input" "$output"
      fi
      test "$(head -c 7 "$output")" != '!<thin>' || { echo "could not materialize thin archive: $input" >&2; exit 1; }
    }
    : > "$stage/metadata/archive-group.txt"
    for target_label in \
      //api/video_codecs:builtin_video_encoder_factory \
      //api/video_codecs:builtin_video_decoder_factory \
      //api/video_codecs:video_codecs_api \
      //api/video_codecs:rtc_software_fallback_wrappers \
      //media:rtc_internal_video_codecs \
      //media:rtc_simulcast_encoder_adapter \
      //:webrtc; do
      target_output=$("$gn" desc --root="$src" "$out" "$target_label" outputs)
      test -n "$target_output" && test "$(wc -l <<< "$target_output")" -eq 1 && test -f "$target_output" || { echo "required core archive is missing: $target_label" >&2; exit 1; }
      package_archive "$target_output" "$stage/lib/$(basename "$target_output")"
      printf 'lib/%s\n' "$(basename "$target_output")" >> "$stage/metadata/archive-group.txt"
      "$gn" desc --root="$src" "$out" "$target_label" deps --all
    done | LC_ALL=C sort -u > "$stage/metadata/target-deps.txt"
    if grep -Eq '^//third_party/(openh264|ffmpeg)(:|/)' "$stage/metadata/target-deps.txt"; then
      echo 'H.264 implementation payload is present in the configured target closure' >&2
      exit 1
    fi
    for source_root in api audio call common_audio common_video experiments logging media modules net p2p pc rtc_base sdk/objc/base system_wrappers video; do
      find "$src/$source_root" -type f \( -name '*.h' -o -name '*.hh' -o -name '*.hpp' -o -name '*.inc' \) -print |
        sed "s#^$src/##"
    done | LC_ALL=C sort -u > "$stage/metadata/header-files.txt"
    test -s "$stage/metadata/header-files.txt" || { echo 'GN target closure yielded no headers' >&2; exit 1; }
    tar -C "$src" -T "$stage/metadata/header-files.txt" -cf - | tar -xf - -C "$stage/include"
    for include_root in third_party/abseil-cpp third_party/perfetto/include third_party/libyuv/include; do
      (cd "$src/$include_root" && find . -type f \( -name '*.h' -o -name '*.hh' -o -name '*.hpp' -o -name '*.inc' \) -print0 | tar --null -T - -cf -) | tar -xf - -C "$stage/include"
    done
    if test -d "$out/gen"; then
      (cd "$out/gen" && find . -type f \( -name '*.h' -o -name '*.hh' -o -name '*.hpp' -o -name '*.inc' \) -print0 | tar --null -T - -cf -) | tar -xf - -C "$stage/include"
      test ! -f "$out/gen/third_party/perfetto/build_config/perfetto_build_flags.h" || cp "$out/gen/third_party/perfetto/build_config/perfetto_build_flags.h" "$stage/include/"
    fi
    printf '%s\n' 'include/' > "$stage/metadata/include-dirs.txt"
    if [[ "{{ target }}" = linux-* ]]; then
      mkdir -p "$stage/runtime/include/libcxx" "$stage/runtime/include/libcxxabi" "$stage/runtime/lib"
      libcxx_output=$("$gn" desc --root="$src" "$out" //buildtools/third_party/libc++ outputs)
      libcxxabi_output=$("$gn" desc --root="$src" "$out" //buildtools/third_party/libc++abi outputs)
      test -n "$libcxx_output" && test -n "$libcxxabi_output" && test "$(wc -l <<< "$libcxx_output")" -eq 1 && test "$(wc -l <<< "$libcxxabi_output")" -eq 1 || { echo 'GN did not declare one libc++ and libc++abi archive' >&2; exit 1; }
      test -f "$libcxx_output" && test -f "$libcxxabi_output" || { echo 'required bundled C++ runtime archive is missing' >&2; exit 1; }
      cp -R "$src/third_party/libc++/src/include/." "$stage/runtime/include/libcxx/"
      cp "$src/buildtools/third_party/libc++/__config_site" "$src/buildtools/third_party/libc++/__assertion_handler" "$stage/runtime/include/libcxx/"
      cp -R "$src/third_party/libc++abi/src/include/." "$stage/runtime/include/libcxxabi/"
      for runtime_archive in "$libcxx_output" "$libcxxabi_output"; do
        packaged_archive="$stage/runtime/lib/$(basename "$runtime_archive")"
        package_archive "$runtime_archive" "$packaged_archive"
      done
      printf '%s\n' 'runtime/lib/libc++.a' 'runtime/lib/libc++abi.a' >> "$stage/metadata/archive-group.txt"
      printf '%s\n' 'runtime/include/libcxx/' 'runtime/include/libcxxabi/' >> "$stage/metadata/include-dirs.txt"
      "$gn" desc --root="$src" "$out" //:webrtc libs --all --blame > "$stage/metadata/upstream-link-inputs.txt"
      grep -Fq 'libclang_rt.builtins.a' "$stage/metadata/upstream-link-inputs.txt" || { echo 'GN closure does not declare the pinned compiler runtime' >&2; exit 1; }
    fi
    export PATH="{{ work }}/depot_tools:$PATH"
    export DEPOT_TOOLS_UPDATE=0
    export VPYTHON_VIRTUALENV_ROOT="{{ work }}/vpython"
    vpython3 "$src/tools_webrtc/libs/generate_licenses.py" --target //:webrtc "$stage/licenses" "$out"
    cp "$src/LICENSE" "$stage/licenses/WEBRTC-BSD.txt"
    test -f "$src/PATENTS" && cp "$src/PATENTS" "$stage/licenses/PATENTS" || true
    cp "{{ root }}/LICENSE" "$stage/licenses/REPOSITORY-APACHE-2.0.txt"
    cp "$out/pulsebeam-gn-args.txt" "$stage/metadata/gn-args.txt"
    (cd "$checkout" && gclient revinfo -a) > "$stage/metadata/resolved-deps.txt"
    just --justfile "{{ root }}/Justfile" _write-link-metadata "{{ target }}" "$stage/metadata"
    runner=${ImageOS:-local}; compiler=$(just --justfile "{{ root }}/Justfile" _toolchain "{{ target }}")
    recipe_sha=$(git -C "{{ root }}" ls-files -co --exclude-standard Justfile consumer | LC_ALL=C sort | while read -r file; do cat "{{ root }}/$file"; done | { if command -v sha256sum >/dev/null; then sha256sum; else shasum -a 256; fi; } | awk '{print $1}')
    deps_lines=$(sed 's/\\/\\\\/g; s/"/\\"/g; s/^/    "/; s/$/",/' "$stage/metadata/resolved-deps.txt" | sed '$ s/,$//')
    cat > "$stage/manifest.json" <<EOF
    {
      "schema_version": 1,
      "artifact": "{{ artifact_prefix }}-{{ target }}",
      "source_url": "{{ webrtc_url }}",
      "source_commit": "{{ webrtc_commit }}",
      "source_milestone": {{ milestone }},
      "recipe_revision": {{ recipe_revision }},
      "recipe_git_commit": "$(git -C "{{ root }}" rev-parse HEAD)",
      "recipe_sha256": "$recipe_sha",
      "target": "{{ target }}",
      "flavor": "{{ flavor }}",
      "optimized": true,
      "component_build": false,
      "h264": false,
      "runner": "$runner",
      "toolchain": "${compiler//\"/\\\"}",
      "runtime": "$(just --justfile "{{ root }}/Justfile" _runtime "{{ target }}")",
      "target_os": "$(just --justfile "{{ root }}/Justfile" _target-os "{{ target }}")",
      "target_cpu": "$(just --justfile "{{ root }}/Justfile" _target-cpu "{{ target }}")",
      "runtime_floor": "$(just --justfile "{{ root }}/Justfile" _runtime-floor "{{ target }}")",
      "gn_args_file": "metadata/gn-args.txt",
      "link_metadata": "metadata/link-flags.txt",
      "resolved_deps": [
    $deps_lines
      ]
    }
    EOF
    checksum() { if command -v sha256sum >/dev/null; then sha256sum "$@"; else shasum -a 256 "$@"; fi; }
    (cd "$stage" && find . -type f ! -name SHA256SUMS -print | LC_ALL=C sort | while read -r file; do checksum "$file"; done > SHA256SUMS)
    tar -czf "$archive" -C "$(dirname "$stage")" "$(basename "$stage")"
    (cd "{{ dist }}" && checksum "$(basename "$archive")" > "$(basename "$archive").sha256")
    printf '%s\n' "$archive"

# Verify one archive from a fresh extraction, compile/linking only packaged inputs.
verify-core target:
    #!/usr/bin/env bash
    set -euo pipefail
    just --justfile "{{ root }}/Justfile" _validate-target "{{ target }}"
    archive="{{ dist }}/{{ artifact_prefix }}-{{ target }}.tar.gz"
    test -f "$archive" || { echo "missing artifact: $archive" >&2; exit 1; }
    verify=$(mktemp -d "${TMPDIR:-/tmp}/webrtc-core-verify.XXXXXX")
    trap 'rm -rf "$verify"' EXIT
    tar -xzf "$archive" -C "$verify"
    kit="$verify/{{ artifact_prefix }}-{{ target }}"
    checksum() { if command -v sha256sum >/dev/null; then sha256sum "$@"; else shasum -a 256 "$@"; fi; }
    (cd "$kit" && checksum -c SHA256SUMS >/dev/null)
    grep -Fq '"source_commit": "{{ webrtc_commit }}"' "$kit/manifest.json"
    grep -Fq '"target": "{{ target }}"' "$kit/manifest.json"
    grep -Fq 'rtc_use_h264=false' "$kit/metadata/gn-args.txt"
    grep -Fq 'rtc_include_internal_audio_device=false' "$kit/metadata/gn-args.txt"
    python3 -m json.tool "$kit/manifest.json" >/dev/null
    library=$(find "$kit/lib" -type f \( -name 'libwebrtc.a' -o -name 'webrtc.lib' \) -print -quit)
    test -n "$library" && test -s "$library"
    just --justfile "{{ root }}/Justfile" _verify-archive-architecture "{{ target }}" "$library"
    while read -r relative_archive; do
      test -s "$kit/$relative_archive"
      test "$(head -c 7 "$kit/$relative_archive")" != '!<thin>' || { echo "thin archive escaped into package: $relative_archive" >&2; exit 1; }
    done < "$kit/metadata/archive-group.txt"
    if command -v ar >/dev/null; then
      while read -r relative_archive; do
        if ar t "$kit/$relative_archive" | grep -Eqi 'openh264|ffmpeg.*h264|h264.*ffmpeg'; then
          echo "forbidden built-in H.264 payload found: $relative_archive" >&2
          exit 1
        fi
      done < "$kit/metadata/archive-group.txt"
    fi
    just --justfile "{{ root }}/Justfile" _consumer-link "{{ target }}" "$kit" "{{ root }}/consumer/link.cc" "$verify/link-consumer"
    case "{{ target }}:$(uname -s):$(uname -m)" in
      linux-x86_64:Linux:x86_64|macos-arm64:Darwin:arm64|macos-x86_64:Darwin:x86_64)
        just --justfile "{{ root }}/Justfile" _consumer-link "{{ target }}" "$kit" "{{ root }}/consumer/smoke.cc" "$verify/smoke-consumer"
        "$verify/smoke-consumer"
        ;;
      windows-x86_64:MINGW*:x86_64|windows-x86_64:MSYS*:x86_64)
        just --justfile "{{ root }}/Justfile" _consumer-link "{{ target }}" "$kit" "{{ root }}/consumer/smoke.cc" "$verify/smoke-consumer"
        "$verify/smoke-consumer.exe"
        ;;
    esac
    if [[ "{{ target }}" = linux-* ]]; then
      just --justfile "{{ root }}/Justfile" _verify-linux-binary "$verify/link-consumer"
    fi

# Acquire, configure, build, package, and verify one target.
core target:
    just --justfile "{{ root }}/Justfile" sync "{{ target }}"
    just --justfile "{{ root }}/Justfile" configure "{{ target }}"
    just --justfile "{{ root }}/Justfile" build "{{ target }}"
    just --justfile "{{ root }}/Justfile" package "{{ target }}"
    just --justfile "{{ root }}/Justfile" verify-core "{{ target }}"

# Verify that the complete nine-artifact matrix is present and independently valid.
verify-core-matrix:
    #!/usr/bin/env bash
    set -euo pipefail
    while read -r target; do just --justfile "{{ root }}/Justfile" verify-core "$target"; done < <(just --justfile "{{ root }}/Justfile" list-targets)

# Remove disposable source/build/package output for one target; release archives remain.
clean target:
    #!/usr/bin/env bash
    set -euo pipefail
    just --justfile "{{ root }}/Justfile" _validate-target "{{ target }}"
    rm -rf "{{ work }}/out/{{ target }}" "{{ work }}/package/{{ artifact_prefix }}-{{ target }}"

_validate-target target:
    @case "{{ target }}" in linux-x86_64|linux-arm64|windows-x86_64|macos-arm64|macos-x86_64|android-arm64-v8a|android-x86_64|ios-arm64|ios-simulator-arm64) ;; *) echo "unsupported target: {{ target }}" >&2; exit 1;; esac

_validate-source:
    #!/usr/bin/env bash
    set -euo pipefail
    src="{{ work }}/checkout/src"
    test -d "$src/.git" || { echo 'source is not synced' >&2; exit 1; }
    test "$(git -C "$src" rev-parse HEAD)" = "{{ webrtc_commit }}" || { echo 'source checkout does not match the immutable pin' >&2; exit 1; }
    git -C "$src" diff --quiet && git -C "$src" diff --cached --quiet || { echo 'source checkout is modified; no patches are permitted' >&2; exit 1; }

_gn-path:
    #!/usr/bin/env bash
    set -euo pipefail
    src="{{ work }}/checkout/src"
    case "$(uname -s)" in
      Linux) gn="$src/buildtools/linux64/gn" ;;
      Darwin) gn="$src/buildtools/mac/gn" ;;
      MINGW*|MSYS*|CYGWIN*) gn="$src/buildtools/win/gn.exe" ;;
      *) echo 'unsupported build host' >&2; exit 1 ;;
    esac
    test -x "$gn" || { echo "pinned GN binary is missing: $gn" >&2; exit 1; }
    printf '%s\n' "$gn"

_runtime target:
    #!/usr/bin/env bash
    case "{{ target }}" in
      linux-*) printf '%s' 'Debian Bullseye sysroot; glibc >=2.31; bundled libc++ and libc++abi static archives; pinned Clang compiler-rt; sysroot libgcc_s unwinder; no host C++ standard library' ;;
      windows-*) printf '%s' 'Windows 10; MSVC C++ ABI; static multithreaded CRT /MT' ;;
      macos-*) printf '%s' 'macOS >=12.0; platform libc++; system frameworks' ;;
      android-*) printf '%s' 'Android API >=26; pinned NDK libc++; AAudio available; static WebRTC' ;;
      ios-*) printf '%s' 'iOS >=18.0; platform libc++; system frameworks' ;;
    esac

_toolchain target:
    #!/usr/bin/env bash
    set -euo pipefail
    src="{{ work }}/checkout/src"
    case "{{ target }}" in
      linux-*) "$src/third_party/llvm-build/Release+Asserts/bin/clang++" --version | head -1 ;;
      windows-*) printf 'MSVC %s; ' "$(cl.exe 2>&1 | sed -n '1p')"; "$src/third_party/llvm-build/Release+Asserts/bin/clang-cl" --version | head -1 ;;
      macos-*) printf 'Xcode %s; SDK %s; ' "$(xcodebuild -version | tr '\n' ' ')" "$(xcrun --sdk macosx --show-sdk-version)"; "$src/third_party/llvm-build/Release+Asserts/bin/clang++" --version | head -1 ;;
      android-*) printf 'NDK %s; ' "$(sed -n 's/^Pkg.Revision[[:space:]]*=[[:space:]]*//p' "$src/third_party/android_toolchain/ndk/source.properties")"; find "$src/third_party/android_toolchain/ndk/toolchains/llvm/prebuilt" -path '*/bin/clang++' -type f -print -quit | xargs -r -I{} {} --version | head -1 ;;
      ios-*) printf 'Xcode %s; SDK %s; ' "$(xcodebuild -version | tr '\n' ' ')" "$(xcrun --sdk iphoneos --show-sdk-version)"; "$src/third_party/llvm-build/Release+Asserts/bin/clang++" --version | head -1 ;;
    esac

_write-link-metadata target metadata:
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{ target }}" in
      linux-*)
        src="{{ work }}/checkout/src"
        out="{{ work }}/out/{{ target }}"
        gn=$(just --justfile "{{ root }}/Justfile" _gn-path)
        definitions=$("$gn" desc --root="$src" "$out" //:webrtc defines --all)
        runtime_definitions=$(grep -E '^(_LIBCPP|_LIBCXXABI|CR_LIBCXX_REVISION)' <<< "$definitions")
        for required in _LIBCPP_HARDENING_MODE _LIBCPP_DISABLE_VISIBILITY_ANNOTATIONS _LIBCXXABI_DISABLE_VISIBILITY_ANNOTATIONS _LIBCPP_INSTRUMENTED_WITH_ASAN CR_LIBCXX_REVISION; do
          grep -Eq "^${required}(=|$)" <<< "$runtime_definitions" || { echo "GN closure is missing required bundled-runtime definition: $required" >&2; exit 1; }
        done
        runtime_flags=$(sed 's/^/-D/' <<< "$runtime_definitions" | tr '\n' ' ')
        compile="-std=c++20 -fno-exceptions -fno-rtti -nostdinc++ -isystem@KIT@/runtime/include/libcxx -isystem@KIT@/runtime/include/libcxxabi -Wno-nullability-completeness -DWEBRTC_POSIX -DWEBRTC_LINUX -DABSL_ALLOCATOR_NOTHROW=1 ${runtime_flags% } -pthread"
        link='-fuse-ld=lld -nostdlib++ -Wl,--start-group @ARCHIVES@ -Wl,--end-group -pthread -ldl -lrt -lm'
        ;;
      windows-*) compile='/std:c++20 /GR- /EHs-c- /MT /DWEBRTC_WIN /DWIN32_LEAN_AND_MEAN /DNOMINMAX /DABSL_ALLOCATOR_NOTHROW=1'; link='advapi32.lib bcrypt.lib crypt32.lib dmoguids.lib iphlpapi.lib msdmo.lib secur32.lib strmiids.lib winmm.lib ws2_32.lib' ;;
      macos-*) compile='-std=c++20 -fno-exceptions -fno-rtti -DWEBRTC_POSIX -DWEBRTC_MAC -DABSL_ALLOCATOR_NOTHROW=1'; link='-framework Foundation -framework CoreFoundation -framework CoreGraphics -framework CoreMedia -framework CoreVideo -framework AudioToolbox -framework AVFoundation' ;;
      android-*) compile='-std=c++20 -fno-exceptions -fno-rtti -DWEBRTC_POSIX -DWEBRTC_LINUX -DWEBRTC_ANDROID -DABSL_ALLOCATOR_NOTHROW=1'; link='-static-libstdc++ -llog -landroid -ldl -lm' ;;
      ios-*) compile='-std=c++20 -fno-exceptions -fno-rtti -DWEBRTC_POSIX -DWEBRTC_IOS -DWEBRTC_MAC -DABSL_ALLOCATOR_NOTHROW=1'; link='-framework Foundation -framework CoreFoundation -framework CoreGraphics -framework CoreMedia -framework CoreVideo -framework AudioToolbox -framework AVFoundation' ;;
    esac
    printf '%s\n' "$compile" > "{{ metadata }}/compile-flags.txt"
    printf '%s\n' "$link" > "{{ metadata }}/link-flags.txt"
    just --justfile "{{ root }}/Justfile" _runtime "{{ target }}" > "{{ metadata }}/runtime.txt"

_consumer-link target kit source output:
    #!/usr/bin/env bash
    set -euo pipefail
    src="{{ work }}/checkout/src"
    archives=()
    while read -r relative_archive; do archives+=("{{ kit }}/$relative_archive"); done < "{{ kit }}/metadata/archive-group.txt"
    compile=$(sed 's#@KIT@#{{ kit }}#g' "{{ kit }}/metadata/compile-flags.txt")
    link=$(sed 's#@KIT@#{{ kit }}#g' "{{ kit }}/metadata/link-flags.txt")
    read -r -a compile_flags <<< "$compile"
    read -r -a link_flags <<< "$link"
    case "{{ target }}" in
      linux-x86_64) cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang++"; extra=(--target=x86_64-linux-gnu --sysroot="$src/build/linux/debian_bullseye_amd64-sysroot") ;;
      linux-arm64) cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang++"; extra=(--target=aarch64-linux-gnu --sysroot="$src/build/linux/debian_bullseye_arm64-sysroot") ;;
      android-arm64-v8a) for candidate in "$src/third_party/android_toolchain/ndk/toolchains/llvm/prebuilt"/*; do test ! -d "$candidate" || { prebuilt=$candidate; break; }; done; cxx="$prebuilt/bin/clang++"; extra=(--target=aarch64-linux-android26) ;;
      android-x86_64) for candidate in "$src/third_party/android_toolchain/ndk/toolchains/llvm/prebuilt"/*; do test ! -d "$candidate" || { prebuilt=$candidate; break; }; done; cxx="$prebuilt/bin/clang++"; extra=(--target=x86_64-linux-android26) ;;
      macos-arm64) cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang++"; sdk=$(xcrun --sdk macosx --show-sdk-path); extra=(-arch arm64 -isysroot "$sdk" -mmacosx-version-min=12.0) ;;
      macos-x86_64) cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang++"; sdk=$(xcrun --sdk macosx --show-sdk-path); extra=(-arch x86_64 -isysroot "$sdk" -mmacosx-version-min=12.0) ;;
      ios-arm64) cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang++"; sdk=$(xcrun --sdk iphoneos --show-sdk-path); extra=(-arch arm64 -isysroot "$sdk" -miphoneos-version-min=18.0) ;;
      ios-simulator-arm64) cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang++"; sdk=$(xcrun --sdk iphonesimulator --show-sdk-path); extra=(-arch arm64 -isysroot "$sdk" -mios-simulator-version-min=18.0) ;;
      windows-x86_64) "$src/third_party/llvm-build/Release+Asserts/bin/clang-cl" "${compile_flags[@]}" /I"{{ kit }}/include" "{{ source }}" "${archives[@]}" "${link_flags[@]}" "/Fe:{{ output }}.exe"; exit ;;
    esac
    final_link=()
    for flag in "${link_flags[@]}"; do
      if test "$flag" = @ARCHIVES@; then final_link+=("${archives[@]}"); else final_link+=("$flag"); fi
    done
    if [[ "{{ target }}" != linux-* ]]; then final_link=("${archives[@]}" "${final_link[@]}"); fi
    "$cxx" "${extra[@]}" "${compile_flags[@]}" -I"{{ kit }}/include" "{{ source }}" "${final_link[@]}" -o "{{ output }}"

_target-os target:
    @case "{{ target }}" in linux-*) printf linux;; windows-*) printf windows;; macos-*) printf macos;; android-*) printf android;; ios-*) printf ios;; esac

_target-cpu target:
    @case "{{ target }}" in *x86_64) printf x86_64;; *) printf arm64;; esac

_runtime-floor target:
    @case "{{ target }}" in linux-*) printf 'glibc 2.31';; windows-*) printf 'Windows 10';; macos-*) printf 'macOS 12';; android-*) printf 'Android API 26';; ios-*) printf 'iOS 18';; esac

_verify-linux-binary binary:
    #!/usr/bin/env bash
    set -euo pipefail
    needed=$(readelf -d "{{ binary }}" | sed -n 's/.*Shared library: \[\([^]]*\)\].*/\1/p')
    grep -Eqi 'lib(std|c)\+\+' <<< "$needed" && { echo 'consumer dynamically depends on a C++ standard library' >&2; exit 1; }
    grep -Eqi 'lib(X11|wayland|pulse|asound)' <<< "$needed" && { echo 'headless core has a native device/window dependency' >&2; exit 1; }
    newest=$(readelf --version-info "{{ binary }}" | grep -oE 'GLIBC_[0-9]+\.[0-9]+' | cut -d_ -f2 | sort -Vu | tail -1)
    test -n "$newest" && test "$(printf '%s\n' "$newest" 2.31 | sort -V | tail -1)" = 2.31 || { echo "consumer requires GLIBC_$newest, newer than 2.31" >&2; exit 1; }

_verify-archive-architecture target archive:
    #!/usr/bin/env bash
    set -euo pipefail
    src="{{ work }}/checkout/src"
    llvm_ar="$src/third_party/llvm-build/Release+Asserts/bin/llvm-ar"
    llvm_readobj="$src/third_party/llvm-build/Release+Asserts/bin/llvm-readobj"
    inspect=$(mktemp -d "${TMPDIR:-/tmp}/webrtc-archive-inspect.XXXXXX")
    trap 'rm -rf "$inspect"' EXIT
    member=$("$llvm_ar" t "{{ archive }}" | sed -n '1p')
    test -n "$member" || { echo 'WebRTC archive has no members' >&2; exit 1; }
    (cd "$inspect" && "$llvm_ar" x "{{ archive }}" "$member")
    architecture=$("$llvm_readobj" --file-headers "$inspect/$member" | sed -n 's/^Arch: //p')
    case "{{ target }}" in
      *x86_64) grep -Eq '^(x86_64|amd64)$' <<< "$architecture" ;;
      *) grep -Eq '^(aarch64|arm64)$' <<< "$architecture" ;;
    esac || { echo "archive architecture mismatch for {{ target }}: $architecture" >&2; exit 1; }
