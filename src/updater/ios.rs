//! iOS: no app can install an IPA itself, but iOS installs one for it from an
//! `itms-services://` link to a `manifest.plist`, which is how ad hoc and
//! enterprise builds are distributed over the air. The release names that
//! manifest; it is downloaded and checked against the signature first, so a
//! tampered release cannot point the device at someone else's build, and
//! iOS then fetches the IPA it lists and checks the provisioning itself.
//!
//! A release may instead name an App Store or TestFlight link, for a build
//! distributed that way. There is nothing to download or sign then; the link
//! is opened and the store takes over.

use super::engine::{DownloadRequest, Transport};
use super::ios_link::{Link, install_link, percent_encode};
use super::release::{Candidate, PlatformKey};
use super::{Update, UpdaterState};
use std::path::{Path, PathBuf};

#[manganis::ffi("src/ios")]
unsafe extern "Swift" {
    pub type UpdaterPlugin;

    pub fn dataDirectoryFromRust(this: &UpdaterPlugin) -> Option<String>;

    pub fn downloadFromRust(this: &UpdaterPlugin, requestJson: String) -> Option<String>;

    pub fn openUrlFromRust(this: &UpdaterPlugin, url: String) -> Option<String>;
}

/// What [`Native::install`] got as far as.
#[allow(
    dead_code,
    reason = "iOS never asks for a permission; the variant is part of the shared contract."
)]
pub(super) enum Step {
    Started,
    NeedsPermission,
    Opened,
}

pub(super) struct Native {
    plugin: UpdaterPlugin,
}

impl Native {
    pub(super) fn new() -> Result<Self, String> {
        let plugin = UpdaterPlugin::new()
            .map_err(|error| format!("Failed to create UpdaterPlugin: {error:?}"))?;
        Ok(Self { plugin })
    }

    pub(super) fn key() -> PlatformKey<'static> {
        PlatformKey {
            target: "ios",
            arch: std::env::consts::ARCH,
            installers: &["ipa", super::release::STORE],
        }
    }

    pub(super) fn data_dir(&self) -> Result<PathBuf, String> {
        dataDirectoryFromRust(&self.plugin)
            .map(PathBuf::from)
            .ok_or_else(|| "The platform did not report a data directory".to_string())
    }

    /// The manifest to download and verify, or `None` for a store link.
    pub(super) fn download_url(candidate: &Candidate) -> Result<Option<String>, String> {
        install_link(&candidate.url).map(|link| match link {
            Link::Manifest(url) => Some(url),
            Link::Store(_) => None,
        })
    }

    pub(super) fn install(
        &self,
        file: Option<&Path>,
        candidate: &Candidate,
    ) -> Result<Step, String> {
        let url = match install_link(&candidate.url)? {
            Link::Manifest(manifest) => {
                if file.is_none() {
                    return Err("The install manifest was not downloaded".to_string());
                }
                format!(
                    "itms-services://?action=download-manifest&url={}",
                    percent_encode(&manifest)
                )
            }
            Link::Store(url) => {
                return match openUrlFromRust(&self.plugin, url) {
                    Some(error) if !error.is_empty() => Err(error),
                    _ => Ok(Step::Opened),
                };
            }
        };
        match openUrlFromRust(&self.plugin, url) {
            Some(error) if !error.is_empty() => Err(error),
            _ => Ok(Step::Started),
        }
    }

    /// iOS reports nothing back: the install prompt or the store is on
    /// screen, and if the user returns without installing the update is
    /// still ready to offer again.
    pub(super) fn install_progress(update: &Update) -> Option<UpdaterState> {
        Some(UpdaterState::Ready(update.clone()))
    }

    pub(super) fn clean_up() {
        if let Ok(directory) = Self::new().and_then(|native| native.data_dir()) {
            let _ = std::fs::remove_dir_all(directory.join("download"));
        }
    }
}

impl Transport for Native {
    fn download(
        &self,
        request: &DownloadRequest<'_>,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<u16, String> {
        let headers: serde_json::Map<String, serde_json::Value> = request
            .headers
            .iter()
            .map(|(name, value)| (name.clone(), serde_json::Value::String(value.clone())))
            .collect();
        let body = serde_json::json!({
            "url": request.url,
            "destination": request.destination.to_string_lossy(),
            "headers": headers,
            "timeoutMs": request.timeout_ms,
        })
        .to_string();
        let reported =
            downloadFromRust(&self.plugin, body).ok_or("The download reported nothing")?;
        let value: serde_json::Value = serde_json::from_str(&reported)
            .map_err(|error| format!("Failed to read the download result: {error}"))?;
        if let Some(error) = value.get("error").and_then(|error| error.as_str()) {
            return Err(error.to_string());
        }
        // Manifests and release files are small: one report at the end.
        if let Ok(metadata) = std::fs::metadata(request.destination) {
            progress(metadata.len(), Some(metadata.len()));
        }
        value
            .get("status")
            .and_then(|status| status.as_u64())
            .and_then(|status| u16::try_from(status).ok())
            .ok_or_else(|| "The download reported no HTTP status".to_string())
    }
}
