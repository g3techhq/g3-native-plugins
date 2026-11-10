//! Updating the installed app itself, in the manner of Tauri's updater: ask an
//! update server whether there is a newer release, download the build for
//! this platform, check it against a public key compiled into the app, and
//! hand it to the platform's installer.
//!
//! | Platform | Release file | Installed by |
//! | --- | --- | --- |
//! | Android | `.apk` | `PackageInstaller`; silent when the system allows it |
//! | Windows | NSIS `.exe` or `.msi`, bare or zipped | The installer, after the app exits |
//! | macOS | `.app.tar.gz` | Replacing the `.app` in place |
//! | Linux | `.AppImage` (bare or `.tar.gz`), `.deb` | Replacing the file; `pkexec dpkg -i` |
//! | iOS | an ad hoc or enterprise `manifest.plist`, or a store link | iOS, through `itms-services://` |
//!
//! The web build is inert: a deploy already is the update.

// Each platform module provides the same `Native` and `Step`: a transport,
// where to keep downloads, how to find itself in a release, and how to
// install. Everything that decides whether to trust a release is here and in
// `engine` and `release`, the same on every platform.
#[cfg(any(
    target_os = "android",
    target_os = "ios",
    windows,
    target_os = "macos",
    target_os = "linux",
    test
))]
mod engine;

#[cfg(any(
    target_os = "android",
    target_os = "ios",
    windows,
    target_os = "macos",
    target_os = "linux",
    test
))]
mod release;

#[cfg(any(target_os = "ios", test))]
mod ios_link;

#[cfg(target_os = "android")]
#[path = "android.rs"]
mod platform;

#[cfg(target_os = "ios")]
#[path = "ios.rs"]
mod platform;

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
#[path = "desktop.rs"]
mod platform;

#[cfg(any(
    target_os = "android",
    target_os = "ios",
    windows,
    target_os = "macos",
    target_os = "linux"
))]
use std::path::PathBuf;

/// Where to look for updates and how to tell a genuine one.
///
/// ```rust,ignore
/// let config = UpdaterConfig::new(UPDATER_PUBLIC_KEY, env!("CARGO_PKG_VERSION"))
///     .endpoint("https://updates.example.com/{{target}}/{{arch}}/{{current_version}}");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdaterConfig {
    /// Update-check URLs, tried in order until one answers. Each may contain
    /// `{{current_version}}`, `{{target}}` (`android`, `ios`, `windows`,
    /// `darwin` or `linux`) and `{{arch}}` (`aarch64`, `x86_64`, `armv7`,
    /// `i686`), filled in before the request: the placeholders Tauri's
    /// updater uses.
    pub endpoints: Vec<String>,
    /// The minisign public key releases are signed with. Accepts the contents
    /// of a `minisign.pub` file, its bare key line, or the base64-wrapped form
    /// `tauri signer generate` prints.
    pub pubkey: String,
    /// The version running now, as semver: usually
    /// `env!("CARGO_PKG_VERSION")`, which is what `dx` builds every platform's
    /// version from. Only a release newer than this is offered.
    pub current_version: String,
    /// Extra request headers sent with every request, for an update server that
    /// wants a token or a channel name.
    pub headers: Vec<(String, String)>,
    /// How long a request may wait for the server before it is abandoned.
    pub timeout_ms: u32,
}

impl UpdaterConfig {
    /// A configuration with no endpoints yet: add them with
    /// [`endpoint`](UpdaterConfig::endpoint).
    pub fn new(pubkey: impl Into<String>, current_version: impl Into<String>) -> Self {
        Self {
            endpoints: Vec::new(),
            pubkey: pubkey.into(),
            current_version: current_version.into(),
            headers: Vec::new(),
            timeout_ms: 30_000,
        }
    }

    /// Add an update-check URL, tried after any added before it.
    pub fn endpoint(mut self, url: impl Into<String>) -> Self {
        self.endpoints.push(url.into());
        self
    }

    /// Send a header with every update request.
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// Change the request timeout from its default of 30 seconds.
    pub fn timeout_ms(mut self, timeout_ms: u32) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }
}

/// A newer release the update server offered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Update {
    /// The version on offer.
    pub version: String,
    /// The version running now.
    pub current_version: String,
    /// Release notes, if the server sent any.
    pub notes: Option<String>,
    /// Publication date as the server wrote it, conventionally RFC 3339.
    pub pub_date: Option<String>,
    /// The update is in a store (App Store, TestFlight, Google Play, the
    /// Microsoft Store...): [`Updater::install`] opens its page, with nothing
    /// to download first.
    pub store: bool,
}

/// How far a download has got.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DownloadProgress {
    /// Bytes received so far.
    pub bytes_done: u64,
    /// The download's size, when the server said.
    pub bytes_total: Option<u64>,
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
    /// A newer release is on offer; fetch it with
    /// [`Updater::start_download`], or, when [`Update::store`] is set, open
    /// its store page with [`Updater::install`].
    Available(Update),
    /// The release is downloading.
    Downloading {
        /// The release being downloaded.
        update: Update,
        /// How far it has got.
        progress: DownloadProgress,
    },
    /// Downloaded and verified; install it with [`Updater::install`].
    Ready(Update),
    /// Android only: the user has not let this app install apps. The system
    /// screen that allows it has been opened; call [`Updater::install`] again
    /// once the app is back in front.
    NeedsPermission(Update),
    /// Handed to the platform. On Android the system may ask the user to
    /// confirm; either way the app is stopped and replaced when it installs.
    /// On iOS the system takes over. On desktop the app exits before this is
    /// ever seen.
    Installing(Update),
    /// The last step failed. Nothing was installed.
    Failed(String),
}

#[cfg(any(
    target_os = "android",
    target_os = "ios",
    windows,
    target_os = "macos",
    target_os = "linux"
))]
mod shared {
    use super::release::Candidate;
    use super::{PathBuf, UpdaterConfig, UpdaterState};
    use std::sync::Mutex;

    #[derive(Default)]
    pub(super) struct Shared {
        pub config: Option<UpdaterConfig>,
        pub state: UpdaterState,
        pub candidate: Option<Candidate>,
        /// What the download step produced: `Some(None)` when the platform
        /// had nothing to download (an iOS store link).
        pub downloaded: Option<Option<PathBuf>>,
        pub busy: bool,
    }

    /// Process-wide, so every [`Updater`](super::Updater) handle (the one in
    /// the provider, one an app made itself) sees the same check.
    static SHARED: Mutex<Option<Shared>> = Mutex::new(None);

    pub(super) fn with<T>(action: impl FnOnce(&mut Shared) -> T) -> T {
        let mut guard = SHARED.lock().unwrap_or_else(|error| error.into_inner());
        action(guard.get_or_insert_with(Shared::default))
    }
}

/// Updates the installed app: checks an update server, downloads the build
/// for this platform, verifies its minisign signature, and installs it.
///
/// ```rust,ignore
/// let mut updater = Updater::new();
/// updater.configure(
///     UpdaterConfig::new(UPDATER_PUBLIC_KEY, env!("CARGO_PKG_VERSION"))
///         .endpoint("https://updates.example.com/{{target}}/{{arch}}/{{current_version}}"),
/// )?;
/// updater.start_check()?;
/// // ...then poll:
/// match updater.state() {
///     UpdaterState::Available(_) => updater.start_download()?,
///     UpdaterState::Ready(_) => updater.install()?, // offer it first
///     _ => {}
/// }
/// ```
///
/// Every handle shares one process-wide state, so `Updater::new()` anywhere
/// and the `updater` field of [`NativePlugins`](crate::NativePlugins) are the
/// same updater. On Windows and Linux, which have no `NativePlugins`, use
/// `Updater::new()`.
///
/// Nothing is installed that the configured key did not sign: the signature
/// is checked over the downloaded file, before any installer sees it. Each
/// platform then applies its own check on top (an APK must be signed by the
/// same key as the installed app; iOS checks its provisioning).
pub struct Updater;

impl Default for Updater {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(
    target_os = "android",
    target_os = "ios",
    windows,
    target_os = "macos",
    target_os = "linux"
))]
impl Updater {
    /// A handle on the process-wide updater.
    pub fn new() -> Self {
        Self
    }

    /// Set where to check and which key to trust. Call it before the first
    /// check; calling it again replaces the configuration.
    pub fn configure(&mut self, config: UpdaterConfig) -> Result<(), String> {
        release::public_key(&config.pubkey)?;
        semver::Version::parse(config.current_version.trim_start_matches('v')).map_err(
            |error| {
                format!(
                    "The current version {} is not semver: {error}",
                    config.current_version
                )
            },
        )?;
        platform::Native::clean_up();
        shared::with(|shared| shared.config = Some(config));
        Ok(())
    }

    /// The configured current version, or `None` before
    /// [`configure`](Updater::configure).
    pub fn current_version(&mut self) -> Option<String> {
        shared::with(|shared| {
            shared
                .config
                .as_ref()
                .map(|config| config.current_version.clone())
        })
    }

    /// Ask the update server whether there is something newer. Returns at once;
    /// poll [`state`](Updater::state) for the answer.
    pub fn start_check(&mut self) -> Result<(), String> {
        let config = Self::begin(UpdaterState::Checking, "start_check")?;
        Self::spawn(move || {
            let outcome = (|| {
                let native = platform::Native::new()?;
                let found = engine::check(
                    &config,
                    &platform::Native::key(),
                    &native,
                    &native.data_dir()?,
                )?;
                found
                    .map(|mut candidate| {
                        candidate.update.store =
                            platform::Native::download_url(&candidate)?.is_none();
                        Ok(candidate)
                    })
                    .transpose()
            })();
            shared::with(|shared| {
                shared.busy = false;
                shared.downloaded = None;
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

    /// Download and verify the release the last check found. Returns at once;
    /// poll [`state`](Updater::state) for progress and the result.
    pub fn start_download(&mut self) -> Result<(), String> {
        let candidate = shared::with(|shared| shared.candidate.clone())
            .ok_or("There is no update to download: run a check first")?;
        let update = candidate.update.clone();
        let config = Self::begin(
            UpdaterState::Downloading {
                update: update.clone(),
                progress: DownloadProgress::default(),
            },
            "start_download",
        )?;
        Self::spawn(move || {
            let outcome = (|| {
                let native = platform::Native::new()?;
                let Some(url) = platform::Native::download_url(&candidate)? else {
                    return Ok(None);
                };
                let mut report = |bytes_done: u64, bytes_total: Option<u64>| {
                    shared::with(|shared| {
                        shared.state = UpdaterState::Downloading {
                            update: update.clone(),
                            progress: DownloadProgress {
                                bytes_done,
                                bytes_total,
                            },
                        };
                    });
                };
                let target = release::Candidate {
                    url,
                    ..candidate.clone()
                };
                let directory = native.data_dir()?.join("download");
                engine::fetch(&target, &config, &native, &directory, &mut report).map(Some)
            })();
            shared::with(|shared| {
                shared.busy = false;
                match outcome {
                    Ok(file) => {
                        shared.downloaded = Some(file);
                        shared.state = UpdaterState::Ready(candidate.update);
                    }
                    Err(error) => shared.state = UpdaterState::Failed(error),
                }
            });
        })
    }

    /// Install the downloaded release, or open the store page of one in a
    /// store ([`Update::store`]), which needs no download first and leaves the
    /// state at [`UpdaterState::Available`].
    ///
    /// - **Android** (with the `updater-install` feature): hands the APK to the
    ///   system installer. Android 12 and later install it without asking
    ///   when the app declares `UPDATE_PACKAGES_WITHOUT_USER_ACTION` (that
    ///   feature does) and targets a recent SDK; otherwise the system asks
    ///   the user. The first time, the user must allow this app to install
    ///   apps: [`UpdaterState::NeedsPermission`] opens that screen.
    /// - **Windows, macOS, Linux**: replaces the app and starts the new
    ///   version. On success this **exits the process**, so save anything that
    ///   needs saving first.
    /// - **iOS**: opens the install link, and iOS takes over.
    pub fn install(&mut self) -> Result<(), String> {
        let (candidate, file) = shared::with(|shared| {
            let candidate = shared.candidate.clone().ok_or("There is no update")?;
            let file = match shared.downloaded.clone() {
                Some(file) => file,
                None if candidate.update.store => None,
                None => return Err("There is no downloaded update: download one first".to_string()),
            };
            Ok::<_, String>((candidate, file))
        })?;
        let update = candidate.update.clone();
        Self::begin(UpdaterState::Installing(update.clone()), "install")?;
        Self::spawn(move || {
            let outcome = platform::Native::new()
                .and_then(|native| native.install(file.as_deref(), &candidate));
            shared::with(|shared| {
                shared.busy = false;
                shared.state = match outcome {
                    Ok(platform::Step::Started) => UpdaterState::Installing(update),
                    // The store is on screen; the update is still on offer
                    // when the user comes back without taking it.
                    Ok(platform::Step::Opened) => UpdaterState::Available(update),
                    Ok(platform::Step::NeedsPermission) => UpdaterState::NeedsPermission(update),
                    Err(error) => UpdaterState::Failed(error),
                };
            });
        })
    }

    /// What the updater is doing now.
    pub fn state(&mut self) -> UpdaterState {
        let state = shared::with(|shared| shared.state.clone());
        if let UpdaterState::Installing(update) = &state
            && let Some(next) = platform::Native::install_progress(update)
        {
            shared::with(|shared| shared.state = next.clone());
            return next;
        }
        state
    }

    /// Mark the updater busy with `state`, or refuse if it already is.
    fn begin(state: UpdaterState, caller: &str) -> Result<UpdaterConfig, String> {
        shared::with(|shared| {
            let config = shared
                .config
                .clone()
                .ok_or_else(|| format!("Call Updater::configure before {caller}"))?;
            if shared.busy {
                return Err("The updater is already busy".to_string());
            }
            shared.busy = true;
            shared.state = state;
            Ok(config)
        })
    }

    /// Network and installing run on a thread of their own: a download can
    /// take minutes, and blocking a Dioxus thread for it would freeze the app.
    fn spawn(work: impl FnOnce() + Send + 'static) -> Result<(), String> {
        std::thread::Builder::new()
            .name("g3-updater".to_string())
            .spawn(work)
            .map(|_| ())
            .map_err(|error| {
                shared::with(|shared| {
                    shared.busy = false;
                    shared.state = UpdaterState::Failed(error.to_string());
                });
                format!("Failed to start the updater thread: {error}")
            })
    }
}

#[cfg(not(any(
    target_os = "android",
    target_os = "ios",
    windows,
    target_os = "macos",
    target_os = "linux"
)))]
impl Updater {
    /// The inert updater: the web build, where a deploy is the update.
    pub fn new() -> Self {
        Self
    }

    /// No-op.
    pub fn configure(&mut self, _config: UpdaterConfig) -> Result<(), String> {
        Ok(())
    }

    /// Always `None`.
    pub fn current_version(&mut self) -> Option<String> {
        None
    }

    /// No-op: the state stays [`UpdaterState::Idle`].
    pub fn start_check(&mut self) -> Result<(), String> {
        Ok(())
    }

    /// No-op.
    pub fn start_download(&mut self) -> Result<(), String> {
        Ok(())
    }

    /// No-op.
    pub fn install(&mut self) -> Result<(), String> {
        Ok(())
    }

    /// Always [`UpdaterState::Idle`].
    pub fn state(&mut self) -> UpdaterState {
        UpdaterState::Idle
    }
}
