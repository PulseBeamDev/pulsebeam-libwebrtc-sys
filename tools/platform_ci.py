"""Repository-owned setup and producer tasks for non-Linux OS release jobs."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tempfile

if sys.version_info < (3, 11):
    raise SystemExit("platform release tooling requires Python 3.11 or newer")

from tools import ci_tasks, mobile_runtime, rust_only_consumer
from tools.release_inventory import OS_TARGETS

ROOT = Path(__file__).resolve().parents[1]
RUST_TOOLCHAIN = "1.96.0"
RUNNERS = {
    "macos-x86_64": "macos-15-intel", "macos-arm64": "macos-15",
    "windows-x86_64": "windows-2025", "ios-arm64": "macos-15",
    "ios-simulator-arm64": "macos-15", "android-x86_64": "ubuntu-24.04",
    "android-arm64-v8a": "ubuntu-24.04",
}


class SetupError(Exception):
    pass


def matrix(scope: str, *, runtime: bool = False) -> dict:
    if scope not in OS_TARGETS or scope == "linux":
        raise SetupError(f"unsupported non-Linux release scope: {scope}")
    targets = sorted(OS_TARGETS[scope])
    if runtime:
        targets = {"android": ["android-x86_64"], "ios": ["ios-simulator-arm64"]}.get(scope, [])
    return {"include": [{"flavor": flavor, "target": target, "runner": RUNNERS[target]}
                        for flavor in ("core", "native") for target in targets]}


def invoke(command, *, environment=None, capture=False, timeout=1800, input=None):
    print("+ " + " ".join(map(str, command)), flush=True)
    return subprocess.run(list(map(str, command)), check=True, env=environment,
                          text=True, capture_output=capture, timeout=timeout, input=input)


def export_environment(values: dict[str, str]) -> None:
    output = os.environ.get("GITHUB_ENV")
    if not output:
        raise SetupError("SDK setup requires GITHUB_ENV; local qualification uses already provisioned SDK inputs")
    with Path(output).open("a", encoding="utf-8") as stream:
        for name, value in sorted(values.items()):
            if not name or any(character in name + value for character in "\r\n") or "=" in name:
                raise SetupError("invalid SDK environment value")
            stream.write(f"{name}={value}\n")


def windows_environment() -> dict[str, str]:
    # Use the installed supported VS 2022/2026 toolchain, not depot_tools'
    # restricted Google toolchain service. The SDK version is the source pin.
    program_files = os.environ.get("ProgramFiles(x86)")
    if not program_files:
        raise SetupError("missing native Windows ProgramFiles(x86)")
    locator = Path(program_files) / "Microsoft Visual Studio/Installer/vswhere.exe"
    installations = json.loads(invoke([locator, "-latest", "-products", "*", "-requires",
                                      "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", "-format", "json"], capture=True).stdout)
    if len(installations) != 1:
        raise SetupError("requires one supported Visual Studio x64 C++ installation")
    installation = installations[0]
    major = installation["installationVersion"].split(".")[0]
    if major not in {"17", "18"}:
        raise SetupError("requires supported Visual Studio 2022 or 2026")
    developer = Path(installation["installationPath"]) / "Common7/Tools/VsDevCmd.bat"
    # Pass a complete native command line: list2cmdline uses CRT escaping,
    # which is not cmd.exe's /s /c quoting convention for a quoted batch path.
    command = f'cmd.exe /d /s /c ""{developer}" -no_logo -arch=amd64 -host_arch=amd64 -winsdk=10.0.26100.0 && set"'
    print(f"+ {command}", flush=True)
    configured = subprocess.run(command, check=True, text=True, capture_output=True, timeout=120).stdout
    accepted = {"PATH", "INCLUDE", "LIB", "LIBPATH", "WINDOWSSDKDIR", "WINDOWSSDKVERSION", "WINDOWSSDKBINPATH",
                "WINDOWSSDKVERBINPATH", "UNIVERSALCRTSDKDIR", "UCRTVERSION", "VSINSTALLDIR", "VCINSTALLDIR",
                "VCTOOLSINSTALLDIR", "VCTOOLSVERSION", "VISUALSTUDIOVERSION", "VSCMD_ARG_TGT_ARCH", "VSCMD_ARG_HOST_ARCH"}
    environment = {name: value for line in configured.splitlines() if "=" in line
                   for name, value in [line.split("=", 1)] if name.upper() in accepted}
    if not all(any(key.upper() == name for key in environment) for name in ("PATH", "INCLUDE", "LIB", "WINDOWSSDKVERSION")):
        raise SetupError("Visual Studio developer environment is incomplete")
    if next(value for key, value in environment.items() if key.upper() == "WINDOWSSDKVERSION").rstrip("\\/") != "10.0.26100.0":
        raise SetupError("requires Windows SDK 10.0.26100.0")
    environment.update(DEPOT_TOOLS_WIN_TOOLCHAIN="0", GYP_MSVS_VERSION={"17": "2022", "18": "2026"}[major])
    # Just recipes invoke python3 from Git Bash. Some native Windows Python
    # installations provide only python.exe; alias it beside its DLL/stdlib.
    executable = Path(sys.executable)
    alias = executable.with_name("python3.exe")
    if not alias.exists():
        shutil.copy2(executable, alias)
    return environment


def android_sdk(*, runtime: bool) -> dict[str, str]:
    root = os.environ.get("ANDROID_SDK_ROOT") or os.environ.get("ANDROID_HOME")
    if not root:
        raise SetupError("requires an installed, licensed Android SDK root")
    sdk = Path(root)
    manager = sdk / "cmdline-tools/latest/bin/sdkmanager"
    packages = [f"ndk;{mobile_runtime.ANDROID_NDK}"]
    if runtime:
        packages += ["platform-tools", "emulator", "platforms;android-26",
                     f"build-tools;{mobile_runtime.ANDROID_BUILD_TOOLS}", mobile_runtime.ANDROID_IMAGE]
    # Never implicitly accept SDK licenses. Hosted/local SDKs must already have
    # the applicable licenses accepted by their owner; EOF fails closed.
    invoke([manager, f"--sdk_root={sdk}", "--channel=3", *packages], input="", timeout=1800)
    ndk = sdk / "ndk" / mobile_runtime.ANDROID_NDK
    driver = ndk / "toolchains/llvm/prebuilt/linux-x86_64/bin/x86_64-linux-android26-clang"
    if not driver.is_file():
        raise SetupError("pinned Android NDK was not installed; inspect SDK license/package diagnostics")
    return {"ANDROID_SDK_ROOT": str(sdk), "ANDROID_HOME": str(sdk), "ANDROID_NDK_HOME": str(ndk)}


def ensure_ios_runtime() -> None:
    def installed():
        return json.loads(invoke(["xcrun", "simctl", "list", "runtimes", "--json"], capture=True).stdout)["runtimes"]
    try:
        mobile_runtime.ios_runtime(installed())
        return
    except mobile_runtime.RuntimeError:
        pass
    # Apple documents exact-version ARM64 downloads/imports in Xcode 16.1+.
    # Xcode must already be initialized and licensed; never accept terms here.
    invoke(["xcodebuild", "-checkFirstLaunchStatus"], input="")
    with tempfile.TemporaryDirectory(prefix="pulsebeam-ios-runtime-") as temporary:
        invoke(["xcodebuild", "-downloadPlatform", "iOS", "-buildVersion", "18.0",
                "-architectureVariant", "arm64", "-exportPath", temporary], input="", timeout=1800)
        images = list(Path(temporary).rglob("*.dmg"))
        if len(images) != 1:
            raise SetupError("exact iOS 18.0 runtime download did not produce one runtime image")
        invoke(["xcodebuild", "-importPlatform", images[0]], input="", timeout=1800)
    mobile_runtime.ios_runtime(installed())


def setup(target: str, purpose: str) -> None:
    if target not in RUNNERS or purpose not in {"build", "consumer", "runtime"}:
        raise SetupError("unsupported platform setup selection")
    rust_only_consumer.require_host(target)
    if not os.environ.get("GITHUB_ENV"):
        raise SetupError("SDK setup requires GITHUB_ENV")
    environment = windows_environment() if target.startswith("windows-") else {}
    if target.startswith("android-"):
        environment.update(android_sdk(runtime=purpose == "runtime"))
    if target.startswith(("macos-", "ios-")):
        sdk = "macosx" if target.startswith("macos-") else ("iphonesimulator" if target == "ios-simulator-arm64" else "iphoneos")
        print(invoke(["xcrun", "--sdk", sdk, "--show-sdk-version"], capture=True).stdout)
    if purpose == "runtime" and target == "ios-simulator-arm64":
        ensure_ios_runtime()
    environment["RUSTUP_TOOLCHAIN"] = RUST_TOOLCHAIN
    invoke(["rustup", "toolchain", "install", RUST_TOOLCHAIN, "--profile", "minimal", "--component", "rustfmt"])
    cargo, _ = rust_only_consumer.coordinate(target, "core")
    invoke(["rustup", "target", "add", cargo, "--toolchain", RUST_TOOLCHAIN])
    export_environment(environment)


def produce(flavor: str, target: str) -> None:
    if target not in RUNNERS or flavor not in {"core", "native"}:
        raise SetupError("unsupported producer selection")
    rust_only_consumer.require_host(target)
    # Android builds execute this within the repository-owned Linux image, so
    # the installed Rust target remains present for the build/smoke-link step.
    ci_tasks.install_target(target)
    invoke(["just", "build", flavor, target], timeout=21600)
    if target.startswith(("macos-", "windows-")):
        archive = ROOT / f"dist/webrtc-{flavor}-{target}.tar.gz"
        archive_sha256 = ci_tasks._sha256(archive)
        ci_tasks.runtime(flavor, target, archive)
        if ci_tasks._sha256(archive) != archive_sha256:
            raise SetupError("runtime qualification mutated the selected archive")
        evidence = ROOT / f".work/qualification/{flavor}-{target}.json"
        evidence.parent.mkdir(parents=True, exist_ok=True)
        evidence.write_text(json.dumps({"schema_version": 1, "target": target, "flavor": flavor,
                                       "archive_sha256": archive_sha256, "runtime": "complete Rust and SDK/bridge C++ suite",
                                       "host": {"system": platform.system(), "architecture": platform.machine()}}, indent=2) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    definition = commands.add_parser("matrix")
    definition.add_argument("scope", choices=("macos", "windows", "ios", "android"))
    definition.add_argument("--runtime", action="store_true")
    prepare = commands.add_parser("setup")
    prepare.add_argument("target", choices=tuple(RUNNERS))
    prepare.add_argument("purpose", choices=("build", "consumer", "runtime"))
    producer = commands.add_parser("produce")
    producer.add_argument("flavor", choices=("core", "native"))
    producer.add_argument("target", choices=tuple(RUNNERS))
    args = parser.parse_args()
    try:
        if args.command == "matrix":
            print(json.dumps(matrix(args.scope, runtime=args.runtime), separators=(",", ":")))
        elif args.command == "setup":
            setup(args.target, args.purpose)
        else:
            produce(args.flavor, args.target)
    except (OSError, ValueError, KeyError, subprocess.SubprocessError, SetupError,
            rust_only_consumer.ProofError, ci_tasks.TaskError, mobile_runtime.RuntimeError) as error:
        print(f"platform CI: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
