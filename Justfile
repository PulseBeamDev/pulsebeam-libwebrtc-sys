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
github_repository := "PulseBeamDev/webrtc-build"
release_tag := "m" + milestone + "-r" + recipe_revision
core_prefix := "pulsebeam-libwebrtc-m" + milestone + "-r" + recipe_revision + "-core"
native_prefix := "pulsebeam-libwebrtc-m" + milestone + "-r" + recipe_revision + "-native"

default: check

# Print the shared core/native build matrix without consulting the network.
list-targets:
    @printf '%s\n' linux-x86_64 linux-arm64 windows-x86_64 macos-arm64 macos-x86_64 android-arm64-v8a android-x86_64 ios-arm64 ios-simulator-arm64

# Print the release build jobs as target, flavor, and concrete runner TSV.
list-release-jobs:
    @printf '%s\n' \
      'linux-x86_64	core	ubuntu-24.04' \
      'linux-arm64	core	ubuntu-24.04' \
      'windows-x86_64	core	windows-2025' \
      'macos-arm64	core	macos-15' \
      'macos-x86_64	core	macos-15-intel' \
      'android-arm64-v8a	core	ubuntu-24.04' \
      'android-x86_64	core	ubuntu-24.04' \
      'ios-arm64	core	macos-15' \
      'ios-simulator-arm64	core	macos-15' \
      'linux-x86_64	native	ubuntu-24.04' \
      'linux-arm64	native	ubuntu-24.04' \
      'windows-x86_64	native	windows-2025' \
      'macos-arm64	native	macos-15' \
      'macos-x86_64	native	macos-15-intel' \
      'android-x86_64	native	ubuntu-24.04' \
      'ios-simulator-arm64	native	macos-15'

# Emit the workflow matrix from the authoritative TSV above.
release-matrix:
    @just --justfile "{{ root }}/Justfile" list-release-jobs | python3 -c 'import csv,json,sys; print(json.dumps({"include": [{"target": r[0], "flavor": r[1], "runner": r[2]} for r in csv.reader(sys.stdin, delimiter="\t")]}))'

# Print the deterministic release files produced by one build job.
release-job-assets target flavor tag=release_tag:
    #!/usr/bin/env bash
    set -euo pipefail
    justfile="{{ root }}/Justfile"
    just --justfile "$justfile" _validate-release-tag "{{ tag }}"
    just --justfile "$justfile" _validate-target "{{ target }}"
    just --justfile "$justfile" _validate-flavor "{{ flavor }}"
    runner=$(just --justfile "$justfile" list-release-jobs | awk -F '\t' -v t='{{ target }}' -v f='{{ flavor }}' '$1 == t && $2 == f {print $3}')
    test -n "$runner" || { echo 'target/flavor is not a release job' >&2; exit 1; }
    prefix="pulsebeam-libwebrtc-{{ tag }}-{{ flavor }}"
    case "{{ flavor }}:{{ target }}" in
      native:android-x86_64) printf '%s\n' "$prefix-android.tar.gz" "$prefix-android.tar.gz.sha256" "$prefix-android.aar" "$prefix-android.aar.sha256" "$prefix-android.jar" "$prefix-android.jar.sha256" ;;
      native:ios-simulator-arm64) printf '%s\n' "$prefix-ios-xcframework.tar.gz" "$prefix-ios-xcframework.tar.gz.sha256" ;;
      *) printf '%s\n' "$prefix-{{ target }}.tar.gz" "$prefix-{{ target }}.tar.gz.sha256" ;;
    esac

# Print every payload and sidecar expected in a release, exactly once.
release-assets tag=release_tag:
    #!/usr/bin/env bash
    set -euo pipefail
    justfile="{{ root }}/Justfile"
    while IFS=$'\t' read -r target flavor runner; do
      just --justfile "$justfile" release-job-assets "$target" "$flavor" "{{ tag }}"
    done < <(just --justfile "$justfile" list-release-jobs)

# Print the identity reviewers and operators must approve.
release-identity:
    #!/usr/bin/env bash
    set -euo pipefail
    printf 'repository=%s\ntag=%s\nsource_url=%s\nsource_commit=%s\nmilestone=%s\nrecipe_revision=%s\nrecipe_sha256=%s\n' \
      '{{ github_repository }}' '{{ release_tag }}' '{{ webrtc_url }}' '{{ webrtc_commit }}' '{{ milestone }}' '{{ recipe_revision }}' \
      "$(just --justfile "{{ root }}/Justfile" _recipe-sha)"

# Quick repository checks. This intentionally does not acquire or build WebRTC.
check:
    #!/usr/bin/env bash
    set -euo pipefail
    cd "{{ root }}"
    test "$(git ls-files | grep -Ev '^(README.md|LICENSE|Justfile|\.gitignore|\.github/workflows/release\.yml|consumer/)' || true)" = ""
    test "$(just --summary | tr ' ' '\n' | grep -E '^(sync|configure|build|package|verify-core|core|native|package-native-android|package-native-ios|verify-native|verify-native-matrix|clean)$' | wc -l)" -eq 12
    test "$(just list-targets | wc -l)" -eq 9
    grep -Fq '{{ webrtc_commit }}' Justfile
    grep -Fq 'rtc_use_h264=false' Justfile
    grep -Fq 'rtc_include_internal_audio_device=false' Justfile
    grep -Fq 'is_component_build=false' Justfile
    grep -Fq 'is_debug=false' Justfile
    just gn-args linux-x86_64 core | grep -Fq 'use_custom_libcxx=true'
    just gn-args linux-x86_64 native | grep -Fq 'rtc_include_internal_audio_device=true'
    just gn-args linux-x86_64 native | grep -Fq 'rtc_use_pipewire=true'
    just gn-args linux-x86_64 native | grep -Fq 'rtc_use_x11=true'
    ! grep -Fq 'system libstdc++' Justfile
    grep -Fq 'CreateModularPeerConnectionFactory' consumer/smoke.cc
    grep -Fq 'SetRandomGenerator' consumer/smoke.cc
    grep -Fq 'CreateAudioDeviceModule' consumer/native.cc
    grep -Fq 'ScreenCapturerAndroid' consumer/android/NativePackageProbe.java
    grep -Fq 'DefaultVideoEncoderFactory' consumer/android/NativePackageProbe.java
    grep -Fq 'RTCCameraVideoCapturer' consumer/apple/NativePackageProbe.mm
    grep -Fq 'RTCDefaultVideoEncoderFactory' consumer/apple/NativePackageProbe.mm
    just release-dry-run "{{ release_tag }}" >/dev/null
    git diff --check

# Validate a concrete upstream commit and update only the reviewed identity pins.
update-source commit milestone_number revision:
    #!/usr/bin/env bash
    set -euo pipefail
    [[ "{{ commit }}" =~ ^[0-9a-f]{40}$ ]] || { echo 'source commit must be a lowercase 40-character SHA' >&2; exit 1; }
    [[ "{{ milestone_number }}" =~ ^[1-9][0-9]*$ ]] || { echo 'milestone must be a positive integer' >&2; exit 1; }
    [[ "{{ revision }}" =~ ^[1-9][0-9]*$ ]] || { echo 'recipe revision must be a positive integer' >&2; exit 1; }
    probe=$(mktemp -d "${TMPDIR:-/tmp}/webrtc-source-probe.XXXXXX")
    trap 'rm -rf "$probe"' EXIT
    git -C "$probe" init -q
    git -C "$probe" fetch -q --depth=1 "{{ webrtc_url }}" "{{ commit }}" || { echo 'source commit is not fetchable from the authoritative repository' >&2; exit 1; }
    test "$(git -C "$probe" rev-parse FETCH_HEAD)" = "{{ commit }}" || { echo 'source fetch did not resolve to the requested immutable commit' >&2; exit 1; }
    python3 - "{{ root }}/Justfile" "{{ commit }}" "{{ milestone_number }}" "{{ revision }}" <<'PY'
    import pathlib, re, sys
    path = pathlib.Path(sys.argv[1])
    text = path.read_text()
    replacements = {
        "webrtc_commit": sys.argv[2],
        "milestone": sys.argv[3],
        "recipe_revision": sys.argv[4],
    }
    for name, value in replacements.items():
        text, count = re.subn(rf'(?m)^{name} := "[^"]+"$', f'{name} := "{value}"', text)
        if count != 1:
            raise SystemExit(f"expected exactly one {name} assignment")
    path.write_text(text)
    PY
    just --justfile "{{ root }}/Justfile" release-identity

# Validate the complete dispatch contract without network access or publication.
release-dry-run tag:
    #!/usr/bin/env bash
    set -euo pipefail
    justfile="{{ root }}/Justfile"
    just --justfile "$justfile" _assert-current-release-tag "{{ tag }}"
    matrix=$(just --justfile "$justfile" release-matrix)
    python3 - "$matrix" <<'PY'
    import json, sys
    matrix = json.loads(sys.argv[1])["include"]
    assert len(matrix) == 16, "release matrix must contain 16 jobs"
    keys = {(entry["target"], entry["flavor"]) for entry in matrix}
    assert len(keys) == len(matrix), "release matrix jobs must be unique"
    allowed = {"ubuntu-24.04", "windows-2025", "macos-15", "macos-15-intel"}
    assert {entry["runner"] for entry in matrix} <= allowed, "release matrix contains a non-standard runner"
    PY
    mapfile -t assets < <(just --justfile "$justfile" release-assets "{{ tag }}")
    test "${#assets[@]}" -eq 36 || { echo 'release must contain exactly 36 build assets and sidecars' >&2; exit 1; }
    test "$(printf '%s\n' "${assets[@]}" | LC_ALL=C sort -u | wc -l)" -eq "${#assets[@]}" || { echo 'release asset names must be unique' >&2; exit 1; }
    test "$(just --justfile "$justfile" list-targets | wc -l)" -eq 9
    test -f "{{ root }}/.github/workflows/release.yml" || { echo 'release workflow is missing' >&2; exit 1; }
    test "$(grep -Ec '^[[:space:]]+(push|pull_request|schedule):' "{{ root }}/.github/workflows/release.yml" || true)" -eq 0 || { echo 'release workflow must be manual-only' >&2; exit 1; }
    grep -Eq '^on:[[:space:]]*$' "{{ root }}/.github/workflows/release.yml"
    grep -Eq '^[[:space:]]+workflow_dispatch:' "{{ root }}/.github/workflows/release.yml"
    ! grep -E '^[[:space:]]*(- )?uses:' "{{ root }}/.github/workflows/release.yml" | grep -Ev '@[0-9a-f]{40}([[:space:]#]|$)'
    printf 'would dispatch release.yml for %s at recipe commit %s (%s jobs, %s assets)\n' \
      '{{ tag }}' "$(git -C "{{ root }}" rev-parse HEAD)" "$(just --justfile "$justfile" list-release-jobs | wc -l)" "${#assets[@]}"

# Dispatch the only publishing workflow after local and remote immutability checks.
release tag:
    #!/usr/bin/env bash
    set -euo pipefail
    justfile="{{ root }}/Justfile"
    just --justfile "$justfile" release-dry-run "{{ tag }}"
    command -v gh >/dev/null || { echo 'release requires GitHub CLI (gh)' >&2; exit 1; }
    gh auth status >/dev/null
    git -C "{{ root }}" diff --quiet && git -C "{{ root }}" diff --cached --quiet || { echo 'tracked recipe state is dirty' >&2; exit 1; }
    branch=$(git -C "{{ root }}" branch --show-current)
    test -n "$branch" || { echo 'release dispatch requires a branch, not detached HEAD' >&2; exit 1; }
    head=$(git -C "{{ root }}" rev-parse HEAD)
    upstream=$(git -C "{{ root }}" rev-parse '@{upstream}' 2>/dev/null || true)
    test "$head" = "$upstream" || { echo 'release commit must be pushed to the configured upstream branch' >&2; exit 1; }
    just --justfile "$justfile" _validate-release-remote "{{ tag }}"
    gh workflow run release.yml --repo "{{ github_repository }}" --ref "$branch" -f tag="{{ tag }}" -f recipe_commit="$head"
    printf 'dispatched %s from %s at %s\n' '{{ tag }}' "$branch" "$head"

# Remove only explicitly expendable hosted-runner caches, then report capacity.
ci-reclaim target:
    #!/usr/bin/env bash
    set -euo pipefail
    test "${GITHUB_ACTIONS:-}" = true || { echo 'ci-reclaim is restricted to GitHub-hosted Actions runners' >&2; exit 1; }
    just --justfile "{{ root }}/Justfile" _validate-target "{{ target }}"
    case "$(uname -s)" in
      Linux)
        sudo rm -rf /usr/share/dotnet /usr/local/.ghcup /opt/hostedtoolcache/CodeQL
        ;;
      Darwin)
        selected_xcode=$(xcode-select -p); selected_xcode=${selected_xcode%/Contents/Developer}
        for xcode in /Applications/Xcode_*.app; do
          test -e "$xcode" || continue
          test "$xcode" = "$selected_xcode" || sudo rm -rf "$xcode"
        done
        test -n "${RUNNER_TOOL_CACHE:-}" && rm -rf "$RUNNER_TOOL_CACHE/CodeQL" || true
        ;;
      MINGW*|MSYS*|CYGWIN*)
        powershell.exe -NoProfile -Command 'Remove-Item -Recurse -Force -ErrorAction SilentlyContinue "C:\Program Files\dotnet", "C:\Program Files (x86)\Android", "C:\ghcup"'
        test -n "${RUNNER_TOOL_CACHE:-}" && rm -rf "$RUNNER_TOOL_CACHE/CodeQL" || true
        ;;
      *) echo 'unsupported hosted runner' >&2; exit 1 ;;
    esac
    available_kib=$(df -Pk "{{ root }}" | awk 'NR==2 {print $4}')
    required_kib=${WEBRTC_MIN_FREE_KIB:-36700160}
    test "$available_kib" -ge "$required_kib" || { echo "insufficient hosted-runner disk after safe cleanup: ${available_kib} KiB free, ${required_kib} KiB required; use a larger standard public runner image or reduce the build closure" >&2; exit 1; }

# Prepare the runtime-only test environment required by package verification.
ci-prepare target:
    #!/usr/bin/env bash
    set -euo pipefail
    test "${GITHUB_ACTIONS:-}" = true || { echo 'ci-prepare is restricted to GitHub Actions' >&2; exit 1; }
    case "{{ target }}" in
      ios-simulator-arm64)
        device=$(xcrun simctl list devices available -j | python3 -c 'import json,sys; data=json.load(sys.stdin)["devices"]; print(next(d["udid"] for runtime in sorted(data, reverse=True) for d in data[runtime] if d["name"].startswith("iPhone")))')
        xcrun simctl boot "$device" 2>/dev/null || true
        xcrun simctl bootstatus "$device" -b
        ;;
      *) : ;;
    esac

# Validate tools, host compatibility, and the runner disk budget for one target.
prerequisites target:
    #!/usr/bin/env bash
    set -euo pipefail
    just --justfile "{{ root }}/Justfile" _validate-target "{{ target }}"
    for tool in git tar python3; do command -v "$tool" >/dev/null || { echo "missing prerequisite: $tool" >&2; exit 1; }; done
    case "{{ target }}" in
      windows-*) command -v cl.exe >/dev/null && test -n "${WindowsSdkDir:-}" && test -n "${WindowsSDKVersion:-}" || { echo 'Windows builds require an MSVC developer shell with Windows SDK identity' >&2; exit 1; } ;;
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

# Print the complete GN argument contract for one target/flavor pair.
gn-args target flavor="core":
    #!/usr/bin/env bash
    set -euo pipefail
    just --justfile "{{ root }}/Justfile" _validate-target "{{ target }}"
    just --justfile "{{ root }}/Justfile" _validate-flavor "{{ flavor }}"
    common='is_debug=false is_component_build=false rtc_include_tests=false rtc_build_examples=false rtc_build_tools=false rtc_use_h264=false rtc_include_builtin_audio_codecs=true rtc_libvpx_build_vp9=true rtc_include_dav1d_in_internal_decoder_factory=true rtc_enable_protobuf=false symbol_level=0 use_siso=false treat_warnings_as_errors=false'
    case "{{ flavor }}" in
      core) common+=" rtc_include_internal_audio_device=false" ;;
      native) common+=" rtc_include_internal_audio_device=true" ;;
    esac
    case "{{ target }}" in
      linux-x86_64) platform='target_os="linux" target_cpu="x64" use_sysroot=true target_sysroot="//build/linux/debian_bullseye_amd64-sysroot" use_custom_libcxx=true' ;;
      linux-arm64) platform='target_os="linux" target_cpu="arm64" use_sysroot=true target_sysroot="//build/linux/debian_bullseye_arm64-sysroot" use_custom_libcxx=true' ;;
      windows-x86_64) platform='target_os="win" target_cpu="x64" is_clang=true use_lld=true' ;;
      macos-arm64) platform='target_os="mac" target_cpu="arm64" mac_sdk_min="12.0" use_lld=true use_custom_libcxx=false' ;;
      macos-x86_64) platform='target_os="mac" target_cpu="x64" mac_sdk_min="12.0" use_lld=true use_custom_libcxx=false' ;;
      android-arm64-v8a) platform='target_os="android" target_cpu="arm64" android_ndk_api_level=26 default_min_sdk_version=26 android_static_analysis="off" rtc_enable_android_aaudio=true' ;;
      android-x86_64) platform='target_os="android" target_cpu="x64" android_ndk_api_level=26 default_min_sdk_version=26 android_static_analysis="off" rtc_enable_android_aaudio=true' ;;
      ios-arm64) platform='target_os="ios" target_cpu="arm64" target_environment="device" ios_deployment_target="18.0" ios_enable_code_signing=false use_lld=true' ;;
      ios-simulator-arm64) platform='target_os="ios" target_cpu="arm64" target_environment="simulator" ios_deployment_target="18.0" ios_enable_code_signing=false use_lld=true' ;;
    esac
    case "{{ target }}:{{ flavor }}" in
      linux-*:core) platform+=' rtc_use_x11=false rtc_use_pipewire=false' ;;
      linux-*:native) platform+=' rtc_use_x11=true rtc_use_pipewire=true rtc_link_pipewire=false rtc_include_pulse_audio=true' ;;
    esac
    printf '%s %s\n' "$common" "$platform"

# Generate an optimized build directory without changing source.
configure target flavor="core":
    #!/usr/bin/env bash
    set -euo pipefail
    just --justfile "{{ root }}/Justfile" _validate-source
    just --justfile "{{ root }}/Justfile" _validate-flavor "{{ flavor }}"
    export PATH="{{ work }}/depot_tools:$PATH"
    export DEPOT_TOOLS_UPDATE=0
    export VPYTHON_VIRTUALENV_ROOT="{{ work }}/vpython"
    src="{{ work }}/checkout/src"
    out="{{ work }}/out/{{ flavor }}/{{ target }}"
    args=$(just --justfile "{{ root }}/Justfile" gn-args "{{ target }}" "{{ flavor }}")
    mkdir -p "$out"
    gn=$(just --justfile "{{ root }}/Justfile" _gn-path)
    "$gn" gen "$out" --root="$src" --args="$args"
    printf '%s\n' "$args" > "$out/pulsebeam-gn-args.txt"

# Build the target/flavor closure and its separately linked runtime.
build target flavor="core":
    #!/usr/bin/env bash
    set -euo pipefail
    just --justfile "{{ root }}/Justfile" _validate-source
    just --justfile "{{ root }}/Justfile" _validate-flavor "{{ flavor }}"
    export PATH="{{ work }}/depot_tools:$PATH"
    export DEPOT_TOOLS_UPDATE=0
    src="{{ work }}/checkout/src"
    out="{{ work }}/out/{{ flavor }}/{{ target }}"
    test -f "$out/build.ninja" || { echo "not configured: {{ target }}" >&2; exit 1; }
    targets=(webrtc api/video_codecs:builtin_video_encoder_factory api/video_codecs:builtin_video_decoder_factory api/video_codecs:video_codecs_api)
    case "{{ target }}" in
      linux-*) targets+=(libc++ libc++abi) ;;
    esac
    if test "{{ flavor }}" = native; then
      case "{{ target }}" in
        android-*) targets+=(sdk/android:libwebrtc sdk/android:libjingle_peerconnection_so) ;;
        ios-*) targets+=(sdk:framework_objc) ;;
        macos-*) targets+=(sdk:mac_framework_objc) ;;
      esac
    fi
    "$src/third_party/ninja/ninja" -C "$out" "${targets[@]}"

# Package a self-contained link kit and provenance captured from the real build.
# Native Android and iOS slices are staged here before their platform container
# recipes combine the compatible variants.
package target flavor="core":
    #!/usr/bin/env bash
    set -euo pipefail
    just --justfile "{{ root }}/Justfile" _validate-source
    just --justfile "{{ root }}/Justfile" _validate-flavor "{{ flavor }}"
    src="{{ work }}/checkout/src"
    checkout="{{ work }}/checkout"
    out="{{ work }}/out/{{ flavor }}/{{ target }}"
    artifact_prefix=$(just --justfile "{{ root }}/Justfile" _artifact-prefix "{{ flavor }}")
    stage="{{ work }}/package/$artifact_prefix-{{ target }}"
    archive="{{ dist }}/$artifact_prefix-{{ target }}.tar.gz"
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
    target_labels=(
      //api/video_codecs:builtin_video_encoder_factory
      //api/video_codecs:builtin_video_decoder_factory
      //api/video_codecs:video_codecs_api
      //api/video_codecs:rtc_software_fallback_wrappers
      //media:rtc_internal_video_codecs
      //media:rtc_simulcast_encoder_adapter
      //:webrtc
    )
    for target_label in "${target_labels[@]}"; do
      target_output=$("$gn" desc --root="$src" "$out" "$target_label" outputs)
      test -n "$target_output" && test "$(wc -l <<< "$target_output")" -eq 1 && test -f "$target_output" || { echo "required core archive is missing: $target_label" >&2; exit 1; }
      package_archive "$target_output" "$stage/lib/$(basename "$target_output")"
      printf 'lib/%s\n' "$(basename "$target_output")" >> "$stage/metadata/archive-group.txt"
      "$gn" desc --root="$src" "$out" "$target_label" deps --all
    done | LC_ALL=C sort -u > "$stage/metadata/target-deps.txt"
    if test "{{ flavor }}" = native; then
      case "{{ target }}" in
        android-*) native_labels=(//sdk/android:libwebrtc //sdk/android:libjingle_peerconnection_so) ;;
        ios-*) native_labels=(//sdk:framework_objc) ;;
        macos-*) native_labels=(//sdk:mac_framework_objc) ;;
        *) native_labels=() ;;
      esac
      for target_label in "${native_labels[@]}"; do
        "$gn" desc --root="$src" "$out" "$target_label" deps --all >> "$stage/metadata/target-deps.txt"
      done
      LC_ALL=C sort -u -o "$stage/metadata/target-deps.txt" "$stage/metadata/target-deps.txt"
    fi
    if grep -Eq '^//third_party/(openh264|ffmpeg)(:|/)' "$stage/metadata/target-deps.txt"; then
      echo 'H.264 implementation payload is present in the configured target closure' >&2
      exit 1
    fi
    if test "{{ flavor }}" = native; then
      case "{{ target }}" in
        android-*)
          grep -Fqx '//sdk/android:default_video_codec_factory_java' "$stage/metadata/target-deps.txt"
          grep -Fqx '//sdk/android:hwcodecs_java' "$stage/metadata/target-deps.txt"
          ;;
        macos-*|ios-*) grep -Fqx '//sdk:videotoolbox_objc' "$stage/metadata/target-deps.txt" ;;
      esac
    fi
    source_roots=(api audio call common_audio common_video experiments logging media modules net p2p pc rtc_base sdk/objc/base system_wrappers video)
    if test "{{ flavor }}" = native; then
      case "{{ target }}" in
        macos-*|ios-*) source_roots+=(sdk/objc/api sdk/objc/components sdk/objc/helpers sdk/objc/native) ;;
      esac
    fi
    for source_root in "${source_roots[@]}"; do
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
    if test "{{ flavor }}" = native; then
      case "{{ target }}" in
        macos-*)
          test -d "$out/WebRTC.framework" || { echo 'native macOS framework is missing' >&2; exit 1; }
          mkdir -p "$stage/framework"
          cp -R "$out/WebRTC.framework" "$stage/framework/"
          python3 "$src/tools_webrtc/apple/generate_privacy_manifest.py" -o "$stage/framework/WebRTC.framework/Resources/PrivacyInfo.xcprivacy"
          ;;
        ios-*)
          test -d "$out/WebRTC.framework" || { echo 'native iOS framework slice is missing' >&2; exit 1; }
          mkdir -p "$stage/framework"
          cp -R "$out/WebRTC.framework" "$stage/framework/"
          python3 "$src/tools_webrtc/apple/generate_privacy_manifest.py" -o "$stage/framework/WebRTC.framework/PrivacyInfo.xcprivacy"
          ;;
        android-*)
          test -f "$out/libjingle_peerconnection_so.so" || { echo 'native Android JNI library is missing' >&2; exit 1; }
          test -f "$out/lib.java/sdk/android/libwebrtc.jar" || { echo 'native Android classes.jar is missing' >&2; exit 1; }
          mkdir -p "$stage/android"
          cp "$out/libjingle_peerconnection_so.so" "$stage/android/"
          cp "$out/lib.java/sdk/android/libwebrtc.jar" "$stage/android/classes.jar"
          cp "$src/sdk/android/AndroidManifest.xml" "$stage/android/"
          ;;
      esac
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
    license_targets=(--target //:webrtc)
    if test "{{ flavor }}" = native; then
      case "{{ target }}" in
        android-*) license_targets+=(--target //sdk/android:libwebrtc --target //sdk/android:libjingle_peerconnection_so) ;;
        ios-*) license_targets+=(--target //sdk:framework_objc) ;;
        macos-*) license_targets+=(--target //sdk:mac_framework_objc) ;;
      esac
    fi
    vpython3 "$src/tools_webrtc/libs/generate_licenses.py" "${license_targets[@]}" "$stage/licenses" "$out"
    cp "$src/LICENSE" "$stage/licenses/WEBRTC-BSD.txt"
    test -f "$src/PATENTS" && cp "$src/PATENTS" "$stage/licenses/PATENTS" || true
    cp "{{ root }}/LICENSE" "$stage/licenses/REPOSITORY-APACHE-2.0.txt"
    cp "$out/pulsebeam-gn-args.txt" "$stage/metadata/gn-args.txt"
    just --justfile "{{ root }}/Justfile" gn-args "{{ target }}" core > "$stage/metadata/core-gn-args.txt"
    just --justfile "{{ root }}/Justfile" gn-args "{{ target }}" native > "$stage/metadata/native-gn-args.txt"
    (cd "$checkout" && gclient revinfo -a) > "$stage/metadata/resolved-deps.txt"
    just --justfile "{{ root }}/Justfile" _write-link-metadata "{{ target }}" "{{ flavor }}" "$stage/metadata"
    runner=${ImageOS:-local}; compiler=$(just --justfile "{{ root }}/Justfile" _toolchain "{{ target }}")
    recipe_sha=$(just --justfile "{{ root }}/Justfile" _recipe-sha)
    deps_lines=$(sed 's/\\/\\\\/g; s/"/\\"/g; s/^/    "/; s/$/",/' "$stage/metadata/resolved-deps.txt" | sed '$ s/,$//')
    platform_h264=$(just --justfile "{{ root }}/Justfile" _platform-h264 "{{ target }}" "{{ flavor }}")
    cat > "$stage/manifest.json" <<EOF
    {
      "schema_version": 1,
      "artifact": "$artifact_prefix-{{ target }}",
      "source_url": "{{ webrtc_url }}",
      "source_commit": "{{ webrtc_commit }}",
      "source_milestone": {{ milestone }},
      "recipe_revision": {{ recipe_revision }},
      "recipe_git_commit": "$(git -C "{{ root }}" rev-parse HEAD)",
      "recipe_sha256": "$recipe_sha",
      "target": "{{ target }}",
      "flavor": "{{ flavor }}",
      "native_integrations": $(just --justfile "{{ root }}/Justfile" _native-integrations "{{ target }}" "{{ flavor }}"),
      "optimized": true,
      "component_build": false,
      "h264_signaling_and_factory_injection": true,
      "packaged_h264_implementations": [],
      "platform_h264_capabilities": $platform_h264,
      "runner": "$runner",
      "toolchain": "${compiler//\"/\\\"}",
      "runtime": "$(just --justfile "{{ root }}/Justfile" _runtime "{{ target }}" "{{ flavor }}")",
      "target_os": "$(just --justfile "{{ root }}/Justfile" _target-os "{{ target }}")",
      "target_cpu": "$(just --justfile "{{ root }}/Justfile" _target-cpu "{{ target }}")",
      "runtime_floor": "$(just --justfile "{{ root }}/Justfile" _runtime-floor "{{ target }}")",
      "gn_args_file": "metadata/gn-args.txt",
      "flavor_gn_contract": {"core": "metadata/core-gn-args.txt", "native": "metadata/native-gn-args.txt"},
      "target_dependencies": "metadata/target-deps.txt",
      "compile_metadata": "metadata/compile-flags.txt",
      "link_metadata": "metadata/link-flags.txt",
      "system_requirements": "metadata/system-packages.txt",
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

# Verify one link-kit slice from a fresh extraction using only packaged inputs.
_verify-link-kit target flavor:
    #!/usr/bin/env bash
    set -euo pipefail
    just --justfile "{{ root }}/Justfile" _validate-target "{{ target }}"
    just --justfile "{{ root }}/Justfile" _validate-flavor "{{ flavor }}"
    artifact_prefix=$(just --justfile "{{ root }}/Justfile" _artifact-prefix "{{ flavor }}")
    archive="{{ dist }}/$artifact_prefix-{{ target }}.tar.gz"
    test -f "$archive" || { echo "missing artifact: $archive" >&2; exit 1; }
    verify=$(mktemp -d "${TMPDIR:-/tmp}/webrtc-{{ flavor }}-verify.XXXXXX")
    trap 'rm -rf "$verify"' EXIT
    tar -xzf "$archive" -C "$verify"
    kit="$verify/$artifact_prefix-{{ target }}"
    checksum() { if command -v sha256sum >/dev/null; then sha256sum "$@"; else shasum -a 256 "$@"; fi; }
    (cd "$kit" && checksum -c SHA256SUMS >/dev/null)
    grep -Fq '"source_commit": "{{ webrtc_commit }}"' "$kit/manifest.json"
    grep -Fq '"target": "{{ target }}"' "$kit/manifest.json"
    grep -Fq '"flavor": "{{ flavor }}"' "$kit/manifest.json"
    grep -Fq '"packaged_h264_implementations": []' "$kit/manifest.json"
    grep -Fq 'rtc_use_h264=false' "$kit/metadata/gn-args.txt"
    case "{{ flavor }}" in
      core) grep -Fq 'rtc_include_internal_audio_device=false' "$kit/metadata/gn-args.txt" ;;
      native) grep -Fq 'rtc_include_internal_audio_device=true' "$kit/metadata/gn-args.txt" ;;
    esac
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
    if test "{{ flavor }}" = native; then
      case "{{ target }}" in
        linux-*|windows-*|macos-*)
          just --justfile "{{ root }}/Justfile" _consumer-link "{{ target }}" "$kit" "{{ root }}/consumer/native.cc" "$verify/native-consumer"
          ;;
      esac
      case "{{ target }}:$(uname -s):$(uname -m)" in
        linux-x86_64:Linux:x86_64|macos-arm64:Darwin:arm64|macos-x86_64:Darwin:x86_64)
          "$verify/native-consumer"
          ;;
        windows-x86_64:MINGW*:x86_64|windows-x86_64:MSYS*:x86_64)
          "$verify/native-consumer.exe"
          ;;
      esac
      case "{{ target }}" in
        linux-*) grep -Fq 'rtc_use_x11=true' "$kit/metadata/gn-args.txt"; grep -Fq 'rtc_use_pipewire=true' "$kit/metadata/gn-args.txt" ;;
        macos-*)
          test -f "$kit/framework/WebRTC.framework/WebRTC"
          ! strings "$kit/framework/WebRTC.framework/WebRTC" | grep -Eqi 'openh264|ffmpeg.*h264'
          expected_arch='{{ target }}'; expected_arch=${expected_arch#macos-}
          lipo -archs "$kit/framework/WebRTC.framework/WebRTC" | grep -Fqw "$expected_arch"
          otool -l "$kit/framework/WebRTC.framework/WebRTC" | grep -A4 LC_BUILD_VERSION | grep -Fq 'minos 12.0'
          case "{{ target }}:$(uname -s):$(uname -m)" in
            macos-arm64:Darwin:arm64|macos-x86_64:Darwin:x86_64)
              just --justfile "{{ root }}/Justfile" _run-macos-probe "$kit/framework" "$verify/apple-native-consumer"
              ;;
          esac
          ;;
        ios-*) test -f "$kit/framework/WebRTC.framework/WebRTC"; test -f "$kit/framework/WebRTC.framework/PrivacyInfo.xcprivacy" ;;
        android-*) test -f "$kit/android/classes.jar" && test -f "$kit/android/libjingle_peerconnection_so.so" ;;
      esac
    fi
    if [[ "{{ target }}" = linux-* ]]; then
      just --justfile "{{ root }}/Justfile" _verify-linux-binary "$verify/link-consumer" "{{ flavor }}"
      test "{{ flavor }}" != native || just --justfile "{{ root }}/Justfile" _verify-linux-binary "$verify/native-consumer" native
    fi

# Verify one core archive.
verify-core target:
    just --justfile "{{ root }}/Justfile" _verify-link-kit "{{ target }}" core

# Acquire, configure, build, package, and verify one target.
core target:
    just --justfile "{{ root }}/Justfile" sync "{{ target }}"
    just --justfile "{{ root }}/Justfile" configure "{{ target }}" core
    just --justfile "{{ root }}/Justfile" build "{{ target }}" core
    just --justfile "{{ root }}/Justfile" package "{{ target }}" core
    just --justfile "{{ root }}/Justfile" verify-core "{{ target }}"

# Verify that the complete nine-artifact matrix is present and independently valid.
verify-core-matrix:
    #!/usr/bin/env bash
    set -euo pipefail
    while read -r target; do just --justfile "{{ root }}/Justfile" verify-core "$target"; done < <(just --justfile "{{ root }}/Justfile" list-targets)

# Build and verify a native deliverable. Android and iOS are multi-slice
# containers, so selecting either of their matrix entries builds both required
# variants before packaging the AAR or XCFramework.
native target:
    #!/usr/bin/env bash
    set -euo pipefail
    justfile="{{ root }}/Justfile"
    just --justfile "$justfile" _validate-target "{{ target }}"
    case "{{ target }}" in
      android-*) targets=(android-arm64-v8a android-x86_64) ;;
      ios-*) targets=(ios-arm64 ios-simulator-arm64) ;;
      *) targets=("{{ target }}") ;;
    esac
    for target in "${targets[@]}"; do
      just --justfile "$justfile" sync "$target"
      just --justfile "$justfile" configure "$target" native
      just --justfile "$justfile" build "$target" native
      just --justfile "$justfile" package "$target" native
    done
    case "{{ target }}" in
      android-*) just --justfile "$justfile" package-native-android ;;
      ios-*) just --justfile "$justfile" package-native-ios ;;
    esac
    just --justfile "$justfile" verify-native "{{ target }}"

# Assemble the two Android slice outputs into one directly consumable AAR/JAR
# package while retaining slice provenance next to it.
package-native-android:
    #!/usr/bin/env bash
    set -euo pipefail
    prefix="{{ native_prefix }}"
    arm="{{ work }}/package/$prefix-android-arm64-v8a"
    x64="{{ work }}/package/$prefix-android-x86_64"
    stage="{{ work }}/package/$prefix-android"
    aar_root="$stage/aar-root"
    for slice in "$arm" "$x64"; do test -f "$slice/manifest.json" || { echo "missing native Android slice: $slice" >&2; exit 1; }; done
    cmp "$arm/android/classes.jar" "$x64/android/classes.jar" || { echo 'Android Java interfaces differ across ABI slices' >&2; exit 1; }
    rm -rf "$stage"
    mkdir -p "$aar_root/jni/arm64-v8a" "$aar_root/jni/x86_64" "$aar_root/META-INF/pulsebeam/manifests" \
      "$stage/META-INF/pulsebeam/manifests" "$stage/licenses" "$stage/metadata"
    cp "$arm/android/AndroidManifest.xml" "$aar_root/"
    cp "$arm/android/classes.jar" "$aar_root/classes.jar"
    cp "$arm/android/libjingle_peerconnection_so.so" "$aar_root/jni/arm64-v8a/"
    cp "$x64/android/libjingle_peerconnection_so.so" "$aar_root/jni/x86_64/"
    cp "$arm/manifest.json" "$stage/META-INF/pulsebeam/manifests/android-arm64-v8a.json"
    cp "$x64/manifest.json" "$stage/META-INF/pulsebeam/manifests/android-x86_64.json"
    cp -R "$stage/META-INF/pulsebeam/manifests/." "$aar_root/META-INF/pulsebeam/manifests/"
    cp -R "$arm/licenses/." "$stage/licenses/"
    cp "$arm/metadata/gn-args.txt" "$stage/metadata/gn-args-android-arm64-v8a.txt"
    cp "$x64/metadata/gn-args.txt" "$stage/metadata/gn-args-android-x86_64.txt"
    cat > "$stage/manifest.json" <<EOF
    {
      "schema_version": 1,
      "artifact": "$prefix-android",
      "source_url": "{{ webrtc_url }}",
      "source_commit": "{{ webrtc_commit }}",
      "source_milestone": {{ milestone }},
      "recipe_revision": {{ recipe_revision }},
      "recipe_git_commit": "$(git -C "{{ root }}" rev-parse HEAD)",
      "recipe_sha256": "$(just --justfile "{{ root }}/Justfile" _recipe-sha)",
      "flavor": "native",
      "target_os": "android",
      "runtime_floor": "Android API 26",
      "variants": ["android-arm64-v8a", "android-x86_64"],
      "slice_manifests": ["META-INF/pulsebeam/manifests/android-arm64-v8a.json", "META-INF/pulsebeam/manifests/android-x86_64.json"],
      "jni_libraries": ["jni/arm64-v8a/libjingle_peerconnection_so.so", "jni/x86_64/libjingle_peerconnection_so.so"],
      "java_archive": "classes.jar",
      "native_integrations": ["AAudio capture/playback", "camera", "MediaProjection screen capture"],
      "h264_signaling_and_factory_injection": true,
      "packaged_h264_implementations": [],
      "platform_h264_capabilities": ["MediaCodec H.264 through upstream default factories when reported compatible by the device"]
    }
    EOF
    cp "$stage/manifest.json" "$aar_root/META-INF/pulsebeam/manifest.json"
    cp -R "$stage/licenses/." "$aar_root/META-INF/pulsebeam/licenses/"
    aar="$stage/$prefix-android.aar"
    (cd "$aar_root" && python3 -m zipfile -c "$aar" AndroidManifest.xml classes.jar jni META-INF)
    cp "$aar_root/classes.jar" "$stage/$prefix-android.jar"
    rm -rf "$aar_root"
    checksum() { if command -v sha256sum >/dev/null; then sha256sum "$@"; else shasum -a 256 "$@"; fi; }
    (cd "$stage" && find . -type f ! -name SHA256SUMS -print | LC_ALL=C sort | while read -r file; do checksum "$file"; done > SHA256SUMS)
    archive="{{ dist }}/$prefix-android.tar.gz"
    tar -czf "$archive" -C "$(dirname "$stage")" "$(basename "$stage")"
    cp "$stage/$prefix-android.aar" "$stage/$prefix-android.jar" "{{ dist }}/"
    (cd "{{ dist }}" && for file in "$prefix-android.tar.gz" "$prefix-android.aar" "$prefix-android.jar"; do checksum "$file" > "$file.sha256"; done)

# Assemble the device and Simulator frameworks with Apple's native tool, after
# proving that their public interfaces are byte-identical.
package-native-ios:
    #!/usr/bin/env bash
    set -euo pipefail
    test "$(uname -s)" = Darwin || { echo 'iOS XCFramework packaging requires macOS' >&2; exit 1; }
    prefix="{{ native_prefix }}"
    device="{{ work }}/package/$prefix-ios-arm64"
    simulator="{{ work }}/package/$prefix-ios-simulator-arm64"
    stage="{{ work }}/package/$prefix-ios"
    for slice in "$device" "$simulator"; do test -d "$slice/framework/WebRTC.framework" || { echo "missing native iOS framework slice: $slice" >&2; exit 1; }; done
    diff -qr "$device/framework/WebRTC.framework/Headers" "$simulator/framework/WebRTC.framework/Headers"
    diff -qr "$device/framework/WebRTC.framework/Modules" "$simulator/framework/WebRTC.framework/Modules"
    rm -rf "$stage"
    mkdir -p "$stage/licenses" "$stage/metadata/slices" "$stage/include"
    xcodebuild -create-xcframework \
      -framework "$device/framework/WebRTC.framework" \
      -framework "$simulator/framework/WebRTC.framework" \
      -output "$stage/WebRTC.xcframework"
    cp "$device/manifest.json" "$stage/metadata/slices/ios-arm64.json"
    cp "$simulator/manifest.json" "$stage/metadata/slices/ios-simulator-arm64.json"
    cp "$device/metadata/gn-args.txt" "$stage/metadata/gn-args-ios-arm64.txt"
    cp "$simulator/metadata/gn-args.txt" "$stage/metadata/gn-args-ios-simulator-arm64.txt"
    cp -R "$device/include/." "$stage/include/"
    cp -R "$device/licenses/." "$stage/licenses/"
    cat > "$stage/manifest.json" <<EOF
    {
      "schema_version": 1,
      "artifact": "$prefix-ios",
      "source_url": "{{ webrtc_url }}",
      "source_commit": "{{ webrtc_commit }}",
      "source_milestone": {{ milestone }},
      "recipe_revision": {{ recipe_revision }},
      "recipe_git_commit": "$(git -C "{{ root }}" rev-parse HEAD)",
      "recipe_sha256": "$(just --justfile "{{ root }}/Justfile" _recipe-sha)",
      "flavor": "native",
      "target_os": "ios",
      "runtime_floor": "iOS 18",
      "variants": ["ios-arm64", "ios-simulator-arm64"],
      "slice_manifests": ["metadata/slices/ios-arm64.json", "metadata/slices/ios-simulator-arm64.json"],
      "xcframework": "WebRTC.xcframework",
      "native_integrations": ["audio capture/playback", "camera"],
      "h264_signaling_and_factory_injection": true,
      "packaged_h264_implementations": [],
      "platform_h264_capabilities": ["VideoToolbox H.264 through upstream default factories"]
    }
    EOF
    checksum() { if command -v sha256sum >/dev/null; then sha256sum "$@"; else shasum -a 256 "$@"; fi; }
    (cd "$stage" && find . -type f ! -name SHA256SUMS -print | LC_ALL=C sort | while read -r file; do checksum "$file"; done > SHA256SUMS)
    archive="{{ dist }}/$prefix-ios-xcframework.tar.gz"
    tar -czf "$archive" -C "$(dirname "$stage")" "$(basename "$stage")"
    (cd "{{ dist }}" && checksum "$(basename "$archive")" > "$(basename "$archive").sha256")

# Verify the native package appropriate to a selected matrix entry.
verify-native target:
    #!/usr/bin/env bash
    set -euo pipefail
    justfile="{{ root }}/Justfile"
    just --justfile "$justfile" _validate-target "{{ target }}"
    case "{{ target }}" in
      android-*) just --justfile "$justfile" _verify-native-android ;;
      ios-*) just --justfile "$justfile" _verify-native-ios ;;
      *) just --justfile "$justfile" _verify-link-kit "{{ target }}" native ;;
    esac

# Recheck all native artifacts and then prove that the complete core matrix
# remains independently valid.
verify-native-matrix:
    #!/usr/bin/env bash
    set -euo pipefail
    justfile="{{ root }}/Justfile"
    for target in linux-x86_64 linux-arm64 windows-x86_64 macos-arm64 macos-x86_64 android-x86_64 ios-simulator-arm64; do
      just --justfile "$justfile" verify-native "$target"
    done
    just --justfile "$justfile" verify-core-matrix

_verify-native-android:
    #!/usr/bin/env bash
    set -euo pipefail
    prefix="{{ native_prefix }}"
    archive="{{ dist }}/$prefix-android.tar.gz"
    test -f "$archive" || { echo "missing artifact: $archive" >&2; exit 1; }
    verify=$(mktemp -d "${TMPDIR:-/tmp}/webrtc-native-android-verify.XXXXXX")
    trap 'rm -rf "$verify"' EXIT
    tar -xzf "$archive" -C "$verify"
    package="$verify/$prefix-android"
    checksum() { if command -v sha256sum >/dev/null; then sha256sum "$@"; else shasum -a 256 "$@"; fi; }
    (cd "$package" && checksum -c SHA256SUMS >/dev/null)
    python3 -m json.tool "$package/manifest.json" >/dev/null
    grep -Fq '"packaged_h264_implementations": []' "$package/manifest.json"
    grep -Fq 'MediaCodec H.264 through upstream default factories' "$package/manifest.json"
    aar="$package/$prefix-android.aar"
    jar="$package/$prefix-android.jar"
    python3 -m zipfile -e "$aar" "$verify/aar"
    cmp "$jar" "$verify/aar/classes.jar"
    for abi in arm64-v8a x86_64; do test -s "$verify/aar/jni/$abi/libjingle_peerconnection_so.so"; done
    test -f "$verify/aar/META-INF/pulsebeam/manifests/android-arm64-v8a.json"
    test -f "$verify/aar/META-INF/pulsebeam/manifests/android-x86_64.json"
    llvm_readobj="{{ work }}/checkout/src/third_party/llvm-build/Release+Asserts/bin/llvm-readobj"
    llvm_nm="{{ work }}/checkout/src/third_party/llvm-build/Release+Asserts/bin/llvm-nm"
    javap="{{ work }}/checkout/src/third_party/jdk/current/bin/javap"
    arm_arch=$($llvm_readobj --file-headers "$verify/aar/jni/arm64-v8a/libjingle_peerconnection_so.so" | sed -n 's/^Arch: //p')
    x64_arch=$($llvm_readobj --file-headers "$verify/aar/jni/x86_64/libjingle_peerconnection_so.so" | sed -n 's/^Arch: //p')
    grep -Eq '^(aarch64|arm64)$' <<< "$arm_arch"
    grep -Eq '^(x86_64|amd64)$' <<< "$x64_arch"
    for library in "$verify/aar"/jni/*/libjingle_peerconnection_so.so; do
      grep -Fq JNI_OnLoad <<< "$("$llvm_nm" -D "$library")"
      ! strings "$library" | grep -Eqi 'openh264|ffmpeg.*h264'
    done
    for class in org.webrtc.PeerConnectionFactory org.webrtc.Camera2Enumerator org.webrtc.ScreenCapturerAndroid org.webrtc.audio.JavaAudioDeviceModule; do
      "$javap" -classpath "$jar" "$class" >/dev/null
    done
    command -v adb >/dev/null || { echo 'Android native verification requires adb and a booted x86_64 emulator' >&2; exit 1; }
    test "$(adb shell getprop ro.product.cpu.abi | tr -d '\r')" = x86_64 || { echo 'Android native verification requires a booted x86_64 emulator' >&2; exit 1; }
    just --justfile "{{ root }}/Justfile" _run-android-probe "$verify/aar" "$verify/android-probe"

_run-android-probe aar output:
    #!/usr/bin/env bash
    set -euo pipefail
    src="{{ work }}/checkout/src"
    jdk_bin="$src/third_party/jdk/current/bin"
    export PATH="$jdk_bin:$PATH"
    android_jar=$(find "$src/third_party/android_sdk/public/platforms" -name android.jar -print | sort -V | tail -1)
    build_tools=$(find "$src/third_party/android_sdk/public/build-tools" -mindepth 1 -maxdepth 1 -type d -print | sort -V | tail -1)
    test -f "$android_jar" && test -x "$build_tools/d8" && test -x "$build_tools/aapt2" && test -x "$build_tools/apksigner"
    mkdir -p "{{ output }}/classes" "{{ output }}/dex" "{{ output }}/apk/lib/x86_64"
    "$jdk_bin/javac" -source 8 -target 8 -bootclasspath "$android_jar" -classpath "{{ aar }}/classes.jar" \
      -d "{{ output }}/classes" "{{ root }}/consumer/android/NativePackageProbe.java"
    mapfile -d '' classes < <(find "{{ output }}/classes" -name '*.class' -print0)
    "$build_tools/d8" --min-api 26 --lib "$android_jar" --output "{{ output }}/dex" \
      "${classes[@]}" "{{ aar }}/classes.jar"
    "$build_tools/aapt2" link -o "{{ output }}/probe-unsigned.apk" -I "$android_jar" \
      --manifest "{{ root }}/consumer/android/AndroidManifest.xml" --min-sdk-version 26
    cp "{{ output }}/dex/"*.dex "{{ output }}/apk/"
    cp "{{ aar }}/jni/x86_64/libjingle_peerconnection_so.so" "{{ output }}/apk/lib/x86_64/"
    (cd "{{ output }}/apk" && zip -qr "{{ output }}/probe-unsigned.apk" .)
    "$jdk_bin/keytool" -genkeypair -keystore "{{ output }}/probe.keystore" -storepass pulsebeam -keypass pulsebeam \
      -alias probe -dname 'CN=Pulsebeam Native Probe' -keyalg RSA -validity 1 >/dev/null 2>&1
    "$build_tools/apksigner" sign --ks "{{ output }}/probe.keystore" --ks-pass pass:pulsebeam \
      --key-pass pass:pulsebeam --out "{{ output }}/probe.apk" "{{ output }}/probe-unsigned.apk"
    adb install -r "{{ output }}/probe.apk" >/dev/null
    adb logcat -c
    adb shell am start -W -n org.pulsebeam.webrtcbuild/.NativePackageProbe >/dev/null
    adb logcat -d -s PulsebeamNativeProbe:I '*:S' | grep -Fq 'PASS native Android factories loaded'

_verify-native-ios:
    #!/usr/bin/env bash
    set -euo pipefail
    test "$(uname -s)" = Darwin || { echo 'iOS native verification requires macOS and a booted arm64 Simulator' >&2; exit 1; }
    prefix="{{ native_prefix }}"
    archive="{{ dist }}/$prefix-ios-xcframework.tar.gz"
    test -f "$archive" || { echo "missing artifact: $archive" >&2; exit 1; }
    verify=$(mktemp -d "${TMPDIR:-/tmp}/webrtc-native-ios-verify.XXXXXX")
    trap 'rm -rf "$verify"' EXIT
    tar -xzf "$archive" -C "$verify"
    package="$verify/$prefix-ios"
    checksum() { if command -v sha256sum >/dev/null; then sha256sum "$@"; else shasum -a 256 "$@"; fi; }
    (cd "$package" && checksum -c SHA256SUMS >/dev/null)
    python3 -m json.tool "$package/manifest.json" >/dev/null
    grep -Fq '"packaged_h264_implementations": []' "$package/manifest.json"
    grep -Fq 'VideoToolbox H.264 through upstream default factories' "$package/manifest.json"
    plist="$package/WebRTC.xcframework/Info.plist"
    plutil -lint "$plist" >/dev/null
    plutil -p "$plist" | grep -Fq 'ios-arm64'
    plutil -p "$plist" | grep -Fq 'ios-arm64-simulator'
    for framework in "$package"/WebRTC.xcframework/*/WebRTC.framework; do
      test -f "$framework/PrivacyInfo.xcprivacy"
      plutil -lint "$framework/PrivacyInfo.xcprivacy" >/dev/null
      otool -l "$framework/WebRTC" | grep -A4 LC_BUILD_VERSION | grep -Fq 'minos 18.0'
      ! strings "$framework/WebRTC" | grep -Eqi 'openh264|ffmpeg.*h264'
    done
    simulator_framework=$(find "$package/WebRTC.xcframework" -path '*simulator*/WebRTC.framework' -type d -print -quit)
    test -n "$simulator_framework"
    test "$(uname -m)" = arm64 || { echo 'iOS native verification requires an arm64 macOS host' >&2; exit 1; }
    xcrun simctl bootstatus booted -b >/dev/null
    app="$verify/NativePackageProbe.app"
    mkdir -p "$app/Frameworks"
    cp "{{ root }}/consumer/apple/Info.plist" "$app/Info.plist"
    cp -R "$simulator_framework" "$app/Frameworks/"
    sdk=$(xcrun --sdk iphonesimulator --show-sdk-path)
    xcrun --sdk iphonesimulator clang++ -std=c++20 -fobjc-arc -arch arm64 -isysroot "$sdk" \
      -mios-simulator-version-min=18.0 -F"$(dirname "$simulator_framework")" \
      "{{ root }}/consumer/apple/NativePackageProbe.mm" -framework WebRTC -framework UIKit \
      -Wl,-rpath,@executable_path/Frameworks -o "$app/NativePackageProbe"
    codesign --force --sign - "$app/Frameworks/WebRTC.framework" >/dev/null
    codesign --force --sign - "$app" >/dev/null
    xcrun simctl install booted "$app"
    output=$(xcrun simctl launch --console booted org.pulsebeam.webrtcbuild.native-package-probe 2>&1)
    grep -Fq 'PASS native iOS factories loaded' <<< "$output"

# Aggregate only the exact verified build outputs, then create release metadata.
stage-release tag incoming output:
    #!/usr/bin/env bash
    set -euo pipefail
    justfile="{{ root }}/Justfile"
    just --justfile "$justfile" _assert-current-release-tag "{{ tag }}"
    test -d "{{ incoming }}" || { echo 'release input directory is missing' >&2; exit 1; }
    if test -e "{{ output }}"; then
      test -d "{{ output }}" && test -z "$(find "{{ output }}" -mindepth 1 -print -quit)" || { echo 'release staging output must not exist or must be empty' >&2; exit 1; }
    fi
    mkdir -p "{{ output }}"
    expected=$(mktemp "${TMPDIR:-/tmp}/webrtc-release-assets.XXXXXX")
    actual=$(mktemp "${TMPDIR:-/tmp}/webrtc-release-actual.XXXXXX")
    trap 'rm -f "$expected" "$actual"' EXIT
    just --justfile "$justfile" release-assets "{{ tag }}" | LC_ALL=C sort > "$expected"
    find "{{ incoming }}" -mindepth 1 -maxdepth 1 -type f -exec basename {} \; | LC_ALL=C sort > "$actual"
    diff -u "$expected" "$actual" || { echo 'release input is incomplete or contains unexpected assets' >&2; exit 1; }
    while read -r asset; do cp "{{ incoming }}/$asset" "{{ output }}/$asset"; done < "$expected"
    checksum() { if command -v sha256sum >/dev/null; then sha256sum "$@"; else shasum -a 256 "$@"; fi; }
    while read -r sidecar; do (cd "{{ output }}" && checksum -c "$sidecar" >/dev/null); done < <(sed -n '/[.]sha256$/p' "$expected")
    recipe_sha=$(just --justfile "$justfile" _recipe-sha)
    python3 - "{{ output }}" "{{ tag }}" "{{ webrtc_url }}" "{{ webrtc_commit }}" "{{ milestone }}" "{{ recipe_revision }}" "$(git -C "{{ root }}" rev-parse HEAD)" "$recipe_sha" "$expected" <<'PY'
    import hashlib, json, pathlib, sys
    directory, tag, source_url, source_commit, milestone, revision, recipe_commit, recipe_sha, expected_path = sys.argv[1:]
    root = pathlib.Path(directory)
    names = pathlib.Path(expected_path).read_text().splitlines()
    assets = []
    for name in names:
        path = root / name
        assets.append({"name": name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest(), "size": path.stat().st_size})
    manifest = {
        "schema_version": 1,
        "repository": "{{ github_repository }}",
        "tag": tag,
        "source_url": source_url,
        "source_commit": source_commit,
        "source_milestone": int(milestone),
        "recipe_revision": int(revision),
        "recipe_git_commit": recipe_commit,
        "recipe_sha256": recipe_sha,
        "assets": assets,
    }
    (root / "release-manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    PY
    (cd "{{ output }}" && { cat "$expected"; printf '%s\n' release-manifest.json; } | while read -r asset; do checksum "$asset"; done > SHA256SUMS)
    just --justfile "$justfile" _validate-release-payload "{{ tag }}" "{{ output }}" "{{ webrtc_commit }}" "$(git -C "{{ root }}" rev-parse HEAD)" "$recipe_sha" false

# Verify and publish a complete draft. A failure leaves only a non-advertised draft.
publish-release tag directory provenance_bundle:
    #!/usr/bin/env bash
    set -euo pipefail
    test "${GITHUB_ACTIONS:-}" = true || { echo 'publish-release is restricted to GitHub Actions' >&2; exit 1; }
    test -n "${GITHUB_SHA:-}" || { echo 'GITHUB_SHA is required' >&2; exit 1; }
    justfile="{{ root }}/Justfile"
    just --justfile "$justfile" _assert-current-release-tag "{{ tag }}"
    just --justfile "$justfile" _validate-release-remote "{{ tag }}"
    provenance="{{ directory }}/pulsebeam-libwebrtc-{{ tag }}-provenance.sigstore.json"
    cp "{{ provenance_bundle }}" "$provenance"
    python3 -m json.tool "$provenance" >/dev/null
    just --justfile "$justfile" _validate-release-payload "{{ tag }}" "{{ directory }}" "{{ webrtc_commit }}" "$GITHUB_SHA" "$(just --justfile "$justfile" _recipe-sha)" true
    while read -r subject; do gh attestation verify "{{ directory }}/$subject" --repo "{{ github_repository }}" >/dev/null; done < <({ just --justfile "$justfile" release-assets "{{ tag }}"; printf '%s\n' release-manifest.json SHA256SUMS; })
    mapfile -t files < <(find "{{ directory }}" -mindepth 1 -maxdepth 1 -type f -print | LC_ALL=C sort)
    gh release create "{{ tag }}" --repo "{{ github_repository }}" --draft --target "$GITHUB_SHA" \
      --title "pulsebeam-libwebrtc {{ tag }}" \
      --notes 'Verified core and native libwebrtc artifacts. See release-manifest.json, SHA256SUMS, and the Sigstore provenance bundle.' \
      "${files[@]}"
    just --justfile "$justfile" _verify-release "{{ tag }}" false
    gh release edit "{{ tag }}" --repo "{{ github_repository }}" --draft=false --latest=false

# Download and independently verify a published release in a disposable directory.
verify-release tag:
    just --justfile "{{ root }}/Justfile" _verify-release "{{ tag }}" true

_verify-release tag require_published:
    #!/usr/bin/env bash
    set -euo pipefail
    justfile="{{ root }}/Justfile"
    just --justfile "$justfile" _validate-release-tag "{{ tag }}"
    command -v gh >/dev/null || { echo 'release verification requires GitHub CLI (gh)' >&2; exit 1; }
    gh auth status >/dev/null
    release_json=$(gh release view "{{ tag }}" --repo "{{ github_repository }}" --json isDraft,tagName)
    python3 - "$release_json" "{{ tag }}" "{{ require_published }}" <<'PY'
    import json, sys
    release = json.loads(sys.argv[1])
    assert release["tagName"] == sys.argv[2], "release tag mismatch"
    if sys.argv[3] == "true":
        assert not release["isDraft"], "release is still a draft"
    PY
    verify=$(mktemp -d "${TMPDIR:-/tmp}/webrtc-release-verify.XXXXXX")
    trap 'rm -rf "$verify"' EXIT
    gh release download "{{ tag }}" --repo "{{ github_repository }}" --dir "$verify"
    just --justfile "$justfile" _validate-release-payload "{{ tag }}" "$verify" '' '' '' true
    manifest_commit=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["recipe_git_commit"])' "$verify/release-manifest.json")
    tag_commit=$(gh api "repos/{{ github_repository }}/git/ref/tags/{{ tag }}" --jq .object.sha)
    test "$tag_commit" = "$manifest_commit" || { echo 'release tag does not identify the attested recipe commit' >&2; exit 1; }
    while read -r subject; do gh attestation verify "$verify/$subject" --repo "{{ github_repository }}" >/dev/null; done < <({ just --justfile "$justfile" release-assets "{{ tag }}"; printf '%s\n' release-manifest.json SHA256SUMS; })
    printf 'verified %s at recipe commit %s\n' '{{ tag }}' "$manifest_commit"

# Validate identity, inventory, checksums, manifests, notices, codec disclosures,
# archive safety/structure, and the absence of bundled H.264 implementations.
_validate-release-payload tag directory expected_source expected_recipe_commit expected_recipe_sha require_provenance:
    #!/usr/bin/env bash
    set -euo pipefail
    justfile="{{ root }}/Justfile"
    just --justfile "$justfile" _validate-release-tag "{{ tag }}"
    test -d "{{ directory }}"
    expected=$(mktemp "${TMPDIR:-/tmp}/webrtc-release-expected.XXXXXX")
    actual=$(mktemp "${TMPDIR:-/tmp}/webrtc-release-files.XXXXXX")
    extracted=$(mktemp -d "${TMPDIR:-/tmp}/webrtc-release-extracted.XXXXXX")
    trap 'rm -f "$expected" "$actual"; rm -rf "$extracted"' EXIT
    { just --justfile "$justfile" release-assets "{{ tag }}"; printf '%s\n' release-manifest.json SHA256SUMS "pulsebeam-libwebrtc-{{ tag }}-provenance.sigstore.json"; } | LC_ALL=C sort > "$expected"
    find "{{ directory }}" -mindepth 1 -maxdepth 1 -type f -exec basename {} \; | LC_ALL=C sort > "$actual"
    if test "{{ require_provenance }}" = false && ! diff -u "$expected" "$actual" >/dev/null; then
      sed -i.bak '/-provenance[.]sigstore[.]json$/d' "$expected" && rm -f "$expected.bak"
    fi
    diff -u "$expected" "$actual" || { echo 'release asset inventory is incomplete or unexpected' >&2; exit 1; }
    provenance="{{ directory }}/pulsebeam-libwebrtc-{{ tag }}-provenance.sigstore.json"
    test ! -e "$provenance" || python3 -m json.tool "$provenance" >/dev/null
    checksum() { if command -v sha256sum >/dev/null; then sha256sum "$@"; else shasum -a 256 "$@"; fi; }
    (cd "{{ directory }}" && checksum -c SHA256SUMS >/dev/null)
    while read -r sidecar; do (cd "{{ directory }}" && checksum -c "$sidecar" >/dev/null); done < <(just --justfile "$justfile" release-assets "{{ tag }}" | sed -n '/[.]sha256$/p')
    index=0
    while read -r archive; do
      index=$((index + 1))
      python3 -c '
    import pathlib, sys, tarfile
    with tarfile.open(sys.argv[1], "r:gz") as archive:
        for member in archive.getmembers():
            path = pathlib.PurePosixPath(member.name)
            assert not path.is_absolute() and ".." not in path.parts, f"unsafe archive path: {member.name}"
            if member.issym() or member.islnk():
                link = pathlib.PurePosixPath(member.linkname)
                assert not link.is_absolute() and ".." not in link.parts, f"unsafe archive link: {member.name}"
    ' "{{ directory }}/$archive"
      mkdir "$extracted/$index"
      tar -xzf "{{ directory }}/$archive" -C "$extracted/$index"
      test "$(find "$extracted/$index" -mindepth 1 -maxdepth 1 | wc -l)" -eq 1 || { echo "archive must have one top-level package: $archive" >&2; exit 1; }
      package=$(find "$extracted/$index" -mindepth 1 -maxdepth 1 -type d -print -quit)
      test -f "$package/SHA256SUMS" || { echo "packaged checksum index missing: $archive" >&2; exit 1; }
      (cd "$package" && checksum -c SHA256SUMS >/dev/null)
    done < <(just --justfile "$justfile" release-assets "{{ tag }}" | sed -n '/[.]tar[.]gz$/p')
    android_root=$(find "$extracted" -type f -name 'pulsebeam-libwebrtc-{{ tag }}-native-android.aar' -print -quit)
    if test -n "$android_root"; then
      cmp "$android_root" "{{ directory }}/pulsebeam-libwebrtc-{{ tag }}-native-android.aar"
      cmp "${android_root%.aar}.jar" "{{ directory }}/pulsebeam-libwebrtc-{{ tag }}-native-android.jar"
      python3 -c '
    import pathlib, sys, zipfile
    aar, jar, output = map(pathlib.Path, sys.argv[1:])
    for archive in (aar, jar):
        with zipfile.ZipFile(archive) as zipped:
            for name in zipped.namelist():
                path = pathlib.PurePosixPath(name)
                assert not path.is_absolute() and ".." not in path.parts, f"unsafe ZIP path: {name}"
    with zipfile.ZipFile(aar) as zipped:
        names = set(zipped.namelist())
        required = {"AndroidManifest.xml", "classes.jar", "jni/arm64-v8a/libjingle_peerconnection_so.so", "jni/x86_64/libjingle_peerconnection_so.so", "META-INF/pulsebeam/manifest.json"}
        assert required <= names, "Android AAR structure is incomplete"
        zipped.extractall(output)
    ' "$android_root" "${android_root%.aar}.jar" "$extracted/android-aar"
    fi
    while IFS= read -r -d '' binary; do
      ! strings "$binary" | grep -Eqi 'openh264|ffmpeg.*h264|h264.*ffmpeg' || { echo "forbidden bundled H.264 implementation marker: $binary" >&2; exit 1; }
    done < <(find "$extracted" -type f \( -name '*.a' -o -name '*.lib' -o -name '*.so' -o -path '*/WebRTC.framework/WebRTC' \) -print0)
    ! find "$extracted" -type f -name target-deps.txt -exec grep -EH '^//third_party/(openh264|ffmpeg)(:|/)' {} + | grep -q . || { echo 'forbidden H.264 implementation dependency found' >&2; exit 1; }
    python3 - "{{ directory }}" "$extracted" "{{ tag }}" "{{ expected_source }}" "{{ expected_recipe_commit }}" "{{ expected_recipe_sha }}" <<'PY'
    import hashlib, json, pathlib, re, sys
    release_dir, extracted_dir, tag, expected_source, expected_commit, expected_sha = sys.argv[1:]
    release_dir, extracted_dir = pathlib.Path(release_dir), pathlib.Path(extracted_dir)
    release = json.loads((release_dir / "release-manifest.json").read_text())
    match = re.fullmatch(r"m([1-9][0-9]*)-r([1-9][0-9]*)", tag)
    assert match and release["tag"] == tag, "release tag syntax/agreement failure"
    assert release["repository"] == "{{ github_repository }}", "repository mismatch"
    assert release["source_url"] == "{{ webrtc_url }}", "source repository mismatch"
    assert release["source_milestone"] == int(match.group(1)), "milestone mismatch"
    assert release["recipe_revision"] == int(match.group(2)), "recipe revision mismatch"
    if expected_source:
        assert release["source_commit"] == expected_source, "source commit mismatch"
    if expected_commit:
        assert release["recipe_git_commit"] == expected_commit, "recipe commit mismatch"
    if expected_sha:
        assert release["recipe_sha256"] == expected_sha, "recipe identity mismatch"
    recorded = {entry["name"]: entry for entry in release["assets"]}
    assert len(recorded) == len(release["assets"]), "duplicate release manifest asset"
    index_lines = (release_dir / "SHA256SUMS").read_text().splitlines()
    indexed = {line.split(None, 1)[1].lstrip("*") for line in index_lines}
    assert len(indexed) == len(index_lines), "duplicate release checksum entry"
    assert set(recorded) == indexed - {"release-manifest.json"}, "release manifest inventory mismatch"
    for name, entry in recorded.items():
        path = release_dir / name
        assert path.is_file(), f"missing recorded asset: {name}"
        assert hashlib.sha256(path.read_bytes()).hexdigest() == entry["sha256"], f"manifest checksum mismatch: {name}"
        assert path.stat().st_size == entry["size"], f"manifest size mismatch: {name}"
    main_manifests = [p for p in extracted_dir.rglob("manifest.json") if (p.parent / "SHA256SUMS").is_file()]
    manifests = list(extracted_dir.rglob("manifest.json")) + [p for p in extracted_dir.rglob("*.json") if "/slices/" in p.as_posix() or "/manifests/" in p.as_posix()]
    assert manifests, "no packaged manifests found"
    coverage = set()
    for path in manifests:
        data = json.loads(path.read_text())
        assert data["source_url"] == release["source_url"], f"source repository mismatch: {path}"
        assert data["source_commit"] == release["source_commit"], f"source mismatch: {path}"
        assert data["recipe_revision"] == release["recipe_revision"], f"revision mismatch: {path}"
        assert data["source_milestone"] == release["source_milestone"], f"milestone mismatch: {path}"
        assert data["recipe_git_commit"] == release["recipe_git_commit"], f"recipe commit mismatch: {path}"
        assert data["recipe_sha256"] == release["recipe_sha256"], f"recipe identity mismatch: {path}"
        assert data["packaged_h264_implementations"] == [], f"bundled H.264 disclosure: {path}"
        if path in main_manifests:
            assert data["artifact"] == path.parent.name, f"artifact/package identity mismatch: {path}"
        flavor = data["flavor"]
        targets = data.get("variants", [data.get("target")])
        coverage.update((target, flavor) for target in targets if target)
        if data.get("target"):
            assert data["runner"] != "local", f"release slice lacks hosted-runner identity: {path}"
            assert data["optimized"] is True and data["component_build"] is False, f"non-release build settings: {path}"
        caps = data.get("platform_h264_capabilities", [])
        target_os = data.get("target_os", "")
        if flavor == "core":
            assert not caps, f"core must disclose no platform H.264 factory: {path}"
        elif target_os in {"android", "ios"} or any(str(t).startswith(("android-", "ios-", "macos-")) for t in targets):
            assert caps, f"Apple/Android native platform H.264 disclosure missing: {path}"
    targets = {"linux-x86_64", "linux-arm64", "windows-x86_64", "macos-arm64", "macos-x86_64", "android-arm64-v8a", "android-x86_64", "ios-arm64", "ios-simulator-arm64"}
    assert coverage == {(target, flavor) for target in targets for flavor in ("core", "native")}, "target/flavor coverage mismatch"
    for package in [p.parent for p in main_manifests]:
        licenses = package / "licenses"
        assert (licenses / "WEBRTC-BSD.txt").is_file(), f"WebRTC license missing: {package}"
        assert (licenses / "REPOSITORY-APACHE-2.0.txt").is_file(), f"repository license missing: {package}"
        assert len(list(licenses.iterdir())) >= 3, f"third-party notices missing: {package}"
        manifest = json.loads((package / "manifest.json").read_text())
        if manifest["flavor"] == "core":
            args = package / "metadata" / "gn-args.txt"
            assert "rtc_use_h264=false" in args.read_text(), f"core H.264 build flag missing: {package}"
            assert "rtc_include_internal_audio_device=false" in args.read_text(), f"core headless flag missing: {package}"
    PY

_validate-release-tag tag:
    @[[ "{{ tag }}" =~ ^m[1-9][0-9]*-r[1-9][0-9]*$ ]] || { echo 'release tag must match m<milestone>-r<revision>' >&2; exit 1; }

_assert-current-release-tag tag:
    @just --justfile "{{ root }}/Justfile" _validate-release-tag "{{ tag }}"; test "{{ tag }}" = "{{ release_tag }}" || { echo "release tag does not agree with milestone {{ milestone }} and recipe revision {{ recipe_revision }}" >&2; exit 1; }

_recipe-sha:
    #!/usr/bin/env bash
    set -euo pipefail
    git -C "{{ root }}" ls-files -co --exclude-standard Justfile consumer | LC_ALL=C sort | while read -r file; do cat "{{ root }}/$file"; done | { if command -v sha256sum >/dev/null; then sha256sum; else shasum -a 256; fi; } | awk '{print $1}'

_validate-release-remote tag:
    #!/usr/bin/env bash
    set -euo pipefail
    just --justfile "{{ root }}/Justfile" _assert-current-release-tag "{{ tag }}"
    command -v gh >/dev/null || { echo 'GitHub CLI (gh) is required' >&2; exit 1; }
    test "$(gh api 'repos/webrtc-sdk/webrtc/commits/{{ webrtc_commit }}' --jq .sha)" = "{{ webrtc_commit }}" || { echo 'authoritative source commit is unavailable' >&2; exit 1; }
    ! git -C "{{ root }}" show-ref --verify --quiet "refs/tags/{{ tag }}" || { echo 'local release tag already exists and is immutable' >&2; exit 1; }
    test -z "$(git -C "{{ root }}" ls-remote --tags origin "refs/tags/{{ tag }}")" || { echo 'release tag already exists and is immutable' >&2; exit 1; }
    if gh release view "{{ tag }}" --repo "{{ github_repository }}" >/dev/null 2>&1; then echo 'release already exists and is immutable' >&2; exit 1; fi

# Remove disposable source/build/package output for one target; release archives remain.
clean target:
    #!/usr/bin/env bash
    set -euo pipefail
    just --justfile "{{ root }}/Justfile" _validate-target "{{ target }}"
    rm -rf "{{ work }}/out/core/{{ target }}" "{{ work }}/out/native/{{ target }}" \
      "{{ work }}/package/{{ core_prefix }}-{{ target }}" "{{ work }}/package/{{ native_prefix }}-{{ target }}"

_validate-target target:
    @case "{{ target }}" in linux-x86_64|linux-arm64|windows-x86_64|macos-arm64|macos-x86_64|android-arm64-v8a|android-x86_64|ios-arm64|ios-simulator-arm64) ;; *) echo "unsupported target: {{ target }}" >&2; exit 1;; esac

_validate-flavor flavor:
    @case "{{ flavor }}" in core|native) ;; *) echo "unsupported flavor: {{ flavor }}" >&2; exit 1;; esac

_artifact-prefix flavor:
    @case "{{ flavor }}" in core) printf '%s' '{{ core_prefix }}';; native) printf '%s' '{{ native_prefix }}';; *) exit 1;; esac

_native-integrations target flavor:
    @case "{{ flavor }}:{{ target }}" in \
      core:*) printf '[]';; \
      native:linux-*) printf '["ALSA/PulseAudio capture/playback", "V4L2 camera", "X11/PipeWire screen/window capture"]';; \
      native:windows-*) printf '["WASAPI capture/playback", "DirectShow camera", "GDI/DXGI screen/window capture"]';; \
      native:macos-*) printf '["CoreAudio capture/playback", "AVFoundation camera", "ScreenCaptureKit screen/window capture"]';; \
      native:android-*) printf '["AAudio capture/playback", "camera", "MediaProjection screen capture"]';; \
      native:ios-*) printf '["audio capture/playback", "camera"]';; \
    esac

_platform-h264 target flavor:
    @case "{{ flavor }}:{{ target }}" in \
      native:macos-*|native:ios-*) printf '["VideoToolbox H.264 through upstream default factories"]';; \
      native:android-*) printf '["MediaCodec H.264 through upstream default factories when reported compatible by the device"]';; \
      *) printf '[]';; \
    esac

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

_runtime target flavor="core":
    #!/usr/bin/env bash
    case "{{ target }}" in
      linux-*) printf '%s' 'Debian Bullseye sysroot; glibc >=2.31; bundled libc++ and libc++abi static archives; pinned Clang compiler-rt; sysroot libgcc_s unwinder; no host C++ standard library' ;;
      windows-*) printf '%s' 'Windows 10; MSVC C++ ABI; static multithreaded CRT /MT' ;;
      macos-*) if test "{{ flavor }}" = native; then printf '%s' 'macOS >=12.0; platform libc++; dynamic WebRTC.framework and system frameworks'; else printf '%s' 'macOS >=12.0; platform libc++; system frameworks'; fi ;;
      android-*) if test "{{ flavor }}" = native; then printf '%s' 'Android API >=26; pinned NDK libc++; AAudio; JNI shared library'; else printf '%s' 'Android API >=26; pinned NDK libc++; static WebRTC'; fi ;;
      ios-*) if test "{{ flavor }}" = native; then printf '%s' 'iOS >=18.0; platform libc++; dynamic XCFramework and system frameworks'; else printf '%s' 'iOS >=18.0; platform libc++; system frameworks'; fi ;;
    esac

_toolchain target:
    #!/usr/bin/env bash
    set -euo pipefail
    src="{{ work }}/checkout/src"
    case "{{ target }}" in
      linux-*) "$src/third_party/llvm-build/Release+Asserts/bin/clang++" --version | head -1 ;;
      windows-*) printf 'MSVC %s; Windows SDK %s at %s; ' "$(cl.exe 2>&1 | sed -n '1p')" "${WindowsSDKVersion:-unknown}" "${WindowsSdkDir:-unknown}"; "$src/third_party/llvm-build/Release+Asserts/bin/clang-cl" --version | head -1 ;;
      macos-*) printf 'Xcode %s; SDK %s; ' "$(xcodebuild -version | tr '\n' ' ')" "$(xcrun --sdk macosx --show-sdk-version)"; "$src/third_party/llvm-build/Release+Asserts/bin/clang++" --version | head -1 ;;
      android-*) ndk=$(grep -m1 "'android_ndk_version':" "$src/DEPS" | cut -d "'" -f4); printf 'NDK CIPD %s; ' "$ndk"; "$src/third_party/llvm-build/Release+Asserts/bin/clang++" --version | head -1 ;;
      ios-arm64) printf 'Xcode %s; iPhoneOS SDK %s; ' "$(xcodebuild -version | tr '\n' ' ')" "$(xcrun --sdk iphoneos --show-sdk-version)"; "$src/third_party/llvm-build/Release+Asserts/bin/clang++" --version | head -1 ;;
      ios-simulator-arm64) printf 'Xcode %s; iPhoneSimulator SDK %s; ' "$(xcodebuild -version | tr '\n' ' ')" "$(xcrun --sdk iphonesimulator --show-sdk-version)"; "$src/third_party/llvm-build/Release+Asserts/bin/clang++" --version | head -1 ;;
    esac

_write-link-metadata target flavor metadata:
    #!/usr/bin/env bash
    set -euo pipefail
    : > "{{ metadata }}/system-packages.txt"
    case "{{ target }}" in
      linux-*)
        src="{{ work }}/checkout/src"
        out="{{ work }}/out/{{ flavor }}/{{ target }}"
        gn=$(just --justfile "{{ root }}/Justfile" _gn-path)
        definitions=$("$gn" desc --root="$src" "$out" //:webrtc defines --all)
        runtime_definitions=$(grep -E '^(_LIBCPP|_LIBCXXABI|CR_LIBCXX_REVISION)' <<< "$definitions")
        for required in _LIBCPP_HARDENING_MODE _LIBCPP_DISABLE_VISIBILITY_ANNOTATIONS _LIBCXXABI_DISABLE_VISIBILITY_ANNOTATIONS _LIBCPP_INSTRUMENTED_WITH_ASAN CR_LIBCXX_REVISION; do
          grep -Eq "^${required}(=|$)" <<< "$runtime_definitions" || { echo "GN closure is missing required bundled-runtime definition: $required" >&2; exit 1; }
        done
        runtime_flags=$(sed 's/^/-D/' <<< "$runtime_definitions" | tr '\n' ' ')
        native_definitions=''
        native_includes=''
        native_libraries=''
        printf '%s\n' 'Bullseye glibc: pthread dl rt m; sysroot libgcc_s; packaged libc++/libc++abi' > "{{ metadata }}/system-packages.txt"
        if test "{{ flavor }}" = native; then
          native_definitions='-DWEBRTC_USE_X11 -DWEBRTC_USE_PIPEWIRE -DWEBRTC_USE_GIO'
          native_includes='-isystem=/usr/include/glib-2.0 -isystem=/usr/lib/@MULTIARCH@/glib-2.0/include -isystem=/usr/include/gio-unix-2.0 -isystem=/usr/include/pipewire-0.3 -isystem=/usr/include/spa-0.2 -isystem=/usr/include/libdrm'
          native_libraries='-lX11 -lgio-2.0 -lglib-2.0 -lgobject-2.0 -lXcomposite -lXdamage -lXext -lXfixes -lXrandr -lXrender -lXtst -lgbm -ldrm'
          printf '%s\n' \
            'Bullseye glibc: pthread dl rt m; sysroot libgcc_s; packaged libc++/libc++abi' \
            'Compile/link: x11 xcomposite xdamage xext xfixes xrandr xrender xtst gio-2.0 gio-unix-2.0 libpipewire-0.3 gbm libdrm' \
            'Runtime-loaded: alsa libpulse libpipewire-0.3' > "{{ metadata }}/system-packages.txt"
        fi
        compile="-std=c++20 -fno-exceptions -fno-rtti -nostdinc++ -isystem@KIT@/runtime/include/libcxx -isystem@KIT@/runtime/include/libcxxabi -Wno-nullability-completeness -DWEBRTC_POSIX -DWEBRTC_LINUX -DABSL_ALLOCATOR_NOTHROW=1 ${runtime_flags% } $native_definitions $native_includes -pthread"
        link="-fuse-ld=lld -nostdlib++ -Wl,--start-group @ARCHIVES@ -Wl,--end-group -pthread -ldl -lrt -lm $native_libraries"
        ;;
      windows-*) compile='/std:c++20 /GR- /EHs-c- /MT /DWEBRTC_WIN /DWIN32_LEAN_AND_MEAN /DNOMINMAX /DABSL_ALLOCATOR_NOTHROW=1'; link='advapi32.lib amstrmid.lib bcrypt.lib crypt32.lib d3d11.lib dmoguids.lib dwmapi.lib dxgi.lib iphlpapi.lib msdmo.lib ole32.lib oleaut32.lib secur32.lib shcore.lib strmiids.lib user32.lib winmm.lib wmcodecdspuuid.lib ws2_32.lib'; printf '%s\n' "$link" > "{{ metadata }}/system-packages.txt" ;;
      macos-*) compile='-std=c++20 -fno-exceptions -fno-rtti -DWEBRTC_POSIX -DWEBRTC_MAC -DABSL_ALLOCATOR_NOTHROW=1'; link='-framework Foundation -framework AppKit -framework ApplicationServices -framework CoreAudio -framework CoreFoundation -framework CoreGraphics -framework CoreMedia -framework CoreVideo -framework AudioToolbox -framework AVFoundation -framework IOKit -framework IOSurface -framework OpenGL -weak_framework ScreenCaptureKit'; printf '%s\n' "$link" > "{{ metadata }}/system-packages.txt" ;;
      android-*) compile='-std=c++20 -fno-exceptions -fno-rtti -DWEBRTC_POSIX -DWEBRTC_LINUX -DWEBRTC_ANDROID -DABSL_ALLOCATOR_NOTHROW=1'; link='-static-libstdc++ -llog -landroid -ldl -lm'; printf '%s\n' 'Android NDK API 26: log android dl m; pinned NDK libc++' > "{{ metadata }}/system-packages.txt" ;;
      ios-*) compile='-std=c++20 -fno-exceptions -fno-rtti -DWEBRTC_POSIX -DWEBRTC_IOS -DWEBRTC_MAC -DABSL_ALLOCATOR_NOTHROW=1'; link='-framework Foundation -framework CoreFoundation -framework CoreGraphics -framework CoreMedia -framework CoreVideo -framework AudioToolbox -framework AVFoundation'; printf '%s\n' "$link" > "{{ metadata }}/system-packages.txt" ;;
    esac
    printf '%s\n' "$compile" > "{{ metadata }}/compile-flags.txt"
    printf '%s\n' "$link" > "{{ metadata }}/link-flags.txt"
    just --justfile "{{ root }}/Justfile" _runtime "{{ target }}" "{{ flavor }}" > "{{ metadata }}/runtime.txt"

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
      linux-x86_64) compile=${compile//@MULTIARCH@/x86_64-linux-gnu}; cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang++"; extra=(--target=x86_64-linux-gnu --sysroot="$src/build/linux/debian_bullseye_amd64-sysroot") ;;
      linux-arm64) compile=${compile//@MULTIARCH@/aarch64-linux-gnu}; cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang++"; extra=(--target=aarch64-linux-gnu --sysroot="$src/build/linux/debian_bullseye_arm64-sysroot") ;;
      android-arm64-v8a) for candidate in "$src/third_party/android_toolchain/ndk/toolchains/llvm/prebuilt"/*; do test ! -d "$candidate" || { prebuilt=$candidate; break; }; done; cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang++"; extra=(--target=aarch64-linux-android26 --sysroot="$prebuilt/sysroot") ;;
      android-x86_64) for candidate in "$src/third_party/android_toolchain/ndk/toolchains/llvm/prebuilt"/*; do test ! -d "$candidate" || { prebuilt=$candidate; break; }; done; cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang++"; extra=(--target=x86_64-linux-android26 --sysroot="$prebuilt/sysroot") ;;
      macos-arm64) cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang++"; sdk=$(xcrun --sdk macosx --show-sdk-path); extra=(-arch arm64 -isysroot "$sdk" -mmacosx-version-min=12.0) ;;
      macos-x86_64) cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang++"; sdk=$(xcrun --sdk macosx --show-sdk-path); extra=(-arch x86_64 -isysroot "$sdk" -mmacosx-version-min=12.0) ;;
      ios-arm64) cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang++"; sdk=$(xcrun --sdk iphoneos --show-sdk-path); extra=(-arch arm64 -isysroot "$sdk" -miphoneos-version-min=18.0) ;;
      ios-simulator-arm64) cxx="$src/third_party/llvm-build/Release+Asserts/bin/clang++"; sdk=$(xcrun --sdk iphonesimulator --show-sdk-path); extra=(-arch arm64 -isysroot "$sdk" -mios-simulator-version-min=18.0) ;;
      windows-x86_64) "$src/third_party/llvm-build/Release+Asserts/bin/clang-cl" "${compile_flags[@]}" /I"{{ kit }}/include" "{{ source }}" "${archives[@]}" "${link_flags[@]}" "/Fe:{{ output }}.exe"; exit ;;
    esac
    read -r -a compile_flags <<< "$compile"
    final_link=()
    for flag in "${link_flags[@]}"; do
      if test "$flag" = @ARCHIVES@; then final_link+=("${archives[@]}"); else final_link+=("$flag"); fi
    done
    if [[ "{{ target }}" != linux-* ]]; then final_link=("${archives[@]}" "${final_link[@]}"); fi
    "$cxx" "${extra[@]}" "${compile_flags[@]}" -I"{{ kit }}/include" "{{ source }}" "${final_link[@]}" -o "{{ output }}"

_run-macos-probe framework_dir output:
    #!/usr/bin/env bash
    set -euo pipefail
    sdk=$(xcrun --sdk macosx --show-sdk-path)
    xcrun --sdk macosx clang++ -std=c++20 -fobjc-arc -isysroot "$sdk" -mmacosx-version-min=12.0 \
      -F"{{ framework_dir }}" "{{ root }}/consumer/apple/NativePackageProbe.mm" \
      -framework WebRTC -framework Foundation -framework AVFoundation \
      -Wl,-rpath,"{{ framework_dir }}" -o "{{ output }}"
    DYLD_FRAMEWORK_PATH="{{ framework_dir }}" "{{ output }}"

_target-os target:
    @case "{{ target }}" in linux-*) printf linux;; windows-*) printf windows;; macos-*) printf macos;; android-*) printf android;; ios-*) printf ios;; esac

_target-cpu target:
    @case "{{ target }}" in *x86_64) printf x86_64;; *) printf arm64;; esac

_runtime-floor target:
    @case "{{ target }}" in linux-*) printf 'glibc 2.31';; windows-*) printf 'Windows 10';; macos-*) printf 'macOS 12';; android-*) printf 'Android API 26';; ios-*) printf 'iOS 18';; esac

_verify-linux-binary binary flavor:
    #!/usr/bin/env bash
    set -euo pipefail
    needed=$(readelf -d "{{ binary }}" | sed -n 's/.*Shared library: \[\([^]]*\)\].*/\1/p')
    grep -Eqi 'lib(std|c)\+\+' <<< "$needed" && { echo 'consumer dynamically depends on a C++ standard library' >&2; exit 1; }
    if test "{{ flavor }}" = core; then
      grep -Eqi 'lib(X11|wayland|pulse|asound)' <<< "$needed" && { echo 'headless core has a native device/window dependency' >&2; exit 1; }
    fi
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
