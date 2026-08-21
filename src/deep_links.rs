use serde_json::{Value, json};

/// The contents of an `apple-app-site-association` file, which iOS fetches
/// from `https://<host>/.well-known/apple-app-site-association` to decide
/// which URLs open in the app instead of Safari.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppleAppSiteAssociation {
    /// The Apple Developer Team ID that prefixes the app identifier.
    pub team_id: String,
    /// The app's bundle identifier, e.g. `com.G3Tech.GreensidePartee`.
    pub bundle_id: String,
    /// URL paths claimed by the app. Supports Apple's wildcard syntax, so
    /// `/games/*/join` matches any game id.
    pub paths: Vec<String>,
}

impl AppleAppSiteAssociation {
    /// Build an association for one app and the paths it claims.
    pub fn new(
        team_id: impl Into<String>,
        bundle_id: impl Into<String>,
        paths: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            team_id: team_id.into(),
            bundle_id: bundle_id.into(),
            paths: paths.into_iter().map(Into::into).collect(),
        }
    }

    /// The fully qualified app identifier Apple expects: `<team_id>.<bundle_id>`.
    pub fn app_id(&self) -> String {
        format!("{}.{}", self.team_id, self.bundle_id)
    }

    /// Render the association as the JSON body to serve at the well-known path.
    pub fn to_json(&self) -> Value {
        json!({
            "applinks": {
                "apps": [],
                "details": [{
                    "appID": self.app_id(),
                    "paths": self.paths
                }]
            }
        })
    }
}

/// The contents of an `assetlinks.json` file, which Android fetches from
/// `https://<host>/.well-known/assetlinks.json` to verify an App Link and
/// open matching URLs in the app without a disambiguation dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidAssetLinks {
    /// The application id of the Android app, e.g. `com.G3Tech.GreensidePartee`.
    pub package_name: String,
    /// SHA-256 fingerprints of the signing certificates. Include both the
    /// upload and Play-managed app-signing keys when Play App Signing is on,
    /// or verification fails for store builds.
    pub sha256_cert_fingerprints: Vec<String>,
}

impl AndroidAssetLinks {
    /// Build an asset-links statement for one app and its signing fingerprints.
    pub fn new(
        package_name: impl Into<String>,
        sha256_cert_fingerprints: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            package_name: package_name.into(),
            sha256_cert_fingerprints: sha256_cert_fingerprints
                .into_iter()
                .map(Into::into)
                .collect(),
        }
    }

    /// Render the statement list as the JSON body to serve at the well-known path.
    pub fn to_json(&self) -> Value {
        json!([{
            "relation": ["delegate_permission/common.handle_all_urls"],
            "target": {
                "namespace": "android_app",
                "package_name": self.package_name,
                "sha256_cert_fingerprints": self.sha256_cert_fingerprints
            }
        }])
    }
}

/// Shorthand for [`AppleAppSiteAssociation::new`] followed by
/// [`AppleAppSiteAssociation::to_json`].
pub fn apple_app_site_association(
    team_id: impl Into<String>,
    bundle_id: impl Into<String>,
    paths: impl IntoIterator<Item = impl Into<String>>,
) -> Value {
    AppleAppSiteAssociation::new(team_id, bundle_id, paths).to_json()
}

/// Shorthand for [`AndroidAssetLinks::new`] followed by
/// [`AndroidAssetLinks::to_json`].
pub fn android_asset_links(
    package_name: impl Into<String>,
    sha256_cert_fingerprints: impl IntoIterator<Item = impl Into<String>>,
) -> Value {
    AndroidAssetLinks::new(package_name, sha256_cert_fingerprints).to_json()
}

/// Define the server route that serves the `apple-app-site-association`
/// file, so iOS universal links resolve for the given team, bundle, and
/// paths.
///
/// Expands to a `#[get]` handler; call it once in a server module.
#[macro_export]
macro_rules! ios_app_site_association_route {
    (
        team_id: $team_id:expr,
        bundle_id: $bundle_id:expr,
        paths: [$($path:expr),* $(,)?] $(,)?
    ) => {
        #[dioxus::prelude::get("/.well-known/apple-app-site-association")]
        async fn get_apple_app_site_association() -> dioxus::prelude::Result<dioxus_fullstack::Json<serde_json::Value>, dioxus::prelude::StatusCode> {
            Ok(dioxus_fullstack::Json(
                $crate::deep_links::apple_app_site_association($team_id, $bundle_id, [$($path),*])
            ))
        }
    };
}

/// Define the server route that serves `assetlinks.json`, so Android App
/// Links resolve for the given package and signing fingerprints.
///
/// Expands to a `#[get]` handler; call it once in a server module.
#[macro_export]
macro_rules! android_asset_links_route {
    (
        package_name: $package_name:expr,
        sha256_cert_fingerprints: [$($fingerprint:expr),* $(,)?] $(,)?
    ) => {
        #[dioxus::prelude::get("/.well-known/assetlinks.json")]
        async fn get_android_asset_links() -> dioxus::prelude::Result<dioxus_fullstack::Json<serde_json::Value>, dioxus::prelude::StatusCode> {
            Ok(dioxus_fullstack::Json(
                $crate::deep_links::android_asset_links($package_name, [$($fingerprint),*])
            ))
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn production_source() -> &'static str {
        include_str!("deep_links.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("source should split before tests")
    }

    #[test]
    fn generates_apple_app_site_association() {
        let association = AppleAppSiteAssociation::new(
            "TEAM123456",
            "com.example.App",
            ["/account", "/games/*/join"],
        );

        assert_eq!(association.app_id(), "TEAM123456.com.example.App");
        assert_eq!(
            association.to_json(),
            json!({
                "applinks": {
                    "apps": [],
                    "details": [{
                        "appID": "TEAM123456.com.example.App",
                        "paths": ["/account", "/games/*/join"]
                    }]
                }
            })
        );
    }

    #[test]
    fn generates_android_asset_links() {
        assert_eq!(
            android_asset_links("com.example.App", ["AA:BB", "CC:DD"]),
            json!([{
                "relation": ["delegate_permission/common.handle_all_urls"],
                "target": {
                    "namespace": "android_app",
                    "package_name": "com.example.App",
                    "sha256_cert_fingerprints": ["AA:BB", "CC:DD"]
                }
            }])
        );
    }

    #[test]
    fn exports_ios_and_android_route_macros() {
        let source = production_source();

        assert!(source.contains("macro_rules! ios_app_site_association_route"));
        assert!(source.contains("macro_rules! android_asset_links_route"));
        assert!(
            source.contains("#[dioxus::prelude::get(\"/.well-known/apple-app-site-association\")]")
        );
        assert!(source.contains("#[dioxus::prelude::get(\"/.well-known/assetlinks.json\")]"));
        assert!(source.contains("dioxus_fullstack::Json"));
    }
}
