use serde_json::{Value, json};
/// Declared for its side effect: this is what tells `dx` to build and bundle
/// the Kotlin module. The calls go through [`crate::android_bridge`] instead,
/// because the macro's own bindings resolve classes against the calling
/// thread's loader and Dioxus does not call from a thread that has one.
#[cfg(all(feature = "deep-links", target_os = "android"))]
#[manganis::ffi("src/android/deep_links")]
unsafe extern "Kotlin" {
    pub type DeepLinksPlugin;
}
#[cfg(all(feature = "deep-links", target_os = "android"))]
const DEEP_LINKS_CLASS: &str = "dev.dioxus.g3_native_plugins.deep_links.DeepLinksPlugin";
#[cfg(all(feature = "deep-links", target_os = "android"))]
type DeepLinksHandle = crate::android_bridge::AndroidPlugin;
#[cfg(all(feature = "deep-links", target_os = "ios"))]
type DeepLinksHandle = DeepLinksPlugin;
#[cfg(all(feature = "deep-links", target_os = "ios"))]
#[manganis::ffi("src/ios")]
unsafe extern "Swift" {
    pub type DeepLinksPlugin;
    pub fn prepareFromRust(this: &DeepLinksPlugin);
    pub fn takeLinkFromRust(this: &DeepLinksPlugin) -> Option<String>;
}
/// The URLs a universal link, App Link, or custom scheme opened the app with.
///
/// This is the receiving half of deep linking. Declaring the links is already
/// handled elsewhere: the Dioxus CLI writes the `Info.plist` and manifest
/// entries from `[deep_links]` in `Dioxus.toml`, and the builders in this
/// module generate the `apple-app-site-association` and `assetlinks.json` files
/// the two platforms fetch to verify the claim. What neither covers is the URL
/// itself once the app is open, which is what this does.
///
/// Links are queued and polled rather than pushed at a listener. A link can
/// arrive before the app has rendered anything able to receive it — a cold
/// start is the normal case, not the edge case — and unlike a back gesture it
/// is a value that must not be dropped. Drain it wherever routing decisions are
/// made:
///
/// ```rust,ignore
/// let mut plugins = use_context::<NativePlugins>();
/// let navigator = use_navigator();
///
/// use_future(move || async move {
///     loop {
///         while let Ok(Some(url)) = plugins.deep_links.write().take_link() {
///             // Route on the URL however the app wants to.
///         }
///         gloo_timers::future::TimeoutFuture::new(200).await;
///     }
/// });
/// ```
///
/// [`prepare`](DeepLinks::prepare) is worth calling as early as the app can
/// manage, because on iOS a cold-start link is read from the launch options and
/// they are only observable before launching finishes. [`DeepLinks::new`] is
/// public so that can happen ahead of the plugin provider, and the queue lives
/// on the native side rather than in this struct, so a link is never stranded
/// on an instance that has since been dropped.
///
/// On the web the browser hands the app its URL directly and the router already
/// routes it, and macOS has no equivalent delivery, so both are inert and
/// callers need no cfg of their own.
#[cfg(all(feature = "deep-links", any(target_os = "android", target_os = "ios")))]
pub struct DeepLinks {
    plugin: Option<DeepLinksHandle>,
}
/// The inert deep-link facade: see the native [`DeepLinks`] for the contract.
#[cfg(all(
    feature = "deep-links",
    any(target_arch = "wasm32", target_os = "macos")
))]
pub struct DeepLinks;
#[cfg(all(feature = "deep-links", any(target_os = "android", target_os = "ios")))]
impl DeepLinks {
    /// Create an unprepared deep-link receiver.
    ///
    /// Public, unlike most plugins here, so an app can prepare the native side
    /// before [`crate::NativePluginsProvider`] exists. The queue is shared
    /// across instances, so an early instance and the provider's own see the
    /// same links.
    pub fn new() -> Self {
        Self { plugin: None }
    }
    fn get_plugin(&mut self) -> Result<&DeepLinksHandle, String> {
        if self.plugin.is_none() {
            #[cfg(target_os = "android")]
            let created = DeepLinksHandle::new(DEEP_LINKS_CLASS)?;
            #[cfg(target_os = "ios")]
            let created = DeepLinksPlugin::new()
                .map_err(|error| format!("Failed to create DeepLinksPlugin: {error:?}"))?;
            self.plugin = Some(created);
        }
        Ok(self.plugin.as_ref().unwrap())
    }
    /// Start collecting links.
    ///
    /// Call this as early as the app can. On iOS a link that started the app is
    /// only readable from the launch options, so preparing after launch has
    /// finished can miss a cold-start link; on Android the launch Intent stays
    /// readable and timing does not matter. Calling it more than once is
    /// harmless and does not re-deliver a link already queued.
    pub fn prepare(&mut self) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        plugin.call_unit("prepareFromRust")?;
        #[cfg(target_os = "ios")]
        prepareFromRust(plugin)?;
        Ok(())
    }
    /// Take the oldest link not yet handled, or `None` when none are waiting.
    ///
    /// Preparation happens on the platform's main thread while this is called
    /// from a Dioxus effect thread, so a link queued at launch may need a poll
    /// or two to appear. Drain in a loop until this returns `None`.
    pub fn take_link(&mut self) -> Result<Option<String>, String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        return plugin.call_string("takeLinkFromRust");
        #[cfg(target_os = "ios")]
        return Ok(takeLinkFromRust(plugin)?);
    }
}
#[cfg(all(
    feature = "deep-links",
    any(target_arch = "wasm32", target_os = "macos")
))]
impl DeepLinks {
    /// Create the inert deep-link facade used where nothing delivers links.
    pub fn new() -> Self {
        Self
    }
    /// No-op: the browser routes its own URL, and macOS delivers no links here.
    pub fn prepare(&mut self) -> Result<(), String> {
        Ok(())
    }
    /// Always `None`: nothing on these targets queues a link.
    pub fn take_link(&mut self) -> Result<Option<String>, String> {
        Ok(None)
    }
}
#[cfg(all(
    feature = "deep-links",
    any(
        target_arch = "wasm32",
        target_os = "android",
        target_os = "ios",
        target_os = "macos"
    )
))]
impl Default for DeepLinks {
    fn default() -> Self {
        Self::new()
    }
}
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
        json!(
            { "applinks" : { "apps" : [], "details" : [{ "appID" : self.app_id(), "paths"
            : self.paths }] } }
        )
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
        json!(
            [{ "relation" : ["delegate_permission/common.handle_all_urls"], "target" : {
            "namespace" : "android_app", "package_name" : self.package_name,
            "sha256_cert_fingerprints" : self.sha256_cert_fingerprints } }]
        )
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
        team_id : $team_id:expr, bundle_id : $bundle_id:expr, paths : [$($path:expr),*
        $(,)?] $(,)?
    ) => {
        #[dioxus::prelude::get("/.well-known/apple-app-site-association")] async fn
        get_apple_app_site_association() -> dioxus::prelude::Result <
        dioxus_fullstack::Json < serde_json::Value >, dioxus::prelude::StatusCode > {
        Ok(dioxus_fullstack::Json($crate::deep_links::apple_app_site_association($team_id,
        $bundle_id, [$($path),*]))) }
    };
}
/// Define the server route that serves `assetlinks.json`, so Android App
/// Links resolve for the given package and signing fingerprints.
///
/// Expands to a `#[get]` handler; call it once in a server module.
#[macro_export]
macro_rules! android_asset_links_route {
    (
        package_name : $package_name:expr, sha256_cert_fingerprints :
        [$($fingerprint:expr),* $(,)?] $(,)?
    ) => {
        #[dioxus::prelude::get("/.well-known/assetlinks.json")] async fn
        get_android_asset_links() -> dioxus::prelude::Result < dioxus_fullstack::Json <
        serde_json::Value >, dioxus::prelude::StatusCode > {
        Ok(dioxus_fullstack::Json($crate::deep_links::android_asset_links($package_name,
        [$($fingerprint),*]))) }
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
            json!(
                { "applinks" : { "apps" : [], "details" : [{ "appID" :
                "TEAM123456.com.example.App", "paths" : ["/account", "/games/*/join"] }]
                } }
            ),
        );
    }
    #[test]
    fn generates_android_asset_links() {
        assert_eq!(
            android_asset_links("com.example.App", ["AA:BB", "CC:DD"]),
            json!(
                [{ "relation" : ["delegate_permission/common.handle_all_urls"], "target"
                : { "namespace" : "android_app", "package_name" : "com.example.App",
                "sha256_cert_fingerprints" : ["AA:BB", "CC:DD"] } }]
            ),
        );
    }
    #[test]
    fn exports_ios_and_android_route_macros() {
        let source = production_source();
        assert!(source.contains("macro_rules! ios_app_site_association_route"));
        assert!(source.contains("macro_rules! android_asset_links_route"));
        assert!(
            source
                .contains("#[dioxus::prelude::get(\"/.well-known/apple-app-site-association\")]",),
        );
        assert!(source.contains("#[dioxus::prelude::get(\"/.well-known/assetlinks.json\")]"),);
        assert!(source.contains("dioxus_fullstack::Json"));
    }
}
