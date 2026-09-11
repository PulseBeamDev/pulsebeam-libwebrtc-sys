use serde::Deserialize;

const SOURCE_REPOSITORY: &str = "https://github.com/webrtc-sdk/webrtc.git";
const SOURCE_REVISION: &str = "ba469aa2093ba950066258ca0a59a6fbd1295582";
const DEPOT_TOOLS_REPOSITORY: &str =
    "https://chromium.googlesource.com/chromium/tools/depot_tools.git";
const DEPOT_TOOLS_REVISION: &str = "ed9c87f6f12f6b87210e7025d4a36a5a72a2ccd4";
const CXX_VERSION: &str = "1.0.200";

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
    abi: Abi,
    pub(crate) archive: Archive,
    pub(crate) links: Vec<Link>,
    licenses: LicenseInventory,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bridge {
    identity: String,
    cxx_version: String,
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LicenseFile {
    path: String,
    sha256: String,
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
        if manifest.schema_version != 1 {
            return Err(format!(
                "unsupported manifest schema {}",
                manifest.schema_version
            ));
        }
        for (name, actual, expected) in [
            (
                "bridge identity",
                manifest.bridge.identity.as_str(),
                expected_bridge,
            ),
            (
                "CXX version",
                manifest.bridge.cxx_version.as_str(),
                CXX_VERSION,
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
        let expected_member = if expected_target == "windows-x86_64" {
            "lib/webrtc.lib"
        } else {
            "lib/libwebrtc.a"
        };
        if manifest.archive.member != expected_member || manifest.archive.name != "webrtc" {
            return Err("wrong native archive identity".into());
        }
        for (name, value) in [
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
        Ok(manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::{ArtifactManifest, SUPPORTED_CARGO_TARGETS, artifact_target};

    const FIXTURE: &[u8] = include_bytes!("../native/manifest.core-linux-x86_64.json");

    fn validate(bytes: &[u8]) -> Result<ArtifactManifest, String> {
        ArtifactManifest::parse_and_validate(
            bytes,
            "pulsebeam-webrtc-sys-bridge-v1",
            "core",
            "linux-x86_64",
            "x86_64-unknown-linux-gnu",
        )
    }

    #[test]
    fn accepts_host_fixture() {
        validate(FIXTURE).unwrap();
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
        let fixture = String::from_utf8(FIXTURE.to_vec()).unwrap();
        for (expected, wrong) in [
            ("pulsebeam-webrtc-sys-bridge-v1", "wrong-bridge"),
            ("linux-x86_64", "linux-arm64"),
            (r#""flavor": "core""#, r#""flavor": "native""#),
            ("x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"),
        ] {
            let invalid = fixture.replacen(expected, wrong, 1);
            assert!(validate(invalid.as_bytes()).is_err());
        }
    }
}
