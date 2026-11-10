//! Windows, macOS and Linux: an HTTP client, a download folder, unpacking the
//! archives `dx bundle` writes for updates, and handing the result to the
//! OS-specific installer below.

use super::engine::{DownloadRequest, Transport};
use super::release::{self, Candidate, PlatformKey};
use super::{Update, UpdaterState};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(windows)]
#[path = "desktop_windows.rs"]
mod os;

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "desktop_unix.rs"]
mod os;

/// What [`Native::install`] got as far as. An install exits the process on
/// success, so only opening a store page comes back; the type is shared with
/// the mobile platforms, where an install continues after the call.
#[allow(
    dead_code,
    reason = "Desktop never asks for a permission; the variant is part of the shared contract."
)]
pub(super) enum Step {
    Started,
    NeedsPermission,
    Opened,
}

pub(super) struct Native {
    agent: ureq::Agent,
}

impl Native {
    pub(super) fn new() -> Result<Self, String> {
        let tls = ureq::tls::TlsConfig::builder()
            .provider(ureq::tls::TlsProvider::Rustls)
            .root_certs(ureq::tls::RootCerts::PlatformVerifier)
            .build();
        let agent = ureq::Agent::config_builder()
            .tls_config(tls)
            .http_status_as_error(false)
            .https_only(false)
            .build()
            .into();
        Ok(Self { agent })
    }

    /// Where Tauri looks a build up: its names for the OS and architecture,
    /// and the installer formats this install can take.
    pub(super) fn key() -> PlatformKey<'static> {
        let target = if cfg!(windows) {
            "windows"
        } else if cfg!(target_os = "macos") {
            "darwin"
        } else {
            "linux"
        };
        let arch = match std::env::consts::ARCH {
            "x86" => "i686",
            "arm" => "armv7",
            other => other,
        };
        PlatformKey {
            target,
            arch,
            installers: os::installers(),
        }
    }

    /// Per user and per app, in the user's cache rather than a shared
    /// temporary folder another account could plant files in.
    pub(super) fn data_dir(&self) -> Result<PathBuf, String> {
        data_dir()
    }

    /// A store page (the Microsoft Store, the Mac App Store, Flathub) has
    /// nothing to download: it is opened.
    pub(super) fn download_url(candidate: &Candidate) -> Result<Option<String>, String> {
        let store = candidate.installer.as_deref() == Some(release::STORE)
            || release::is_store_link(&candidate.url);
        Ok((!store).then(|| candidate.url.clone()))
    }

    /// Unpack the verified download and install it, which exits the process
    /// on success; or open a store page.
    pub(super) fn install(
        &self,
        file: Option<&Path>,
        candidate: &Candidate,
    ) -> Result<Step, String> {
        if Self::download_url(candidate)?.is_none() {
            os::open_url(&candidate.url)?;
            return Ok(Step::Opened);
        }
        let file = file.ok_or("There is no downloaded update to install")?;
        let unpacked = data_dir()?.join("unpacked");
        let payload = unpack(file, &unpacked)?;
        os::install(&payload, candidate.installer.as_deref())?;
        // The new version is starting, or the installer is about to replace
        // this one and start it; either way this process has to go.
        std::process::exit(0)
    }

    /// Desktop installs do not report back: the process is gone.
    pub(super) fn install_progress(_update: &Update) -> Option<UpdaterState> {
        None
    }

    /// Leftovers of an earlier update: what was downloaded and unpacked, and
    /// on macOS the replaced app bundle.
    pub(super) fn clean_up() {
        if let Ok(directory) = data_dir() {
            let _ = fs::remove_dir_all(directory.join("download"));
            let _ = fs::remove_dir_all(directory.join("unpacked"));
        }
        os::clean_up();
    }
}

impl Transport for Native {
    fn download(
        &self,
        request: &DownloadRequest<'_>,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<u16, String> {
        let timeout = Some(Duration::from_millis(u64::from(request.timeout_ms)));
        let mut call = self
            .agent
            .get(request.url)
            .config()
            .timeout_connect(timeout)
            .timeout_recv_response(timeout)
            .build();
        for (name, value) in request.headers {
            call = call.header(name.as_str(), value.as_str());
        }
        let response = call.call().map_err(|error| error.to_string())?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) || status == 204 {
            return Ok(status);
        }
        let total = response
            .headers()
            .get("content-length")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse().ok());
        let mut body = response
            .into_body()
            .into_with_config()
            .limit(u64::MAX)
            .reader();
        // Written beside and renamed into place, so a half-downloaded file
        // never sits at the destination looking complete.
        let partial = request.destination.with_extension("part");
        let result = (|| {
            let mut output = fs::File::create(&partial)
                .map_err(|error| format!("Failed to create the download: {error}"))?;
            let mut buffer = vec![0u8; 256 * 1024];
            let mut done = 0u64;
            loop {
                let read = body
                    .read(&mut buffer)
                    .map_err(|error| format!("Download failed: {error}"))?;
                if read == 0 {
                    break;
                }
                output
                    .write_all(&buffer[..read])
                    .map_err(|error| format!("Failed to write the download: {error}"))?;
                done += read as u64;
                progress(done, total);
            }
            output
                .sync_all()
                .map_err(|error| format!("Failed to write the download: {error}"))?;
            drop(output);
            fs::rename(&partial, request.destination)
                .map_err(|error| format!("Failed to move the download into place: {error}"))
        })();
        if result.is_err() {
            let _ = fs::remove_file(&partial);
        }
        result.map(|()| status)
    }
}

fn data_dir() -> Result<PathBuf, String> {
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Caches"))
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
    }
    .ok_or("Found no cache folder for the updater")?;
    let app = std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "app".to_string());
    Ok(base.join("g3-native-plugins").join("updater").join(app))
}

/// The installable thing inside a download: the file itself, or the one
/// entry at the top of a `.zip` or `.tar.gz` (an installer, an AppImage, an
/// `.app` bundle). Told apart by content, not by name.
pub(super) fn unpack(file: &Path, directory: &Path) -> Result<PathBuf, String> {
    let mut magic = [0u8; 4];
    let read = fs::File::open(file)
        .and_then(|mut opened| opened.read(&mut magic))
        .map_err(|error| format!("Failed to read the download: {error}"))?;
    let magic = &magic[..read];
    let archive = magic.starts_with(b"PK\x03\x04") || magic.starts_with(&[0x1f, 0x8b]);
    if !archive {
        return Ok(file.to_path_buf());
    }
    let _ = fs::remove_dir_all(directory);
    fs::create_dir_all(directory)
        .map_err(|error| format!("Failed to create a folder to unpack into: {error}"))?;
    let opened =
        fs::File::open(file).map_err(|error| format!("Failed to read the download: {error}"))?;
    if magic.starts_with(b"PK") {
        let mut zip = zip::ZipArchive::new(opened)
            .map_err(|error| format!("The update is not a valid zip: {error}"))?;
        // `extract` refuses entries that would land outside the folder.
        zip.extract(directory)
            .map_err(|error| format!("Failed to unpack the update: {error}"))?;
    } else {
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(opened));
        // An app bundle and an AppImage need their executable bits.
        tar.set_preserve_permissions(true);
        // `unpack` refuses entries that would land outside the folder.
        tar.unpack(directory)
            .map_err(|error| format!("Failed to unpack the update: {error}"))?;
    }
    let mut entries: Vec<PathBuf> = fs::read_dir(directory)
        .map_err(|error| format!("Failed to read the unpacked update: {error}"))?
        .flatten()
        .map(|entry| entry.path())
        .collect();
    match entries.len() {
        1 => Ok(entries.remove(0)),
        0 => Err("The update archive is empty".to_string()),
        _ => Err("The update archive holds more than one thing to install".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("g3-unpack-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn a_bare_file_is_its_own_payload() {
        let root = temp("bare");
        let file = root.join("Tawny-setup.exe");
        fs::write(&file, b"MZ\x90\x00").unwrap();
        assert_eq!(unpack(&file, &root.join("out")).unwrap(), file);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn zips_and_tarballs_unpack_to_their_one_entry() {
        let root = temp("archives");
        // What dx writes for Windows: the installer, zipped on its own.
        let zipped = root.join("Tawny_1.1.0_x64-setup.zip");
        let mut writer = zip::ZipWriter::new(fs::File::create(&zipped).unwrap());
        writer
            .start_file(
                "Tawny_1.1.0_x64-setup.exe",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(b"MZ installer").unwrap();
        writer.finish().unwrap();
        let payload = unpack(&zipped, &root.join("zip")).unwrap();
        assert_eq!(payload.file_name().unwrap(), "Tawny_1.1.0_x64-setup.exe");
        assert_eq!(fs::read(&payload).unwrap(), b"MZ installer");
        // What dx writes for macOS: the .app directory, tarred and gzipped.
        let tarred = root.join("Tawny.app.tar.gz");
        let encoder = flate2::write::GzEncoder::new(
            fs::File::create(&tarred).unwrap(),
            flate2::Compression::fast(),
        );
        let mut builder = tar::Builder::new(encoder);
        let body = b"#!/bin/sh\n";
        let mut header = tar::Header::new_gnu();
        header.set_size(body.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, "Tawny.app/Contents/MacOS/tawny", &body[..])
            .unwrap();
        builder.into_inner().unwrap().finish().unwrap();
        let payload = unpack(&tarred, &root.join("tar")).unwrap();
        assert_eq!(payload.file_name().unwrap(), "Tawny.app");
        assert!(payload.join("Contents/MacOS/tawny").is_file());
        let _ = fs::remove_dir_all(root);
    }
}
