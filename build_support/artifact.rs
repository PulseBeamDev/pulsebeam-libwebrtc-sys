use std::{
    collections::HashSet,
    env,
    fs::{self, File},
    io::{self, Read},
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use flate2::read::GzDecoder;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::manifest::{ArtifactManifest, SUPPORTED_CARGO_TARGETS, artifact_target};

pub(crate) const ARTIFACT_DIR_ENV: &str = "PULSEBEAM_WEBRTC_SYS_ARTIFACT_DIR";
pub(crate) const CACHE_DIR_ENV: &str = "PULSEBEAM_WEBRTC_SYS_CACHE_DIR";
pub(crate) const OFFLINE_ENV: &str = "PULSEBEAM_WEBRTC_SYS_OFFLINE";
const DOWNLOAD_ATTEMPTS: usize = 3;
const RETRYABLE_HTTP_STATUSES: &[u16] = &[408, 429, 500, 502, 503, 504];
static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ArtifactLock {
    schema_version: u32,
    release_ready: bool,
    bridge_identity: String,
    artifacts: Vec<LockedArtifact>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LockedArtifact {
    cargo_target: String,
    artifact_target: String,
    flavor: String,
    asset_name: String,
    url: Option<String>,
    sha256: Option<String>,
}

impl ArtifactLock {
    pub(crate) fn parse_and_validate(bytes: &[u8], expected_bridge: &str) -> Result<Self, String> {
        let lock: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if lock.schema_version != 1 {
            return Err(format!(
                "unsupported artifact lock schema {}",
                lock.schema_version
            ));
        }
        if lock.bridge_identity != expected_bridge {
            return Err(format!(
                "wrong artifact lock bridge identity: expected {expected_bridge}, got {}",
                lock.bridge_identity
            ));
        }

        let mut selections = HashSet::new();
        for entry in &lock.artifacts {
            let expected_target = artifact_target(&entry.cargo_target).ok_or_else(|| {
                format!(
                    "unsupported Cargo target in artifact lock: {}",
                    entry.cargo_target
                )
            })?;
            if entry.artifact_target != expected_target {
                return Err(format!(
                    "wrong artifact target for {}: expected {expected_target}, got {}",
                    entry.cargo_target, entry.artifact_target
                ));
            }
            if !matches!(entry.flavor.as_str(), "core" | "native") {
                return Err(format!("unsupported artifact flavor: {}", entry.flavor));
            }
            let expected_asset =
                format!("webrtc-{}-{}.tar.gz", entry.flavor, entry.artifact_target);
            if entry.asset_name != expected_asset {
                return Err(format!(
                    "wrong asset name: expected {expected_asset}, got {}",
                    entry.asset_name
                ));
            }
            match (&entry.url, &entry.sha256) {
                (Some(url), Some(hash)) => {
                    let secure = url.starts_with("https://");
                    let loopback_test =
                        url.starts_with("http://127.0.0.1:") || url.starts_with("http://[::1]:");
                    if !(secure || loopback_test) || !url.ends_with(&entry.asset_name) {
                        return Err(format!(
                            "artifact URL is not an immutable HTTPS asset URL: {url}"
                        ));
                    }
                    validate_sha256(hash)?;
                }
                (None, None) => {}
                _ => {
                    return Err(format!(
                        "incomplete release coordinates for {expected_asset}"
                    ));
                }
            }
            if !selections.insert((entry.cargo_target.as_str(), entry.flavor.as_str())) {
                return Err(format!(
                    "duplicate artifact lock selection: {} {}",
                    entry.cargo_target, entry.flavor
                ));
            }
        }

        for target in SUPPORTED_CARGO_TARGETS {
            for flavor in ["core", "native"] {
                if !selections.contains(&(target, flavor)) {
                    return Err(format!(
                        "missing artifact lock selection: {target} {flavor}"
                    ));
                }
            }
        }
        if selections.len() != SUPPORTED_CARGO_TARGETS.len() * 2 {
            return Err("artifact lock contains unexpected selections".into());
        }
        if lock.release_ready
            && lock
                .artifacts
                .iter()
                .any(|entry| entry.url.is_none() || entry.sha256.is_none())
        {
            return Err("release-ready artifact lock contains unreleased selections".into());
        }
        Ok(lock)
    }

    pub(crate) fn select(&self, cargo_target: &str, flavor: &str) -> &LockedArtifact {
        self.artifacts
            .iter()
            .find(|entry| entry.cargo_target == cargo_target && entry.flavor == flavor)
            .expect("validated artifact lock contains every target/flavor selection")
    }
}

impl LockedArtifact {
    fn released(&self) -> Result<(&str, &str), String> {
        self.url
            .as_deref()
            .zip(self.sha256.as_deref())
            .ok_or_else(|| {
                format!(
                    "{} is not released for this development revision; set {ARTIFACT_DIR_ENV} to its matching extracted artifact",
                    self.asset_name
                )
            })
    }
}

pub(crate) fn resolve(
    lock_bytes: &[u8],
    expected_bridge: &str,
    cargo_target: &str,
    flavor: &str,
) -> Result<PathBuf, String> {
    let artifact_target = artifact_target(cargo_target).ok_or_else(|| {
        format!(
            "unsupported Cargo target {cargo_target}; supported targets: {}",
            SUPPORTED_CARGO_TARGETS.join(", ")
        )
    })?;

    if let Some(path) = env::var_os(ARTIFACT_DIR_ENV) {
        let path = PathBuf::from(path);
        validate_extracted(
            &path,
            expected_bridge,
            flavor,
            artifact_target,
            cargo_target,
        )?;
        return Ok(path);
    }

    let lock = ArtifactLock::parse_and_validate(lock_bytes, expected_bridge)?;
    let entry = lock.select(cargo_target, flavor);
    let (url, expected_sha256) = entry.released()?;
    let cache = cache_directory()?;
    fs::create_dir_all(&cache).map_err(|error| {
        format!(
            "failed to create artifact cache {}: {error}",
            cache.display()
        )
    })?;
    let archive = cache.join(&entry.asset_name);
    let extracted = cache.join(expected_sha256);

    let cache_state = if is_regular_file(&archive) && sha256_file(&archive)? == expected_sha256 {
        return ensure_extracted(
            &archive,
            &extracted,
            expected_bridge,
            flavor,
            artifact_target,
            cargo_target,
        );
    } else if fs::symlink_metadata(&archive).is_ok() {
        "corrupt"
    } else {
        "missing"
    };

    if fs::symlink_metadata(&archive).is_ok() {
        remove_cache_entry(&archive).map_err(|error| {
            format!(
                "failed to remove invalid cached artifact {}: {error}",
                archive.display()
            )
        })?;
    }

    if offline() {
        return Err(format!(
            "offline artifact cache miss: {} (SHA-256 {expected_sha256}); provide a matching extracted artifact with {ARTIFACT_DIR_ENV} or populate {} while online",
            entry.asset_name,
            archive.display()
        ));
    }

    download(url, &archive, expected_sha256, cache_state)?;
    ensure_extracted(
        &archive,
        &extracted,
        expected_bridge,
        flavor,
        artifact_target,
        cargo_target,
    )
}

fn cache_directory() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os(CACHE_DIR_ENV) {
        return Ok(PathBuf::from(path));
    }
    let cargo_home = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")))
        .or_else(|| env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".cargo")))
        .ok_or_else(|| format!("cannot determine artifact cache; set {CACHE_DIR_ENV}"))?;
    Ok(cargo_home.join("pulsebeam-webrtc-sys/artifacts-v1"))
}

fn offline() -> bool {
    [OFFLINE_ENV, "CARGO_NET_OFFLINE"]
        .iter()
        .any(|name| matches!(env::var(name).as_deref(), Ok("1" | "true")))
}

fn remove_cache_entry(path: &Path) -> io::Result<()> {
    if fs::symlink_metadata(path)?.file_type().is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn temporary_path(destination: &Path) -> PathBuf {
    destination.with_extension(format!(
        "tmp-{}-{}",
        std::process::id(),
        TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ))
}

fn retryable_download_error(error: &ureq::Error) -> bool {
    match error {
        ureq::Error::StatusCode(status) => RETRYABLE_HTTP_STATUSES.contains(status),
        ureq::Error::Timeout(_) | ureq::Error::HostNotFound | ureq::Error::ConnectionFailed => true,
        ureq::Error::Io(error) => retryable_io_error(error),
        _ => false,
    }
}

fn retryable_io_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::TimedOut
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionRefused
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::NotConnected
            | io::ErrorKind::Interrupted
            | io::ErrorKind::UnexpectedEof
    )
}

fn download(
    url: &str,
    destination: &Path,
    expected_sha256: &str,
    cache_state: &str,
) -> Result<(), String> {
    let provider = Arc::new(oxitls_rustcrypto_provider::provider());
    let agent = ureq::Agent::config_builder()
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .provider(ureq::tls::TlsProvider::Rustls)
                .unversioned_rustls_crypto_provider(provider)
                .build(),
        )
        .build()
        .new_agent();
    for attempt in 1..=DOWNLOAD_ATTEMPTS {
        let response = match agent.get(url).call() {
            Ok(response) => response,
            Err(error) if retryable_download_error(&error) && attempt < DOWNLOAD_ATTEMPTS => {
                std::thread::sleep(std::time::Duration::from_millis(200 * attempt as u64));
                continue;
            }
            Err(error) => {
                let failure = if retryable_download_error(&error) {
                    "transient-transport-exhausted"
                } else {
                    "deterministic-retrieval"
                };
                return Err(format!(
                    "cannot retrieve pinned artifact {url}: attempts={attempt}, cache={cache_state}, failure={failure}: {error}"
                ));
            }
        };
        let temporary = temporary_path(destination);
        let result = (|| -> Result<(), String> {
            let mut source = response.into_body().into_reader();
            let mut output = File::create(&temporary)
                .map_err(|error| format!("failed to create {}: {error}", temporary.display()))?;
            if let Err(error) = io::copy(&mut source, &mut output) {
                let prefix = if retryable_io_error(&error) {
                    "retryable transfer interruption"
                } else {
                    "failed to write"
                };
                return Err(format!("{prefix} {}: {error}", temporary.display()));
            }
            output
                .sync_all()
                .map_err(|error| format!("failed to sync {}: {error}", temporary.display()))?;
            let actual = sha256_file(&temporary)?;
            if actual != expected_sha256 {
                return Err(format!(
                    "checksum mismatch for {url}: expected {expected_sha256}, got {actual}"
                ));
            }
            if let Err(error) = fs::rename(&temporary, destination) {
                if !is_regular_file(destination) || sha256_file(destination)? != expected_sha256 {
                    return Err(format!(
                        "failed to install cached artifact {}: {error}",
                        destination.display()
                    ));
                }
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
            let retryable_interruption = result
                .as_ref()
                .err()
                .is_some_and(|error| error.starts_with("retryable transfer interruption"));
            if retryable_interruption && attempt < DOWNLOAD_ATTEMPTS {
                std::thread::sleep(std::time::Duration::from_millis(200 * attempt as u64));
                continue;
            }
            let failure = if retryable_interruption {
                "transient-transport-exhausted"
            } else {
                "integrity-or-interruption"
            };
            return result.map_err(|error| format!(
                "cannot retrieve pinned artifact {url}: attempts={attempt}, cache={cache_state}, failure={failure}: {error}"
            ));
        }
        return result;
    }
    unreachable!("bounded download loop unexpectedly ended")
}

fn ensure_extracted(
    archive: &Path,
    destination: &Path,
    expected_bridge: &str,
    flavor: &str,
    artifact_target: &str,
    cargo_target: &str,
) -> Result<PathBuf, String> {
    if validate_extracted(
        destination,
        expected_bridge,
        flavor,
        artifact_target,
        cargo_target,
    )
    .is_ok()
    {
        return Ok(destination.to_owned());
    }
    if let Ok(metadata) = fs::symlink_metadata(destination) {
        let remove = if metadata.file_type().is_dir() {
            fs::remove_dir_all(destination)
        } else {
            fs::remove_file(destination)
        };
        remove.map_err(|error| {
            format!(
                "failed to remove invalid extracted artifact {}: {error}",
                destination.display()
            )
        })?;
    }

    let temporary = temporary_path(destination);
    if fs::symlink_metadata(&temporary).is_ok() {
        remove_cache_entry(&temporary)
            .map_err(|error| format!("failed to clear {}: {error}", temporary.display()))?;
    }
    fs::create_dir(&temporary)
        .map_err(|error| format!("failed to create {}: {error}", temporary.display()))?;
    let result = (|| {
        extract_safely(archive, &temporary)?;
        validate_extracted(
            &temporary,
            expected_bridge,
            flavor,
            artifact_target,
            cargo_target,
        )?;
        match fs::rename(&temporary, destination) {
            Ok(()) => Ok(()),
            Err(_) if destination.is_dir() => Ok(()),
            Err(error) => Err(format!(
                "failed to install extracted artifact {}: {error}",
                destination.display()
            )),
        }
    })();
    if let Ok(metadata) = fs::symlink_metadata(&temporary) {
        let _ = if metadata.file_type().is_dir() {
            fs::remove_dir_all(&temporary)
        } else {
            fs::remove_file(&temporary)
        };
    }
    result?;
    validate_extracted(
        destination,
        expected_bridge,
        flavor,
        artifact_target,
        cargo_target,
    )?;
    Ok(destination.to_owned())
}

pub(crate) fn extract_safely(archive: &Path, destination: &Path) -> Result<(), String> {
    let file = File::open(archive)
        .map_err(|error| format!("failed to open {}: {error}", archive.display()))?;
    let mut archive = tar::Archive::new(GzDecoder::new(file));
    let mut paths = HashSet::new();
    let entries = archive
        .entries()
        .map_err(|error| format!("failed to read archive: {error}"))?;
    for entry in entries {
        let mut entry = entry.map_err(|error| format!("failed to read archive entry: {error}"))?;
        let path = entry
            .path()
            .map_err(|error| format!("invalid archive path: {error}"))?
            .into_owned();
        validate_relative_path(&path)?;
        if !paths.insert(path.clone()) {
            return Err(format!("duplicate archive path: {}", path.display()));
        }
        let output = destination.join(&path);
        let kind = entry.header().entry_type();
        if kind.is_dir() {
            fs::create_dir_all(&output)
                .map_err(|error| format!("failed to create {}: {error}", output.display()))?;
        } else if kind.is_file() {
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
            }
            let mut file = File::create(&output)
                .map_err(|error| format!("failed to create {}: {error}", output.display()))?;
            io::copy(&mut entry, &mut file)
                .map_err(|error| format!("failed to extract {}: {error}", path.display()))?;
        } else {
            return Err(format!(
                "unsupported archive entry type for {}",
                path.display()
            ));
        }
    }
    Ok(())
}

fn validate_relative_path(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!("unsafe archive path: {}", path.display()));
    }
    Ok(())
}

pub(crate) fn validate_extracted(
    artifact: &Path,
    expected_bridge: &str,
    expected_flavor: &str,
    expected_target: &str,
    expected_cargo_target: &str,
) -> Result<ArtifactManifest, String> {
    if !fs::symlink_metadata(artifact)
        .map(|metadata| metadata.file_type().is_dir())
        .unwrap_or(false)
    {
        return Err(format!(
            "artifact root is not a regular directory: {}",
            artifact.display()
        ));
    }
    let manifest_path = artifact.join("manifest.json");
    if !is_regular_file(&manifest_path) {
        return Err(format!("missing regular file {}", manifest_path.display()));
    }
    let bytes = fs::read(&manifest_path)
        .map_err(|error| format!("failed to read {}: {error}", manifest_path.display()))?;
    let manifest = ArtifactManifest::parse_and_validate(
        &bytes,
        expected_bridge,
        expected_flavor,
        expected_target,
        expected_cargo_target,
    )?;
    let library = artifact.join(&manifest.archive.member);
    if !is_regular_file(&library) {
        return Err(format!(
            "missing regular native archive {}",
            library.display()
        ));
    }
    Ok(manifest)
}

fn is_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_file())
        .unwrap_or(false)
}

fn validate_sha256(value: &str) -> Result<(), String> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(format!("invalid SHA-256: {value}"))
    }
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file =
        File::open(path).map_err(|error| format!("failed to open {}: {error}", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Cursor, Write},
        net::TcpListener,
        sync::Mutex,
        thread,
    };

    const LOCK: &[u8] = include_bytes!("../artifacts.lock.json");
    static ENVIRONMENT_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn lock_has_all_eighteen_exact_selections() {
        let lock =
            ArtifactLock::parse_and_validate(LOCK, "pulsebeam-webrtc-sys-bridge-v2").unwrap();
        assert_eq!(lock.artifacts.len(), 18);
        for target in SUPPORTED_CARGO_TARGETS {
            for flavor in ["core", "native"] {
                let entry = lock.select(target, flavor);
                assert_eq!(entry.artifact_target, artifact_target(target).unwrap());
            }
        }
        assert!(!lock.release_ready);
    }

    #[test]
    fn release_ready_lock_must_pin_every_selection() {
        let lock = String::from_utf8(LOCK.to_vec()).unwrap().replacen(
            r#""release_ready": false"#,
            r#""release_ready": true"#,
            1,
        );
        assert!(
            ArtifactLock::parse_and_validate(lock.as_bytes(), "pulsebeam-webrtc-sys-bridge-v2")
                .unwrap_err()
                .contains("unreleased selections")
        );
    }

    #[test]
    fn rejects_unsupported_target_before_artifact_resolution() {
        let error = resolve(
            LOCK,
            "pulsebeam-webrtc-sys-bridge-v2",
            "x86_64-unknown-linux-musl",
            "core",
        )
        .unwrap_err();
        assert!(error.contains("unsupported Cargo target"));
        assert!(error.contains("x86_64-unknown-linux-gnu"));
    }

    fn write_archive(path: &Path, entries: &[(&str, tar::EntryType)]) {
        let file = File::create(path).unwrap();
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        for (name, kind) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(*kind);
            header.set_size(if kind.is_file() { 1 } else { 0 });
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, name, Cursor::new([0_u8]))
                .unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap();
    }

    #[test]
    fn rejects_links_and_duplicate_paths() {
        let root =
            std::env::temp_dir().join(format!("webrtc-artifact-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let archive = root.join("attack.tar.gz");
        write_archive(&archive, &[("manifest.json", tar::EntryType::Symlink)]);
        assert!(
            extract_safely(&archive, &root.join("out"))
                .unwrap_err()
                .contains("unsupported")
        );

        write_archive(&archive, &[("manifest.json", tar::EntryType::Link)]);
        assert!(
            extract_safely(&archive, &root.join("out-hardlink"))
                .unwrap_err()
                .contains("unsupported")
        );

        write_archive(
            &archive,
            &[
                ("manifest.json", tar::EntryType::Regular),
                ("manifest.json", tar::EntryType::Regular),
            ],
        );
        assert!(
            extract_safely(&archive, &root.join("out2"))
                .unwrap_err()
                .contains("duplicate")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_absolute_and_traversal_paths() {
        for path in [
            Path::new("/absolute"),
            Path::new("../escape"),
            Path::new("a/../../escape"),
        ] {
            assert!(validate_relative_path(path).is_err(), "{}", path.display());
        }
        assert!(validate_relative_path(Path::new("lib/libwebrtc.a")).is_ok());
    }

    fn artifact_test_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "webrtc-artifact-{name}-{}-{}",
            std::process::id(),
            TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        root
    }

    fn server(responses: Vec<Vec<u8>>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 1024];
                let _ = stream.read(&mut request);
                stream.write_all(&response).unwrap();
            }
        });
        format!("http://{address}/artifact")
    }

    fn resolver_server(responses: Vec<Vec<u8>>) -> String {
        server(responses).replace("/artifact", "/webrtc-core-linux-x86_64.tar.gz")
    }

    fn http(status: &str, body: &[u8]) -> Vec<u8> {
        let mut response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        response.extend_from_slice(body);
        response
    }

    fn released_lock(url: &str, sha256: &str) -> Vec<u8> {
        String::from_utf8(LOCK.to_vec())
            .unwrap()
            .replacen(
                r#""url": null,
      "sha256": null"#,
                &format!(
                    r#""url": "{url}",
      "sha256": "{sha256}""#
                ),
                1,
            )
            .into_bytes()
    }

    fn fixture_archive() -> Vec<u8> {
        fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/webrtc-core-linux-x86_64.tar.gz"),
        )
        .unwrap()
    }

    fn resolve_core(lock: &[u8]) -> Result<PathBuf, String> {
        resolve(
            lock,
            "pulsebeam-webrtc-sys-bridge-v2",
            "x86_64-unknown-linux-gnu",
            "core",
        )
    }

    fn configure_test_environment(cache: &Path, offline: bool) {
        unsafe {
            env::set_var(CACHE_DIR_ENV, cache);
            if offline {
                env::set_var(OFFLINE_ENV, "1");
            } else {
                env::remove_var(OFFLINE_ENV);
            }
        }
    }

    #[test]
    fn download_retries_only_transient_statuses_and_publishes_verified_content() {
        let root = artifact_test_root("retry");
        let destination = root.join("artifact.tar.gz");
        let body = b"verified artifact";
        let expected = format!("{:x}", Sha256::digest(body));
        let url = server(vec![
            http("503 Service Unavailable", b"retry"),
            http("200 OK", body),
        ]);
        download(&url, &destination, &expected, "missing").unwrap();
        assert_eq!(fs::read(&destination).unwrap(), body);
        assert!(fs::read_dir(&root).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".tmp-")
        }));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn download_does_not_publish_checksum_mismatch_or_retry_permanent_status() {
        let root = artifact_test_root("integrity");
        let destination = root.join("artifact.tar.gz");
        let expected = "0".repeat(64);
        let url = server(vec![http("200 OK", b"wrong")]);
        assert!(
            download(&url, &destination, &expected, "corrupt")
                .unwrap_err()
                .contains("integrity-or-interruption")
        );
        assert!(!destination.exists());

        let url = server(vec![http("404 Not Found", b"missing")]);
        assert!(
            download(&url, &destination, &expected, "missing")
                .unwrap_err()
                .contains("deterministic-retrieval")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn concurrent_verified_downloads_converge_without_temporary_collisions() {
        let root = artifact_test_root("concurrent");
        let destination = root.join("artifact.tar.gz");
        let body = b"concurrent verified artifact";
        let expected = format!("{:x}", Sha256::digest(body));
        let url = server(vec![http("200 OK", body), http("200 OK", body)]);
        thread::scope(|scope| {
            for _ in 0..2 {
                scope.spawn(|| download(&url, &destination, &expected, "missing").unwrap());
            }
        });
        assert_eq!(fs::read(&destination).unwrap(), body);
        assert!(fs::read_dir(&root).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".tmp-")
        }));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolve_reuses_verified_cache_offline_and_rejects_corrupt_cache() {
        let _environment = ENVIRONMENT_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let artifact_override = env::var_os(ARTIFACT_DIR_ENV);
        unsafe { env::remove_var(ARTIFACT_DIR_ENV) };
        let root = artifact_test_root("resolve-cache");
        let cache = root.join("cache");
        let archive = fixture_archive();
        let digest = format!("{:x}", Sha256::digest(&archive));
        let lock = released_lock(&resolver_server(vec![http("200 OK", &archive)]), &digest);

        configure_test_environment(&cache, false);
        let resolved = resolve_core(&lock).unwrap();
        assert!(resolved.is_dir());
        configure_test_environment(&cache, true);
        assert_eq!(resolve_core(&lock).unwrap(), resolved);

        let corrupt_cache = root.join("corrupt-cache");
        fs::create_dir(&corrupt_cache).unwrap();
        fs::write(
            corrupt_cache.join("webrtc-core-linux-x86_64.tar.gz"),
            b"corrupt",
        )
        .unwrap();
        configure_test_environment(&corrupt_cache, true);
        let error = resolve_core(&lock).unwrap_err();
        assert!(error.contains("offline artifact cache miss"));
        assert!(
            !corrupt_cache
                .join("webrtc-core-linux-x86_64.tar.gz")
                .exists()
        );

        unsafe {
            env::remove_var(CACHE_DIR_ENV);
            env::remove_var(OFFLINE_ENV);
            if let Some(path) = artifact_override {
                env::set_var(ARTIFACT_DIR_ENV, path);
            }
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolve_retries_interrupted_transfers_exhausts_them_and_converges() {
        let _environment = ENVIRONMENT_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let artifact_override = env::var_os(ARTIFACT_DIR_ENV);
        unsafe { env::remove_var(ARTIFACT_DIR_ENV) };
        let root = artifact_test_root("resolve-retry");
        let archive = fixture_archive();
        let digest = format!("{:x}", Sha256::digest(&archive));
        let interrupted = {
            let mut response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                archive.len() + 1
            )
            .into_bytes();
            response.extend_from_slice(&archive);
            response
        };

        let retry_cache = root.join("retry-cache");
        configure_test_environment(&retry_cache, false);
        let retry_lock = released_lock(
            &resolver_server(vec![interrupted.clone(), http("200 OK", &archive)]),
            &digest,
        );
        assert!(resolve_core(&retry_lock).unwrap().is_dir());

        let exhausted_cache = root.join("exhausted-cache");
        configure_test_environment(&exhausted_cache, false);
        let exhausted_lock = released_lock(
            &resolver_server(vec![interrupted.clone(), interrupted.clone(), interrupted]),
            &digest,
        );
        let error = resolve_core(&exhausted_lock).unwrap_err();
        assert!(error.contains("attempts=3"));
        assert!(error.contains("transient-transport-exhausted"));
        assert!(fs::read_dir(&exhausted_cache).unwrap().next().is_none());

        let concurrent_cache = root.join("concurrent-cache");
        configure_test_environment(&concurrent_cache, false);
        let concurrent_lock = released_lock(
            &resolver_server(vec![http("200 OK", &archive), http("200 OK", &archive)]),
            &digest,
        );
        thread::scope(|scope| {
            for _ in 0..2 {
                scope.spawn(|| assert!(resolve_core(&concurrent_lock).unwrap().is_dir()));
            }
        });
        assert!(fs::read_dir(&concurrent_cache).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".tmp-")
        }));

        unsafe {
            env::remove_var(CACHE_DIR_ENV);
            env::remove_var(OFFLINE_ENV);
            if let Some(path) = artifact_override {
                env::set_var(ARTIFACT_DIR_ENV, path);
            }
        }
        fs::remove_dir_all(root).unwrap();
    }
}
