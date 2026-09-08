/// Declared for its side effect: this is what tells `dx` to build and bundle
/// the Kotlin module. Android calls go through [`crate::android_bridge`] so
/// they resolve the class through the app's loader on Dioxus worker threads.
#[cfg(target_os = "android")]
#[manganis::ffi("src/android/external_url")]
unsafe extern "Kotlin" {
    pub type ExternalUrlPlugin;
}
#[cfg(target_os = "android")]
const EXTERNAL_URL_CLASS: &str = "dev.dioxus.g3_native_plugins.external_url.ExternalUrlPlugin";
#[cfg(target_os = "android")]
type ExternalUrlHandle = crate::android_bridge::AndroidPlugin;
#[cfg(target_os = "ios")]
type ExternalUrlHandle = ExternalUrlPlugin;
#[cfg(target_os = "ios")]
#[manganis::ffi("src/ios")]
unsafe extern "Swift" {
    pub type ExternalUrlPlugin;
    pub fn openExternalUrlFromRust(this: &ExternalUrlPlugin, url: String);
}
#[cfg(any(target_os = "android", target_os = "ios"))]
pub struct ExternalUrl {
    plugin: Option<ExternalUrlHandle>,
}
#[cfg(target_arch = "wasm32")]
pub struct ExternalUrl;
#[cfg(any(target_os = "android", target_os = "ios"))]
impl ExternalUrl {
    pub(crate) fn new() -> Self {
        Self { plugin: None }
    }
    fn get_plugin(&mut self) -> Result<&ExternalUrlHandle, String> {
        if self.plugin.is_none() {
            #[cfg(target_os = "android")]
            let created = ExternalUrlHandle::new(EXTERNAL_URL_CLASS)?;
            #[cfg(target_os = "ios")]
            let created = ExternalUrlPlugin::new()
                .map_err(|error| format!("Failed to create ExternalUrlPlugin: {error:?}"))?;
            self.plugin = Some(created);
        }
        Ok(self.plugin.as_ref().unwrap())
    }
    pub fn prepare(&mut self) -> Result<(), String> {
        self.get_plugin()?;
        Ok(())
    }
    pub fn open(&mut self, url: &str) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        plugin.call_unit_str("openExternalUrlFromRust", url)?;
        #[cfg(target_os = "ios")]
        openExternalUrlFromRust(plugin, url.to_string())?;
        Ok(())
    }
}
#[cfg(target_arch = "wasm32")]
impl ExternalUrl {
    pub(crate) fn new() -> Self {
        Self
    }
    pub fn prepare(&mut self) -> Result<(), String> {
        web_sys::window()
            .ok_or_else(|| "Window is not available".to_string())
            .map(|_| ())
    }
    pub fn open(&mut self, url: &str) -> Result<(), String> {
        let window = web_sys::window().ok_or_else(|| "Window is not available".to_string())?;
        window
            .location()
            .assign(url)
            .map_err(|_| "Unable to open external URL".to_string())
    }
}
