//! Android: downloads through the platform's HTTP stack, installs through
//! `PackageInstaller`.
//!
//! An app outside Google Play installs its own update like any other APK: it
//! needs `REQUEST_INSTALL_PACKAGES`, and the user has to have let it install
//! apps once (Settings, "Install unknown apps"). From Android 12, with
//! `UPDATE_PACKAGES_WITHOUT_USER_ACTION` and a recent target SDK, an app
//! updating itself does so without a confirmation dialog; otherwise, or when
//! the system decides it wants one anyway, Android shows its own. Android
//! itself refuses an APK signed with a different key or carrying a lower
//! `versionCode`.

use super::engine::{DownloadRequest, Transport};
use super::release::{Candidate, PlatformKey};
use super::{Update, UpdaterState};
use crate::android_bridge::AndroidPlugin;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

/// Declared for its side effect: this is what tells `dx` to build and bundle
/// the Kotlin module. The calls go through [`crate::android_bridge`] instead,
/// because the macro's own bindings resolve classes against the calling
/// thread's loader and Dioxus does not call from a thread that has one.
#[manganis::ffi("src/android/updater")]
unsafe extern "Kotlin" {
    pub type UpdaterPlugin;
}

const UPDATER_CLASS: &str = "dev.dioxus.g3_native_plugins.updater.UpdaterPlugin";

/// What [`Native::install`] got as far as.
pub(super) enum Step {
    /// The APK is with the system installer.
    Started,
    /// The user has not let this app install apps; the settings screen is
    /// open.
    NeedsPermission,
}

/// One Kotlin object for the process: the download runs on one thread while
/// another reads its progress, and the install status is polled on every
/// `state()`.
static PLUGIN: OnceLock<AndroidPlugin> = OnceLock::new();

pub(super) struct Native {
    plugin: &'static AndroidPlugin,
}

impl Native {
    pub(super) fn new() -> Result<Self, String> {
        if let Some(plugin) = PLUGIN.get() {
            return Ok(Self { plugin });
        }
        let built = AndroidPlugin::new(UPDATER_CLASS)?;
        Ok(Self {
            plugin: PLUGIN.get_or_init(|| built),
        })
    }

    pub(super) fn key() -> PlatformKey<'static> {
        let arch = match std::env::consts::ARCH {
            "x86" => "i686",
            "arm" => "armv7",
            other => other,
        };
        PlatformKey {
            target: "android",
            arch,
            installers: &["apk"],
        }
    }

    pub(super) fn data_dir(&self) -> Result<PathBuf, String> {
        self.plugin
            .call_string("dataDirectoryFromRust")?
            .map(PathBuf::from)
            .ok_or_else(|| "The platform did not report a data directory".to_string())
    }

    pub(super) fn download_url(candidate: &Candidate) -> Result<Option<String>, String> {
        Ok(Some(candidate.url.clone()))
    }

    pub(super) fn install(
        &self,
        file: Option<&Path>,
        _candidate: &Candidate,
    ) -> Result<Step, String> {
        let file = file.ok_or("There is no downloaded APK to install")?;
        let allowed = self.plugin.call_string("canInstallFromRust")?;
        if allowed.as_deref() != Some("true") {
            self.plugin.call_unit("openInstallSettingsFromRust")?;
            return Ok(Step::NeedsPermission);
        }
        let reported = self
            .plugin
            .call_string_str("installFromRust", &file.to_string_lossy())?
            .ok_or("The installer reported nothing")?;
        error_of(&reported).map_or(Ok(Step::Started), Err)
    }

    /// Where the system installer has got. Success never reports: Android
    /// stops this process to replace it.
    pub(super) fn install_progress(update: &Update) -> Option<UpdaterState> {
        let reported = Self::new()
            .ok()?
            .plugin
            .call_string("installStatusFromRust")
            .ok()??;
        let value: serde_json::Value = serde_json::from_str(&reported).ok()?;
        match value.get("status").and_then(|status| status.as_str())? {
            // Cancelled in the system dialog: the APK is still good, so the
            // app can offer the install again.
            "aborted" => Some(UpdaterState::Ready(update.clone())),
            "failed" => Some(UpdaterState::Failed(
                value
                    .get("message")
                    .and_then(|message| message.as_str())
                    .unwrap_or("The system installer refused the update")
                    .to_string(),
            )),
            _ => None,
        }
    }

    pub(super) fn clean_up() {
        if let Ok(directory) = Self::new().and_then(|native| native.data_dir()) {
            let _ = std::fs::remove_dir_all(directory.join("download"));
        }
    }
}

fn error_of(reported: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(reported).ok()?;
    value
        .get("error")
        .and_then(|error| error.as_str())
        .map(str::to_string)
}

impl Transport for Native {
    /// The Kotlin download blocks, so it runs on a thread of its own while
    /// this one watches the partial file grow and reports it.
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
        let plugin = self.plugin;
        let worker = std::thread::Builder::new()
            .name("g3-updater-download".to_string())
            .spawn(move || plugin.call_string_str("downloadFromRust", &body))
            .map_err(|error| format!("Failed to start the download: {error}"))?;
        // The Kotlin side's names for the partial file and the size it
        // learned from the response headers.
        let partial = PathBuf::from(format!("{}.part", request.destination.display()));
        let length = PathBuf::from(format!("{}.length", request.destination.display()));
        while !worker.is_finished() {
            if let Ok(metadata) = std::fs::metadata(&partial) {
                let total = std::fs::read_to_string(&length)
                    .ok()
                    .and_then(|text| text.trim().parse().ok());
                progress(metadata.len(), total);
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        let _ = std::fs::remove_file(&length);
        let reported = worker
            .join()
            .map_err(|_| "The download thread panicked".to_string())??
            .ok_or("The download reported nothing")?;
        if let Some(error) = error_of(&reported) {
            return Err(error);
        }
        let value: serde_json::Value = serde_json::from_str(&reported)
            .map_err(|error| format!("Failed to read the download result: {error}"))?;
        let status = value
            .get("status")
            .and_then(|status| status.as_u64())
            .and_then(|status| u16::try_from(status).ok())
            .ok_or("The download reported no HTTP status")?;
        if let Ok(metadata) = std::fs::metadata(request.destination) {
            progress(metadata.len(), Some(metadata.len()));
        }
        Ok(status)
    }
}
