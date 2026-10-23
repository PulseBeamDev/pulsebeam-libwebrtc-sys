"""Build and execute producer-owned headless mobile qualification hosts.

Requires an installed, licensed SDK and minimum-baseline emulator/simulator
runtime. SDK installation, physical devices and published application SDKs are
not part of this command. Every successful result identifies the archived bytes.
"""
from __future__ import annotations

import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import plistlib
import re
import shlex
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import uuid
import zipfile

from tools import rust_only_consumer as consumer
from tools.native_provenance import validate_configuration

ROOT = Path(__file__).resolve().parents[1]
ANDROID_BUILD_TOOLS = "35.0.0"
ANDROID_NDK = "30.0.14608247"
ANDROID_IMAGE = "system-images;android-26;google_apis;x86_64"


class RuntimeError(Exception):
    pass


def invoke(command, *, environment=None, capture=False, input=None, timeout=300):
    print("+ " + shlex.join(map(str, command)), flush=True)
    return subprocess.run(list(map(str, command)), env=environment, check=True,
                          text=True, capture_output=capture, input=input, timeout=timeout)


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def android_environment(properties: dict[str, str]) -> None:
    if properties != {"api": "26", "abi": "x86_64", "qemu": "1"}:
        raise RuntimeError(f"requires an Android API 26 x86_64 emulator, never a physical device: {properties}")


def ios_runtime(runtimes: list[dict]) -> str:
    matches = [item["identifier"] for item in runtimes
               if item.get("version") == "18.0" and item.get("isAvailable") is True
               and item.get("identifier", "").startswith("com.apple.CoreSimulator.SimRuntime.iOS-")]
    if len(matches) != 1:
        raise RuntimeError("requires exactly one available iOS 18.0 simulator runtime; newer runtimes are not baseline proof")
    return matches[0]


def validate_result(result: dict, nonce: str, archive_sha256: str) -> None:
    if (set(result) != {"pulsebeam_status", "pulsebeam_nonce", "pulsebeam_archive_sha256"}
            or type(result["pulsebeam_status"]) is not int or result["pulsebeam_status"] != 0
            or result["pulsebeam_nonce"] != nonce
            or result["pulsebeam_archive_sha256"] != archive_sha256):
        raise RuntimeError(f"missing, failed or stale mobile result: {result}")


def android_result(output: str, nonce: str, archive_sha256: str) -> dict:
    result = {}
    for key, value in re.findall(r"^INSTRUMENTATION_RESULT: (pulsebeam_\w+)=(.*)$", output, re.MULTILINE):
        if key in result:
            raise RuntimeError("duplicate Android instrumentation result")
        result[key] = value.strip()
    if result.get("pulsebeam_status") == "0":
        result["pulsebeam_status"] = 0
    if re.findall(r"^INSTRUMENTATION_CODE: (-?\d+)\s*$", output, re.MULTILINE) != ["-1"]:
        raise RuntimeError(f"Android instrumentation did not finish successfully:\n{output}")
    validate_result(result, nonce, archive_sha256)
    return result


def extract_kit(archive_path: Path, expected_sha256: str, destination: Path, target: str, flavor: str) -> dict:
    if not re.fullmatch(r"[0-9a-f]{64}", expected_sha256) or digest(archive_path) != expected_sha256:
        raise RuntimeError("mobile archive SHA-256 mismatch")
    destination.mkdir()
    with tarfile.open(archive_path, "r:gz") as archive:
        seen = set()
        total = 0
        for member in archive:
            path = PurePosixPath(member.name)
            if (member.name in seen or path.is_absolute() or "\\" in member.name
                    or any(part in ("", ".", "..") for part in member.name.split("/"))
                    or not (member.isdir() or member.isfile())):
                raise RuntimeError(f"unsafe or duplicate mobile archive member: {member.name}")
            seen.add(member.name)
            total += member.size
            if total > 2 * 1024**3:
                raise RuntimeError("mobile archive expanded payload exceeds 2 GiB")
            output = destination.joinpath(*path.parts)
            if member.isdir():
                output.mkdir(parents=True, exist_ok=True)
            else:
                output.parent.mkdir(parents=True, exist_ok=True)
                with archive.extractfile(member) as source, output.open("wb") as sink:
                    shutil.copyfileobj(source, sink)
    manifest = json.loads((destination / "manifest.json").read_text())
    cargo, _ = consumer.coordinate(target, flavor)
    if manifest.get("artifact") != {"target": target, "cargo_target": cargo, "flavor": flavor}:
        raise RuntimeError("mobile archive target/flavor does not match qualification")
    validate_configuration(manifest, ROOT)
    native_archive = manifest["archive"]
    if native_archive["member"] != "lib/libwebrtc.a" or digest(destination / native_archive["member"]) != native_archive["sha256"]:
        raise RuntimeError("mobile native archive identity mismatch")
    unwind = manifest.get("unwind_archive")
    if unwind and digest(destination / unwind["member"]) != unwind["sha256"]:
        raise RuntimeError("mobile Android unwind archive identity mismatch")
    return manifest


def compile_host(kit: Path, work: Path, target: str, flavor: str, manifest: dict) -> tuple[Path, list[Path], dict]:
    cargo, _ = consumer.coordinate(target, flavor)
    environment = os.environ.copy()
    consumer.configure_target_environment(environment, cargo)
    environment["PULSEBEAM_WEBRTC_SYS_ARTIFACT_DIR"] = str(kit)
    environment["CARGO_TARGET_DIR"] = str(work / "rust-target")
    environment["WEBRTC_WORK"] = str(work / "no-producer-checkout")
    for name in ("PULSEBEAM_WEBRTC_SYS_SKIP_LINK", "PULSEBEAM_WEBRTC_SYS_CANDIDATE_LOCK"):
        environment.pop(name, None)
    flags = ["-std=c++20", "-fno-exceptions", "-fno-rtti", "-Wno-nullability-completeness",
             "-DPULSEBEAM_SMOKE_NO_MAIN", *manifest["native_configuration"]["cxx_defines"], "-I", str(kit / "include")]
    if target == "android-x86_64":
        ndk = environment.get("ANDROID_NDK_HOME")
        if not ndk:
            raise RuntimeError("mobile C++ qualification requires a standalone ANDROID_NDK_HOME")
        properties = (Path(ndk) / "source.properties").read_text()
        if not re.search(rf"^Pkg\.Revision\s*=\s*{re.escape(ANDROID_NDK)}\s*$", properties, re.MULTILINE):
            raise RuntimeError(f"mobile qualification requires pinned Android NDK {ANDROID_NDK}")
        compiler = Path(ndk) / "toolchains/llvm/prebuilt/linux-x86_64/bin/x86_64-linux-android26-clang++"
        flags += ["-nostdinc++", "-isystem", str(kit / "include/c++/v1"), "-fPIC"]
    else:
        compiler = Path(invoke(["xcrun", "--sdk", "iphonesimulator", "--find", "clang++"], capture=True).stdout.strip())
        flags += ["-target", "arm64-apple-ios18.0-simulator", "-isysroot", environment["SDKROOT"]]
    print(invoke([compiler, "--version"], capture=True).stdout)
    objects = []
    for name in ("smoke", "frame_callback_slot", "video_frame_metadata", "software_codecs"):
        output = work / f"{name}.o"
        invoke([compiler, *flags, "-c", ROOT / f"consumer/{name}.cc", "-o", output])
        objects.append(output)
    command = ["cargo", "rustc", "--locked", "--manifest-path", ROOT / "consumer/mobile/Cargo.toml", "--target", cargo, "--lib"]
    if flavor == "native":
        command += ["--features", "native"]
    if target == "android-x86_64":
        command += ["--crate-type", "cdylib", "--", *[argument for obj in objects for argument in ("-C", f"link-arg={obj}")]]
    invoke(command, environment=environment, timeout=1800)
    filename = "libpulsebeam_headless.so" if target == "android-x86_64" else "libpulsebeam_headless.a"
    return work / "rust-target" / cargo / "debug" / filename, objects, environment


def android_apk(work: Path, library: Path, sdk: Path) -> Path:
    tools = sdk / "build-tools" / ANDROID_BUILD_TOOLS
    platform_jar = sdk / "platforms/android-26/android.jar"
    classes, dex = work / "classes", work / "dex"
    classes.mkdir(); dex.mkdir()
    invoke(["javac", "-source", "8", "-target", "8", "-bootclasspath", platform_jar, "-d", classes,
            ROOT / "consumer/mobile/android/Headless.java"])
    invoke([tools / "d8", "--min-api", "26", "--lib", platform_jar, "--output", dex,
            classes / "org/pulsebeam/Headless.class"])
    unsigned, aligned, apk = work / "unsigned.apk", work / "aligned.apk", work / "headless.apk"
    invoke([tools / "aapt2", "link", "--manifest", ROOT / "consumer/mobile/android/AndroidManifest.xml",
            "-I", platform_jar, "--min-sdk-version", "26", "--target-sdk-version", "26", "-o", unsigned])
    with zipfile.ZipFile(unsigned, "a", compression=zipfile.ZIP_STORED) as package:
        package.write(dex / "classes.dex", "classes.dex")
        package.write(library, "lib/x86_64/libpulsebeam_headless.so")
    invoke([tools / "zipalign", "-f", "-p", "4", unsigned, aligned])
    key = work / "qualification.keystore"
    invoke(["keytool", "-genkeypair", "-keystore", key, "-storepass", "android", "-keypass", "android",
            "-alias", "pulsebeam", "-dname", "CN=PulseBeamQualification", "-keyalg", "RSA", "-keysize", "2048", "-validity", "1"])
    invoke([tools / "apksigner", "sign", "--ks", key, "--ks-pass", "pass:android", "--key-pass", "pass:android",
            "--min-sdk-version", "26", "--out", apk, aligned])
    return apk


@contextmanager
def android_emulator(sdk: Path, work: Path, serial: str | None):
    adb = sdk / "platform-tools/adb"
    process = None
    if serial is None:
        environment = os.environ.copy()
        environment["ANDROID_AVD_HOME"] = str(work / "avd")
        Path(environment["ANDROID_AVD_HOME"]).mkdir()
        environment["ANDROID_SDK_ROOT"] = str(sdk)
        emulator = sdk / "emulator/emulator"
        # Acceleration is mandatory, not an optional optimization or a skip.
        invoke([emulator, "-accel-check"], environment=environment)
        invoke([sdk / "cmdline-tools/latest/bin/avdmanager", "create", "avd", "--name", "pulsebeam-headless",
                "--package", ANDROID_IMAGE, "--device", "pixel_2"], environment=environment, input="no\n")
        serial = "emulator-5554"
        existing = invoke([adb, "devices"], capture=True).stdout
        if re.search(r"^emulator-5554\s", existing, re.MULTILINE):
            raise RuntimeError("emulator-5554 is already in use; select an existing verified emulator explicitly")
        log = (work / "emulator.log").open("w")
        process = subprocess.Popen([str(emulator), "-avd", "pulsebeam-headless", "-port", "5554", "-no-window",
                                    "-no-audio", "-no-snapshot", "-no-boot-anim", "-gpu", "swiftshader_indirect", "-accel", "on"],
                                   env=environment, stdout=log, stderr=subprocess.STDOUT)
    try:
        if not re.fullmatch(r"emulator-\d+", serial):
            raise RuntimeError("physical Android devices are not qualification targets")
        invoke([adb, "-s", serial, "wait-for-device"], timeout=240)
        deadline = time.monotonic() + 240
        while invoke([adb, "-s", serial, "shell", "getprop", "sys.boot_completed"], capture=True).stdout.strip() != "1":
            if time.monotonic() >= deadline or (process is not None and process.poll() is not None):
                raise RuntimeError("Android emulator failed to boot")
            time.sleep(1)
        yield adb, serial
    finally:
        if process is not None:
            try:
                subprocess.run([str(adb), "-s", serial, "emu", "kill"], capture_output=True, timeout=15)
            except subprocess.TimeoutExpired:
                pass
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.kill(); process.wait()
            log.close()


def run_android(work: Path, library: Path, archive_sha256: str, serial: str | None) -> dict:
    sdk_root = os.environ.get("ANDROID_SDK_ROOT") or os.environ.get("ANDROID_HOME")
    if not sdk_root:
        raise RuntimeError("ANDROID_SDK_ROOT must identify an installed API 26 SDK/emulator")
    sdk = Path(sdk_root)
    apk = android_apk(work, library, sdk)
    nonce = uuid.uuid4().hex
    with android_emulator(sdk, work, serial) as (adb, selected):
        def prop(name):
            return invoke([adb, "-s", selected, "shell", "getprop", name], capture=True).stdout.strip()
        properties = {"api": prop("ro.build.version.sdk"), "abi": prop("ro.product.cpu.abi"), "qemu": prop("ro.kernel.qemu")}
        android_environment(properties)
        fingerprint = prop("ro.build.fingerprint")
        invoke([adb, "-s", selected, "install", "-r", apk])
        try:
            output = invoke([adb, "-s", selected, "shell", "am", "instrument", "-w", "-e", "pulsebeam_nonce", nonce,
                             "-e", "pulsebeam_archive_sha256", archive_sha256,
                             "org.pulsebeam.headless/org.pulsebeam.Headless"], capture=True, timeout=180).stdout
            print(output)
            android_result(output, nonce, archive_sha256)
        finally:
            invoke([adb, "-s", selected, "uninstall", "org.pulsebeam.headless"])
    return {"platform": "Android", **properties, "fingerprint": fingerprint, "ndk": ANDROID_NDK,
            "build_tools": ANDROID_BUILD_TOOLS,
            "nonce": nonce, "package_sha256": digest(apk), "library_sha256": digest(library)}


def run_ios(work: Path, library: Path, objects: list[Path], environment: dict, manifest: dict, archive_sha256: str) -> dict:
    runtimes = json.loads(invoke(["xcrun", "simctl", "list", "runtimes", "--json"], capture=True).stdout)["runtimes"]
    runtime = ios_runtime(runtimes)
    app = work / "PulseBeamHeadless.app"
    app.mkdir()
    bundle = "dev.pulsebeam.headless"
    info = {"CFBundleExecutable": "PulseBeamHeadless", "CFBundleIdentifier": bundle, "CFBundleName": "PulseBeamHeadless",
            "CFBundlePackageType": "APPL", "CFBundleVersion": "1", "CFBundleShortVersionString": "1.0",
            "MinimumOSVersion": "18.0", "UIDeviceFamily": [1, 2], "LSRequiresIPhoneOS": True,
            "CFBundleSupportedPlatforms": ["iPhoneSimulator"]}
    (app / "Info.plist").write_bytes(plistlib.dumps(info))
    links = ["-lc++", "-lobjc", "-liconv"]
    for link in manifest["links"]:
        if link["kind"] == "framework":
            links += ["-framework", link["name"]]
        elif link["kind"] == "dylib":
            links += ["-l" + link["name"]]
        else:
            raise RuntimeError(f"unexpected iOS link requirement: {link}")
    invoke(["xcrun", "--sdk", "iphonesimulator", "clang++", "-std=c++20", "-fobjc-arc",
            "-target", "arm64-apple-ios18.0-simulator", "-isysroot", environment["SDKROOT"],
            ROOT / "consumer/mobile/ios/main.mm", library, *objects, *links, "-o", app / "PulseBeamHeadless"])
    invoke(["codesign", "--force", "--sign", "-", app])
    device = invoke(["xcrun", "simctl", "create", "PulseBeamHeadless", "com.apple.CoreSimulator.SimDeviceType.iPhone-16", runtime], capture=True).stdout.strip()
    nonce = uuid.uuid4().hex
    try:
        invoke(["xcrun", "simctl", "boot", device])
        invoke(["xcrun", "simctl", "bootstatus", device, "-b"], timeout=300)
        invoke(["xcrun", "simctl", "install", device, app])
        child = {**os.environ, "SIMCTL_CHILD_PULSEBEAM_NONCE": nonce, "SIMCTL_CHILD_PULSEBEAM_ARCHIVE_SHA256": archive_sha256}
        invoke(["xcrun", "simctl", "launch", "--console-pty", device, bundle], environment=child, timeout=180)
        container = Path(invoke(["xcrun", "simctl", "get_app_container", device, bundle, "data"], capture=True).stdout.strip())
        result = json.loads((container / "Documents/pulsebeam-result.json").read_text())
        version = result.pop("os_version", None)
        if version != "18.0":
            raise RuntimeError(f"executed application is not on iOS 18.0: {version}")
        validate_result(result, nonce, archive_sha256)
    finally:
        subprocess.run(["xcrun", "simctl", "shutdown", device], capture_output=True, timeout=30)
        invoke(["xcrun", "simctl", "delete", device])
    return {"platform": "iOS simulator", "runtime": runtime, "version": version, "architecture": "arm64",
            "nonce": nonce, "executable_sha256": digest(app / "PulseBeamHeadless")}


def qualify(archive: Path, sha256: str, target: str, flavor: str, output: Path, serial: str | None = None) -> None:
    if target not in {"android-x86_64", "ios-simulator-arm64"}:
        raise RuntimeError("only Android x86_64 and iOS ARM64 simulator have mobile runtime qualification")
    consumer.require_host(target)
    if output.exists():
        raise RuntimeError(f"refusing to overwrite qualification evidence: {output}")
    with tempfile.TemporaryDirectory(prefix="pulsebeam-mobile-") as temporary:
        work = Path(temporary)
        manifest = extract_kit(archive, sha256, work / "kit", target, flavor)
        library, objects, environment = compile_host(work / "kit", work, target, flavor, manifest)
        execution = run_android(work, library, sha256, serial) if target == "android-x86_64" else run_ios(work, library, objects, environment, manifest, sha256)
        report = {"schema_version": 1, "target": target, "flavor": flavor, "archive_sha256": sha256,
                  "native_configuration_sha256": manifest["native_configuration_sha256"], "execution": execution,
                  "host": {"system": platform.system(), "architecture": platform.machine()},
                  "scenarios": ["Rust bridge identity and clock", "headless factory/peer/channel lifecycle and shutdown",
                                "Rust software PCM processing", "SDK C++ software audio/video encode/decode and PCM processing"]}
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps(report, indent=2))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--sha256", required=True)
    parser.add_argument("--target", choices=("android-x86_64", "ios-simulator-arm64"), required=True)
    parser.add_argument("--flavor", choices=("core", "native"), required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--android-serial")
    args = parser.parse_args()
    try:
        qualify(args.archive.resolve(), args.sha256, args.target, args.flavor, args.output, args.android_serial)
    except (OSError, ValueError, KeyError, subprocess.SubprocessError, consumer.ProofError, tarfile.TarError, RuntimeError) as error:
        print(f"mobile qualification: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
