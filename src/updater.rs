use serde::{Deserialize, Serialize};
#[cfg(any(
    target_arch = "wasm32",
    target_os = "android",
    target_os = "ios",
    target_os = "macos"
))]
use std::path::PathBuf;
/// Declared for its side effect: this is what tells `dx` to build and bundle
/// the Kotlin module. The calls go through [`crate::android_bridge`] instead,
/// because the macro's own bindings resolve classes against the calling
/// thread's loader and Dioxus does not call from a thread that has one.
#[cfg(target_os = "android")]
#[manganis::ffi("src/android/updater")]
unsafe extern "Kotlin" {
    pub type UpdaterPlugin;
}
#[cfg(target_os = "android")]
const UPDATER_CLASS: &str = "dev.dioxus.g3_native_plugins.updater.UpdaterPlugin";
#[cfg(target_os = "android")]
type UpdaterHandle = crate::android_bridge::AndroidPlugin;
#[cfg(target_os = "ios")]
type UpdaterHandle = UpdaterPlugin;
#[cfg(target_os = "ios")]
#[manganis::ffi("src/ios")]
unsafe extern "Swift" {
    pub type UpdaterPlugin;
    pub fn dataDirectoryFromRust(this: &UpdaterPlugin) -> Option<String>;
    pub fn appVersionFromRust(this: &UpdaterPlugin) -> Option<String>;
    pub fn downloadFromRust(this: &UpdaterPlugin, requestJson: String) -> Option<String>;
}
/// Where to look for updates and how to tell a genuine one.
///
/// ```rust,ignore
/// let config = UpdaterConfig::new(UPDATER_PUBLIC_KEY, "1.4.0")
///     .endpoint("https://updates.example.com/{{target}}/{{runtime_version}}/{{current_version}}");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdaterConfig {
    /// Update-check URLs, tried in order until one answers. Each may contain
    /// `{{current_version}}`, `{{runtime_version}}`, `{{target}}` (`ios` or
    /// `android`) and `{{arch}}` (`aarch64`, `x86_64`, …), which are filled in
    /// before the request, the same placeholders Tauri's updater uses.
    pub endpoints: Vec<String>,
    /// The minisign public key bundles are signed with. Accepts the contents of
    /// a `minisign.pub` file, its bare key line, or the base64-wrapped form
    /// `tauri signer generate` prints.
    pub pubkey: String,
    /// The version of the content compiled into this build. It is what is
    /// running until a downloaded bundle takes over, and a downloaded bundle no
    /// newer than it is discarded rather than served.
    pub embedded_version: String,
    /// Which native build a bundle may run on. A bundle is only installed when
    /// its manifest names exactly this runtime, and every downloaded bundle is
    /// discarded when it changes. Defaults to the app's version string
    /// (`CFBundleShortVersionString`, `versionName`), so a store release starts
    /// clean; set it explicitly to keep bundles across native releases that
    /// did not change what the bundle relies on.
    pub runtime_version: Option<String>,
    /// Extra request headers sent with every request, for an update server that
    /// wants a token or a channel name.
    pub headers: Vec<(String, String)>,
    /// How long one request may take before it is abandoned.
    pub timeout_ms: u32,
}
impl UpdaterConfig {
    /// A configuration with no endpoints yet: add them with
    /// [`endpoint`](UpdaterConfig::endpoint).
    pub fn new(pubkey: impl Into<String>, embedded_version: impl Into<String>) -> Self {
        Self {
            endpoints: Vec::new(),
            pubkey: pubkey.into(),
            embedded_version: embedded_version.into(),
            runtime_version: None,
            headers: Vec::new(),
            timeout_ms: 30_000,
        }
    }
    /// Add an update-check URL, tried after any added before it.
    pub fn endpoint(mut self, url: impl Into<String>) -> Self {
        self.endpoints.push(url.into());
        self
    }
    /// Pin the runtime version rather than deriving it from the app version.
    pub fn runtime_version(mut self, runtime_version: impl Into<String>) -> Self {
        self.runtime_version = Some(runtime_version.into());
        self
    }
    /// Send a header with every update request.
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }
    /// Change the per-request timeout from its default of 30 seconds.
    pub fn timeout_ms(mut self, timeout_ms: u32) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }
}
/// A newer bundle the update server offered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Update {
    /// The bundle version on offer.
    pub version: String,
    /// The version running now: the active downloaded bundle's, or
    /// [`UpdaterConfig::embedded_version`] when none is active.
    pub current_version: String,
    /// Release notes, if the server sent any.
    pub notes: Option<String>,
    /// Publication date as the server wrote it, conventionally RFC 3339.
    pub pub_date: Option<String>,
}
/// How far a download has got.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DownloadProgress {
    /// Files verified and staged so far, including those reused unchanged from
    /// the active bundle.
    pub files_done: usize,
    /// Files in the bundle.
    pub files_total: usize,
    /// Bytes verified and staged so far.
    pub bytes_done: u64,
    /// Bytes in the bundle, counting only files whose manifest entry gave a
    /// size.
    pub bytes_total: u64,
}
/// What the updater is doing, for the UI to poll.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum UpdaterState {
    /// Nothing asked for yet.
    #[default]
    Idle,
    /// An update check is in flight.
    Checking,
    /// The server has nothing newer than what is running.
    UpToDate,
    /// A newer bundle is on offer; start downloading it with
    /// [`Updater::start_download`].
    Available(Update),
    /// The bundle is downloading and being verified.
    Downloading {
        /// The bundle being downloaded.
        update: Update,
        /// How far it has got.
        progress: DownloadProgress,
    },
    /// Downloaded, verified, and staged. It becomes active at the next launch,
    /// or immediately through [`Updater::apply_now`].
    Ready(Update),
    /// The last check or download failed. Nothing was installed.
    Failed(String),
}
/// The MIME type to serve a bundle file with, by extension.
///
/// For the asset handler that serves the active bundle to the WebView. It
/// matters more than it looks: WebKit refuses a module script served as
/// `application/octet-stream`, and `WebAssembly.instantiateStreaming` refuses
/// anything but `application/wasm`.
pub fn bundle_content_type(path: &str) -> &'static str {
    let extension = path
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "wasm" => "application/wasm",
        "txt" => "text/plain; charset=utf-8",
        "xml" => "application/xml",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}
/// The platform-independent half: checking, verifying, staging, and the launch
/// state machine that installs and rolls back. Everything here is ordinary
/// filesystem and parsing work behind a [`engine::Transport`], so it runs — and
/// is tested — on the host as well as on a phone.
#[cfg(any(target_os = "android", target_os = "ios", test))]
#[cfg_attr(not(any(target_os = "android", target_os = "ios")), allow(dead_code))]
mod engine {
    use super::{DownloadProgress, Update, UpdaterConfig};
    use serde::{Deserialize, Serialize};
    use sha2::{Digest, Sha256};
    use std::collections::{BTreeMap, HashSet};
    use std::fs;
    use std::io::Read;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    /// The verified manifest is kept beside the files it describes. The prefix
    /// is reserved, so a bundle cannot ship a file that overwrites it.
    pub(super) const MANIFEST_FILE: &str = ".g3-manifest.json";
    const RESERVED_PREFIX: &str = ".g3-";
    /// One lock for every state-file change in the process. A download
    /// finishing on the worker and a launch confirming on the UI thread both
    /// read, change, and write the same file, and interleaved they would lose
    /// one of the changes.
    static STATE_LOCK: Mutex<()> = Mutex::new(());
    pub(super) struct DownloadRequest<'a> {
        pub url: &'a str,
        pub destination: &'a Path,
        pub headers: &'a [(String, String)],
        pub timeout_ms: u32,
    }
    /// Fetches one URL to a file and reports the HTTP status. The body is
    /// written only for a 2xx other than 204; anything else leaves no file.
    pub(super) trait Transport {
        fn download(&self, request: &DownloadRequest<'_>) -> Result<u16, String>;
    }
    /// The installed bundles, as recorded on disk.
    #[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
    pub(super) struct BundleState {
        /// The native runtime the bundles below were installed for.
        pub runtime_version: Option<String>,
        /// The bundle served now, or none for the embedded content.
        pub current: Option<String>,
        /// A confirmed bundle to fall back to while `current` is on trial.
        pub previous: Option<String>,
        /// Downloaded and verified, waiting for the next launch.
        pub pending: Option<String>,
        /// `current` has launched but not yet been confirmed healthy.
        pub trial: bool,
        /// Launches of `current` without a confirmation.
        pub trial_launches: u32,
        /// Versions rolled back from, never to be offered again.
        pub failed: Vec<String>,
    }
    pub(super) struct Store {
        root: PathBuf,
    }
    impl Store {
        pub fn new(root: PathBuf) -> Self {
            Self { root }
        }
        fn state_path(&self) -> PathBuf {
            self.root.join("state.json")
        }
        fn bundles(&self) -> PathBuf {
            self.root.join("bundles")
        }
        pub fn bundle_dir(&self, version: &str) -> PathBuf {
            self.bundles().join(version)
        }
        pub fn scratch(&self) -> PathBuf {
            self.root.join("scratch")
        }
        fn staging(&self) -> PathBuf {
            self.root.join("staging")
        }
        /// A missing or unreadable state file reads as "nothing installed",
        /// which serves the embedded content: the one answer that is always
        /// safe.
        pub fn load(&self) -> BundleState {
            fs::read(self.state_path())
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default()
        }
        /// Written beside and renamed over, so a crash mid-write leaves the
        /// old state rather than half of the new one.
        fn save(&self, state: &BundleState) -> Result<(), String> {
            fs::create_dir_all(&self.root)
                .map_err(|error| format!("Failed to create the updater directory: {error}"))?;
            let bytes = serde_json::to_vec_pretty(state)
                .map_err(|error| format!("Failed to encode updater state: {error}"))?;
            let temporary = self.root.join("state.json.tmp");
            fs::write(&temporary, bytes)
                .map_err(|error| format!("Failed to write updater state: {error}"))?;
            fs::rename(&temporary, self.state_path())
                .map_err(|error| format!("Failed to replace updater state: {error}"))
        }
        fn active(&self, state: &BundleState) -> Option<(String, PathBuf)> {
            let version = state.current.clone()?;
            let dir = self.bundle_dir(&version);
            Some((version, dir))
        }
        /// Decide what this launch serves, installing a pending bundle and
        /// rolling back one that never confirmed. Call once per process.
        pub fn launch(
            &self,
            runtime_version: &str,
            embedded_version: &str,
        ) -> Result<Option<(String, PathBuf)>, String> {
            let _guard = STATE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
            let mut state = self.load();
            // A new native build invalidates everything downloaded for the old
            // one: those bundles were checked against a runtime that is gone.
            if state.runtime_version.as_deref() != Some(runtime_version) {
                state = BundleState {
                    runtime_version: Some(runtime_version.to_string()),
                    ..BundleState::default()
                };
            }
            // Still on trial from an earlier launch: that launch never called
            // notify_ready, so treat the bundle as broken and go back.
            if state.trial && state.trial_launches >= 1 {
                if let Some(broken) = state.current.take() {
                    state.failed.push(broken);
                }
                state.current = state.previous.take();
                state.trial = false;
                state.trial_launches = 0;
            }
            if let Some(next) = state.pending.take()
                && self.bundle_dir(&next).is_dir()
            {
                // Only a confirmed bundle is a safe place to fall back to.
                let outgoing = state.current.take();
                if !state.trial {
                    state.previous = outgoing;
                }
                state.current = Some(next);
                state.trial = true;
                state.trial_launches = 0;
            }
            // Missing on disk, or no newer than what this build carries: the
            // embedded content wins.
            let stale = state.current.as_deref().is_some_and(|current| {
                !self.bundle_dir(current).is_dir() || !is_newer(current, embedded_version)
            });
            if stale {
                state.current = None;
                state.previous = None;
                state.trial = false;
                state.trial_launches = 0;
            }
            if state.trial {
                state.trial_launches += 1;
            }
            self.save(&state)?;
            self.collect_garbage(&state);
            Ok(self.active(&state))
        }
        /// The running bundle works: stop counting it as on trial.
        pub fn confirm(&self) -> Result<(), String> {
            let _guard = STATE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
            let mut state = self.load();
            if !state.trial {
                return Ok(());
            }
            state.trial = false;
            state.trial_launches = 0;
            // Rollback only ever happens from a trial, so the fallback has
            // nothing left to do once the trial is over.
            state.previous = None;
            self.save(&state)?;
            self.collect_garbage(&state);
            Ok(())
        }
        /// Make the pending bundle current without waiting for a relaunch. It
        /// is on trial as of this launch, so a crash before confirming still
        /// rolls it back.
        pub fn apply_pending(&self) -> Result<Option<(String, PathBuf)>, String> {
            let _guard = STATE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
            let mut state = self.load();
            let Some(next) = state.pending.take() else {
                return Err("No downloaded update is waiting to be applied".to_string());
            };
            if !self.bundle_dir(&next).is_dir() {
                self.save(&state)?;
                return Err(format!("The downloaded bundle {next} is missing"));
            }
            let outgoing = state.current.take();
            if !state.trial {
                state.previous = outgoing;
            }
            state.current = Some(next);
            state.trial = true;
            state.trial_launches = 1;
            self.save(&state)?;
            self.collect_garbage(&state);
            Ok(self.active(&state))
        }
        /// Forget every downloaded bundle and serve the embedded content.
        pub fn reset(&self) -> Result<(), String> {
            let _guard = STATE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
            let state = BundleState {
                runtime_version: self.load().runtime_version,
                ..BundleState::default()
            };
            self.save(&state)?;
            let _ = fs::remove_dir_all(self.bundles());
            let _ = fs::remove_dir_all(self.staging());
            Ok(())
        }
        fn collect_garbage(&self, state: &BundleState) {
            let keep: HashSet<&str> = [&state.current, &state.previous, &state.pending]
                .into_iter()
                .filter_map(|version| version.as_deref())
                .collect();
            if let Ok(entries) = fs::read_dir(self.bundles()) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    if !keep.contains(name.to_string_lossy().as_ref()) {
                        let _ = fs::remove_dir_all(entry.path());
                    }
                }
            }
        }
        /// Download every file the verified manifest lists into a staging
        /// directory, check each against its hash, and only then move the
        /// whole bundle into place and mark it pending. A failure at any point
        /// leaves the installed bundles as they were.
        pub fn stage(
            &self,
            manifest_bytes: &[u8],
            manifest: &Manifest,
            manifest_url: &str,
            config: &UpdaterConfig,
            transport: &dyn Transport,
            progress: &mut dyn FnMut(DownloadProgress),
        ) -> Result<(), String> {
            let staging = self.staging().join(&manifest.version);
            let result = self.stage_into(
                &staging,
                manifest_bytes,
                manifest,
                manifest_url,
                config,
                transport,
                progress,
            );
            if result.is_err() {
                let _ = fs::remove_dir_all(&staging);
            }
            result
        }
        #[allow(clippy::too_many_arguments)]
        fn stage_into(
            &self,
            staging: &Path,
            manifest_bytes: &[u8],
            manifest: &Manifest,
            manifest_url: &str,
            config: &UpdaterConfig,
            transport: &dyn Transport,
            progress: &mut dyn FnMut(DownloadProgress),
        ) -> Result<(), String> {
            let _ = fs::remove_dir_all(staging);
            fs::create_dir_all(staging)
                .map_err(|error| format!("Failed to create a staging directory: {error}"))?;
            let reuse_from = self.active(&self.load()).map(|(_, dir)| dir);
            let mut done = DownloadProgress {
                files_total: manifest.files.len(),
                bytes_total: manifest.files.iter().filter_map(|file| file.size).sum(),
                ..DownloadProgress::default()
            };
            progress(done);
            for file in &manifest.files {
                let destination = staging.join(&file.path);
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent).map_err(|error| {
                        format!("Failed to create a directory for {}: {error}", file.path)
                    })?;
                }
                // A file the active bundle already has, byte for byte, is
                // copied rather than fetched again, so a release that changes
                // one stylesheet downloads one stylesheet.
                let reusable = reuse_from.as_ref().map(|dir| dir.join(&file.path));
                let reused = match reusable {
                    Some(existing) if existing.is_file() => {
                        sha256_file(&existing)
                            .is_ok_and(|hash| hash.eq_ignore_ascii_case(&file.sha256))
                            && fs::copy(&existing, &destination).is_ok()
                    }
                    _ => false,
                };
                if !reused {
                    let url = file_url(manifest_url, manifest.base_url.as_deref(), file);
                    let status = transport.download(&DownloadRequest {
                        url: &url,
                        destination: &destination,
                        headers: &config.headers,
                        timeout_ms: config.timeout_ms,
                    })?;
                    if status != 200 {
                        return Err(format!("Downloading {} answered HTTP {status}", file.path));
                    }
                }
                let length = fs::metadata(&destination)
                    .map_err(|error| format!("Failed to read {}: {error}", file.path))?
                    .len();
                if let Some(expected) = file.size
                    && expected != length
                {
                    return Err(format!(
                        "{} is {length} bytes, but the manifest says {expected}",
                        file.path
                    ));
                }
                let hash = sha256_file(&destination)?;
                if !hash.eq_ignore_ascii_case(&file.sha256) {
                    return Err(format!(
                        "{} does not match the hash in the signed manifest",
                        file.path
                    ));
                }
                done.files_done += 1;
                done.bytes_done += length;
                progress(done);
            }
            fs::write(staging.join(MANIFEST_FILE), manifest_bytes)
                .map_err(|error| format!("Failed to record the manifest: {error}"))?;
            let _guard = STATE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
            let mut state = self.load();
            let destination = self.bundle_dir(&manifest.version);
            if state.current.as_deref() == Some(manifest.version.as_str()) {
                return Err(format!(
                    "{} is already the running bundle",
                    manifest.version
                ));
            }
            let _ = fs::remove_dir_all(&destination);
            fs::create_dir_all(self.bundles())
                .map_err(|error| format!("Failed to create the bundle directory: {error}"))?;
            fs::rename(staging, &destination)
                .map_err(|error| format!("Failed to move the bundle into place: {error}"))?;
            state.pending = Some(manifest.version.clone());
            state.runtime_version = Some(manifest.runtime_version.clone());
            self.save(&state)?;
            self.collect_garbage(&state);
            Ok(())
        }
    }
    /// What the update server answers a check with. The same shape as a Tauri
    /// updater response: a per-platform table, or one `url` and `signature` for
    /// a server that already picked the platform from the request URL.
    #[derive(Debug, Clone, Deserialize)]
    struct Release {
        version: String,
        #[serde(default)]
        notes: Option<String>,
        #[serde(default)]
        pub_date: Option<String>,
        #[serde(default)]
        url: Option<String>,
        #[serde(default)]
        signature: Option<String>,
        #[serde(default)]
        platforms: BTreeMap<String, PlatformRelease>,
    }
    #[derive(Debug, Clone, Deserialize)]
    struct PlatformRelease {
        url: String,
        signature: String,
    }
    /// The signed description of a bundle: every file, and the hash it must
    /// have. The signature covers these exact bytes, and the hashes carry that
    /// guarantee on to the files.
    #[derive(Debug, Clone, Deserialize)]
    pub(super) struct Manifest {
        pub version: String,
        pub runtime_version: String,
        #[serde(default)]
        pub base_url: Option<String>,
        pub files: Vec<ManifestFile>,
    }
    #[derive(Debug, Clone, Deserialize)]
    pub(super) struct ManifestFile {
        pub path: String,
        pub sha256: String,
        #[serde(default)]
        pub size: Option<u64>,
        #[serde(default)]
        pub url: Option<String>,
    }
    /// An update the server offered, not yet downloaded.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(super) struct Candidate {
        pub update: Update,
        pub manifest_url: String,
        pub signature: String,
    }
    pub(super) fn fill_placeholders(
        endpoint: &str,
        current_version: &str,
        runtime_version: &str,
        target: &str,
        arch: &str,
    ) -> String {
        endpoint
            .replace("{{current_version}}", current_version)
            .replace("{{runtime_version}}", runtime_version)
            .replace("{{target}}", target)
            .replace("{{arch}}", arch)
    }
    /// Ask each endpoint in turn. A 204 is a definite "nothing new" and ends
    /// the search; an error moves on to the next endpoint, and only when all
    /// of them fail is the check a failure.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check(
        config: &UpdaterConfig,
        runtime_version: &str,
        current_version: &str,
        failed: &[String],
        target: &str,
        arch: &str,
        transport: &dyn Transport,
        scratch: &Path,
    ) -> Result<Option<Candidate>, String> {
        if config.endpoints.is_empty() {
            return Err("The updater has no endpoints configured".to_string());
        }
        fs::create_dir_all(scratch)
            .map_err(|error| format!("Failed to create a scratch directory: {error}"))?;
        let response = scratch.join("release.json");
        let mut last_error = String::new();
        for endpoint in &config.endpoints {
            let url = fill_placeholders(endpoint, current_version, runtime_version, target, arch);
            let _ = fs::remove_file(&response);
            let status = match transport.download(&DownloadRequest {
                url: &url,
                destination: &response,
                headers: &config.headers,
                timeout_ms: config.timeout_ms,
            }) {
                Ok(status) => status,
                Err(error) => {
                    last_error = format!("{url}: {error}");
                    continue;
                }
            };
            if status == 204 {
                return Ok(None);
            }
            if status != 200 {
                last_error = format!("{url} answered HTTP {status}");
                continue;
            }
            let body = fs::read(&response)
                .map_err(|error| format!("Failed to read the update response: {error}"))?;
            let _ = fs::remove_file(&response);
            return choose_release(&body, current_version, failed, target, arch);
        }
        Err(format!(
            "No update endpoint answered. Last error: {last_error}"
        ))
    }
    fn choose_release(
        body: &[u8],
        current_version: &str,
        failed: &[String],
        target: &str,
        arch: &str,
    ) -> Result<Option<Candidate>, String> {
        let release: Release = serde_json::from_slice(body)
            .map_err(|error| format!("The update response is not a release: {error}"))?;
        semver::Version::parse(&release.version).map_err(|error| {
            format!(
                "The update response has an invalid version {}: {error}",
                release.version
            )
        })?;
        if !is_newer(&release.version, current_version) || failed.contains(&release.version) {
            return Ok(None);
        }
        let specific = format!("{target}-{arch}");
        let (url, signature) = match release
            .platforms
            .get(&specific)
            .or_else(|| release.platforms.get(target))
        {
            Some(platform) => (platform.url.clone(), platform.signature.clone()),
            None => match (&release.url, &release.signature) {
                (Some(url), Some(signature)) => (url.clone(), signature.clone()),
                _ => {
                    return Err(format!(
                        "The update response has no bundle for {specific} or {target}"
                    ));
                }
            },
        };
        Ok(Some(Candidate {
            update: Update {
                version: release.version,
                current_version: current_version.to_string(),
                notes: release.notes,
                pub_date: release.pub_date,
            },
            manifest_url: url,
            signature,
        }))
    }
    /// Fetch the manifest, check it against the public key and the running
    /// runtime, then stage every file it lists.
    pub(super) fn download(
        candidate: &Candidate,
        config: &UpdaterConfig,
        runtime_version: &str,
        store: &Store,
        transport: &dyn Transport,
        progress: &mut dyn FnMut(DownloadProgress),
    ) -> Result<(), String> {
        let scratch = store.scratch();
        fs::create_dir_all(&scratch)
            .map_err(|error| format!("Failed to create a scratch directory: {error}"))?;
        let manifest_path = scratch.join("manifest.json");
        let _ = fs::remove_file(&manifest_path);
        let status = transport.download(&DownloadRequest {
            url: &candidate.manifest_url,
            destination: &manifest_path,
            headers: &config.headers,
            timeout_ms: config.timeout_ms,
        })?;
        if status != 200 {
            return Err(format!("The bundle manifest answered HTTP {status}"));
        }
        let bytes = fs::read(&manifest_path)
            .map_err(|error| format!("Failed to read the bundle manifest: {error}"))?;
        let _ = fs::remove_file(&manifest_path);
        // Nothing in the manifest is trusted, or even parsed, before this.
        verify_signature(&config.pubkey, &candidate.signature, &bytes)?;
        let manifest = parse_manifest(&bytes)?;
        // The check response is unsigned, so what it advertised only counts
        // if the signed manifest agrees.
        if manifest.version != candidate.update.version {
            return Err(format!(
                "The server offered {} but the signed manifest is for {}",
                candidate.update.version, manifest.version
            ));
        }
        if manifest.runtime_version != runtime_version {
            return Err(format!(
                "Bundle {} needs runtime {}, but this build is runtime {runtime_version}",
                manifest.version, manifest.runtime_version
            ));
        }
        store.stage(
            &bytes,
            &manifest,
            &candidate.manifest_url,
            config,
            transport,
            progress,
        )
    }
    pub(super) fn parse_manifest(bytes: &[u8]) -> Result<Manifest, String> {
        let manifest: Manifest = serde_json::from_slice(bytes)
            .map_err(|error| format!("The bundle manifest is not valid: {error}"))?;
        semver::Version::parse(&manifest.version).map_err(|error| {
            format!(
                "The bundle manifest has an invalid version {}: {error}",
                manifest.version
            )
        })?;
        let mut seen = HashSet::new();
        for file in &manifest.files {
            if !is_safe_relative_path(&file.path) || file.path.starts_with(RESERVED_PREFIX) {
                return Err(format!(
                    "The bundle manifest lists an unsafe path: {}",
                    file.path
                ));
            }
            if !seen.insert(file.path.as_str()) {
                return Err(format!("The bundle manifest lists {} twice", file.path));
            }
            let valid_hash =
                file.sha256.len() == 64 && file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit());
            if !valid_hash {
                return Err(format!(
                    "{} has no valid SHA-256 in the manifest",
                    file.path
                ));
            }
        }
        Ok(manifest)
    }
    /// Relative, forward-slashed, and unable to climb out of the directory it
    /// is joined to.
    pub(super) fn is_safe_relative_path(path: &str) -> bool {
        !path.is_empty()
            && !path.starts_with('/')
            && !path.contains('\\')
            && !path.contains('\0')
            && !path.contains(':')
            && path
                .split('/')
                .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
    }
    /// Join an asset request onto the bundle directory, or `None` if the path
    /// would reach outside it or names nothing there.
    pub(super) fn resolve(bundle: &Path, path: &str) -> Option<PathBuf> {
        let path = path.split(['?', '#']).next().unwrap_or_default();
        let path = path.trim_start_matches('/');
        if !is_safe_relative_path(path) || path.starts_with(RESERVED_PREFIX) {
            return None;
        }
        let resolved = bundle.join(path);
        resolved.is_file().then_some(resolved)
    }
    fn file_url(manifest_url: &str, base_url: Option<&str>, file: &ManifestFile) -> String {
        if let Some(url) = &file.url {
            return url.clone();
        }
        let base = match base_url {
            Some(base) => base.trim_end_matches('/').to_string(),
            None => {
                let without_query = manifest_url.split(['?', '#']).next().unwrap_or_default();
                match without_query.rsplit_once('/') {
                    Some((directory, _)) => directory.to_string(),
                    None => without_query.to_string(),
                }
            }
        };
        format!("{base}/{}", encode_path(&file.path))
    }
    fn encode_path(path: &str) -> String {
        let mut encoded = String::with_capacity(path.len());
        for byte in path.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                    encoded.push(byte as char)
                }
                _ => encoded.push_str(&format!("%{byte:02X}")),
            }
        }
        encoded
    }
    /// `true` when `candidate` is a strictly newer semantic version. A version
    /// that does not parse is never newer, so garbage cannot win.
    pub(super) fn is_newer(candidate: &str, current: &str) -> bool {
        match (
            semver::Version::parse(candidate),
            semver::Version::parse(current),
        ) {
            (Ok(candidate), Ok(current)) => candidate > current,
            _ => false,
        }
    }
    pub(super) fn sha256_file(path: &Path) -> Result<String, String> {
        let mut file = fs::File::open(path)
            .map_err(|error| format!("Failed to open {}: {error}", path.display()))?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = file
                .read(&mut buffer)
                .map_err(|error| format!("Failed to read {}: {error}", path.display()))?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        Ok(hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
    }
    pub(super) fn public_key(text: &str) -> Result<minisign_verify::PublicKey, String> {
        let text = text.trim();
        let invalid =
            |error: minisign_verify::Error| format!("The updater public key is invalid: {error}");
        if text.starts_with("untrusted comment:") {
            return minisign_verify::PublicKey::decode(text).map_err(invalid);
        }
        if let Ok(key) = minisign_verify::PublicKey::from_base64(text) {
            return Ok(key);
        }
        // `tauri signer generate` prints the whole key file, base64 again.
        let unwrapped = base64_text(text).ok_or("The updater public key is not base64")?;
        minisign_verify::PublicKey::decode(unwrapped.trim()).map_err(invalid)
    }
    pub(super) fn verify_signature(
        pubkey: &str,
        signature: &str,
        bytes: &[u8],
    ) -> Result<(), String> {
        let key = public_key(pubkey)?;
        let signature = signature.trim();
        let signature = if signature.starts_with("untrusted comment:") {
            signature.to_string()
        } else {
            // What `tauri signer sign` writes: the .sig file, base64 again.
            base64_text(signature).ok_or("The bundle signature is not base64")?
        };
        let signature = minisign_verify::Signature::decode(signature.trim())
            .map_err(|error| format!("The bundle signature is malformed: {error}"))?;
        // Legacy (non-prehashed) signatures are still Ed25519 over the bytes;
        // older Tauri signers produce them.
        key.verify(bytes, &signature, true)
            .map_err(|error| format!("The bundle manifest failed signature verification: {error}"))
    }
    fn base64_text(text: &str) -> Option<String> {
        String::from_utf8(base64_decode(text)?).ok()
    }
    fn base64_decode(text: &str) -> Option<Vec<u8>> {
        let mut output = Vec::with_capacity(text.len() * 3 / 4);
        let mut buffer = 0u32;
        let mut bits = 0;
        for byte in text.bytes() {
            let value = match byte {
                b'A'..=b'Z' => byte - b'A',
                b'a'..=b'z' => byte - b'a' + 26,
                b'0'..=b'9' => byte - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                b'=' => break,
                b'\n' | b'\r' | b' ' => continue,
                _ => return None,
            };
            buffer = (buffer << 6) | u32::from(value);
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                output.push((buffer >> bits) as u8);
                buffer &= (1 << bits) - 1;
            }
        }
        Some(output)
    }
}
/// The native half: where the bundles live, what the app's version is, and an
/// HTTP client that honors the platform's proxy and TLS policy.
#[cfg(any(target_os = "android", target_os = "ios"))]
struct Native {
    plugin: UpdaterHandle,
}
#[cfg(any(target_os = "android", target_os = "ios"))]
impl Native {
    fn new() -> Result<Self, String> {
        #[cfg(target_os = "android")]
        let plugin = UpdaterHandle::new(UPDATER_CLASS)?;
        #[cfg(target_os = "ios")]
        let plugin = UpdaterPlugin::new()
            .map_err(|error| format!("Failed to create UpdaterPlugin: {error:?}"))?;
        Ok(Self { plugin })
    }
    fn data_dir(&self) -> Result<PathBuf, String> {
        #[cfg(target_os = "android")]
        let reported = self.plugin.call_string("dataDirectoryFromRust")?;
        #[cfg(target_os = "ios")]
        let reported = dataDirectoryFromRust(&self.plugin)?;
        reported
            .map(PathBuf::from)
            .ok_or_else(|| "The platform did not report a data directory".to_string())
    }
    fn app_version(&self) -> Result<String, String> {
        #[cfg(target_os = "android")]
        let reported = self.plugin.call_string("appVersionFromRust")?;
        #[cfg(target_os = "ios")]
        let reported = appVersionFromRust(&self.plugin)?;
        reported.ok_or_else(|| "The platform did not report an app version".to_string())
    }
}
#[cfg(any(target_os = "android", target_os = "ios"))]
impl engine::Transport for Native {
    fn download(&self, request: &engine::DownloadRequest<'_>) -> Result<u16, String> {
        let headers: serde_json::Map<String, serde_json::Value> = request
            .headers
            .iter()
            .map(|(name, value)| (name.clone(), serde_json::Value::String(value.clone())))
            .collect();
        let request = serde_json::json!({
            "url": request.url,
            "destination": request.destination.to_string_lossy(),
            "headers": headers,
            "timeoutMs": request.timeout_ms,
        })
        .to_string();
        #[cfg(target_os = "android")]
        let reported = self.plugin.call_string_str("downloadFromRust", &request)?;
        #[cfg(target_os = "ios")]
        let reported = downloadFromRust(&self.plugin, request)?;
        let reported = reported.ok_or_else(|| "The download reported nothing".to_string())?;
        let value: serde_json::Value = serde_json::from_str(&reported)
            .map_err(|error| format!("Failed to read the download result: {error}"))?;
        if let Some(error) = value.get("error").and_then(|error| error.as_str()) {
            return Err(error.to_string());
        }
        value
            .get("status")
            .and_then(|status| status.as_u64())
            .and_then(|status| u16::try_from(status).ok())
            .ok_or_else(|| "The download reported no HTTP status".to_string())
    }
}
#[cfg(any(target_os = "android", target_os = "ios"))]
#[derive(Default)]
struct Shared {
    config: Option<UpdaterConfig>,
    runtime_version: Option<String>,
    root: Option<PathBuf>,
    active: Option<(String, PathBuf)>,
    state: UpdaterState,
    candidate: Option<engine::Candidate>,
    busy: bool,
}
/// Process-wide, like the deep-link queue: [`Updater::launch`] runs in `main`
/// on an instance that is gone by the time the provider's own exists, and both
/// must see the same bundle.
#[cfg(any(target_os = "android", target_os = "ios"))]
static SHARED: std::sync::Mutex<Option<Shared>> = std::sync::Mutex::new(None);
#[cfg(any(target_os = "android", target_os = "ios"))]
fn with_shared<T>(action: impl FnOnce(&mut Shared) -> T) -> T {
    let mut guard = SHARED.lock().unwrap_or_else(|error| error.into_inner());
    action(guard.get_or_insert_with(Shared::default))
}
/// Over-the-air updates for what the WebView loads, in the manner of Expo
/// Updates or CodePush, with Tauri's update-server format and signing.
///
/// # What this can and cannot update
///
/// The Rust side of a Dioxus app is compiled machine code, signed as part of
/// the app. Neither store lets an app replace that after review — iOS will not
/// even execute a native library that was not signed with the app — so no
/// updater, this one included, ships new Rust. What both stores do allow is
/// new content for a WebView or JavaScript engine that does not change what
/// the app is for: Apple's guideline 3.3.1(B) and Google Play's device and
/// network abuse policy each carve out exactly that, and it is the ground Expo
/// and CodePush stand on.
///
/// So this updates **files**: HTML, CSS, JavaScript, WebAssembly run by the
/// WebView, images, fonts, JSON, whatever the app reads. How much of an app
/// that covers depends on where its logic lives. Assets and styles update
/// under any Dioxus app; logic updates only where it runs in the WebView
/// rather than in the native binary.
///
/// # Safety net
///
/// A bundle is only installed if its manifest carries a valid minisign
/// signature from [`UpdaterConfig::pubkey`], names this build's runtime
/// version, and every file matches the SHA-256 the manifest gives it. It is
/// staged beside the running bundle and switched in atomically at the next
/// launch, on trial: if that launch never calls
/// [`notify_ready`](Updater::notify_ready), the launch after it goes back to
/// the previous bundle and never offers the broken version again.
///
/// ```rust,ignore
/// fn main() {
///     let config = UpdaterConfig::new(UPDATER_PUBLIC_KEY, "1.4.0")
///         .endpoint("https://updates.example.com/{{target}}/{{runtime_version}}");
///     // Before anything reads a bundle file: this is where a waiting update
///     // is installed, or a broken one rolled back.
///     let _ = Updater::new().launch(config);
///     dioxus::launch(App);
/// }
///
/// // Once the app has come up on the new bundle:
/// plugins.updater.write().notify_ready()?;
///
/// plugins.updater.write().start_check()?;
/// // ...then poll:
/// match plugins.updater.write().state() {
///     UpdaterState::Available(_) => plugins.updater.write().start_download()?,
///     UpdaterState::Ready(update) => { /* offer a restart, or apply_now() */ }
///     _ => {}
/// }
/// ```
///
/// The web build is inert, because a web deploy already is an update, and so
/// is macOS. Callers need no cfg of their own.
#[cfg(any(target_os = "android", target_os = "ios"))]
pub struct Updater;
/// The inert updater: see the native [`Updater`] for the contract.
#[cfg(any(target_arch = "wasm32", target_os = "macos"))]
pub struct Updater;
#[cfg(any(target_os = "android", target_os = "ios"))]
impl Updater {
    /// A handle on the process-wide updater. Every instance shares one state,
    /// so the one [`launch`](Updater::launch)ed in `main` and the provider's
    /// agree.
    pub fn new() -> Self {
        Self
    }
    /// Decide which bundle this launch serves: install one downloaded last
    /// time, or roll back one that never confirmed. Returns the directory to
    /// serve from, or `None` for the content compiled into the app.
    ///
    /// Call it once, in `main`, before anything reads a bundle file. Calling it
    /// again in the same process only replaces the configuration.
    pub fn launch(&mut self, config: UpdaterConfig) -> Result<Option<PathBuf>, String> {
        engine::public_key(&config.pubkey)?;
        semver::Version::parse(&config.embedded_version).map_err(|error| {
            format!(
                "The embedded version {} is not semver: {error}",
                config.embedded_version
            )
        })?;
        if let Some(active) = with_shared(|shared| {
            shared.root.is_some().then(|| {
                shared.config = Some(config.clone());
                shared.active.as_ref().map(|(_, dir)| dir.clone())
            })
        }) {
            return Ok(active);
        }
        let native = Native::new()?;
        let root = native.data_dir()?;
        let runtime_version = match &config.runtime_version {
            Some(runtime_version) => runtime_version.clone(),
            None => native.app_version()?,
        };
        let store = engine::Store::new(root.clone());
        let active = store.launch(&runtime_version, &config.embedded_version)?;
        let dir = active.as_ref().map(|(_, dir)| dir.clone());
        with_shared(|shared| {
            shared.config = Some(config);
            shared.runtime_version = Some(runtime_version);
            shared.root = Some(root);
            shared.active = active;
        });
        Ok(dir)
    }
    /// The directory of the downloaded bundle being served, or `None` while the
    /// embedded content is.
    pub fn bundle_dir(&mut self) -> Option<PathBuf> {
        with_shared(|shared| shared.active.as_ref().map(|(_, dir)| dir.clone()))
    }
    /// The version being served: the active bundle's, or the embedded one's.
    /// `None` before [`launch`](Updater::launch).
    pub fn current_version(&mut self) -> Option<String> {
        with_shared(|shared| {
            shared
                .active
                .as_ref()
                .map(|(version, _)| version.clone())
                .or_else(|| {
                    shared
                        .config
                        .as_ref()
                        .map(|config| config.embedded_version.clone())
                })
        })
    }
    /// The file in the active bundle an asset request names, or `None` when no
    /// bundle is active or it has no such file — the cue to serve the embedded
    /// copy instead. A path that would climb out of the bundle is `None` too.
    pub fn resolve_asset(&mut self, path: &str) -> Option<PathBuf> {
        let bundle = self.bundle_dir()?;
        engine::resolve(&bundle, path)
    }
    /// Say the bundle this launch is running works, which ends its trial.
    ///
    /// Until this is called, a newly installed bundle is provisional: if the
    /// app is launched again without it, the updater assumes the new bundle
    /// broke something and goes back to the one before. Call it once the app
    /// has come up far enough to prove the bundle — typically after the first
    /// screen renders.
    pub fn notify_ready(&mut self) -> Result<(), String> {
        let root = with_shared(|shared| shared.root.clone())
            .ok_or("Call Updater::launch before notify_ready")?;
        engine::Store::new(root).confirm()
    }
    /// Ask the update server whether there is something newer. Returns at once;
    /// poll [`state`](Updater::state) for the answer.
    pub fn start_check(&mut self) -> Result<(), String> {
        let (config, runtime_version, root, current) = with_shared(|shared| {
            let config = shared
                .config
                .clone()
                .ok_or("Call Updater::launch before start_check")?;
            if shared.busy {
                return Err("The updater is already checking or downloading");
            }
            shared.busy = true;
            shared.state = UpdaterState::Checking;
            let current = shared
                .active
                .as_ref()
                .map(|(version, _)| version.clone())
                .unwrap_or_else(|| config.embedded_version.clone());
            Ok((
                config,
                shared.runtime_version.clone().unwrap_or_default(),
                shared.root.clone().unwrap_or_default(),
                current,
            ))
        })?;
        Self::spawn(move || {
            let outcome = (|| {
                let native = Native::new()?;
                let store = engine::Store::new(root);
                let failed = store.load().failed;
                engine::check(
                    &config,
                    &runtime_version,
                    &current,
                    &failed,
                    TARGET,
                    std::env::consts::ARCH,
                    &native,
                    &store.scratch(),
                )
            })();
            with_shared(|shared| {
                shared.busy = false;
                match outcome {
                    Ok(Some(candidate)) => {
                        shared.state = UpdaterState::Available(candidate.update.clone());
                        shared.candidate = Some(candidate);
                    }
                    Ok(None) => {
                        shared.state = UpdaterState::UpToDate;
                        shared.candidate = None;
                    }
                    Err(error) => shared.state = UpdaterState::Failed(error),
                }
            });
        })
    }
    /// Download, verify, and stage the update the last check found. Returns at
    /// once; poll [`state`](Updater::state) for progress and the result.
    pub fn start_download(&mut self) -> Result<(), String> {
        let (config, runtime_version, root, candidate) = with_shared(|shared| {
            let config = shared
                .config
                .clone()
                .ok_or("Call Updater::launch before start_download")?;
            if shared.busy {
                return Err("The updater is already checking or downloading");
            }
            let candidate = shared
                .candidate
                .clone()
                .ok_or("There is no update to download: run a check first")?;
            shared.busy = true;
            shared.state = UpdaterState::Downloading {
                update: candidate.update.clone(),
                progress: DownloadProgress::default(),
            };
            Ok((
                config,
                shared.runtime_version.clone().unwrap_or_default(),
                shared.root.clone().unwrap_or_default(),
                candidate,
            ))
        })?;
        Self::spawn(move || {
            let update = candidate.update.clone();
            let outcome = (|| {
                let native = Native::new()?;
                let store = engine::Store::new(root);
                let mut report = |progress: DownloadProgress| {
                    with_shared(|shared| {
                        shared.state = UpdaterState::Downloading {
                            update: update.clone(),
                            progress,
                        };
                    });
                };
                engine::download(
                    &candidate,
                    &config,
                    &runtime_version,
                    &store,
                    &native,
                    &mut report,
                )
            })();
            with_shared(|shared| {
                shared.busy = false;
                shared.state = match outcome {
                    Ok(()) => {
                        shared.candidate = None;
                        UpdaterState::Ready(candidate.update)
                    }
                    Err(error) => UpdaterState::Failed(error),
                };
            });
        })
    }
    /// What the updater is doing now.
    pub fn state(&mut self) -> UpdaterState {
        with_shared(|shared| shared.state.clone())
    }
    /// Switch to the downloaded bundle without waiting for a relaunch, and
    /// return its directory.
    ///
    /// Only for an app that reads its bundle files on demand — through
    /// [`resolve_asset`](Updater::resolve_asset) in an asset handler, say — and
    /// can reload what it already showed. The bundle is on trial from now, so
    /// [`notify_ready`](Updater::notify_ready) still has to follow.
    pub fn apply_now(&mut self) -> Result<Option<PathBuf>, String> {
        let root = with_shared(|shared| shared.root.clone())
            .ok_or("Call Updater::launch before apply_now")?;
        let active = engine::Store::new(root).apply_pending()?;
        let dir = active.as_ref().map(|(_, dir)| dir.clone());
        with_shared(|shared| {
            shared.active = active;
            shared.state = UpdaterState::Idle;
        });
        Ok(dir)
    }
    /// Delete every downloaded bundle and go back to the embedded content.
    /// Anything already read from a bundle stays on screen until reloaded.
    pub fn reset(&mut self) -> Result<(), String> {
        let root =
            with_shared(|shared| shared.root.clone()).ok_or("Call Updater::launch before reset")?;
        engine::Store::new(root).reset()?;
        with_shared(|shared| {
            shared.active = None;
            shared.candidate = None;
            shared.state = UpdaterState::Idle;
        });
        Ok(())
    }
    /// Network and verification run on a thread of their own: a download can
    /// take minutes, and blocking a Dioxus thread for it would freeze the app.
    fn spawn(work: impl FnOnce() + Send + 'static) -> Result<(), String> {
        std::thread::Builder::new()
            .name("g3-updater".to_string())
            .spawn(work)
            .map(|_| ())
            .map_err(|error| {
                with_shared(|shared| {
                    shared.busy = false;
                    shared.state = UpdaterState::Failed(error.to_string());
                });
                format!("Failed to start the updater thread: {error}")
            })
    }
}
#[cfg(target_os = "android")]
const TARGET: &str = "android";
#[cfg(target_os = "ios")]
const TARGET: &str = "ios";
#[cfg(any(target_arch = "wasm32", target_os = "macos"))]
impl Updater {
    /// The inert updater.
    pub fn new() -> Self {
        Self
    }
    /// Always `None`: the embedded content is all there is.
    pub fn launch(&mut self, _config: UpdaterConfig) -> Result<Option<PathBuf>, String> {
        Ok(None)
    }
    /// Always `None`.
    pub fn bundle_dir(&mut self) -> Option<PathBuf> {
        None
    }
    /// Always `None`.
    pub fn current_version(&mut self) -> Option<String> {
        None
    }
    /// Always `None`: serve the embedded asset.
    pub fn resolve_asset(&mut self, _path: &str) -> Option<PathBuf> {
        None
    }
    /// No-op.
    pub fn notify_ready(&mut self) -> Result<(), String> {
        Ok(())
    }
    /// No-op: the state stays [`UpdaterState::Idle`].
    pub fn start_check(&mut self) -> Result<(), String> {
        Ok(())
    }
    /// No-op.
    pub fn start_download(&mut self) -> Result<(), String> {
        Ok(())
    }
    /// Always [`UpdaterState::Idle`].
    pub fn state(&mut self) -> UpdaterState {
        UpdaterState::Idle
    }
    /// Always `None`.
    pub fn apply_now(&mut self) -> Result<Option<PathBuf>, String> {
        Ok(None)
    }
    /// No-op.
    pub fn reset(&mut self) -> Result<(), String> {
        Ok(())
    }
}
#[cfg(any(
    target_arch = "wasm32",
    target_os = "android",
    target_os = "ios",
    target_os = "macos"
))]
impl Default for Updater {
    fn default() -> Self {
        Self::new()
    }
}
#[cfg(test)]
mod tests {
    use super::engine::{self, DownloadRequest, Store, Transport};
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::fs;
    use std::path::Path;
    // A throwaway key with a fixed seed, and manifests signed with it the way
    // `minisign -S` signs: prehashed BLAKE2b, with a trusted comment.
    const PUBLIC_KEY: &str = "untrusted comment: minisign public key EFCDAB8967452301\nRWQBI0VniavN7wOhB7/zzhC+HXDdGOdLwJln5NYwm6UNXx3chmQSVTG4\n";
    const PUBLIC_KEY_LINE: &str = "RWQBI0VniavN7wOhB7/zzhC+HXDdGOdLwJln5NYwm6UNXx3chmQSVTG4";
    const TAURI_PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXkgRUZDREFCODk2NzQ1MjMwMQpSV1FCSTBWbmlhdk43d09oQjcvenpoQytIWERkR09kTHdKbG41Tll3bTZVTlh4M2NobVFTVlRHNAo=";
    const MANIFEST_1_1: &str = r#"{"version":"1.1.0","runtime_version":"1.0.0","files":[{"path":"index.html","sha256":"ff2eca2f6c4b9b6970340e8aa384379e0cdc6fa5b69d8589614bc8b283207118","size":13},{"path":"css/app.css","sha256":"8dfb68c5781865682600b50d23b899aad50266d3ae120933504e64cda5781e3d","size":21}]}"#;
    const SIGNATURE_1_1: &str = "untrusted comment: signature from minisign secret key\nRUQBI0VniavN77NjGcAtGJUWAMcvC2YTJWkqVhRpaKj3tHJ7QLmZPG8SNC5LspXfHJMeGC1PWY/kmL7mLK4NSFX6TvX86YWCiAo=\ntrusted comment: timestamp:0\tfile:manifest.json\n2Y6s2X5LNlxoRHKxEpkyGDOKgypzYKhIa3YB279Uz5wkV4F6MD14jvw2zn2WeAZ3HPR3DN7w9VpTfhBt7O/pCQ==\n";
    // The same manifest signed legacy-style and wrapped in base64, which is
    // what `tauri signer sign` hands over.
    const TAURI_SIGNATURE_1_1: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIG1pbmlzaWduIHNlY3JldCBrZXkKUldRQkkwVm5pYXZON3pSZmZWcGpoZmViNlRaM1ZSQ0c1REIzNXBNUUZhU3hXNkVWeXBvdGQ2UTJzaVlvNGE4Y2tJYVFaSDJvUlB0dm5RcllPd1g5SHNCR0Iwa09tU2UwK2dFPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDowCWZpbGU6bWFuaWZlc3QuanNvbgpOa3pubVVRWUZDSmdwc1ZlWm9HVG5OY1dFeW5DUVNmSVBGRHFxS0lqMnVkY0paVkY1VGc3Mkt2dTlIRy9SNWZhdlJGUnRXNXA4dXhEMXJRMjBxcGhDdz09Cg==";
    const MANIFEST_1_2: &str = r#"{"version":"1.2.0","runtime_version":"1.0.0","files":[{"path":"index.html","sha256":"788f429e90efaf259122aa495ef906d3bfcd86eb026bc0faa8e8c5daebe4b451","size":13},{"path":"css/app.css","sha256":"8dfb68c5781865682600b50d23b899aad50266d3ae120933504e64cda5781e3d","size":21}]}"#;
    const SIGNATURE_1_2: &str = "untrusted comment: signature from minisign secret key\nRUQBI0VniavN70MTsTmGl0W27tH9Ig7csyle6bwys5MSyKUA+pFOmbyj26hCfNYYJ5jb+p/Ij9UFyioPZHXJw2XGpbku/rxhKgY=\ntrusted comment: timestamp:0\tfile:manifest.json\nz6yVvXK/TxeKzIyonHV/UuihKnY8wTiDytiTDMZffQ0fA4e5yDMDI06/xzq7O6iZ19o6yD9DrcMtBEk0VikfDQ==\n";
    const ENDPOINT: &str = "https://updates.test/{{target}}/{{current_version}}";
    /// Serves canned responses and remembers what was asked for.
    #[derive(Default)]
    struct FakeServer {
        routes: HashMap<String, (u16, Vec<u8>)>,
        requested: RefCell<Vec<String>>,
    }
    impl FakeServer {
        fn route(mut self, url: &str, status: u16, body: impl AsRef<[u8]>) -> Self {
            self.routes
                .insert(url.to_string(), (status, body.as_ref().to_vec()));
            self
        }
        fn requested(&self) -> Vec<String> {
            self.requested.borrow().clone()
        }
    }
    impl Transport for FakeServer {
        fn download(&self, request: &DownloadRequest<'_>) -> Result<u16, String> {
            self.requested.borrow_mut().push(request.url.to_string());
            let (status, body) = self
                .routes
                .get(request.url)
                .ok_or_else(|| format!("connection refused: {}", request.url))?;
            if (200..300).contains(status) && *status != 204 {
                fs::write(request.destination, body).map_err(|error| error.to_string())?;
            }
            Ok(*status)
        }
    }
    fn release(version: &str, signature: &str) -> String {
        serde_json::json!({
            "version": version,
            "notes": "Fixes",
            "pub_date": "2026-09-20T12:00:00Z",
            "platforms": {
                "ios-aarch64": {
                    "url": format!("https://cdn.test/{version}/manifest.json"),
                    "signature": signature,
                },
            },
        })
        .to_string()
    }
    fn server_for_1_1() -> FakeServer {
        FakeServer::default()
            .route(
                "https://updates.test/ios/1.0.0",
                200,
                release("1.1.0", SIGNATURE_1_1),
            )
            .route("https://cdn.test/1.1.0/manifest.json", 200, MANIFEST_1_1)
            .route("https://cdn.test/1.1.0/index.html", 200, "<h1>v1.1</h1>")
            .route(
                "https://cdn.test/1.1.0/css/app.css",
                200,
                "body { color: teal; }",
            )
    }
    fn config() -> UpdaterConfig {
        UpdaterConfig::new(PUBLIC_KEY, "1.0.0").endpoint(ENDPOINT)
    }
    /// A fresh directory per test, so tests running in parallel never share a
    /// store.
    fn temp_root(name: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("g3-updater-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        root
    }
    fn check(server: &FakeServer, store: &Store, current: &str) -> Option<engine::Candidate> {
        let failed = store.load().failed;
        engine::check(
            &config(),
            "1.0.0",
            current,
            &failed,
            "ios",
            "aarch64",
            server,
            &store.scratch(),
        )
        .expect("check should succeed")
    }
    fn download(
        server: &FakeServer,
        store: &Store,
        candidate: &engine::Candidate,
    ) -> Result<(), String> {
        engine::download(candidate, &config(), "1.0.0", store, server, &mut |_| {})
    }
    fn install_1_1(root: &Path) -> (FakeServer, Store) {
        let server = server_for_1_1();
        let store = Store::new(root.to_path_buf());
        store.launch("1.0.0", "1.0.0").unwrap();
        let candidate = check(&server, &store, "1.0.0").expect("1.1.0 should be offered");
        download(&server, &store, &candidate).expect("1.1.0 should install");
        (server, store)
    }
    #[test]
    fn a_check_offers_a_newer_signed_release_for_this_platform() {
        let root = temp_root("offer");
        let server = server_for_1_1();
        let store = Store::new(root.clone());
        let candidate = check(&server, &store, "1.0.0").expect("1.1.0 should be offered");
        assert_eq!(candidate.update.version, "1.1.0");
        assert_eq!(candidate.update.current_version, "1.0.0");
        assert_eq!(candidate.update.notes.as_deref(), Some("Fixes"));
        assert_eq!(
            candidate.manifest_url,
            "https://cdn.test/1.1.0/manifest.json"
        );
        assert_eq!(server.requested(), ["https://updates.test/ios/1.0.0"]);
        // Nothing newer than what is running is an update.
        let server = FakeServer::default().route(
            "https://updates.test/ios/1.1.0",
            200,
            release("1.1.0", SIGNATURE_1_1),
        );
        assert_eq!(check(&server, &store, "1.1.0"), None);
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn a_204_means_no_update_and_a_dead_endpoint_falls_through_to_the_next() {
        let root = temp_root("fallthrough");
        let store = Store::new(root.clone());
        let server = FakeServer::default().route("https://updates.test/ios/1.0.0", 204, "");
        assert_eq!(check(&server, &store, "1.0.0"), None);
        let config = UpdaterConfig::new(PUBLIC_KEY, "1.0.0")
            .endpoint("https://down.test/{{target}}")
            .endpoint(ENDPOINT);
        let server = server_for_1_1();
        let offered = engine::check(
            &config,
            "1.0.0",
            "1.0.0",
            &[],
            "ios",
            "aarch64",
            &server,
            &store.scratch(),
        )
        .unwrap();
        assert_eq!(
            offered.map(|candidate| candidate.update.version).as_deref(),
            Some("1.1.0")
        );
        assert_eq!(
            server.requested(),
            ["https://down.test/ios", "https://updates.test/ios/1.0.0"]
        );
        let error = engine::check(
            &UpdaterConfig::new(PUBLIC_KEY, "1.0.0").endpoint("https://down.test/"),
            "1.0.0",
            "1.0.0",
            &[],
            "ios",
            "aarch64",
            &server,
            &store.scratch(),
        )
        .unwrap_err();
        assert!(error.contains("No update endpoint answered"), "{error}");
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn a_platform_missing_from_the_release_is_an_error_not_a_silent_skip() {
        let root = temp_root("platform");
        let store = Store::new(root.clone());
        let server = FakeServer::default().route(
            "https://updates.test/android/1.0.0",
            200,
            release("1.1.0", SIGNATURE_1_1),
        );
        let error = engine::check(
            &config(),
            "1.0.0",
            "1.0.0",
            &[],
            "android",
            "aarch64",
            &server,
            &store.scratch(),
        )
        .unwrap_err();
        assert!(error.contains("android-aarch64"), "{error}");
        // A dynamic server answering for one platform uses top-level fields.
        let dynamic = serde_json::json!({
            "version": "1.1.0",
            "url": "https://cdn.test/1.1.0/manifest.json",
            "signature": SIGNATURE_1_1,
        });
        let server = FakeServer::default().route(
            "https://updates.test/android/1.0.0",
            200,
            dynamic.to_string(),
        );
        let offered = engine::check(
            &config(),
            "1.0.0",
            "1.0.0",
            &[],
            "android",
            "aarch64",
            &server,
            &store.scratch(),
        )
        .unwrap();
        assert!(offered.is_some());
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn a_downloaded_bundle_installs_at_the_next_launch_on_trial() {
        let root = temp_root("install");
        let (_, store) = install_1_1(&root);
        assert_eq!(store.load().pending.as_deref(), Some("1.1.0"));
        let (version, dir) = store
            .launch("1.0.0", "1.0.0")
            .unwrap()
            .expect("1.1.0 active");
        assert_eq!(version, "1.1.0");
        assert_eq!(
            fs::read_to_string(dir.join("index.html")).unwrap(),
            "<h1>v1.1</h1>"
        );
        assert_eq!(
            engine::resolve(&dir, "/css/app.css?v=2").map(|path| fs::read(path).unwrap()),
            Some(b"body { color: teal; }".to_vec())
        );
        let state = store.load();
        assert!(state.trial);
        assert_eq!(state.trial_launches, 1);
        store.confirm().unwrap();
        let state = store.load();
        assert!(!state.trial);
        // Confirmed, it survives any number of launches.
        for _ in 0..3 {
            let active = store.launch("1.0.0", "1.0.0").unwrap();
            assert_eq!(active.map(|(version, _)| version).as_deref(), Some("1.1.0"));
        }
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn a_bundle_that_never_confirms_is_rolled_back_and_never_offered_again() {
        let root = temp_root("rollback");
        let (server, store) = install_1_1(&root);
        assert!(store.launch("1.0.0", "1.0.0").unwrap().is_some());
        // The app died before notify_ready. The next launch goes back.
        assert_eq!(store.launch("1.0.0", "1.0.0").unwrap(), None);
        let state = store.load();
        assert_eq!(state.failed, ["1.1.0"]);
        assert!(!store.bundle_dir("1.1.0").exists());
        assert_eq!(check(&server, &store, "1.0.0"), None);
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn rollback_returns_to_the_last_confirmed_bundle() {
        let root = temp_root("rollback-previous");
        let (_, store) = install_1_1(&root);
        store.launch("1.0.0", "1.0.0").unwrap();
        store.confirm().unwrap();
        let server = FakeServer::default()
            .route(
                "https://updates.test/ios/1.1.0",
                200,
                release("1.2.0", SIGNATURE_1_2),
            )
            .route("https://cdn.test/1.2.0/manifest.json", 200, MANIFEST_1_2)
            .route("https://cdn.test/1.2.0/index.html", 200, "<h1>v1.2</h1>");
        let candidate = check(&server, &store, "1.1.0").expect("1.2.0 should be offered");
        download(&server, &store, &candidate).unwrap();
        // Only the changed file was fetched; the stylesheet came from 1.1.0.
        assert!(
            !server
                .requested()
                .iter()
                .any(|url| url.ends_with("app.css"))
        );
        let active = store.launch("1.0.0", "1.0.0").unwrap();
        assert_eq!(active.map(|(version, _)| version).as_deref(), Some("1.2.0"));
        let active = store.launch("1.0.0", "1.0.0").unwrap();
        assert_eq!(active.map(|(version, _)| version).as_deref(), Some("1.1.0"));
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn a_file_that_does_not_match_its_hash_installs_nothing() {
        let root = temp_root("tamper");
        let server =
            server_for_1_1().route("https://cdn.test/1.1.0/index.html", 200, "<h1>evil</h1>");
        let store = Store::new(root.clone());
        let candidate = check(&server, &store, "1.0.0").unwrap();
        let error = download(&server, &store, &candidate).unwrap_err();
        assert!(error.contains("index.html"), "{error}");
        assert_eq!(store.load().pending, None);
        assert!(!store.bundle_dir("1.1.0").exists());
        assert!(!root.join("staging/1.1.0").exists());
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn a_manifest_that_does_not_match_its_signature_is_refused_before_parsing() {
        let root = temp_root("signature");
        let forged = MANIFEST_1_1.replace("1.1.0", "1.1.1");
        let server = server_for_1_1().route("https://cdn.test/1.1.0/manifest.json", 200, forged);
        let store = Store::new(root.clone());
        let candidate = check(&server, &store, "1.0.0").unwrap();
        let error = download(&server, &store, &candidate).unwrap_err();
        assert!(error.contains("signature verification"), "{error}");
        assert!(
            !server
                .requested()
                .iter()
                .any(|url| url.ends_with("index.html"))
        );
        // Signed by the right key, but for a different build of the app.
        let server = server_for_1_1();
        let error = engine::download(&candidate, &config(), "2.0.0", &store, &server, &mut |_| {})
            .unwrap_err();
        assert!(error.contains("runtime"), "{error}");
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn keys_and_signatures_are_accepted_in_every_form_minisign_and_tauri_write() {
        let bytes = MANIFEST_1_1.as_bytes();
        for key in [PUBLIC_KEY, PUBLIC_KEY_LINE, TAURI_PUBLIC_KEY] {
            engine::verify_signature(key, SIGNATURE_1_1, bytes).unwrap();
            engine::verify_signature(key, TAURI_SIGNATURE_1_1, bytes).unwrap();
        }
        assert!(engine::verify_signature(PUBLIC_KEY, SIGNATURE_1_2, bytes).is_err());
        assert!(engine::public_key("not a key").is_err());
    }
    #[test]
    fn a_new_native_runtime_discards_every_downloaded_bundle() {
        let root = temp_root("runtime");
        let (_, store) = install_1_1(&root);
        assert!(store.launch("1.0.0", "1.0.0").unwrap().is_some());
        store.confirm().unwrap();
        assert_eq!(store.launch("2.0.0", "1.0.0").unwrap(), None);
        assert!(!store.bundle_dir("1.1.0").exists());
        // So does a build that embeds content at least as new as the bundle.
        let embedded = temp_root("embedded");
        let (_, store) = install_1_1(&embedded);
        assert_eq!(store.launch("1.0.0", "1.1.0").unwrap(), None);
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(embedded);
    }
    #[test]
    fn apply_now_switches_immediately_and_stays_on_trial() {
        let root = temp_root("apply");
        let (_, store) = install_1_1(&root);
        let (version, _) = store.apply_pending().unwrap().expect("1.1.0 active");
        assert_eq!(version, "1.1.0");
        assert!(store.apply_pending().is_err());
        // This process counted as its first launch, so a relaunch without a
        // confirmation rolls it back.
        assert_eq!(store.launch("1.0.0", "1.0.0").unwrap(), None);
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn reset_goes_back_to_the_embedded_content() {
        let root = temp_root("reset");
        let (_, store) = install_1_1(&root);
        store.launch("1.0.0", "1.0.0").unwrap();
        store.reset().unwrap();
        assert_eq!(store.launch("1.0.0", "1.0.0").unwrap(), None);
        assert!(!store.bundle_dir("1.1.0").exists());
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn paths_cannot_climb_out_of_the_bundle() {
        for path in [
            "../x",
            "a/../../x",
            "/etc/passwd",
            "a//b",
            "./a",
            "a\\b",
            "C:/x",
            "",
        ] {
            assert!(!engine::is_safe_relative_path(path), "{path}");
        }
        assert!(engine::is_safe_relative_path("assets/app-1.2.css"));
        let manifest = MANIFEST_1_1.replace("css/app.css", "../app.css");
        assert!(engine::parse_manifest(manifest.as_bytes()).is_err());
        let manifest = MANIFEST_1_1.replace("css/app.css", ".g3-manifest.json");
        assert!(engine::parse_manifest(manifest.as_bytes()).is_err());
        let manifest = MANIFEST_1_1.replace("css/app.css", "index.html");
        assert!(engine::parse_manifest(manifest.as_bytes()).is_err());
        let root = temp_root("resolve");
        let (_, store) = install_1_1(&root);
        let (_, dir) = store.launch("1.0.0", "1.0.0").unwrap().unwrap();
        assert!(engine::resolve(&dir, "/../state.json").is_none());
        assert!(engine::resolve(&dir, engine::MANIFEST_FILE).is_none());
        assert!(engine::resolve(&dir, "missing.js").is_none());
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn endpoints_fill_the_same_placeholders_as_tauri() {
        assert_eq!(
            engine::fill_placeholders(
                "https://u.test/{{target}}-{{arch}}/{{runtime_version}}/{{current_version}}",
                "1.2.0",
                "7",
                "android",
                "aarch64",
            ),
            "https://u.test/android-aarch64/7/1.2.0"
        );
    }
    #[test]
    fn versions_compare_as_semver() {
        assert!(engine::is_newer("1.10.0", "1.9.0"));
        assert!(engine::is_newer("1.0.0", "1.0.0-beta.2"));
        assert!(!engine::is_newer("1.0.0", "1.0.0"));
        assert!(!engine::is_newer("garbage", "1.0.0"));
    }
    #[test]
    fn bundle_files_are_served_with_types_webkit_accepts() {
        assert_eq!(bundle_content_type("app.WASM"), "application/wasm");
        assert_eq!(
            bundle_content_type("a/b.mjs"),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            bundle_content_type("noextension"),
            "application/octet-stream"
        );
    }
}
