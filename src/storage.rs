//! Files the app must not lose: `helper.toml` (the accounts), the guild
//! diary, the dungeon probe history, the daily spend ledger.
//!
//! They used to be written with one `std::fs::write` straight over the old
//! file. A crash, a full disk or an antivirus holding the file half way
//! through left a truncated file behind, and the next start read it as
//! "nothing here" - for the config that looked like a fresh install with
//! every account gone. So every write goes to a temporary file next to the
//! target first, is flushed to disk, and only then replaces the target in
//! one rename; the file that was there before is kept as `<name>.bak`.

use std::{
    io,
    path::{Path, PathBuf},
    sync::Mutex,
};

/// The last write that failed, for the UI to show. Writes happen from
/// dozens of places that all wrote `_ = config.write()`; rather than
/// teaching every one of them to report, the failure is remembered here
static LAST_ERROR: Mutex<Option<(PathBuf, String)>> = Mutex::new(None);

/// What the UI shows while a save keeps failing, `None` once the file
/// that failed has been written successfully
pub fn last_error() -> Option<String> {
    LAST_ERROR
        .lock()
        .ok()
        .and_then(|e| e.as_ref().map(|(_, msg)| msg.clone()))
}

fn note_result(path: &Path, result: &io::Result<()>) {
    let Ok(mut slot) = LAST_ERROR.lock() else {
        return;
    };
    match result {
        Err(e) => {
            log::error!("could not save {}: {e}", path.display());
            *slot = Some((
                path.to_path_buf(),
                format!("Could not save {}: {e}", path.display()),
            ));
        }
        // Only the file that failed clears its own notice: a config that
        // cannot be written must not be hidden by a diary that can
        Ok(()) => {
            if slot.as_ref().is_some_and(|(p, _)| p == path) {
                *slot = None;
            }
        }
    }
}

/// `<path>.bak`, `<path>.tmp` - the extension is appended, not replaced,
/// so `helper.toml` becomes `helper.toml.bak`
pub fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

/// Writes `bytes` to `path` so that at no moment the file is half written:
/// temporary file, flush, then one rename over the target. The previous
/// content survives as `<path>.bak`
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let result = write_atomic_inner(path, bytes);
    note_result(path, &result);
    result
}

fn write_atomic_inner(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let tmp = sibling(path, ".tmp");
    // Any leftover from a crash mid-write is ours to replace
    let mut file = std::fs::File::create(&tmp)?;
    let written = (|| {
        file.write_all(bytes)?;
        file.sync_all()
    })();
    drop(file);
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    // Keep what was there. A copy, not a rename: should the process die
    // between the two steps, the target is still in place
    if path.exists() {
        std::fs::copy(path, sibling(path, ".bak"))?;
    }
    // `rename` replaces an existing target on every platform std supports
    // (MoveFileEx with REPLACE_EXISTING on Windows)
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

/// Where a file was read from, so the caller can say so
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Main,
    Backup,
}

/// Reads `path`, and when that is missing or does not parse, the `.bak`
/// next to it. `parse` decides what "does not parse" means. Returns the
/// value and where it came from; the error carries both failures
pub fn read_with_backup<T>(
    path: &Path,
    parse: impl Fn(&str) -> Result<T, String>,
) -> Result<(T, Source), String> {
    let main = std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|s| parse(&s));
    let main_err = match main {
        Ok(v) => return Ok((v, Source::Main)),
        Err(e) => e,
    };
    let bak = sibling(path, ".bak");
    let backup = std::fs::read_to_string(&bak)
        .map_err(|e| e.to_string())
        .and_then(|s| parse(&s));
    match backup {
        Ok(v) => Ok((v, Source::Backup)),
        Err(bak_err) => Err(format!(
            "{}: {main_err}; {}: {bak_err}",
            path.display(),
            bak.display()
        )),
    }
}

/// Moves a file the app could not read out of the way as
/// `<name>.broken-<timestamp>`, so a later save does not bury the one copy
/// the user might still want to look at
pub fn set_aside(path: &Path) -> io::Result<PathBuf> {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let aside = sibling(path, &format!(".broken-{stamp}"));
    std::fs::rename(path, &aside)?;
    Ok(aside)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sf-commander-storage-{}-{}",
            std::process::id(),
            fastrand::u64(..)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn second_write_keeps_the_first_as_backup() {
        let path = scratch("helper.toml");
        write_atomic(&path, b"one").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "one");
        assert!(!sibling(&path, ".bak").exists());
        write_atomic(&path, b"two").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "two");
        assert_eq!(
            std::fs::read_to_string(sibling(&path, ".bak")).unwrap(),
            "one"
        );
        assert!(!sibling(&path, ".tmp").exists());
    }

    #[test]
    fn a_failed_write_leaves_the_old_file_and_is_reported() {
        let path = scratch("helper.toml");
        write_atomic(&path, b"good").unwrap();
        // A directory sitting where the temp file should go makes the
        // write fail before anything touches the target
        std::fs::create_dir_all(sibling(&path, ".tmp")).unwrap();
        assert!(write_atomic(&path, b"never").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "good");
        let mine = path.display().to_string();
        assert!(last_error().is_some_and(|e| e.contains(&mine)));
        // The next good write clears the notice (other tests share the
        // slot, so only this file's own message is checked)
        std::fs::remove_dir(sibling(&path, ".tmp")).unwrap();
        write_atomic(&path, b"again").unwrap();
        assert!(!last_error().is_some_and(|e| e.contains(&mine)));
    }

    #[test]
    fn a_corrupt_main_file_falls_back_to_the_backup() {
        let path = scratch("helper.toml");
        let parse = |s: &str| {
            if s.starts_with("ok") {
                Ok(s.to_string())
            } else {
                Err("bad".to_string())
            }
        };
        assert!(read_with_backup(&path, parse).is_err());
        write_atomic(&path, b"ok 1").unwrap();
        write_atomic(&path, b"ok 2").unwrap();
        assert_eq!(
            read_with_backup(&path, parse).unwrap(),
            ("ok 2".into(), Source::Main)
        );
        // One byte off in the main file: the backup steps in, and the
        // main file is not touched by reading
        std::fs::write(&path, "xk 2").unwrap();
        assert_eq!(
            read_with_backup(&path, parse).unwrap(),
            ("ok 1".into(), Source::Backup)
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "xk 2");
        let aside = set_aside(&path).unwrap();
        assert!(!path.exists());
        assert!(aside.to_string_lossy().contains(".broken-"));
    }
}
