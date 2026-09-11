use serde::Deserialize;

const SOURCE_REPOSITORY: &str = "https://github.com/webrtc-sdk/webrtc.git";
const SOURCE_REVISION: &str = "ba469aa2093ba950066258ca0a59a6fbd1295582";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ArtifactManifest {
    schema_version: u32,
    bridge_identity: String,
    source_repository: String,
    source_revision: String,
    flavor: String,
    target: String,
    cargo_target: String,
    pub(crate) archive: Archive,
    pub(crate) links: Vec<Link>,
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
                manifest.bridge_identity.as_str(),
                expected_bridge,
            ),
            (
                "source repository",
                manifest.source_repository.as_str(),
                SOURCE_REPOSITORY,
            ),
            (
                "source revision",
                manifest.source_revision.as_str(),
                SOURCE_REVISION,
            ),
            ("flavor", manifest.flavor.as_str(), expected_flavor),
            ("target", manifest.target.as_str(), expected_target),
            (
                "Cargo target",
                manifest.cargo_target.as_str(),
                expected_cargo_target,
            ),
        ] {
            if actual != expected {
                return Err(format!("wrong {name}: expected {expected}, got {actual}"));
            }
        }
        if manifest.archive.member != "lib/libwebrtc.a" || manifest.archive.name != "webrtc" {
            return Err("wrong native archive identity".into());
        }
        Ok(manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::ArtifactManifest;

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
    fn rejects_wrong_bridge_target_and_flavor() {
        let fixture = String::from_utf8(FIXTURE.to_vec()).unwrap();
        for (expected, wrong) in [
            ("pulsebeam-webrtc-sys-bridge-v1", "wrong-bridge"),
            ("linux-x86_64", "linux-arm64"),
            (r#""flavor": "core""#, r#""flavor": "native""#),
        ] {
            let invalid = fixture.replacen(expected, wrong, 1);
            assert!(validate(invalid.as_bytes()).is_err());
        }
    }
}
