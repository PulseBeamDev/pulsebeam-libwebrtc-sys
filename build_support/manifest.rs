use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const SOURCE_REPOSITORY: &str = "https://github.com/webrtc-sdk/webrtc.git";
const SOURCE_REVISION: &str = "ba469aa2093ba950066258ca0a59a6fbd1295582";
const CORE_IOS_PATCH_SHA256: &str =
    "c05d3e629c6c59f621e0c89be1a26fce31ee6625a9763d4a3d9f492f80454037";
const DEPOT_TOOLS_REPOSITORY: &str =
    "https://chromium.googlesource.com/chromium/tools/depot_tools.git";
const DEPOT_TOOLS_REVISION: &str = "ed9c87f6f12f6b87210e7025d4a36a5a72a2ccd4";
const CXX_VERSION: &str = "1.0.200";
const CXX_PACKAGE_SHA256: &str = "2d93a9baeb9798b2e8ff0de24463b9e5cf9a0dd0a88f5ea02afc9ea35e7abae7";
const CXXBRIDGE_CMD_PACKAGE_SHA256: &str =
    "25784b9f840a76b9b5151553ad70fc2abb3ec3b21fe379c5c7b029b2e0b91faf";
const CXX_RUST_RUNTIME_SHA256: &str =
    "35bce0faa8fc61e8724ed6febaab7e9cedc0e221af9d73bc3d8fb59e9b23471d";
const CXX_HEADER_SHA256: &str = "ea2c1f9fe95b02f055836dd75e22b558f616385a522ec9beeaba8d043c3a32d8";
const CXX_NATIVE_RUNTIME_SHA256: &str =
    "6a64476a783ef8a42da9f30a9e3d48deb7757b87442f6ed68b9d2c38f2768866";
const BRIDGE_SOURCE_SHA256: &str =
    "5dd15a9e42e18f3a279a6b2127c706f69bee03a7071754af877ab420543cdf54";
const GENERATED_BRIDGE_HEADER_SHA256: &str =
    "7bf6377e3e16f5a2aa5aeb26afe28a464502148bf39059649a4ebcaf094c09c8";
const GENERATED_BRIDGE_SOURCE_SHA256: &str =
    "9a5999e97df519e6d2f562e864435bf90865a6f61f2e98a9fbf2981c175ef206";

// Keep this inventory synchronized with tools/native_provenance.py. Embedding
// bytes supports offline Rust-only consumers without running a native compiler.
const NATIVE_ADAPTER_INPUTS: &[(&str, &[u8])] = &[
    ("Justfile", include_bytes!("../Justfile")),
    (
        "tools/native_cc_wrapper.py",
        include_bytes!("../tools/native_cc_wrapper.py"),
    ),
    ("native/audio.cc", include_bytes!("../native/audio.cc")),
    ("native/audio.h", include_bytes!("../native/audio.h")),
    (
        "native/audio_sink_queue.h",
        include_bytes!("../native/audio_sink_queue.h"),
    ),
    (
        "native/video_sink_queue.h",
        include_bytes!("../native/video_sink_queue.h"),
    ),
    (
        "native/frame_callback_slot.h",
        include_bytes!("../native/frame_callback_slot.h"),
    ),
    (
        "native/audio_frame_values.cc",
        include_bytes!("../native/audio_frame_values.cc"),
    ),
    (
        "native/audio_frame_values.h",
        include_bytes!("../native/audio_frame_values.h"),
    ),
    (
        "native/audio_processor.cc",
        include_bytes!("../native/audio_processor.cc"),
    ),
    (
        "native/audio_processor.h",
        include_bytes!("../native/audio_processor.h"),
    ),
    (
        "native/audio_encoder_ana_access.h",
        include_bytes!("../native/audio_encoder_ana_access.h"),
    ),
    (
        "native/audio_encoder_ana.inc",
        include_bytes!("../native/audio_encoder_ana.inc"),
    ),
    (
        "native/audio_device.cc",
        include_bytes!("../native/audio_device.cc"),
    ),
    (
        "native/audio_device.h",
        include_bytes!("../native/audio_device.h"),
    ),
    (
        "native/audio_echo_config.cc",
        include_bytes!("../native/audio_echo_config.cc"),
    ),
    (
        "native/audio_echo_config.h",
        include_bytes!("../native/audio_echo_config.h"),
    ),
    (
        "native/certificate_generator.cc",
        include_bytes!("../native/certificate_generator.cc"),
    ),
    (
        "native/certificate_generator.h",
        include_bytes!("../native/certificate_generator.h"),
    ),
    (
        "native/async_dns.cc",
        include_bytes!("../native/async_dns.cc"),
    ),
    (
        "native/async_dns.h",
        include_bytes!("../native/async_dns.h"),
    ),
    (
        "native/video_bitrate.cc",
        include_bytes!("../native/video_bitrate.cc"),
    ),
    (
        "native/video_bitrate.h",
        include_bytes!("../native/video_bitrate.h"),
    ),
    (
        "native/video_metadata.cc",
        include_bytes!("../native/video_metadata.cc"),
    ),
    (
        "native/video_metadata.h",
        include_bytes!("../native/video_metadata.h"),
    ),
    (
        "native/network_control.cc",
        include_bytes!("../native/network_control.cc"),
    ),
    (
        "native/network_control.h",
        include_bytes!("../native/network_control.h"),
    ),
    (
        "native/network_predictor.cc",
        include_bytes!("../native/network_predictor.cc"),
    ),
    (
        "native/network_predictor.h",
        include_bytes!("../native/network_predictor.h"),
    ),
    ("native/fec.cc", include_bytes!("../native/fec.cc")),
    ("native/fec.h", include_bytes!("../native/fec.h")),
    ("native/clock.cc", include_bytes!("../native/clock.cc")),
    ("native/clock.h", include_bytes!("../native/clock.h")),
    ("native/codec.cc", include_bytes!("../native/codec.cc")),
    ("native/codec.h", include_bytes!("../native/codec.h")),
    (
        "native/data_channel.cc",
        include_bytes!("../native/data_channel.cc"),
    ),
    (
        "native/data_channel.h",
        include_bytes!("../native/data_channel.h"),
    ),
    (
        "native/event_log.cc",
        include_bytes!("../native/event_log.cc"),
    ),
    (
        "native/event_log.h",
        include_bytes!("../native/event_log.h"),
    ),
    (
        "native/execution.cc",
        include_bytes!("../native/execution.cc"),
    ),
    (
        "native/execution.h",
        include_bytes!("../native/execution.h"),
    ),
    (
        "native/local_network_permission.cc",
        include_bytes!("../native/local_network_permission.cc"),
    ),
    (
        "native/local_network_permission.h",
        include_bytes!("../native/local_network_permission.h"),
    ),
    (
        "native/media_values.cc",
        include_bytes!("../native/media_values.cc"),
    ),
    (
        "native/media_values.h",
        include_bytes!("../native/media_values.h"),
    ),
    (
        "native/metronome.cc",
        include_bytes!("../native/metronome.cc"),
    ),
    (
        "native/metronome.h",
        include_bytes!("../native/metronome.h"),
    ),
    ("native/network.cc", include_bytes!("../native/network.cc")),
    ("native/network.h", include_bytes!("../native/network.h")),
    (
        "native/opus_carrier.h",
        include_bytes!("../native/opus_carrier.h"),
    ),
    ("native/peer.cc", include_bytes!("../native/peer.cc")),
    ("native/peer.h", include_bytes!("../native/peer.h")),
    ("native/stats.cc", include_bytes!("../native/stats.cc")),
    ("native/stats.h", include_bytes!("../native/stats.h")),
    (
        "native/stats_objects.h",
        include_bytes!("../native/stats_objects.h"),
    ),
    ("native/probe.cc", include_bytes!("../native/probe.cc")),
    ("native/probe.h", include_bytes!("../native/probe.h")),
    (
        "native/socket_address.cc",
        include_bytes!("../native/socket_address.cc"),
    ),
    (
        "native/socket_address.h",
        include_bytes!("../native/socket_address.h"),
    ),
    (
        "native/task_queue_provider.cc",
        include_bytes!("../native/task_queue_provider.cc"),
    ),
    (
        "native/task_queue_provider.h",
        include_bytes!("../native/task_queue_provider.h"),
    ),
    ("native/tls.cc", include_bytes!("../native/tls.cc")),
    ("native/tls.h", include_bytes!("../native/tls.h")),
    ("native/video.cc", include_bytes!("../native/video.cc")),
    ("native/video.h", include_bytes!("../native/video.h")),
    (
        "native/media_stream.cc",
        include_bytes!("../native/media_stream.cc"),
    ),
    (
        "native/media_stream.h",
        include_bytes!("../native/media_stream.h"),
    ),
    (
        "native/frame_transformer.cc",
        include_bytes!("../native/frame_transformer.cc"),
    ),
    (
        "native/frame_transformer.h",
        include_bytes!("../native/frame_transformer.h"),
    ),
    (
        "native/rtc_error.cc",
        include_bytes!("../native/rtc_error.cc"),
    ),
    (
        "native/rtc_error.h",
        include_bytes!("../native/rtc_error.h"),
    ),
    (
        "native/rtp_values.cc",
        include_bytes!("../native/rtp_values.cc"),
    ),
    (
        "native/rtp_values.h",
        include_bytes!("../native/rtp_values.h"),
    ),
    (
        "native/string_builder.cc",
        include_bytes!("../native/string_builder.cc"),
    ),
    (
        "native/string_builder.h",
        include_bytes!("../native/string_builder.h"),
    ),
    (
        "native/transport.cc",
        include_bytes!("../native/transport.cc"),
    ),
    (
        "native/transport.h",
        include_bytes!("../native/transport.h"),
    ),
    (
        "native/video_codec_values.cc",
        include_bytes!("../native/video_codec_values.cc"),
    ),
    (
        "native/video_codec_values.h",
        include_bytes!("../native/video_codec_values.h"),
    ),
    (
        "native/video_encoder_selector.cc",
        include_bytes!("../native/video_encoder_selector.cc"),
    ),
    (
        "native/video_encoder_selector.h",
        include_bytes!("../native/video_encoder_selector.h"),
    ),
];

fn native_adapter_digest() -> String {
    let mut inventory: Vec<_> = NATIVE_ADAPTER_INPUTS
        .iter()
        .map(|(path, bytes)| (*path, format!("{:x}", Sha256::digest(bytes))))
        .collect();
    inventory.sort_unstable_by_key(|(path, _)| *path);
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&inventory).unwrap())
    )
}
pub(crate) const SUPPORTED_CARGO_TARGETS: [&str; 9] = [
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-pc-windows-msvc",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-linux-android",
    "aarch64-linux-android",
    "aarch64-apple-ios",
    "aarch64-apple-ios-sim",
];

pub(crate) fn artifact_target(cargo_target: &str) -> Option<&'static str> {
    match cargo_target {
        "x86_64-unknown-linux-gnu" => Some("linux-x86_64"),
        "aarch64-unknown-linux-gnu" => Some("linux-arm64"),
        "x86_64-pc-windows-msvc" => Some("windows-x86_64"),
        "x86_64-apple-darwin" => Some("macos-x86_64"),
        "aarch64-apple-darwin" => Some("macos-arm64"),
        "x86_64-linux-android" => Some("android-x86_64"),
        "aarch64-linux-android" => Some("android-arm64-v8a"),
        "aarch64-apple-ios" => Some("ios-arm64"),
        "aarch64-apple-ios-sim" => Some("ios-simulator-arm64"),
        _ => None,
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ArtifactManifest {
    schema_version: u32,
    bridge: Bridge,
    sources: Sources,
    artifact: Artifact,
    native_configuration_sha256: String,
    native_configuration: NativeConfiguration,
    abi: Abi,
    pub(crate) archive: Archive,
    pub(crate) links: Vec<Link>,
    licenses: LicenseInventory,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bridge {
    identity: String,
    native_adapter_sha256: String,
    cxx: CxxProvenance,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CxxProvenance {
    version: String,
    runtime_package_sha256: String,
    generator_package_sha256: String,
    rust_runtime_sha256: String,
    header_sha256: String,
    native_runtime_sha256: String,
    bridge_source_sha256: String,
    generated_header_sha256: String,
    generated_source_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Sources {
    webrtc: Source,
    depot_tools: Source,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    repository: String,
    revision: String,
    #[serde(default)]
    patch_sha256: Option<String>,
    #[serde(default)]
    state: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    flavor: String,
    target: String,
    cargo_target: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Abi {
    toolchain: String,
    cxx_standard: String,
    cxx_runtime: String,
    crt: String,
    minimum_runtime: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Archive {
    pub(crate) member: String,
    pub(crate) name: String,
    sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Link {
    pub(crate) kind: LinkKind,
    pub(crate) name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LinkKind {
    Dylib,
    Framework,
    WeakFramework,
    LinkArg,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LicenseInventory {
    sha256: String,
    files: Vec<LicenseFile>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LicenseFile {
    path: String,
    sha256: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeConfiguration {
    native_adapter_sha256: String,
    bridge_identity: String,
    cxx: CxxProvenance,
    source_revision: String,
    source_patch_sha256: String,
    source_state: String,
    depot_tools_revision: String,
    flavor: String,
    target: String,
    cargo_target: String,
    toolchain: String,
    gn_args: std::collections::BTreeMap<String, String>,
    cxx_defines: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    floating_point: Option<FloatingPoint>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FloatingPoint {
    final_compiler_flags: Vec<String>,
    compiler_wrapper_sha256: String,
}

fn canonical_digest(value: &impl Serialize) -> String {
    // Value maps are sorted recursively, matching the producer's sort_keys=True.
    let value = serde_json::to_value(value).unwrap();
    format!("{:x}", Sha256::digest(serde_json::to_vec(&value).unwrap()))
}

// The manifest is portable, so build-host tool prefixes may differ. Match the
// producer's command structure, not a digest substring inside an unrelated shell
// command. shlex preserves quoted paths without accepting shell execution here.
fn valid_compiler_wrapper(value: &str, digest: &str) -> bool {
    let Some(command) = shlex::split(value) else {
        return false;
    };
    if command.len() != 5 {
        return false;
    }
    let interpreter = std::path::Path::new(&command[0])
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let python = interpreter.strip_prefix("python").is_some_and(|version| {
        version.is_empty()
            || (version.bytes().any(|byte| byte.is_ascii_digit())
                && version
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || byte == b'.'))
    });
    python
        && std::path::Path::new(&command[1]).ends_with("tools/native_cc_wrapper.py")
        && command[2] == "--contract-sha256"
        && command[3] == digest
        && std::path::Path::new(&command[4])
            .file_name()
            .is_some_and(|name| name == "sccache")
}

impl ArtifactManifest {
    pub(crate) fn parse_and_validate(
        bytes: &[u8],
        expected_bridge: &str,
        expected_flavor: &str,
        expected_target: &str,
        expected_cargo_target: &str,
    ) -> Result<Self, String> {
        let manifest: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if manifest.schema_version != 4 {
            return Err(format!(
                "unsupported manifest schema {}",
                manifest.schema_version
            ));
        }
        // Source-only FFI edits can change layouts/call signatures without
        // changing a native adapter file. Never accept an older pin merely
        // because its archived manifest matches the still-recorded constants.
        let current_bridge_source =
            format!("{:x}", Sha256::digest(include_bytes!("../src/lib.rs")));
        if current_bridge_source != BRIDGE_SOURCE_SHA256 {
            return Err("recorded bridge source pin is stale for this Rust FFI; regenerate bridge provenance and build a matching native artifact".to_owned());
        }
        if manifest.bridge.native_adapter_sha256 != native_adapter_digest() {
            return Err("artifact was generated from stale native adapter/build inputs".to_owned());
        }
        for (name, actual, expected) in [
            (
                "bridge identity",
                manifest.bridge.identity.as_str(),
                expected_bridge,
            ),
            (
                "CXX version",
                manifest.bridge.cxx.version.as_str(),
                CXX_VERSION,
            ),
            (
                "CXX runtime package checksum",
                manifest.bridge.cxx.runtime_package_sha256.as_str(),
                CXX_PACKAGE_SHA256,
            ),
            (
                "CXX generator package checksum",
                manifest.bridge.cxx.generator_package_sha256.as_str(),
                CXXBRIDGE_CMD_PACKAGE_SHA256,
            ),
            (
                "CXX Rust runtime digest",
                manifest.bridge.cxx.rust_runtime_sha256.as_str(),
                CXX_RUST_RUNTIME_SHA256,
            ),
            (
                "CXX header digest",
                manifest.bridge.cxx.header_sha256.as_str(),
                CXX_HEADER_SHA256,
            ),
            (
                "CXX native runtime digest",
                manifest.bridge.cxx.native_runtime_sha256.as_str(),
                CXX_NATIVE_RUNTIME_SHA256,
            ),
            (
                "bridge source digest",
                manifest.bridge.cxx.bridge_source_sha256.as_str(),
                BRIDGE_SOURCE_SHA256,
            ),
            (
                "generated bridge header digest",
                manifest.bridge.cxx.generated_header_sha256.as_str(),
                GENERATED_BRIDGE_HEADER_SHA256,
            ),
            (
                "generated bridge source digest",
                manifest.bridge.cxx.generated_source_sha256.as_str(),
                GENERATED_BRIDGE_SOURCE_SHA256,
            ),
            (
                "source repository",
                manifest.sources.webrtc.repository.as_str(),
                SOURCE_REPOSITORY,
            ),
            (
                "source revision",
                manifest.sources.webrtc.revision.as_str(),
                SOURCE_REVISION,
            ),
            (
                "depot_tools repository",
                manifest.sources.depot_tools.repository.as_str(),
                DEPOT_TOOLS_REPOSITORY,
            ),
            (
                "depot_tools revision",
                manifest.sources.depot_tools.revision.as_str(),
                DEPOT_TOOLS_REVISION,
            ),
            ("flavor", manifest.artifact.flavor.as_str(), expected_flavor),
            ("target", manifest.artifact.target.as_str(), expected_target),
            (
                "Cargo target",
                manifest.artifact.cargo_target.as_str(),
                expected_cargo_target,
            ),
        ] {
            if actual != expected {
                return Err(format!("wrong {name}: expected {expected}, got {actual}"));
            }
        }
        let expected_source_state = if expected_flavor == "core"
            && matches!(expected_target, "ios-arm64" | "ios-simulator-arm64")
        {
            "applied"
        } else {
            "pristine"
        };
        match (
            manifest.sources.webrtc.state.as_deref(),
            manifest.sources.webrtc.patch_sha256.as_deref(),
        ) {
            (Some(state), Some(patch))
                if state == expected_source_state && patch == CORE_IOS_PATCH_SHA256 => {}
            (None, None) if expected_source_state == "pristine" => {}
            (state, patch) => {
                return Err(format!(
                    "wrong WebRTC source state: expected {expected_source_state} with patch {CORE_IOS_PATCH_SHA256}, got state={state:?} patch={patch:?}"
                ));
            }
        }
        let expected_member = if expected_target == "windows-x86_64" {
            "lib/webrtc.lib"
        } else {
            "lib/libwebrtc.a"
        };
        if manifest.archive.member != expected_member || manifest.archive.name != "webrtc" {
            return Err("wrong native archive identity".into());
        }
        for (name, value) in [
            ("archive digest", manifest.archive.sha256.as_str()),
            (
                "native configuration digest",
                manifest.native_configuration_sha256.as_str(),
            ),
            (
                "license inventory digest",
                manifest.licenses.sha256.as_str(),
            ),
        ] {
            if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(format!("invalid {name}"));
            }
        }
        if manifest.abi.toolchain.is_empty()
            || manifest.abi.cxx_standard != "C++20"
            || manifest.abi.cxx_runtime.is_empty()
            || manifest.abi.crt.is_empty()
            || manifest.abi.minimum_runtime.is_empty()
        {
            return Err("incomplete ABI identity".into());
        }
        if manifest.licenses.files.is_empty()
            || manifest.licenses.files.iter().any(|file| {
                file.path.is_empty()
                    || file.sha256.len() != 64
                    || !file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        {
            return Err("invalid license inventory".into());
        }
        if manifest
            .links
            .iter()
            .any(|link| link.name.is_empty() || link.name.contains(['\r', '\n']))
        {
            return Err("invalid native link entry".into());
        }
        manifest.validate_configuration()?;
        Ok(manifest)
    }

    fn validate_configuration(&self) -> Result<(), String> {
        let config = &self.native_configuration;
        if canonical_digest(config) != self.native_configuration_sha256 {
            return Err("native configuration digest mismatch".into());
        }
        if config.native_adapter_sha256 != self.bridge.native_adapter_sha256
            || config.bridge_identity != self.bridge.identity
            || canonical_digest(&config.cxx) != canonical_digest(&self.bridge.cxx)
            || config.source_revision != self.sources.webrtc.revision
            || Some(&config.source_patch_sha256) != self.sources.webrtc.patch_sha256.as_ref()
            || Some(&config.source_state) != self.sources.webrtc.state.as_ref()
            || config.depot_tools_revision != self.sources.depot_tools.revision
            || config.flavor != self.artifact.flavor
            || config.target != self.artifact.target
            || config.cargo_target != self.artifact.cargo_target
            || config.toolchain != self.abi.toolchain
        {
            return Err("native configuration identity mismatch".into());
        }
        if config.cxx_defines.is_empty()
            || config
                .cxx_defines
                .iter()
                .any(|value| !value.starts_with("-D") || value.contains(['\n', '\r']))
        {
            return Err("invalid native configuration definitions".into());
        }
        if self.artifact.target == "linux-x86_64" {
            let native = self.artifact.flavor == "native";
            for (key, expected) in [
                ("target_os", "linux"),
                ("target_cpu", "x64"),
                ("rtc_use_h264", "true"),
                ("proprietary_codecs", "true"),
                ("ffmpeg_branding", "Chrome"),
                ("rtc_system_openh264", "false"),
                ("rtc_include_opus", "true"),
                ("rtc_build_opus", "true"),
                ("rtc_build_libvpx", "true"),
                ("rtc_libvpx_build_vp9", "true"),
                ("rtc_include_builtin_audio_codecs", "true"),
                ("rtc_include_dav1d_in_internal_decoder_factory", "true"),
                (
                    "rtc_include_internal_audio_device",
                    if native { "true" } else { "false" },
                ),
                ("rtc_use_x11", if native { "true" } else { "false" }),
                ("rtc_use_pipewire", if native { "true" } else { "false" }),
            ] {
                if config.gn_args.get(key).map(String::as_str) != Some(expected) {
                    return Err(format!("wrong native GN configuration: {key}"));
                }
            }
            if !matches!(
                config
                    .gn_args
                    .get("rtc_enable_protobuf")
                    .map(String::as_str),
                Some("true" | "false")
            ) {
                return Err("missing native protobuf configuration".into());
            }
            let wrapper_digest = format!(
                "{:x}",
                Sha256::digest(include_bytes!("../tools/native_cc_wrapper.py"))
            );
            let fp = config
                .floating_point
                .as_ref()
                .ok_or("missing native floating-point configuration")?;
            if fp.final_compiler_flags != ["-fno-fast-math", "-ffp-contract=off"]
                || fp.compiler_wrapper_sha256 != wrapper_digest
                || !config
                    .gn_args
                    .get("cc_wrapper")
                    .is_some_and(|wrapper| valid_compiler_wrapper(wrapper, &wrapper_digest))
            {
                return Err("wrong native floating-point configuration".into());
            }
        }
        Ok(())
    }

    pub(crate) fn validate_payloads(&self, root: &std::path::Path) -> Result<(), String> {
        let check = |relative: &str, expected: &str| -> Result<(), String> {
            let path = std::path::Path::new(relative);
            if !path
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_)))
            {
                return Err(format!("invalid artifact payload path: {relative}"));
            }
            let path = root.join(path);
            if !std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_file()) {
                return Err(format!("missing regular artifact payload: {relative}"));
            }
            let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
            if format!("{:x}", Sha256::digest(bytes)) != expected {
                return Err(format!("artifact payload digest mismatch: {relative}"));
            }
            Ok(())
        };
        check(&self.archive.member, &self.archive.sha256)?;
        if canonical_digest(&self.licenses.files) != self.licenses.sha256 {
            return Err("license inventory digest mismatch".into());
        }
        for file in &self.licenses.files {
            if !file.path.starts_with("LICENSES/") {
                return Err("license outside LICENSES directory".into());
            }
            check(&file.path, &file.sha256)?;
        }
        Ok(())
    }

    pub(crate) fn cargo_link_directives(&self, library_dir: &str) -> Vec<String> {
        let mut directives = vec![
            format!("cargo::rustc-link-search=native={library_dir}"),
            format!("cargo::rustc-link-lib=static={}", self.archive.name),
        ];
        for link in &self.links {
            match link.kind {
                LinkKind::Dylib => directives.push(format!("cargo::rustc-link-lib={}", link.name)),
                LinkKind::Framework => {
                    directives.push(format!("cargo::rustc-link-lib=framework={}", link.name));
                }
                LinkKind::WeakFramework => {
                    directives.push("cargo::rustc-link-arg=-weak_framework".into());
                    directives.push(format!("cargo::rustc-link-arg={}", link.name));
                }
                LinkKind::LinkArg => {
                    directives.push(format!("cargo::rustc-link-arg={}", link.name));
                }
            }
        }
        directives
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ArtifactManifest, BRIDGE_SOURCE_SHA256, NATIVE_ADAPTER_INPUTS, SUPPORTED_CARGO_TARGETS,
        artifact_target, canonical_digest, native_adapter_digest,
    };
    use sha2::{Digest, Sha256};

    const FIXTURE: &[u8] = include_bytes!("../tests/fixtures/artifact-support-manifest.json");

    fn validate(bytes: &[u8]) -> Result<ArtifactManifest, String> {
        ArtifactManifest::parse_and_validate(
            bytes,
            "pulsebeam-webrtc-sys-bridge-v2",
            "core",
            "linux-x86_64",
            "x86_64-unknown-linux-gnu",
        )
    }

    #[test]
    fn configuration_hash_uses_sorted_utf8_json() {
        assert_eq!(
            canonical_digest(&serde_json::json!({"path": "é/路径"})),
            format!("{:x}", Sha256::digest("{\"path\":\"é/路径\"}".as_bytes()))
        );
    }

    #[test]
    fn accepts_host_fixture() {
        validate(FIXTURE).unwrap();
    }

    #[test]
    fn bridge_source_digest_matches_tracked_bridge_source() {
        let digest = Sha256::digest(include_bytes!("../src/lib.rs"));
        assert_eq!(format!("{digest:x}"), BRIDGE_SOURCE_SHA256);
    }

    #[test]
    fn adapter_inventory_covers_all_native_sources_headers_and_build_recipe() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut expected = vec![
            "Justfile".to_owned(),
            "tools/native_cc_wrapper.py".to_owned(),
        ];
        for entry in std::fs::read_dir(root.join("native")).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file()
                && matches!(
                    path.extension().and_then(|e| e.to_str()),
                    Some("cc" | "h" | "inc")
                )
            {
                expected.push(format!(
                    "native/{}",
                    path.file_name().unwrap().to_str().unwrap()
                ));
            }
        }
        expected.sort();
        let mut actual = NATIVE_ADAPTER_INPUTS
            .iter()
            .map(|(path, _)| path.to_string())
            .collect::<Vec<_>>();
        actual.sort();
        assert_eq!(expected, actual);
    }

    #[test]
    fn rejects_missing_or_stale_adapter_identity_and_legacy_manifest_schema() {
        let mut value: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
        assert_eq!(
            value["bridge"]["native_adapter_sha256"],
            native_adapter_digest()
        );
        value["bridge"]["native_adapter_sha256"] = "0".repeat(64).into();
        assert!(
            validate(&serde_json::to_vec(&value).unwrap())
                .unwrap_err()
                .contains("stale native")
        );
        value["bridge"]
            .as_object_mut()
            .unwrap()
            .remove("native_adapter_sha256");
        assert!(validate(&serde_json::to_vec(&value).unwrap()).is_err());
        let mut value: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
        value["schema_version"] = 2.into();
        assert!(
            validate(&serde_json::to_vec(&value).unwrap())
                .unwrap_err()
                .contains("schema 2")
        );
    }

    #[test]
    fn maps_every_supported_cargo_target_exactly() {
        let artifact_targets = [
            "linux-x86_64",
            "linux-arm64",
            "windows-x86_64",
            "macos-x86_64",
            "macos-arm64",
            "android-x86_64",
            "android-arm64-v8a",
            "ios-arm64",
            "ios-simulator-arm64",
        ];
        for (cargo_target, expected) in SUPPORTED_CARGO_TARGETS.into_iter().zip(artifact_targets) {
            assert_eq!(artifact_target(cargo_target), Some(expected));
        }
        assert_eq!(artifact_target("x86_64-unknown-linux-musl"), None);
    }

    #[test]
    fn rejects_substitution_by_bridge_target_flavor_or_cargo_target() {
        for (pointer, wrong) in [
            ("/bridge/identity", "wrong-bridge"),
            ("/artifact/target", "linux-arm64"),
            ("/artifact/flavor", "native"),
            ("/artifact/cargo_target", "aarch64-unknown-linux-gnu"),
        ] {
            let mut invalid: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
            *invalid.pointer_mut(pointer).expect("fixture field") = wrong.into();
            assert!(
                validate(&serde_json::to_vec(&invalid).unwrap()).is_err(),
                "{pointer}"
            );
        }
    }

    #[test]
    fn rejects_missing_malformed_or_inconsistent_cxx_provenance() {
        for (key, replacement) in [
            ("generated_source_sha256", None),
            ("runtime_package_sha256", Some("not-a-sha256")),
            (
                "generator_package_sha256",
                Some("0000000000000000000000000000000000000000000000000000000000000000"),
            ),
        ] {
            let mut invalid: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
            let cxx = invalid["bridge"]["cxx"].as_object_mut().unwrap();
            if let Some(value) = replacement {
                *cxx.get_mut(key).expect("fixture CXX field") = value.into();
            } else {
                assert!(cxx.remove(key).is_some());
            }
            assert!(
                validate(&serde_json::to_vec(&invalid).unwrap()).is_err(),
                "{key}"
            );
        }
    }

    #[test]
    fn rejects_changed_configuration_even_when_the_replacement_is_rehashed() {
        for (pointer, replacement) in [
            ("/native_configuration/gn_args/rtc_use_h264", "false"),
            (
                "/native_configuration/gn_args/rtc_include_internal_audio_device",
                "true",
            ),
            ("/native_configuration/toolchain", "unrelated compiler"),
            (
                "/native_configuration/floating_point/compiler_wrapper_sha256",
                "bad",
            ),
        ] {
            let mut value: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
            *value.pointer_mut(pointer).unwrap() = replacement.into();
            assert!(
                validate(&serde_json::to_vec(&value).unwrap())
                    .unwrap_err()
                    .contains("configuration digest mismatch")
            );
            value["native_configuration_sha256"] =
                canonical_digest(&value["native_configuration"]).into();
            assert!(
                validate(&serde_json::to_vec(&value).unwrap()).is_err(),
                "{pointer}"
            );
        }
    }

    #[test]
    fn rejects_unrelated_wrapper_commands_even_with_the_expected_digest() {
        let mut value: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
        let digest = value["native_configuration"]["floating_point"]["compiler_wrapper_sha256"]
            .as_str()
            .unwrap()
            .to_owned();
        for wrapper in [
            format!("unrelated --contract-sha256 {digest} sccache"),
            format!("false tools/native_cc_wrapper.py --contract-sha256 {digest} sccache"),
            format!("python tools/unrelated.py --contract-sha256 {digest} sccache"),
            format!("python tools/native_cc_wrapper.py --contract-sha256 {digest} sccache extra"),
        ] {
            value["native_configuration"]["gn_args"]["cc_wrapper"] = wrapper.into();
            value["native_configuration_sha256"] =
                canonical_digest(&value["native_configuration"]).into();
            assert!(
                validate(&serde_json::to_vec(&value).unwrap())
                    .unwrap_err()
                    .contains("floating-point")
            );
        }
        value["native_configuration"]["gn_args"]["cc_wrapper"] = format!(
            "'/host tools/bin/python3.13' '/relocated source/tools/native_cc_wrapper.py' --contract-sha256 {digest} '/cache tools/sccache'"
        ).into();
        value["native_configuration_sha256"] =
            canonical_digest(&value["native_configuration"]).into();
        validate(&serde_json::to_vec(&value).unwrap()).unwrap();
    }

    #[test]
    fn distinguishes_protobuf_configurations_without_changing_software_capabilities() {
        let mut value: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
        value["native_configuration"]["gn_args"]["rtc_enable_protobuf"] = "false".into();
        value["native_configuration_sha256"] =
            canonical_digest(&value["native_configuration"]).into();
        validate(&serde_json::to_vec(&value).unwrap()).unwrap();
    }

    #[test]
    fn translates_every_link_kind_in_order_and_rejects_unknown_kinds() {
        let mut fixture: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
        fixture["links"] = serde_json::json!([
            {"kind": "dylib", "name": "pthread"},
            {"kind": "framework", "name": "CoreMedia"},
            {"kind": "weak_framework", "name": "VideoToolbox"},
            {"kind": "link_arg", "name": "-pthread"}
        ]);
        let fixture = serde_json::to_vec(&fixture).unwrap();
        let manifest = validate(&fixture).unwrap();
        assert_eq!(
            manifest.cargo_link_directives("/artifact/lib"),
            [
                "cargo::rustc-link-search=native=/artifact/lib",
                "cargo::rustc-link-lib=static=webrtc",
                "cargo::rustc-link-lib=pthread",
                "cargo::rustc-link-lib=framework=CoreMedia",
                "cargo::rustc-link-arg=-weak_framework",
                "cargo::rustc-link-arg=VideoToolbox",
                "cargo::rustc-link-arg=-pthread",
            ]
        );
        let unknown = String::from_utf8(fixture)
            .unwrap()
            .replacen("link_arg", "mystery", 1);
        assert!(validate(unknown.as_bytes()).is_err());
    }
}
