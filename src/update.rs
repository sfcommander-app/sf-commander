//! Keeping the app up to date on its own.
//!
//! The server side is two files in one folder:
//!
//! ```text
//! <BASE_URL>/latest.json      { "version": "1.23.0", "exe": "SF-Commander-1.23.exe", "sha256": "..." }
//! <BASE_URL>/SF-Commander-1.23.exe
//! ```
//!
//! The client asks for `latest.json`, compares the version with its own, and
//! if there is a newer one it downloads the exe next to the running one,
//! starts it and quits.
//!
//! ## What the checksum is and is not
//!
//! The hash sits in the same file as the link to the exe, so anyone who can
//! replace one can replace the other. It is there to catch a download that
//! was cut short or corrupted - **not** to protect against someone who got
//! into the web space. That part is HTTPS and the upload password, nothing
//! else. Whatever lies at `BASE_URL` runs on every user's machine, and those
//! machines have their game logins in `helper.toml`.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Where the updates live. Must be HTTPS and must be a place only the author
/// can upload to - see the note at the top of this file
pub const BASE_URL: &str = "https://sfc-updates.vercel.app/";

/// A fresh anonymous id: sixteen hex characters out of the system clock and
/// the process's own randomness. Not a fingerprint of the machine - two
/// installs on the same computer get different ones, and deleting the line
/// from the config gets you a new one
#[must_use]
pub fn new_install_id() -> String {
    let mut out = String::with_capacity(16);
    for _ in 0..16 {
        out.push(char::from_digit(fastrand::u32(0..16), 16).unwrap_or('0'));
    }
    out
}

/// Says hello with nothing but that id, so the author can count installs.
/// Fire and forget: it is not allowed to matter, and the update check does
/// not wait for it
pub async fn ping(install_id: String) {
    if install_id.is_empty() {
        return;
    }
    let Ok(client) = reqwest::ClientBuilder::new()
        .user_agent(concat!("sf-commander/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(10))
        .build()
    else {
        return;
    };
    let url = join(BASE_URL, &format!("api/ping?id={install_id}"));
    if let Err(e) = client.get(url).send().await {
        log::debug!("ping did not go through: {e}");
    }
}

/// How many different installs have said hello. `None` for a number the
/// server did not give us
pub async fn user_count() -> Result<(Option<u64>, Option<u64>), String> {
    let client = reqwest::ClientBuilder::new()
        .user_agent(concat!("sf-commander/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    #[derive(Deserialize)]
    struct Stats {
        today: Option<u64>,
        week: Option<u64>,
    }
    // The `json` feature is not on for reqwest here - the text is small and
    // parsing it with serde_json costs nothing
    let body = client
        .get(join(BASE_URL, "api/stats"))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let stats: Stats =
        serde_json::from_str(&body).map_err(|e| e.to_string())?;
    Ok((stats.today, stats.week))
}

/// How long to wait between two checks. `latest.json` is a few dozen bytes
/// and is served with a one minute cache, so asking often costs nothing and
/// is the difference between the guild being on the new version in minutes
/// or the next morning
pub const CHECK_EVERY: Duration = Duration::from_secs(10 * 60);

/// What `latest.json` has to contain
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    /// Semver, e.g. "1.23.0"
    pub version: String,
    /// File name of the exe, next to `latest.json`
    pub exe: String,
    /// Lowercase hex SHA-256 of that exe
    pub sha256: String,
    /// One line for the banner. Optional
    #[serde(default)]
    pub notes: String,
}

/// A newer version was found and this is what it says
#[derive(Debug, Clone)]
pub struct Available {
    pub manifest: Manifest,
    pub version: semver::Version,
}

/// The version this build was compiled as
#[must_use]
pub fn own_version() -> semver::Version {
    semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .unwrap_or_else(|_| semver::Version::new(0, 0, 0))
}

fn join(base: &str, file: &str) -> String {
    if base.ends_with('/') {
        format!("{base}{file}")
    } else {
        format!("{base}/{file}")
    }
}

// Windows filenames and URLs have different special characters. Accept only
// the release naming convention, not paths, alternate streams or URL queries.
fn valid_exe_name(name: &str) -> bool {
    name.len() <= 128
        && name.starts_with("SF-Commander")
        && name.ends_with(".exe")
        && !name.contains("..")
        && name.bytes().all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
}

fn validate_manifest(manifest: &Manifest) -> Result<(), &'static str> {
    if !valid_exe_name(&manifest.exe) {
        return Err("the manifest names a bad exe file");
    }
    let hash = manifest.sha256.trim();
    if hash.len() != 64 || !hash.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("the manifest has no valid SHA-256 checksum");
    }
    Ok(())
}

/// Asks the server what the newest version is. `Ok(None)` means we are
/// already up to date - that is the normal, boring answer
pub async fn check(
) -> Result<Option<Available>, Box<dyn std::error::Error + Send + Sync>> {
    if !cfg!(target_os = "windows") { return Ok(None); }
    if !BASE_URL.starts_with("https://") {
        return Err("the update URL has to be https".into());
    }
    let client = reqwest::ClientBuilder::new()
        .user_agent(concat!("sf-commander/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .build()?;
    let text = client
        .get(join(BASE_URL, "latest.json"))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let manifest: Manifest = serde_json::from_str(&text)?;
    let version = semver::Version::parse(manifest.version.trim())?;
    if version <= own_version() {
        return Ok(None);
    }
    validate_manifest(&manifest)?;
    Ok(Some(Available { manifest, version }))
}

/// Downloads the new exe next to the running one and returns its path. The
/// download goes to a `.part` file first, so a half finished download can
/// never be mistaken for a usable program
pub async fn download(
    available: &Available,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    validate_manifest(&available.manifest)?;
    let current = std::env::current_exe()?;
    let dir = current
        .parent()
        .ok_or("the running exe has no folder")?
        .to_path_buf();
    let target = dir.join(&available.manifest.exe);
    if target == current {
        return Err("the new version has the same file name".into());
    }

    let client = reqwest::ClientBuilder::new()
        .user_agent(concat!("sf-commander/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(600))
        .build()?;
    let bytes = client
        .get(join(BASE_URL, &available.manifest.exe))
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let got = hex(&hasher.finalize());
    let want = available.manifest.sha256.trim().to_lowercase();
    if got != want {
        return Err(format!(
            "the download does not match the checksum (got {got}, expected \
             {want}) - nothing was installed"
        )
        .into());
    }

    let part = dir.join(format!("{}.{}.part", available.manifest.exe, fastrand::u64(..)));
    tokio::fs::write(&part, &bytes).await?;
    if let Err(error) = tokio::fs::rename(&part, &target).await {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(error.into());
    }
    Ok(target)
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// Starts the downloaded version and tells it to clean up after this one.
/// The caller has to quit right after - Windows will not let the old file be
/// deleted while it is still running, which is why the new process does it.
/// With `handover` the new version also gets the file the live sessions
/// were written to (see `handover.rs`), so nobody has to log in again
pub fn restart_into(
    new_exe: &Path,
    handover: Option<&Path>,
) -> std::io::Result<()> {
    let current = std::env::current_exe()?;
    let mut cmd = std::process::Command::new(new_exe);
    cmd.arg("--replaced").arg(&current);
    if let Some(file) = handover {
        cmd.arg("--handover").arg(file);
    }
    cmd.spawn()?;
    Ok(())
}

/// Called on start with the path the previous version ran from. It may still
/// be running for a moment, so this retries for a few seconds before giving
/// up - a leftover file is untidy, never broken
pub async fn clean_up_old(old: PathBuf) {
    let Ok(current) = std::env::current_exe().and_then(|p| p.canonicalize()) else { return };
    let Ok(meta) = std::fs::symlink_metadata(&old) else { return };
    if !meta.file_type().is_file() { return; }
    let Ok(old) = old.canonicalize() else { return };
    if !safe_cleanup_target(&current, &old) {
        log::warn!("refusing to clean up a path outside the release naming convention");
        return;
    }
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if !old.exists() || tokio::fs::remove_file(&old).await.is_ok() {
            return;
        }
    }
    log::warn!("could not remove the old version at {}", old.display());
}

fn safe_cleanup_target(current: &Path, old: &Path) -> bool {
    old != current && old.parent() == current.parent()
        && old.file_name().and_then(|n| n.to_str()).is_some_and(valid_exe_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_url_is_built_with_exactly_one_slash() {
        assert_eq!(
            join("https://a.cz/sfc", "latest.json"),
            "https://a.cz/sfc/latest.json"
        );
        assert_eq!(
            join("https://a.cz/sfc/", "latest.json"),
            "https://a.cz/sfc/latest.json"
        );
    }

    #[test]
    fn manifest_rejects_paths_streams_urls_and_missing_checksums() {
        let mut manifest = Manifest { version: "999.0.0".into(), exe: "SF-Commander-999.0.exe".into(), sha256: "a".repeat(64), notes: String::new() };
        assert!(validate_manifest(&manifest).is_ok());
        for bad in ["../evil.exe", "sub/evil.exe", r"..\evil.exe", "SF-Commander:evil.exe", "SF-Commander?x.exe", "CON.exe", "SF-Commander.exe ", "SF-Commander..exe"] {
            manifest.exe = bad.into();
            assert!(validate_manifest(&manifest).is_err(), "{bad}");
        }
        manifest.exe = "SF-Commander-2.exe".into();
        manifest.sha256 = "ab".into();
        assert!(validate_manifest(&manifest).is_err());
    }

    #[test]
    fn cleanup_only_removes_another_release_in_the_same_folder() {
        let current = Path::new("/app/SF-Commander-2.exe");
        assert!(safe_cleanup_target(current, Path::new("/app/SF-Commander-1.exe")));
        for old in [current, Path::new("/other/SF-Commander-1.exe"), Path::new("/app/other.exe"), Path::new("/app/helper.toml")] {
            assert!(!safe_cleanup_target(current, old));
        }
    }

    /// The id is the whole privacy story: if it ever stopped being a plain
    /// random number, or started being the same on two installs, the thing
    /// being counted would quietly turn into something else
    #[test]
    fn the_install_id_is_random_and_says_nothing_about_the_machine() {
        let a = new_install_id();
        let b = new_install_id();
        assert_eq!(a.len(), 16);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b, "two installs must not get the same id");
        // The server only accepts this shape, so a change here without a
        // change there would silently stop the counting
        assert!(a.len() >= 16 && a.len() <= 64);
    }

    #[test]
    fn hex_is_lowercase_and_padded() {
        assert_eq!(hex(&[0x00, 0x0f, 0xff]), "000fff");
    }
}
