//! What an update server says, and whether to believe it: the Tauri release
//! format, the platform table, version order, and minisign signatures. Pure
//! parsing, so it is the same code and the same tests on every target.

use super::Update;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::Path;

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
    // Optional only so an iOS entry can name a store page, which has nothing
    // to download and so nothing to sign. Everything that is downloaded must
    // carry one; `engine` refuses it otherwise.
    #[serde(default)]
    signature: Option<String>,
}

/// An update the server offered, not yet downloaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Candidate {
    pub update: Update,
    pub url: String,
    pub signature: Option<String>,
    /// The installer named by the platform key that matched (`nsis`, `msi`,
    /// `appimage`, `deb`), or `None` for a key without one.
    pub installer: Option<String>,
}

/// The installer name of an entry that is a store page: opened, not
/// downloaded.
pub(super) const STORE: &str = "store";

/// Whether a URL is a store page this platform opens rather than downloads:
/// the App Store and TestFlight, Google Play, the Microsoft Store, Flathub
/// and Snapcraft.
pub(super) fn is_store_link(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    [
        "https://apps.apple.com/",
        "https://testflight.apple.com/",
        "itms-apps://",
        "itms-beta://",
        "macappstore://",
        "https://play.google.com/",
        "market://",
        "https://apps.microsoft.com/",
        "ms-windows-store://",
        "https://flathub.org/",
        "https://snapcraft.io/",
        "appstream://",
        "snap://",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
}

/// Where this build looks itself up in a release's `platforms` table.
pub(super) struct PlatformKey<'a> {
    /// `android`, `ios`, `windows`, `darwin` or `linux`: Tauri's names.
    pub target: &'a str,
    /// `aarch64`, `x86_64`, `armv7`, `i686`.
    pub arch: &'a str,
    /// The installer formats this install can take, most preferred first.
    pub installers: &'a [&'a str],
}

pub(super) fn fill_placeholders(
    endpoint: &str,
    current_version: &str,
    key: &PlatformKey<'_>,
) -> String {
    endpoint
        .replace("{{current_version}}", current_version)
        .replace("{{target}}", key.target)
        .replace("{{arch}}", key.arch)
}

/// The update on offer in a check response, or `None` when it is no newer
/// than what is running.
pub(super) fn choose(
    body: &[u8],
    current_version: &str,
    key: &PlatformKey<'_>,
) -> Result<Option<Candidate>, String> {
    let release: Release = serde_json::from_slice(body)
        .map_err(|error| format!("The update response is not a release: {error}"))?;
    semver::Version::parse(release.version.trim_start_matches('v')).map_err(|error| {
        format!(
            "The update response has an invalid version {}: {error}",
            release.version
        )
    })?;
    if !is_newer(&release.version, current_version) {
        return Ok(None);
    }
    let base = format!("{}-{}", key.target, key.arch);
    // Most specific first: each installer this install can take, for this
    // architecture and then for any; then the architecture, then the bare
    // target; and a store page last, so a build that can install a download
    // takes one over sending its user to a store.
    let mut keys: Vec<(String, Option<&str>)> = Vec::new();
    for installer in key
        .installers
        .iter()
        .copied()
        .filter(|installer| *installer != STORE)
    {
        keys.push((format!("{base}-{installer}"), Some(installer)));
        keys.push((format!("{}-{installer}", key.target), Some(installer)));
    }
    // An entry without an installer is a file to download. A build that can
    // only open its store (a Play build, which may not install APKs) must
    // not pick one up.
    let downloads =
        key.installers.is_empty() || key.installers.iter().any(|installer| *installer != STORE);
    if downloads {
        keys.push((base.clone(), None));
        keys.push((key.target.to_string(), None));
    }
    if key.installers.contains(&STORE) {
        keys.push((format!("{base}-{STORE}"), Some(STORE)));
        keys.push((format!("{}-{STORE}", key.target), Some(STORE)));
    }
    let found = keys.iter().find_map(|(name, installer)| {
        release
            .platforms
            .get(name)
            .map(|platform| (platform.clone(), *installer))
    });
    let (url, signature, installer) = match found {
        Some((platform, installer)) => (platform.url, platform.signature, installer),
        None => match &release.url {
            Some(url) if downloads => (url.clone(), release.signature.clone(), None),
            _ => {
                let looked = keys
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(format!(
                    "The update response has no build for this app: looked for {looked}"
                ));
            }
        },
    };
    Ok(Some(Candidate {
        update: Update {
            version: release.version.trim_start_matches('v').to_string(),
            current_version: current_version.to_string(),
            notes: release.notes,
            pub_date: release.pub_date,
            // Decided by the platform, which knows its stores' links.
            store: false,
        },
        url,
        signature: signature.filter(|signature| !signature.trim().is_empty()),
        installer: installer.map(str::to_string),
    }))
}

/// `true` when `candidate` is a strictly newer semantic version. A version
/// that does not parse is never newer, so garbage cannot win. A leading `v`
/// is allowed, since release tags often carry one.
pub(super) fn is_newer(candidate: &str, current: &str) -> bool {
    match (
        semver::Version::parse(candidate.trim_start_matches('v')),
        semver::Version::parse(current.trim_start_matches('v')),
    ) {
        (Ok(candidate), Ok(current)) => candidate > current,
        _ => false,
    }
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

fn signature(text: &str) -> Result<minisign_verify::Signature, String> {
    let text = text.trim();
    let text = if text.starts_with("untrusted comment:") {
        text.to_string()
    } else {
        // What `tauri signer sign` writes: the .sig file, base64 again.
        base64_text(text).ok_or("The update signature is not base64")?
    };
    minisign_verify::Signature::decode(text.trim())
        .map_err(|error| format!("The update signature is malformed: {error}"))
}

/// Check a downloaded file against the signature the release gave it.
///
/// Read in pieces for a prehashed signature, which is what `minisign -S` and
/// current Tauri signers write, so a large installer is never held in memory
/// whole. A legacy signature is over the raw bytes and has to be.
pub(super) fn verify_file(pubkey: &str, signature_text: &str, path: &Path) -> Result<(), String> {
    let key = public_key(pubkey)?;
    let signature = signature(signature_text)?;
    let failed = |error: minisign_verify::Error| {
        format!("The downloaded update failed signature verification: {error}")
    };
    match key.verify_stream(&signature) {
        Ok(mut stream) => {
            let mut file = fs::File::open(path)
                .map_err(|error| format!("Failed to open the download: {error}"))?;
            let mut buffer = vec![0u8; 256 * 1024];
            loop {
                let read = file
                    .read(&mut buffer)
                    .map_err(|error| format!("Failed to read the download: {error}"))?;
                if read == 0 {
                    break;
                }
                stream.update(&buffer[..read]);
            }
            stream.finalize().map_err(failed)
        }
        Err(minisign_verify::Error::UnsupportedLegacyMode) => {
            let bytes =
                fs::read(path).map_err(|error| format!("Failed to read the download: {error}"))?;
            key.verify(&bytes, &signature, true).map_err(failed)
        }
        Err(error) => Err(failed(error)),
    }
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

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    // A throwaway key with a fixed seed, and a file signed with it the way
    // `minisign -S` signs (prehashed, with a trusted comment) and the way an
    // older `tauri signer sign` does (legacy, wrapped in base64).
    pub(crate) const PUBLIC_KEY: &str = "untrusted comment: minisign public key EFCDAB8967452301\nRWQBI0VniavN7wOhB7/zzhC+HXDdGOdLwJln5NYwm6UNXx3chmQSVTG4\n";

    const PUBLIC_KEY_LINE: &str = "RWQBI0VniavN7wOhB7/zzhC+HXDdGOdLwJln5NYwm6UNXx3chmQSVTG4";

    const TAURI_PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXkgRUZDREFCODk2NzQ1MjMwMQpSV1FCSTBWbmlhdk43d09oQjcvenpoQytIWERkR09kTHdKbG41Tll3bTZVTlh4M2NobVFTVlRHNAo=";

    /// The signed bytes. Any file will do; these happen to be JSON.
    pub(crate) const SIGNED: &str = r#"{"version":"1.1.0","runtime_version":"1.0.0","files":[{"path":"index.html","sha256":"ff2eca2f6c4b9b6970340e8aa384379e0cdc6fa5b69d8589614bc8b283207118","size":13},{"path":"css/app.css","sha256":"8dfb68c5781865682600b50d23b899aad50266d3ae120933504e64cda5781e3d","size":21}]}"#;

    pub(crate) const SIGNATURE: &str = "untrusted comment: signature from minisign secret key\nRUQBI0VniavN77NjGcAtGJUWAMcvC2YTJWkqVhRpaKj3tHJ7QLmZPG8SNC5LspXfHJMeGC1PWY/kmL7mLK4NSFX6TvX86YWCiAo=\ntrusted comment: timestamp:0\tfile:manifest.json\n2Y6s2X5LNlxoRHKxEpkyGDOKgypzYKhIa3YB279Uz5wkV4F6MD14jvw2zn2WeAZ3HPR3DN7w9VpTfhBt7O/pCQ==\n";

    const TAURI_SIGNATURE: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIG1pbmlzaWduIHNlY3JldCBrZXkKUldRQkkwVm5pYXZON3pSZmZWcGpoZmViNlRaM1ZSQ0c1REIzNXBNUUZhU3hXNkVWeXBvdGQ2UTJzaVlvNGE4Y2tJYVFaSDJvUlB0dm5RcllPd1g5SHNCR0Iwa09tU2UwK2dFPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDowCWZpbGU6bWFuaWZlc3QuanNvbgpOa3pubVVRWUZDSmdwc1ZlWm9HVG5OY1dFeW5DUVNmSVBGRHFxS0lqMnVkY0paVkY1VGc3Mkt2dTlIRy9SNWZhdlJGUnRXNXA4dXhEMXJRMjBxcGhDdz09Cg==";

    const WINDOWS: PlatformKey<'static> = PlatformKey {
        target: "windows",
        arch: "x86_64",
        installers: &["nsis", "msi"],
    };

    fn release(platforms: serde_json::Value) -> Vec<u8> {
        serde_json::json!({
            "version": "v1.1.0",
            "notes": "Fixes",
            "pub_date": "2026-11-01T12:00:00Z",
            "platforms": platforms,
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn the_most_specific_platform_entry_wins() {
        let body = release(serde_json::json!({
            "windows": { "url": "https://x.test/any.zip", "signature": "s0" },
            "windows-x86_64": { "url": "https://x.test/arch.zip", "signature": "s1" },
            "windows-x86_64-msi": { "url": "https://x.test/setup.msi.zip", "signature": "s2" },
            "windows-x86_64-nsis": { "url": "https://x.test/setup.exe.zip", "signature": "s3" },
        }));
        let candidate = choose(&body, "1.0.0", &WINDOWS).unwrap().unwrap();
        assert_eq!(candidate.url, "https://x.test/setup.exe.zip");
        assert_eq!(candidate.installer.as_deref(), Some("nsis"));
        assert_eq!(candidate.update.version, "1.1.0");
        assert_eq!(candidate.update.notes.as_deref(), Some("Fixes"));
        let body = release(serde_json::json!({
            "windows": { "url": "https://x.test/any.zip", "signature": "s0" },
        }));
        let candidate = choose(&body, "1.0.0", &WINDOWS).unwrap().unwrap();
        assert_eq!(candidate.url, "https://x.test/any.zip");
        assert_eq!(candidate.installer, None);
    }

    #[test]
    fn nothing_newer_is_no_update_and_a_missing_platform_is_an_error() {
        let body = release(serde_json::json!({
            "windows": { "url": "https://x.test/any.zip", "signature": "s0" },
        }));
        assert_eq!(choose(&body, "1.1.0", &WINDOWS).unwrap(), None);
        assert_eq!(choose(&body, "v2.0.0", &WINDOWS).unwrap(), None);
        let linux = PlatformKey {
            target: "linux",
            arch: "x86_64",
            installers: &["appimage"],
        };
        let error = choose(&body, "1.0.0", &linux).unwrap_err();
        assert!(error.contains("linux-x86_64"), "{error}");
        // A server that picked the platform from the URL answers flat.
        let flat = serde_json::json!({
            "version": "1.1.0",
            "url": "https://x.test/Tawny.AppImage.tar.gz",
            "signature": "s",
        });
        let candidate = choose(flat.to_string().as_bytes(), "1.0.0", &linux)
            .unwrap()
            .unwrap();
        assert_eq!(candidate.signature.as_deref(), Some("s"));
        assert!(choose(b"{\"version\":\"soon\"}", "1.0.0", &linux).is_err());
    }

    #[test]
    fn keys_and_signatures_are_accepted_in_every_form_minisign_and_tauri_write() {
        let directory = std::env::temp_dir().join(format!("g3-release-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("signed.bin");
        fs::write(&path, SIGNED).unwrap();
        for key in [PUBLIC_KEY, PUBLIC_KEY_LINE, TAURI_PUBLIC_KEY] {
            verify_file(key, SIGNATURE, &path).unwrap();
            verify_file(key, TAURI_SIGNATURE, &path).unwrap();
        }
        fs::write(&path, SIGNED.replace("1.1.0", "1.1.1")).unwrap();
        let error = verify_file(PUBLIC_KEY, SIGNATURE, &path).unwrap_err();
        assert!(error.contains("signature verification"), "{error}");
        assert!(verify_file(PUBLIC_KEY, TAURI_SIGNATURE, &path).is_err());
        assert!(public_key("not a key").is_err());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn endpoints_fill_the_same_placeholders_as_tauri() {
        let key = PlatformKey {
            target: "android",
            arch: "aarch64",
            installers: &[],
        };
        assert_eq!(
            fill_placeholders(
                "https://u.test/{{target}}-{{arch}}/{{current_version}}",
                "1.2.0",
                &key
            ),
            "https://u.test/android-aarch64/1.2.0"
        );
    }

    #[test]
    fn a_store_build_finds_the_store_page_and_never_a_download() {
        let body = release(serde_json::json!({
            "android": { "url": "https://x.test/Tawny.apk", "signature": "s" },
            "android-store": { "url": "https://play.google.com/store/apps/details?id=x" },
        }));
        let play = PlatformKey {
            target: "android",
            arch: "aarch64",
            installers: &[STORE],
        };
        let candidate = choose(&body, "1.0.0", &play).unwrap().unwrap();
        assert!(candidate.url.starts_with("https://play.google.com/"));
        assert_eq!(candidate.installer.as_deref(), Some(STORE));
        assert_eq!(candidate.signature, None);
        // A sideloaded build of the same app takes the APK.
        let sideloaded = PlatformKey {
            target: "android",
            arch: "aarch64",
            installers: &["apk", STORE],
        };
        let candidate = choose(&body, "1.0.0", &sideloaded).unwrap().unwrap();
        assert_eq!(candidate.url, "https://x.test/Tawny.apk");
        // A store build with no store entry has nothing, even with an APK there.
        let body = release(serde_json::json!({
            "android": { "url": "https://x.test/Tawny.apk", "signature": "s" },
        }));
        let error = choose(&body, "1.0.0", &play).unwrap_err();
        assert!(error.contains("android-store"), "{error}");
        assert!(is_store_link("market://details?id=x"));
        assert!(is_store_link("https://apps.apple.com/app/id1"));
        assert!(!is_store_link("https://x.test/Tawny.apk"));
    }

    #[test]
    fn versions_compare_as_semver() {
        assert!(is_newer("1.10.0", "1.9.0"));
        assert!(is_newer("v1.0.0", "1.0.0-beta.2"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("garbage", "1.0.0"));
    }
}
