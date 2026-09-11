#[path = "build_support/artifact.rs"]
mod artifact;
#[path = "build_support/manifest.rs"]
mod manifest;

use std::{env, path::Path};

const BRIDGE_IDENTITY: &str = "pulsebeam-webrtc-sys-bridge-v2";
const ARTIFACT_LOCK: &[u8] = include_bytes!("artifacts.lock.json");

fn main() {
    for name in [
        artifact::ARTIFACT_DIR_ENV,
        artifact::CACHE_DIR_ENV,
        artifact::OFFLINE_ENV,
        "CARGO_NET_OFFLINE",
        "PULSEBEAM_WEBRTC_SYS_SKIP_LINK",
    ] {
        println!("cargo::rerun-if-env-changed={name}");
    }
    println!("cargo::rerun-if-changed=artifacts.lock.json");

    let target = env::var("TARGET").expect("Cargo did not provide TARGET");
    let artifact_target = manifest::artifact_target(&target).unwrap_or_else(|| {
        panic!(
            "unsupported Cargo target {target}; supported targets: {}",
            manifest::SUPPORTED_CARGO_TARGETS.join(", ")
        )
    });
    let flavor = if env::var_os("CARGO_FEATURE_NATIVE").is_some() {
        "native"
    } else {
        "core"
    };

    if env::var_os("PULSEBEAM_WEBRTC_SYS_SKIP_LINK").is_some() {
        return;
    }

    let artifact = artifact::resolve(ARTIFACT_LOCK, BRIDGE_IDENTITY, &target, flavor)
        .unwrap_or_else(|error| panic!("failed to resolve native artifact: {error}"));
    let manifest =
        artifact::validate_extracted(&artifact, BRIDGE_IDENTITY, flavor, artifact_target, &target)
            .unwrap_or_else(|error| panic!("invalid native artifact: {error}"));
    let library = artifact.join(&manifest.archive.member);
    let library_dir = library
        .parent()
        .expect("native archive path must have a parent directory");
    println!(
        "cargo::rerun-if-changed={}",
        artifact.join("manifest.json").display()
    );
    println!("cargo::rerun-if-changed={}", library.display());
    for directive in manifest.cargo_link_directives(path_string(library_dir)) {
        println!("{directive}");
    }
}

fn path_string(path: &Path) -> &str {
    path.to_str()
        .expect("native artifact path must be valid Unicode for Cargo link directives")
}
