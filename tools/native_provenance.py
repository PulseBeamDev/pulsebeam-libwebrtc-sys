"""Path-aware identity of repository adapters and their native build recipe.

The generated bridge and vendor CXX runtime have separate provenance. Include
all native C++ sources/headers (but not manifest fixtures), the Justfile and
native compiler wrapper that select, compile and link them. JSON arrays use UTF-8 and compact separators.
"""

import hashlib
import json
from pathlib import Path
import shlex


def adapter_inputs(root: Path) -> list[Path]:
    return sorted(
        [root / "Justfile", root / "tools/native_cc_wrapper.py",
         *root.glob("native/*.cc"), *root.glob("native/*.h"), *root.glob("native/*.inc")],
        key=lambda path: path.relative_to(root).as_posix(),
    )


def configuration_digest(value: object) -> str:
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def valid_compiler_wrapper(value: str, digest: str) -> bool:
    """Match the producer's five-token command, allowing relocated tool paths."""
    try:
        command = shlex.split(value)
    except ValueError:
        return False
    if len(command) != 5:
        return False
    interpreter = Path(command[0]).name
    version = interpreter.removeprefix("python")
    return (interpreter.startswith("python")
            and (not version or (any(char.isdigit() for char in version)
                                 and all(char in "0123456789." for char in version)))
            and Path(command[1]).parts[-2:] == ("tools", "native_cc_wrapper.py")
            and command[2:4] == ["--contract-sha256", digest]
            and Path(command[4]).name == "sccache")


def validate_configuration(manifest: dict, root: Path) -> None:
    config = manifest.get("native_configuration")
    if not isinstance(config, dict) or configuration_digest(config) != manifest.get("native_configuration_sha256"):
        raise ValueError("native configuration digest mismatch")
    bridge, sources, artifact = manifest["bridge"], manifest["sources"], manifest["artifact"]
    expected = {
        "native_adapter_sha256": adapter_digest(root), "bridge_identity": bridge["identity"],
        "cxx": bridge["cxx"], "source_revision": sources["webrtc"]["revision"],
        "source_patch_sha256": sources["webrtc"]["patch_sha256"],
        "source_encoder_switch_patch_sha256": sources["webrtc"]["encoder_switch_patch_sha256"],
        "source_state": sources["webrtc"]["state"],
        "depot_tools_revision": sources["depot_tools"]["revision"],
        **artifact, "toolchain": manifest["abi"]["toolchain"],
    }
    if any(config.get(key) != value for key, value in expected.items()):
        raise ValueError("native configuration identity mismatch")
    definitions = config.get("cxx_defines")
    if not isinstance(definitions, list) or not definitions or any(
            not isinstance(value, str) or not value.startswith("-D") or "\n" in value or "\r" in value
            for value in definitions):
        raise ValueError("invalid native configuration definitions")
    if artifact["target"] == "linux-x86_64":
        native = artifact["flavor"] == "native"
        expected_gn = {key: "true" for key in (
            "rtc_use_h264", "proprietary_codecs", "rtc_include_opus", "rtc_build_opus",
            "rtc_build_libvpx", "rtc_libvpx_build_vp9", "rtc_include_builtin_audio_codecs",
            "rtc_include_dav1d_in_internal_decoder_factory")}
        expected_gn.update(target_os="linux", target_cpu="x64", ffmpeg_branding="Chrome",
                           rtc_system_openh264="false", rtc_include_internal_audio_device=str(native).lower(),
                           rtc_use_x11=str(native).lower(), rtc_use_pipewire=str(native).lower())
        gn = config.get("gn_args", {})
        if any(gn.get(key) != value for key, value in expected_gn.items()) or gn.get("rtc_enable_protobuf") not in ("true", "false"):
            raise ValueError("wrong native GN configuration")
        wrapper_digest = hashlib.sha256((root / "tools/native_cc_wrapper.py").read_bytes()).hexdigest()
        if config.get("floating_point") != {
                "final_compiler_flags": ["-fno-fast-math", "-ffp-contract=off"],
                "compiler_wrapper_sha256": wrapper_digest} or not valid_compiler_wrapper(gn.get("cc_wrapper", ""), wrapper_digest):
            raise ValueError("wrong native floating-point configuration")


def adapter_digest(root: Path) -> str:
    inventory = [
        [path.relative_to(root).as_posix(), hashlib.sha256(path.read_bytes()).hexdigest()]
        for path in adapter_inputs(root)
    ]
    encoded = json.dumps(inventory, separators=(",", ":"), ensure_ascii=True).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()
