//! Installed native package metadata, independent of over-the-air content.
use serde::{Deserialize, Serialize};

/// Version and build from the installed iOS/Android package, not Cargo.toml.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppInfo {
    /// Bundle identifier / Android application ID.
    pub identifier: String,
    /// CFBundleShortVersionString / Android versionName.
    pub version: String,
    /// CFBundleVersion / Android longVersionCode, preserved as text.
    pub build: String,
}

#[cfg(target_os = "android")]
#[manganis::ffi("src/android/app_info")]
unsafe extern "Kotlin" {
    pub type AppInfoPlugin;
}
#[cfg(target_os = "ios")]
#[manganis::ffi("src/ios")]
unsafe extern "Swift" {
    pub type AppInfoPlugin;
    pub fn appInfoFromRust(this: &AppInfoPlugin) -> Option<String>;
}

#[cfg(any(target_os = "android", target_os = "ios", test))]
fn decode(value: &str) -> Result<AppInfo, String> {
    let info: AppInfo = serde_json::from_str(value).map_err(|_| "Invalid native app metadata")?;
    if [&info.identifier, &info.version, &info.build]
        .iter()
        .any(|value| value.is_empty() || value.len() > 255)
    {
        return Err("Incomplete native app metadata".into());
    }
    Ok(info)
}

/// Read the installed package metadata. Returns `None` on web/desktop/server.
///
/// Call from a handler or effect on mobile, not during SSR. No provider context
/// or permission request is needed. Never substitute a content-bundle version
/// for this result when deciding whether a store update is required.
pub fn installed_app_info() -> Result<Option<AppInfo>, String> {
    #[cfg(target_os = "android")]
    {
        let plugin = crate::android_bridge::AndroidPlugin::new(
            "dev.dioxus.g3_native_plugins.app_info.AppInfoPlugin",
        )?;
        let value = plugin
            .call_string("appInfoFromRust")?
            .ok_or("Native metadata unavailable")?;
        decode(&value).map(Some)
    }
    #[cfg(target_os = "ios")]
    {
        let plugin = AppInfoPlugin::new().map_err(|_| "App metadata bridge unavailable")?;
        let value = appInfoFromRust(&plugin)?.ok_or("Native metadata unavailable")?;
        decode(&value).map(Some)
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_native_builds_without_semver_assumptions() {
        let value =
            decode(r#"{"identifier":"net.g3tech.app","version":"1.2","build":"4294967297"}"#)
                .unwrap();
        assert_eq!(value.build, "4294967297");
        assert!(decode(r#"{"identifier":"net.g3tech.app","version":"1.2","build":""}"#).is_err());
    }
    #[test]
    fn bridge_contracts_match_both_platforms() {
        let kotlin = include_str!(
            "android/app_info/src/main/kotlin/dev/dioxus/g3_native_plugins/app_info/AppInfoPlugin.kt"
        );
        let swift = include_str!("ios/Sources/AppInfoPlugin.swift");
        for source in [kotlin, swift] {
            for key in ["identifier", "version", "build", "appInfoFromRust"] {
                assert!(source.contains(key));
            }
        }
        assert!(kotlin.contains("longVersionCode"));
        assert!(swift.contains("CFBundleShortVersionString"));
        assert!(swift.contains("CFBundleVersion"));
    }
}
