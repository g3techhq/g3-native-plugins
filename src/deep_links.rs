use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppleAppSiteAssociation {
    pub team_id: String,
    pub bundle_id: String,
    pub paths: Vec<String>,
}

impl AppleAppSiteAssociation {
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

    pub fn app_id(&self) -> String {
        format!("{}.{}", self.team_id, self.bundle_id)
    }

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidAssetLinks {
    pub package_name: String,
    pub sha256_cert_fingerprints: Vec<String>,
}

impl AndroidAssetLinks {
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

pub fn apple_app_site_association(
    team_id: impl Into<String>,
    bundle_id: impl Into<String>,
    paths: impl IntoIterator<Item = impl Into<String>>,
) -> Value {
    AppleAppSiteAssociation::new(team_id, bundle_id, paths).to_json()
}

pub fn android_asset_links(
    package_name: impl Into<String>,
    sha256_cert_fingerprints: impl IntoIterator<Item = impl Into<String>>,
) -> Value {
    AndroidAssetLinks::new(package_name, sha256_cert_fingerprints).to_json()
}

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
