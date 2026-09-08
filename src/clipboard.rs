/// Declared for its side effect: this is what tells `dx` to build and bundle
/// the Kotlin module. Android calls go through [`crate::android_bridge`] so
/// they resolve the class through the app's loader on Dioxus worker threads.
#[cfg(target_os = "android")]
#[manganis::ffi("src/android/clipboard")]
unsafe extern "Kotlin" {
    pub type ClipboardPlugin;
}
#[cfg(target_os = "android")]
const CLIPBOARD_CLASS: &str = "dev.dioxus.g3_native_plugins.clipboard.ClipboardPlugin";
#[cfg(target_os = "android")]
type ClipboardHandle = crate::android_bridge::AndroidPlugin;
#[cfg(target_os = "ios")]
type ClipboardHandle = ClipboardPlugin;
#[cfg(target_os = "ios")]
#[manganis::ffi("src/ios")]
#[allow(missing_docs)]
unsafe extern "Swift" {
    /// Native Apple clipboard and share-sheet bridge.
    pub type ClipboardPlugin;
    /// Copies text to the Apple platform clipboard.
    pub fn copyToClipboardFromRust(this: &ClipboardPlugin, text: String) -> String;
    /// Opens the Apple platform share sheet.
    pub fn shareFromRust(this: &ClipboardPlugin, text: String) -> String;
}
#[cfg(any(target_os = "android", target_os = "ios"))]
/// Access to the system clipboard and native share sheet.
pub struct Clipboard {
    plugin: Option<ClipboardHandle>,
}
#[cfg(target_os = "macos")]
/// An inert clipboard facade for the macOS smoke-test build.
pub struct Clipboard;
#[cfg(target_arch = "wasm32")]
/// Access to the browser clipboard and Web Share APIs.
pub struct Clipboard;
#[cfg(any(target_os = "android", target_os = "ios"))]
impl Clipboard {
    pub(crate) fn new() -> Self {
        Self { plugin: None }
    }
    fn get_plugin(&mut self) -> Result<&ClipboardHandle, String> {
        if self.plugin.is_none() {
            #[cfg(target_os = "android")]
            let created = ClipboardHandle::new(CLIPBOARD_CLASS)?;
            #[cfg(target_os = "ios")]
            let created = ClipboardPlugin::new()
                .map_err(|error| format!("Failed to create ClipboardPlugin: {error:?}"))?;
            self.plugin = Some(created);
        }
        Ok(self.plugin.as_ref().unwrap())
    }
    /// Copies `text` to the system clipboard.
    pub fn copy_to_clipboard(&mut self, text: String) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        {
            plugin.call_unit_str("copyToClipboardFromRust", &text)?;
            return Ok(());
        }
        #[cfg(target_os = "ios")]
        {
            _ = copyToClipboardFromRust(plugin, text)?;
            return Ok(());
        }
        #[allow(unreachable_code)]
        {
            Ok(())
        }
    }
    /// Opens the platform share sheet with `text`.
    pub fn share(&mut self, text: String) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        {
            plugin.call_unit_str("shareFromRust", &text)?;
            return Ok(());
        }
        #[cfg(target_os = "ios")]
        {
            _ = shareFromRust(plugin, text)?;
            return Ok(());
        }
        #[allow(unreachable_code)]
        {
            Ok(())
        }
    }
}
#[cfg(target_os = "macos")]
impl Clipboard {
    pub(crate) fn new() -> Self {
        Self
    }

    /// No-op copy operation for the macOS smoke-test build.
    pub fn copy_to_clipboard(&mut self, _text: String) -> Result<(), String> {
        Ok(())
    }

    /// No-op share operation for the macOS smoke-test build.
    pub fn share(&mut self, _text: String) -> Result<(), String> {
        Ok(())
    }
}
#[cfg(target_arch = "wasm32")]
impl Clipboard {
    pub(crate) fn new() -> Self {
        Self
    }
    /// Copies `text` with the browser Clipboard API.
    pub fn copy_to_clipboard(&mut self, text: String) -> Result<(), String> {
        let window = web_sys::window().ok_or_else(|| "Window is not available".to_string())?;
        let navigator = window.navigator();
        let clipboard = js_sys::Reflect::get(&navigator, &"clipboard".into())
            .map_err(|_| "Clipboard API is not available".to_string())?;
        let write_text = js_sys::Reflect::get(&clipboard, &"writeText".into())
            .map_err(|_| "Clipboard writeText API is not available".to_string())?
            .dyn_into::<js_sys::Function>()
            .map_err(|_| "Clipboard writeText API is not callable".to_string())?;
        write_text
            .call1(&clipboard, &text.into())
            .map_err(|_| "Unable to write to clipboard".to_string())?;
        Ok(())
    }
    /// Shares `text` with the Web Share API, falling back to copying it.
    pub fn share(&mut self, text: String) -> Result<(), String> {
        let window = web_sys::window().ok_or_else(|| "Window is not available".to_string())?;
        let navigator = window.navigator();
        let share = js_sys::Reflect::get(&navigator, &"share".into())
            .map_err(|_| "Web Share API is not available".to_string())?
            .dyn_into::<js_sys::Function>()
            .map_err(|_| "Web Share API is not callable".to_string());
        let Ok(share) = share else {
            return self.copy_to_clipboard(text);
        };
        let share_data = js_sys::Object::new();
        js_sys::Reflect::set(&share_data, &"title".into(), &"Share".into())
            .map_err(|_| "Unable to prepare share title".to_string())?;
        js_sys::Reflect::set(&share_data, &"text".into(), &text.into())
            .map_err(|_| "Unable to prepare share text".to_string())?;
        share
            .call1(&navigator, &share_data)
            .map_err(|_| "Unable to open share sheet".to_string())?;
        Ok(())
    }
}
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
