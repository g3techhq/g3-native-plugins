/// Declared for its side effect: this is what tells `dx` to build and bundle
/// the Kotlin module. The calls go through [`crate::android_bridge`] instead,
/// because the macro's own bindings resolve classes against the calling
/// thread's loader and Dioxus does not call from a thread that has one.
#[cfg(target_os = "android")]
#[manganis::ffi("src/android/auth")]
unsafe extern "Kotlin" {
    pub type AuthPlugin;
}
#[cfg(target_os = "android")]
const AUTH_CLASS: &str = "dev.dioxus.g3_native_plugins.auth.AuthPlugin";
#[cfg(target_os = "android")]
type AuthHandle = crate::android_bridge::AndroidPlugin;
#[cfg(any(target_os = "ios", target_os = "macos"))]
type AuthHandle = AuthPlugin;
#[cfg(any(target_os = "ios", target_os = "macos"))]
#[manganis::ffi("src/ios")]
unsafe extern "Swift" {
    pub type AuthPlugin;
    pub fn startAppleAuthFromRust(this: &AuthPlugin) -> Option<String>;
    pub fn getPendingResult(this: &AuthPlugin) -> Option<String>;
    pub fn getAuthState(this: &AuthPlugin) -> Option<String>;
}
#[cfg(any(target_os = "android", target_os = "ios", target_os = "macos"))]
pub struct Auth {
    plugin: Option<AuthHandle>,
}
#[cfg(any(target_os = "android", target_os = "ios", target_os = "macos"))]
impl Auth {
    pub(crate) fn new() -> Self {
        Self { plugin: None }
    }
    fn get_plugin(&mut self) -> Result<&AuthHandle, String> {
        if self.plugin.is_none() {
            #[cfg(target_os = "android")]
            let created = AuthHandle::new(AUTH_CLASS)?;
            #[cfg(any(target_os = "ios", target_os = "macos"))]
            let created = AuthPlugin::new()
                .map_err(|error| format!("Failed to create AuthPlugin: {error:?}"))?;
            self.plugin = Some(created);
        }
        Ok(self.plugin.as_ref().unwrap())
    }
    /// Start Google Sign-In against the caller's own Google Cloud project.
    ///
    /// `server_client_id` is the **web** client id from that project's
    /// credentials page — the one ending `.apps.googleusercontent.com`, not the
    /// Android client id. Credential Manager checks it against an Android
    /// client registered in the same project for this app's package name and
    /// signing certificate, so an id from someone else's project cannot work
    /// however well formed it is. That is why it is a parameter: a library
    /// carrying its own would only ever authenticate the app it was written
    /// for.
    ///
    /// This returns as soon as the system sign-in UI has been requested. Poll
    /// [`poll_auth_result`](Auth::poll_auth_result) until it returns a token, or
    /// until [`is_auth_awaiting`](Auth::is_auth_awaiting) becomes false when the
    /// user cancels or the platform refuses the request.
    ///
    /// A wrong or unregistered id fails the same way a missing account does,
    /// with no credential and no explanation, so check the registration first
    /// when the flow ends without a token.
    #[cfg(target_os = "android")]
    pub fn start_google_auth(&mut self, server_client_id: &str) -> Result<(), String> {
        if server_client_id.trim().is_empty() {
            return Err("A Google server client id is required for sign-in".to_string());
        }
        let plugin = self.get_plugin()?;
        plugin.call_unit_str("startGoogleAuthFromRust", server_client_id)
    }
    /// Starts the Apple Sign-In flow (fire-and-forget, non-blocking).
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    pub fn start_apple_auth(&mut self) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        _ = startAppleAuthFromRust(plugin)?;
        Ok(())
    }
    /// Poll for the native sign-in result.
    ///
    /// Returns `Ok(None)` while awaiting, or the platform credential when one
    /// arrives. Android returns a Google ID token. Apple returns a JSON object
    /// containing `identity_token` and, on the first authorization, any email
    /// and display name Apple supplied.
    pub fn poll_auth_result(&mut self) -> Result<Option<String>, String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        return plugin.call_string("getPendingResult");
        #[cfg(any(target_os = "ios", target_os = "macos"))]
        return Ok(getPendingResult(plugin)?);
    }
    /// Returns true while waiting for native sign-in to complete.
    pub fn is_auth_awaiting(&mut self) -> bool {
        let Ok(plugin) = self.get_plugin() else {
            return false;
        };
        #[cfg(target_os = "android")]
        let state = plugin.call_string("getAuthState").ok().flatten();
        #[cfg(any(target_os = "ios", target_os = "macos"))]
        let state = getAuthState(plugin).ok().flatten();
        state.as_deref() == Some("awaiting")
    }
}
