#[path = "build_support/manifest.rs"]
mod manifest;

use std::{env, fs, path::PathBuf};

use manifest::{ArtifactManifest, LinkKind, SUPPORTED_CARGO_TARGETS, artifact_target};

const BRIDGE_IDENTITY: &str = "pulsebeam-webrtc-sys-bridge-v1";

fn main() {
    println!("cargo::rerun-if-env-changed=PULSEBEAM_WEBRTC_SYS_ARTIFACT_DIR");
    println!("cargo::rerun-if-env-changed=PULSEBEAM_WEBRTC_SYS_SKIP_LINK");

    if env::var_os("PULSEBEAM_WEBRTC_SYS_SKIP_LINK").is_some() {
        return;
    }

    let target = env::var("TARGET").expect("Cargo did not provide TARGET");
    let artifact_target = artifact_target(&target).unwrap_or_else(|| {
        panic!(
            "unsupported Cargo target {target}; supported targets: {}",
            SUPPORTED_CARGO_TARGETS.join(", ")
        )
    });
    let flavor = if env::var_os("CARGO_FEATURE_NATIVE").is_some() {
        "native"
    } else {
        "core"
    };

    let artifact = PathBuf::from(
        env::var_os("PULSEBEAM_WEBRTC_SYS_ARTIFACT_DIR")
            .expect("set PULSEBEAM_WEBRTC_SYS_ARTIFACT_DIR to the matching extracted artifact"),
    );
    let manifest_path = artifact.join("manifest.json");
    println!("cargo::rerun-if-changed={}", manifest_path.display());

    let bytes = fs::read(&manifest_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", manifest_path.display()));
    let manifest = ArtifactManifest::parse_and_validate(
        &bytes,
        BRIDGE_IDENTITY,
        flavor,
        artifact_target,
        &target,
    )
    .unwrap_or_else(|error| panic!("invalid {}: {error}", manifest_path.display()));

    let library = artifact.join(&manifest.archive.member);
    assert!(
        library.is_file(),
        "missing native archive {}",
        library.display()
    );
    let library_dir = library
        .parent()
        .expect("native archive path must have a parent directory");
    println!("cargo::rustc-link-search=native={}", library_dir.display());
    println!("cargo::rustc-link-lib=static={}", manifest.archive.name);
    for link in manifest.links {
        match link.kind {
            LinkKind::Dylib => println!("cargo::rustc-link-lib={}", link.name),
            LinkKind::Framework => println!("cargo::rustc-link-lib=framework={}", link.name),
            LinkKind::WeakFramework => {
                println!("cargo::rustc-link-arg=-weak_framework");
                println!("cargo::rustc-link-arg={}", link.name);
            }
            LinkKind::LinkArg => println!("cargo::rustc-link-arg={}", link.name),
        }
    }
}
