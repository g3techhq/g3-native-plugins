#[cfg(target_os = "android")]
#[manganis::ffi("src/android/clipboard")]
unsafe extern "Kotlin" {
    pub type ClipboardPlugin;
    pub fn copyToClipboardFromRust(this: &ClipboardPlugin, text: String);
    pub fn shareFromRust(this: &ClipboardPlugin, text: String);
}
#[cfg(any(target_os = "ios", target_os = "macos"))]
#[manganis::ffi("src/ios")]
unsafe extern "Swift" {
    pub type ClipboardPlugin;
    pub fn copyToClipboardFromRust(this: &ClipboardPlugin, text: String) -> String;
    pub fn shareFromRust(this: &ClipboardPlugin, text: String) -> String;
}
#[cfg(any(target_os = "android", target_os = "ios", target_os = "macos"))]
pub struct Clipboard {
    plugin: Option<ClipboardPlugin>,
}
#[cfg(target_arch = "wasm32")]
pub struct Clipboard;
#[cfg(any(target_os = "android", target_os = "ios", target_os = "macos"))]
impl Clipboard {
    pub(crate) fn new() -> Self {
        Self { plugin: None }
    }
    fn get_plugin(&mut self) -> Result<&ClipboardPlugin, String> {
        if self.plugin.is_none() {
            self.plugin = Some(
                ClipboardPlugin::new()
                    .map_err(|error| format!("Failed to create ClipboardPlugin: {error:?}"))?,
            );
        }
        Ok(self.plugin.as_ref().unwrap())
    }
    pub fn copy_to_clipboard(&mut self, text: String) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        {
            copyToClipboardFromRust(plugin, text)?;
            return Ok(());
        }
        #[cfg(any(target_os = "ios", target_os = "macos"))]
        {
            _ = copyToClipboardFromRust(plugin, text)?;
            return Ok(());
        }
        #[allow(unreachable_code)]
        {
            Ok(())
        }
    }
    pub fn share(&mut self, text: String) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        {
            shareFromRust(plugin, text)?;
            return Ok(());
        }
        #[cfg(any(target_os = "ios", target_os = "macos"))]
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
#[cfg(target_arch = "wasm32")]
impl Clipboard {
    pub(crate) fn new() -> Self {
        Self
    }
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
