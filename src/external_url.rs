#[cfg(target_os = "android")]
#[manganis::ffi("src/android/external_url")]
unsafe extern "Kotlin" {
    pub type ExternalUrlPlugin;

    pub fn openExternalUrlFromRust(this: &ExternalUrlPlugin, url: String);
}

#[cfg(target_os = "ios")]
#[manganis::ffi("src/ios")]
unsafe extern "Swift" {
    pub type ExternalUrlPlugin;

    pub fn openExternalUrlFromRust(this: &ExternalUrlPlugin, url: String);
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub struct ExternalUrl {
    plugin: Option<ExternalUrlPlugin>,
}

#[cfg(target_arch = "wasm32")]
pub struct ExternalUrl;

#[cfg(any(target_os = "android", target_os = "ios"))]
impl ExternalUrl {
    pub(crate) fn new() -> Self {
        Self { plugin: None }
    }

    fn get_plugin(&mut self) -> Result<&ExternalUrlPlugin, String> {
        if self.plugin.is_none() {
            self.plugin = Some(
                ExternalUrlPlugin::new()
                    .map_err(|error| format!("Failed to create ExternalUrlPlugin: {error:?}"))?,
            );
        }
        Ok(self.plugin.as_ref().unwrap())
    }

    pub fn prepare(&mut self) -> Result<(), String> {
        self.get_plugin()?;
        Ok(())
    }

    pub fn open(&mut self, url: &str) -> Result<(), String> {
        let plugin = self.get_plugin()?;
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
