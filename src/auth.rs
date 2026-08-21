#[cfg(target_os = "android")]
#[manganis::ffi("src/android/auth")]
unsafe extern "Kotlin" {
    pub type AuthPlugin;

    pub fn startGoogleAuthFromRust(this: &AuthPlugin) -> Option<String>;
}

#[cfg(any(target_os = "ios", target_os = "macos"))]
#[manganis::ffi("src/ios")]
unsafe extern "Swift" {
    pub type AuthPlugin;

    pub fn startAppleAuthFromRust(this: &AuthPlugin) -> Option<String>;
    pub fn getPendingResult(this: &AuthPlugin) -> Option<String>;
    pub fn getAuthState(this: &AuthPlugin) -> Option<String>;
}

/// The outcome of a native sign-in attempt.
#[derive(Clone, Debug)]
pub struct AuthResult {
    /// The identity token returned by the platform - an Apple ID token on
    /// iOS, a Google ID token on Android - or `None` when the user cancelled
    /// or the flow is still pending.
    pub credential: Option<String>,
}

#[cfg(any(target_os = "android", target_os = "ios", target_os = "macos"))]
pub struct Auth {
    plugin: Option<AuthPlugin>,
}

#[cfg(any(target_os = "android", target_os = "ios", target_os = "macos"))]
impl Auth {
    pub(crate) fn new() -> Self {
        Self { plugin: None }
    }

    fn get_plugin(&mut self) -> Result<&AuthPlugin, String> {
        if self.plugin.is_none() {
            self.plugin = Some(
                AuthPlugin::new()
                    .map_err(|error| format!("Failed to create AuthPlugin: {error:?}"))?,
            );
        }
        Ok(self.plugin.as_ref().unwrap())
    }

    #[cfg(target_os = "android")]
    pub fn start_google_auth(&mut self) -> Result<AuthResult, String> {
        let plugin = self.get_plugin()?;
        let credential = startGoogleAuthFromRust(plugin)?;
        Ok(AuthResult { credential })
    }

    /// Starts the Apple Sign-In flow (fire-and-forget, non-blocking).
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    pub fn start_apple_auth(&mut self) -> Result<AuthResult, String> {
        let plugin = self.get_plugin()?;
        let credential = startAppleAuthFromRust(plugin)?;
        Ok(AuthResult { credential })
    }

    /// Poll for the Apple Sign-In result.
    /// Returns None while awaiting, Some(credential_json) when complete.
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    pub fn poll_auth_result(&mut self) -> Option<String> {
        let Ok(plugin) = self.get_plugin() else {
            return None;
        };
        getPendingResult(plugin).expect("Failed to get pending result")
    }

    /// Returns true while waiting for Apple Sign-In to complete.
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    pub fn is_auth_awaiting(&mut self) -> bool {
        let Ok(plugin) = self.get_plugin() else {
            return false;
        };
        match getAuthState(plugin) {
            Ok(Some(state)) => state == "awaiting",
            Ok(None) => false,
            Err(_) => false,
        }
    }
}
