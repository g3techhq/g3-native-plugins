/// Declared for its side effect: this is what tells `dx` to build and bundle
/// the Kotlin module. The calls go through [`crate::android_bridge`] instead,
/// because the macro's own bindings resolve classes against the calling
/// thread's loader and Dioxus does not call from a thread that has one.
#[cfg(target_os = "android")]
#[manganis::ffi("src/android/storage")]
unsafe extern "Kotlin" {
    pub type StoragePlugin;
}
#[cfg(target_os = "android")]
const STORAGE_CLASS: &str = "dev.dioxus.g3_native_plugins.storage.StoragePlugin";
#[cfg(target_os = "android")]
type StorageHandle = crate::android_bridge::AndroidPlugin;
#[cfg(target_os = "ios")]
type StorageHandle = StoragePlugin;
#[cfg(target_os = "ios")]
#[manganis::ffi("src/ios")]
unsafe extern "Swift" {
    pub type StoragePlugin;
    pub fn getFromRust(this: &StoragePlugin, key: String) -> Option<String>;
    pub fn setFromRust(this: &StoragePlugin, entryJson: String) -> Option<String>;
    pub fn removeFromRust(this: &StoragePlugin, key: String) -> Option<String>;
    pub fn clearFromRust(this: &StoragePlugin) -> Option<String>;
    pub fn keysFromRust(this: &StoragePlugin) -> Option<String>;
}
/// Key-value storage that survives restarts, encrypted where the platform can.
///
/// Named `KeyValueStore` rather than `Storage` because `dioxus::prelude` already
/// exports a `Storage` trait. An app doing `use dioxus::prelude::*;` alongside
/// `use g3_native_plugins::*;` would otherwise find the name ambiguous and fail
/// to compile, which is a poor greeting from a plugin crate.
///
/// Values are held by the platform's own secret store: the Keychain on iOS, and
/// on Android AES-GCM under a key the hardware-backed Keystore will not hand
/// out. `localStorage` on the web, which is **not** encrypted and is readable by
/// any script on the origin — see below.
///
/// ```rust,ignore
/// let mut plugins = use_context::<NativePlugins>();
///
/// plugins.storage.write().set("session", &token)?;
/// if let Some(token) = plugins.storage.write().get("session")? {
///     // Use it.
/// }
/// plugins.storage.write().remove("session")?;
/// ```
///
/// Unlike most plugins here these calls are synchronous, because both stores
/// are: a Keychain lookup and a `SharedPreferences` read are ordinary in-process
/// work, and nothing is gained by making the caller poll for them.
///
/// # What is and is not protected
///
/// **Key names are stored in the clear** on both platforms, and only values are
/// encrypted. Name a key `session` rather than putting anything meaningful in
/// the name itself.
///
/// **The web build offers no confidentiality at all.** `localStorage` is plain
/// text on disk and readable by any script running on the origin. It is here so
/// one call site works everywhere, not because it is equivalent. Anything that
/// would matter if it leaked does not belong in a web build of this.
///
/// On iOS the Keychain entry is written with `kSecAttrAccessibleAfterFirstUnlock`,
/// so it survives a reboot and is readable in the background once the user has
/// unlocked the device at least once, but never while it has never been
/// unlocked.
///
/// # macOS returns an error
///
/// Every other plugin here compiles to an inert no-op where a platform is not
/// implemented, because nothing happening is an acceptable outcome for a share
/// sheet or a back gesture. It is not acceptable for a store: a `set` that
/// silently discarded and a `get` that always returned `None` would look like a
/// working cache and lose data. This one reports an error instead.
#[cfg(any(target_os = "android", target_os = "ios"))]
pub struct KeyValueStore {
    plugin: Option<StorageHandle>,
}
/// Browser-backed storage: see the native [`KeyValueStore`] for the contract, and
/// note that this build encrypts nothing.
#[cfg(target_arch = "wasm32")]
pub struct KeyValueStore;
/// The unimplemented store: every call reports an error rather than pretending
/// to work. See the native [`KeyValueStore`].
#[cfg(target_os = "macos")]
pub struct KeyValueStore;
#[cfg(any(target_os = "android", target_os = "ios"))]
impl KeyValueStore {
    pub(crate) fn new() -> Self {
        Self { plugin: None }
    }
    fn get_plugin(&mut self) -> Result<&StorageHandle, String> {
        if self.plugin.is_none() {
            #[cfg(target_os = "android")]
            let created = StorageHandle::new(STORAGE_CLASS)?;
            #[cfg(target_os = "ios")]
            let created = StoragePlugin::new()
                .map_err(|error| format!("Failed to create StoragePlugin: {error:?}"))?;
            self.plugin = Some(created);
        }
        Ok(self.plugin.as_ref().unwrap())
    }
    /// Open the store, creating its encryption key if this is the first run.
    pub fn prepare(&mut self) -> Result<(), String> {
        self.get_plugin()?;
        Ok(())
    }
    /// Read a value, or `None` if nothing is stored under `key`.
    pub fn get(&mut self, key: &str) -> Result<Option<String>, String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        let raw = plugin.call_string_str("getFromRust", key)?;
        #[cfg(target_os = "ios")]
        let raw = getFromRust(plugin, key.to_string())?;
        let Some(raw) = raw else {
            return Ok(None);
        };
        Self::unwrap_result(&raw).map(Some)
    }
    /// Write a value, replacing whatever was under `key`.
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        return Self::check(plugin.call_string_str_str("setFromRust", key, value)?);
        #[cfg(target_os = "ios")]
        {
            let entry = serde_json::to_string(&serde_json::json!({
                "key": key,
                "value": value,
            }))
            .map_err(|error| format!("Failed to encode storage entry: {error}"))?;
            Self::check(setFromRust(plugin, entry)?)
        }
    }
    /// Delete a value. Removing a key that is not there is not an error.
    pub fn remove(&mut self, key: &str) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        return Self::check(plugin.call_string_str("removeFromRust", key)?);
        #[cfg(target_os = "ios")]
        return Self::check(removeFromRust(plugin, key.to_string())?);
    }
    /// Delete everything this app stored.
    ///
    /// Scoped to this app: it cannot reach another app's Keychain items or
    /// preferences. On iOS it removes only entries written through this plugin,
    /// not Keychain items the app put there by other means.
    pub fn clear(&mut self) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        return Self::check(plugin.call_string("clearFromRust")?);
        #[cfg(target_os = "ios")]
        return Self::check(clearFromRust(plugin)?);
    }
    /// Every key currently stored, in no particular order.
    pub fn keys(&mut self) -> Result<Vec<String>, String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        let reported = plugin.call_string("keysFromRust")?;
        #[cfg(target_os = "ios")]
        let reported = keysFromRust(plugin)?;
        let raw = reported.ok_or_else(|| "The store did not report its keys".to_string())?;
        let value: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|error| format!("Failed to read stored keys: {error}"))?;
        if let Some(error) = value.get("error").and_then(|error| error.as_str()) {
            return Err(error.to_string());
        }
        serde_json::from_value(value)
            .map_err(|error| format!("Failed to read stored keys: {error}"))
    }
    /// Whether anything is stored under `key`.
    pub fn contains(&mut self, key: &str) -> Result<bool, String> {
        Ok(self.get(key)?.is_some())
    }
    /// The native side answers with the value itself, or with a JSON object
    /// carrying `error`. A stored value that happens to look like that object is
    /// still returned as a value: only a failure produces one.
    fn unwrap_result(raw: &str) -> Result<String, String> {
        if let Some(error) = Self::error_in(raw) {
            return Err(error);
        }
        Ok(raw.to_string())
    }
    fn check(raw: Option<String>) -> Result<(), String> {
        match raw.as_deref().and_then(Self::error_in) {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
    fn error_in(raw: &str) -> Option<String> {
        if !raw.starts_with('{') {
            return None;
        }
        serde_json::from_str::<serde_json::Value>(raw)
            .ok()?
            .get("error")?
            .as_str()
            .map(str::to_string)
    }
}
#[cfg(target_arch = "wasm32")]
impl KeyValueStore {
    pub(crate) fn new() -> Self {
        Self
    }
    /// No-op: `localStorage` needs no setting up.
    pub fn prepare(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn store() -> Result<web_sys::Storage, String> {
        web_sys::window()
            .ok_or_else(|| "Window is not available".to_string())?
            .local_storage()
            .map_err(|_| "Local storage is not available".to_string())?
            .ok_or_else(|| "Local storage is disabled".to_string())
    }
    /// Read a value, or `None` if nothing is stored under `key`.
    pub fn get(&mut self, key: &str) -> Result<Option<String>, String> {
        Self::store()?
            .get_item(key)
            .map_err(|_| "Unable to read from local storage".to_string())
    }
    /// Write a value. Not encrypted: see the type documentation.
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        Self::store()?
            .set_item(key, value)
            // Browsers throw here when the origin's quota is full, and in
            // private modes that refuse storage outright.
            .map_err(|_| "Unable to write to local storage".to_string())
    }
    /// Delete a value.
    pub fn remove(&mut self, key: &str) -> Result<(), String> {
        Self::store()?
            .remove_item(key)
            .map_err(|_| "Unable to remove from local storage".to_string())
    }
    /// Delete everything on this origin.
    pub fn clear(&mut self) -> Result<(), String> {
        Self::store()?
            .clear()
            .map_err(|_| "Unable to clear local storage".to_string())
    }
    /// Every key currently stored, in no particular order.
    pub fn keys(&mut self) -> Result<Vec<String>, String> {
        let store = Self::store()?;
        let length = store
            .length()
            .map_err(|_| "Unable to read local storage".to_string())?;
        let mut keys = Vec::new();
        for index in 0..length {
            if let Ok(Some(key)) = store.key(index) {
                keys.push(key);
            }
        }
        Ok(keys)
    }
    /// Whether anything is stored under `key`.
    pub fn contains(&mut self, key: &str) -> Result<bool, String> {
        Ok(self.get(key)?.is_some())
    }
}
#[cfg(target_os = "macos")]
impl KeyValueStore {
    pub(crate) fn new() -> Self {
        Self
    }
    fn unimplemented<T>() -> Result<T, String> {
        Err("Secure storage is not implemented on macOS".to_string())
    }
    /// Reports an error: there is no store here to open.
    pub fn prepare(&mut self) -> Result<(), String> {
        Self::unimplemented()
    }
    /// Reports an error rather than answering `None`, which would read as
    /// "nothing stored" and hide the fact that nothing can be stored.
    pub fn get(&mut self, _key: &str) -> Result<Option<String>, String> {
        Self::unimplemented()
    }
    /// Reports an error rather than silently discarding the value.
    pub fn set(&mut self, _key: &str, _value: &str) -> Result<(), String> {
        Self::unimplemented()
    }
    /// Reports an error.
    pub fn remove(&mut self, _key: &str) -> Result<(), String> {
        Self::unimplemented()
    }
    /// Reports an error.
    pub fn clear(&mut self) -> Result<(), String> {
        Self::unimplemented()
    }
    /// Reports an error.
    pub fn keys(&mut self) -> Result<Vec<String>, String> {
        Self::unimplemented()
    }
    /// Reports an error.
    pub fn contains(&mut self, _key: &str) -> Result<bool, String> {
        Self::unimplemented()
    }
}
