//! macOS and Linux: a running program's files can be replaced under it, so
//! these install in place and then start the new version.
//!
//! - macOS: the `.app` bundle is swapped for the new one by renaming, with
//!   the old one moved aside first and put back if the swap fails.
//! - Linux AppImage: the file `$APPIMAGE` names is replaced by renaming over
//!   it; the running copy keeps its open file until it exits.
//! - Linux `.deb`: `pkexec dpkg -i`, which asks for an administrator's
//!   password through the desktop's own prompt.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) fn installers() -> &'static [&'static str] {
    if cfg!(target_os = "macos") {
        &["app"]
    } else if std::env::var_os("APPIMAGE").is_some() {
        &["appimage"]
    } else {
        &["deb"]
    }
}

/// The `.app` bundle this executable runs from.
#[cfg(target_os = "macos")]
fn app_bundle() -> Result<PathBuf, String> {
    let exe = std::env::current_exe()
        .map_err(|error| format!("Failed to find this app's executable: {error}"))?;
    exe.ancestors()
        .find(|path| path.extension().is_some_and(|extension| extension == "app"))
        .map(Path::to_path_buf)
        .ok_or_else(|| "This app is not running from an .app bundle".to_string())
}

/// Beside the bundle, hidden, and on the same volume, so moving it aside and
/// back are renames that cannot half-happen.
fn aside(target: &Path, tag: &str) -> PathBuf {
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    target.with_file_name(format!(".{name}.g3-{tag}"))
}

pub(super) fn clean_up() {
    #[cfg(target_os = "macos")]
    if let Ok(bundle) = app_bundle() {
        let _ = fs::remove_dir_all(aside(&bundle, "old"));
        let _ = fs::remove_dir_all(aside(&bundle, "new"));
    }
}

/// Move `from` to `to`, by rename when they share a volume and by copying
/// otherwise.
fn move_into_place(from: &Path, to: &Path) -> Result<(), String> {
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    // `ditto` on macOS keeps a bundle's symlinks, permissions and signature;
    // `cp -a` does the same on Linux.
    let copier = if cfg!(target_os = "macos") {
        Command::new("/usr/bin/ditto").arg(from).arg(to).status()
    } else {
        Command::new("cp").arg("-a").arg(from).arg(to).status()
    };
    match copier {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("Copying the update into place failed ({status})")),
        Err(error) => Err(format!("Copying the update into place failed: {error}")),
    }
}

/// Put `payload` where `target` is, keeping `target` until the new one is in.
pub(super) fn replace(payload: &Path, target: &Path) -> Result<(), String> {
    let staged = aside(target, "new");
    let old = aside(target, "old");
    let _ = fs::remove_dir_all(&staged);
    let _ = fs::remove_file(&staged);
    move_into_place(payload, &staged)?;
    let _ = fs::remove_dir_all(&old);
    let _ = fs::remove_file(&old);
    fs::rename(target, &old).map_err(|error| {
        format!(
            "Cannot replace {}: {error}. The app needs write access to its own folder to update itself",
            target.display()
        )
    })?;
    if let Err(error) = fs::rename(&staged, target) {
        let _ = fs::rename(&old, target);
        return Err(format!("Failed to move the update into place: {error}"));
    }
    if old.is_dir() {
        let _ = fs::remove_dir_all(&old);
    } else {
        let _ = fs::remove_file(&old);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub(super) fn install(payload: &Path, _hint: Option<&str>) -> Result<(), String> {
    let is_bundle = payload.is_dir()
        && payload
            .extension()
            .is_some_and(|extension| extension == "app");
    if !is_bundle {
        return Err("The macOS update is not an .app bundle".to_string());
    }
    let bundle = app_bundle()?;
    replace(payload, &bundle)?;
    // -n: a new instance even though this one is still running.
    Command::new("/usr/bin/open")
        .arg("-n")
        .arg(&bundle)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Failed to start the new version: {error}"))
}

#[cfg(target_os = "linux")]
pub(super) fn install(payload: &Path, _hint: Option<&str>) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let mut magic = [0u8; 8];
    let read = fs::File::open(payload)
        .and_then(|mut file| std::io::Read::read(&mut file, &mut magic))
        .map_err(|error| format!("Failed to read the update: {error}"))?;
    let magic = &magic[..read];
    if magic.starts_with(b"!<arch>\n") {
        return install_deb(payload);
    }
    if !magic.starts_with(b"\x7fELF") {
        return Err("The Linux update is neither an AppImage nor a .deb".to_string());
    }
    let target = std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .ok_or("This app is not running as an AppImage, so there is no file to replace")?;
    fs::set_permissions(payload, fs::Permissions::from_mode(0o755))
        .map_err(|error| format!("Failed to make the update executable: {error}"))?;
    replace(payload, &target)?;
    Command::new(&target)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Failed to start the new version: {error}"))
}

#[cfg(target_os = "linux")]
fn install_deb(payload: &Path) -> Result<(), String> {
    let status = Command::new("pkexec")
        .arg("dpkg")
        .arg("-i")
        .arg(payload)
        .status()
        .map_err(|error| format!("Failed to run pkexec to install the package: {error}"))?;
    if !status.success() {
        return Err(format!(
            "Installing the package failed or was refused ({status})"
        ));
    }
    // The old binary was unlinked under this process, which then reads as
    // "/usr/bin/app (deleted)"; the new one is at the plain path.
    let exe = std::env::current_exe()
        .map_err(|error| format!("Failed to find this app's executable: {error}"))?;
    let exe = PathBuf::from(
        exe.to_string_lossy()
            .trim_end_matches(" (deleted)")
            .to_string(),
    );
    Command::new(exe)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Failed to start the new version: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacing_swaps_in_the_new_copy_and_drops_the_old() {
        let root = std::env::temp_dir().join(format!("g3-replace-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("Tawny.app/Contents")).unwrap();
        fs::write(root.join("Tawny.app/Contents/version"), "1.0").unwrap();
        fs::create_dir_all(root.join("unpacked/Tawny.app/Contents")).unwrap();
        fs::write(root.join("unpacked/Tawny.app/Contents/version"), "1.1").unwrap();
        replace(&root.join("unpacked/Tawny.app"), &root.join("Tawny.app")).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("Tawny.app/Contents/version")).unwrap(),
            "1.1"
        );
        assert!(!root.join(".Tawny.app.g3-old").exists());
        assert!(!root.join(".Tawny.app.g3-new").exists());
        // A target that cannot be moved aside leaves everything as it was.
        fs::create_dir_all(root.join("unpacked/Tawny.app")).unwrap();
        assert!(replace(&root.join("unpacked/Tawny.app"), &root.join("Missing.app")).is_err());
        let _ = fs::remove_dir_all(root);
    }
}
