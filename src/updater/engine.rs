//! Checking and downloading, behind a [`Transport`] so it runs (and is
//! tested) the same on the host as on a phone. Installing is each platform's
//! own business; this ends with a verified file on disk.

use super::UpdaterConfig;
use super::release::{self, Candidate, PlatformKey};
use std::fs;
use std::path::{Path, PathBuf};

pub(super) struct DownloadRequest<'a> {
    pub url: &'a str,
    pub destination: &'a Path,
    pub headers: &'a [(String, String)],
    pub timeout_ms: u32,
}

/// Fetches one URL to a file and reports the HTTP status. The body is
/// written only for a 2xx other than 204; anything else leaves no file.
/// `progress` hears bytes so far and the total when the server gave one.
pub(super) trait Transport {
    fn download(
        &self,
        request: &DownloadRequest<'_>,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<u16, String>;
}

/// Ask each endpoint in turn. A 204 is a definite "nothing new" and ends the
/// search; an error moves on to the next endpoint, and only when all of them
/// fail is the check a failure.
pub(super) fn check(
    config: &UpdaterConfig,
    key: &PlatformKey<'_>,
    transport: &dyn Transport,
    scratch: &Path,
) -> Result<Option<Candidate>, String> {
    if config.endpoints.is_empty() {
        return Err("The updater has no endpoints configured".to_string());
    }
    fs::create_dir_all(scratch)
        .map_err(|error| format!("Failed to create the updater directory: {error}"))?;
    let response = scratch.join("release.json");
    let mut last_error = String::new();
    for endpoint in &config.endpoints {
        let url = release::fill_placeholders(endpoint, &config.current_version, key);
        let _ = fs::remove_file(&response);
        let request = DownloadRequest {
            url: &url,
            destination: &response,
            headers: &config.headers,
            timeout_ms: config.timeout_ms,
        };
        let status = match transport.download(&request, &mut |_, _| {}) {
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
        return release::choose(&body, &config.current_version, key);
    }
    Err(format!(
        "No update endpoint answered. Last error: {last_error}"
    ))
}

/// Download the offered build into `directory` and check it against the
/// release's signature. Only a file that passes is returned; one that does
/// not is deleted before anything could open it.
pub(super) fn fetch(
    candidate: &Candidate,
    config: &UpdaterConfig,
    transport: &dyn Transport,
    directory: &Path,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<PathBuf, String> {
    let signature = candidate
        .signature
        .as_deref()
        .ok_or("The release gives no signature for this build, so it cannot be trusted")?;
    // Checked before the download, so a bad key fails in a second, not after
    // a hundred megabytes.
    release::public_key(&config.pubkey)?;
    // Earlier downloads are of no use once a newer one is on offer.
    let _ = fs::remove_dir_all(directory);
    fs::create_dir_all(directory)
        .map_err(|error| format!("Failed to create the download directory: {error}"))?;
    let destination = directory.join(file_name(&candidate.url));
    let status = transport.download(
        &DownloadRequest {
            url: &candidate.url,
            destination: &destination,
            headers: &config.headers,
            timeout_ms: config.timeout_ms,
        },
        progress,
    )?;
    if status != 200 {
        let _ = fs::remove_file(&destination);
        return Err(format!("Downloading the update answered HTTP {status}"));
    }
    if let Err(error) = release::verify_file(&config.pubkey, signature, &destination) {
        let _ = fs::remove_file(&destination);
        return Err(error);
    }
    Ok(destination)
}

/// The URL's last path segment, kept to safe characters: installers are
/// told apart by extension (`.msi`, `.apk`), so it is worth keeping, but it
/// came from the network and must not name a path.
pub(super) fn file_name(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or_default();
    let last = path.rsplit('/').next().unwrap_or_default();
    let cleaned: String = last
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .collect();
    let cleaned = cleaned.trim_start_matches('.');
    if cleaned.is_empty() {
        "update.bin".to_string()
    } else {
        cleaned.to_string()
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::updater::release::tests::{PUBLIC_KEY, SIGNATURE, SIGNED};
    use std::cell::RefCell;
    use std::collections::HashMap;

    const ENDPOINT: &str = "https://updates.test/{{target}}/{{current_version}}";

    const ANDROID: PlatformKey<'static> = PlatformKey {
        target: "android",
        arch: "aarch64",
        installers: &[],
    };

    /// Serves canned responses and remembers what was asked for.
    #[derive(Default)]
    pub(crate) struct FakeServer {
        routes: HashMap<String, (u16, Vec<u8>)>,
        requested: RefCell<Vec<String>>,
    }

    impl FakeServer {
        pub(crate) fn route(mut self, url: &str, status: u16, body: impl AsRef<[u8]>) -> Self {
            self.routes
                .insert(url.to_string(), (status, body.as_ref().to_vec()));
            self
        }

        fn requested(&self) -> Vec<String> {
            self.requested.borrow().clone()
        }
    }

    impl Transport for FakeServer {
        fn download(
            &self,
            request: &DownloadRequest<'_>,
            progress: &mut dyn FnMut(u64, Option<u64>),
        ) -> Result<u16, String> {
            self.requested.borrow_mut().push(request.url.to_string());
            let (status, body) = self
                .routes
                .get(request.url)
                .ok_or_else(|| format!("connection refused: {}", request.url))?;
            if (200..300).contains(status) && *status != 204 {
                fs::write(request.destination, body).map_err(|error| error.to_string())?;
                progress(body.len() as u64, Some(body.len() as u64));
            }
            Ok(*status)
        }
    }

    fn release(signature: &str) -> String {
        serde_json::json!({
            "version": "1.1.0",
            "platforms": {
                "android": { "url": "https://cdn.test/1.1.0/Tawny.apk", "signature": signature },
            },
        })
        .to_string()
    }

    fn config() -> UpdaterConfig {
        UpdaterConfig::new(PUBLIC_KEY, "1.0.0").endpoint(ENDPOINT)
    }

    fn temp(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("g3-engine-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        root
    }

    #[test]
    fn a_204_means_no_update_and_a_dead_endpoint_falls_through_to_the_next() {
        let scratch = temp("fallthrough");
        let server = FakeServer::default().route("https://updates.test/android/1.0.0", 204, "");
        assert_eq!(check(&config(), &ANDROID, &server, &scratch).unwrap(), None);
        let config = UpdaterConfig::new(PUBLIC_KEY, "1.0.0")
            .endpoint("https://down.test/{{target}}")
            .endpoint(ENDPOINT);
        let server = FakeServer::default().route(
            "https://updates.test/android/1.0.0",
            200,
            release(SIGNATURE),
        );
        let offered = check(&config, &ANDROID, &server, &scratch)
            .unwrap()
            .unwrap();
        assert_eq!(offered.update.version, "1.1.0");
        assert_eq!(
            server.requested(),
            [
                "https://down.test/android",
                "https://updates.test/android/1.0.0"
            ]
        );
        let config = UpdaterConfig::new(PUBLIC_KEY, "1.0.0").endpoint("https://down.test/");
        let error = check(&config, &ANDROID, &server, &scratch).unwrap_err();
        assert!(error.contains("No update endpoint answered"), "{error}");
        let _ = fs::remove_dir_all(scratch);
    }

    #[test]
    fn a_download_is_kept_only_when_its_signature_holds() {
        let directory = temp("fetch");
        let server = FakeServer::default()
            .route(
                "https://updates.test/android/1.0.0",
                200,
                release(SIGNATURE),
            )
            .route("https://cdn.test/1.1.0/Tawny.apk", 200, SIGNED);
        let candidate = check(&config(), &ANDROID, &server, &directory)
            .unwrap()
            .unwrap();
        let mut seen = (0, None);
        let path = fetch(
            &candidate,
            &config(),
            &server,
            &directory,
            &mut |done, total| seen = (done, total),
        )
        .unwrap();
        assert_eq!(path, directory.join("Tawny.apk"));
        assert_eq!(seen, (SIGNED.len() as u64, Some(SIGNED.len() as u64)));
        // Tampered in transit: refused and deleted.
        let server = FakeServer::default().route(
            "https://cdn.test/1.1.0/Tawny.apk",
            200,
            SIGNED.replace("1.1.0", "6.6.6"),
        );
        let error = fetch(&candidate, &config(), &server, &directory, &mut |_, _| {}).unwrap_err();
        assert!(error.contains("signature verification"), "{error}");
        assert!(!directory.join("Tawny.apk").exists());
        // Unsigned: never downloaded at all.
        let unsigned = Candidate {
            signature: None,
            ..candidate
        };
        let server = FakeServer::default();
        assert!(fetch(&unsigned, &config(), &server, &directory, &mut |_, _| {}).is_err());
        assert!(server.requested().is_empty());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn file_names_from_the_network_cannot_name_a_path() {
        assert_eq!(
            file_name("https://x.test/a/Tawny_1.2.0_x64-setup.exe.zip?t=1"),
            "Tawny_1.2.0_x64-setup.exe.zip"
        );
        assert_eq!(file_name("https://x.test/a/..%2F..%2Fetc"), "2F..2Fetc");
        assert_eq!(file_name("https://x.test/"), "update.bin");
        assert_eq!(file_name("https://x.test/..."), "update.bin");
    }
}
