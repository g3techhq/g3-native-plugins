//! Reading the link an iOS release names: an over-the-air manifest to
//! verify and hand to `itms-services://`, or a store page to open. Apart from
//! the Swift so it is tested on the host.

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Link {
    /// An over-the-air install manifest, by its HTTPS URL.
    Manifest(String),
    /// An App Store or TestFlight page.
    Store(String),
}

pub(super) fn install_link(url: &str) -> Result<Link, String> {
    if let Some(query) = url.strip_prefix("itms-services://?") {
        let manifest = query
            .split('&')
            .find_map(|pair| pair.strip_prefix("url="))
            .ok_or("The itms-services link names no manifest")?;
        return install_link(&percent_decode(manifest));
    }
    let lower = url.to_ascii_lowercase();
    let store = [
        "https://apps.apple.com/",
        "https://testflight.apple.com/",
        "itms-apps://",
        "itms-beta://",
    ];
    if store.iter().any(|prefix| lower.starts_with(prefix)) {
        return Ok(Link::Store(url.to_string()));
    }
    let path = lower.split(['?', '#']).next().unwrap_or_default();
    // iOS refuses a manifest served over plain HTTP.
    if lower.starts_with("https://") && path.ends_with(".plist") {
        return Ok(Link::Manifest(url.to_string()));
    }
    Err(format!(
        "iOS installs only from an HTTPS manifest.plist or a store link, not {url}"
    ))
}

pub(super) fn percent_encode(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let hex = bytes
            .get(index + 1..index + 3)
            .and_then(|hex| std::str::from_utf8(hex).ok())
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match (bytes[index], hex) {
            (b'%', Some(value)) => {
                decoded.push(value);
                index += 3;
            }
            (byte, _) => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifests_are_verified_and_store_links_are_opened() {
        assert_eq!(
            install_link("https://x.test/1.1.0/manifest.plist?t=1").unwrap(),
            Link::Manifest("https://x.test/1.1.0/manifest.plist?t=1".to_string())
        );
        // A full itms-services link in the release is read back to its
        // manifest, so that is what gets downloaded and checked.
        assert_eq!(
            install_link(
                "itms-services://?action=download-manifest&url=https%3A%2F%2Fx.test%2Fm.plist"
            )
            .unwrap(),
            Link::Manifest("https://x.test/m.plist".to_string())
        );
        assert!(matches!(
            install_link("https://testflight.apple.com/join/abc").unwrap(),
            Link::Store(_)
        ));
        assert!(matches!(
            install_link("https://apps.apple.com/app/id123").unwrap(),
            Link::Store(_)
        ));
        assert!(install_link("http://x.test/manifest.plist").is_err());
        assert!(install_link("https://x.test/Tawny.ipa").is_err());
        assert_eq!(
            percent_encode("https://x.test/a b.plist"),
            "https%3A%2F%2Fx.test%2Fa%20b.plist"
        );
    }
}
