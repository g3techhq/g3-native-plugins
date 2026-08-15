#[cfg(target_os = "android")]
#[manganis::ffi("src/android/back_button")]
unsafe extern "Kotlin" {
    pub type BackButtonPlugin;

    pub fn setInterceptingFromRust(this: &BackButtonPlugin, intercepting: bool);
}

/// The system back gesture, routed into the app's own history.
///
/// Android delivers back to the Activity rather than the WebView, so a web app
/// hosted this way exits on the first press however it is written. Intercepting
/// turns the press into `history.back()` instead, which is the same event a
/// browser's back button produces — so whatever the app already does for a
/// traversal keeps working unchanged.
///
/// Interception is off until asked for. Only the app knows whether there is
/// anywhere to go back to, and a permanently enabled callback would leave the
/// user unable to leave.
///
/// Other platforms have no equivalent to intercept: iOS has no system back, and
/// on the web the browser owns it. Both are no-ops so callers need no cfg of
/// their own.
#[cfg(target_os = "android")]
pub struct BackButton {
    plugin: Option<BackButtonPlugin>,
    intercepting: bool,
}

#[cfg(any(target_arch = "wasm32", target_os = "ios", target_os = "macos"))]
pub struct BackButton {
    intercepting: bool,
}

#[cfg(target_os = "android")]
impl BackButton {
    pub(crate) fn new() -> Self {
        Self {
            plugin: None,
            intercepting: false,
        }
    }

    fn get_plugin(&mut self) -> Result<&BackButtonPlugin, String> {
        if self.plugin.is_none() {
            self.plugin = Some(
                BackButtonPlugin::new()
                    .map_err(|error| format!("Failed to create BackButtonPlugin: {error:?}"))?,
            );
        }
        Ok(self.plugin.as_ref().unwrap())
    }

    pub fn prepare(&mut self) -> Result<(), String> {
        self.get_plugin()?;
        Ok(())
    }

    /// Whether the system back press is taken by the app.
    ///
    /// Call with `false` wherever back should leave the app, so the press keeps
    /// its usual meaning at the root of the navigation stack.
    pub fn set_intercepting(&mut self, intercepting: bool) -> Result<(), String> {
        if self.intercepting == intercepting {
            return Ok(());
        }
        let plugin = self.get_plugin()?;
        setInterceptingFromRust(plugin, intercepting)?;
        self.intercepting = intercepting;
        Ok(())
    }

    pub fn is_intercepting(&self) -> bool {
        self.intercepting
    }
}

#[cfg(any(target_arch = "wasm32", target_os = "ios", target_os = "macos"))]
impl BackButton {
    pub(crate) fn new() -> Self {
        Self {
            intercepting: false,
        }
    }

    pub fn prepare(&mut self) -> Result<(), String> {
        Ok(())
    }

    /// Recorded but inert: no other platform has a system back press to take.
    pub fn set_intercepting(&mut self, intercepting: bool) -> Result<(), String> {
        self.intercepting = intercepting;
        Ok(())
    }

    pub fn is_intercepting(&self) -> bool {
        self.intercepting
    }
}
