#[cfg(target_os = "android")]
#[manganis::ffi("src/android/media")]
unsafe extern "Kotlin" {
    pub type MediaPlugin;
}

#[cfg(target_os = "android")]
use jni::{
    JavaVM,
    objects::{GlobalRef, JClass, JObject, JValue},
};

#[cfg(target_os = "android")]
const MEDIA_PLUGIN_CLASS: &str = "dev.dioxus.dx_native_plugins.media.MediaPlugin";

#[cfg(target_os = "android")]
pub struct Media {
    plugin: Option<GlobalRef>,
    vm: Option<JavaVM>,
}

#[cfg(any(target_arch = "wasm32", target_os = "ios", target_os = "macos"))]
pub struct Media;

#[cfg(target_os = "android")]
impl Media {
    pub(crate) fn new() -> Self {
        Self {
            plugin: None,
            vm: None,
        }
    }

    fn get_plugin(&mut self) -> Result<GlobalRef, String> {
        if self.plugin.is_none() {
            let android = ndk_context::android_context();
            let vm = unsafe { JavaVM::from_raw(android.vm().cast()) }
                .map_err(|error| format!("Failed to access Android VM: {error}"))?;
            let mut env = vm
                .attach_current_thread_permanently()
                .map_err(|error| format!("Failed to attach media plugin thread: {error}"))?;
            let activity = unsafe { JObject::from_raw(android.context().cast()) };
            let loader = env
                .call_method(
                    &activity,
                    "getClassLoader",
                    "()Ljava/lang/ClassLoader;",
                    &[],
                )
                .and_then(|value| value.l())
                .map_err(|error| format!("Failed to obtain app class loader: {error}"))?;
            let name = env
                .new_string(MEDIA_PLUGIN_CLASS)
                .map_err(|error| format!("Failed to create media plugin class name: {error}"))?;
            let name_object = JObject::from(name);
            let class_object = env
                .call_method(
                    loader,
                    "loadClass",
                    "(Ljava/lang/String;)Ljava/lang/Class;",
                    &[JValue::Object(&name_object)],
                )
                .and_then(|value| value.l())
                .map_err(|error| format!("Failed to load MediaPlugin: {error}"))?;
            let class = JClass::from(class_object);
            let instance = env
                .new_object(
                    class,
                    "(Landroid/app/Activity;)V",
                    &[JValue::Object(&activity)],
                )
                .map_err(|error| format!("Failed to create MediaPlugin: {error}"))?;
            self.plugin = Some(
                env.new_global_ref(instance)
                    .map_err(|error| format!("Failed to retain MediaPlugin: {error}"))?,
            );
            self.vm = Some(vm);
            std::mem::forget(activity);
        }
        Ok(self.plugin.as_ref().unwrap().clone())
    }

    fn env(&self) -> Result<jni::JNIEnv<'_>, String> {
        let vm = self
            .vm
            .as_ref()
            .ok_or_else(|| "Media plugin VM is not initialized".to_string())?;
        vm.attach_current_thread_permanently()
            .map_err(|error| format!("Failed to attach media plugin thread: {error}"))
    }

    pub fn prepare(&mut self) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        let mut env = self.env()?;
        env.call_method(plugin.as_obj(), "prepareFromRust", "()V", &[])
            .map_err(|error| format!("Failed to prepare media plugin: {error}"))?;
        Ok(())
    }

    pub fn enter_picture_in_picture(&mut self, width: i32, height: i32) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        let mut env = self.env()?;
        env.call_method(
            plugin.as_obj(),
            "enterPictureInPictureFromRust",
            "(II)V",
            &[JValue::Int(width), JValue::Int(height)],
        )
        .map_err(|error| format!("Failed to enter picture-in-picture: {error}"))?;
        Ok(())
    }

    pub fn set_orientation(&mut self, orientation: impl Into<String>) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        let mut env = self.env()?;
        let orientation = env
            .new_string(orientation.into())
            .map_err(|error| format!("Failed to prepare orientation: {error}"))?;
        let orientation = JObject::from(orientation);
        env.call_method(
            plugin.as_obj(),
            "setOrientationFromRust",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&orientation)],
        )
        .map_err(|error| format!("Failed to set orientation: {error}"))?;
        Ok(())
    }

    pub fn set_playback_active(
        &mut self,
        active: bool,
        title: impl Into<String>,
    ) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        let mut env = self.env()?;
        let title = env
            .new_string(title.into())
            .map_err(|error| format!("Failed to prepare playback title: {error}"))?;
        let title = JObject::from(title);
        env.call_method(
            plugin.as_obj(),
            "setPlaybackActiveFromRust",
            "(ZLjava/lang/String;)V",
            &[JValue::Bool(active.into()), JValue::Object(&title)],
        )
        .map_err(|error| format!("Failed to update background playback: {error}"))?;
        Ok(())
    }
}

#[cfg(any(target_arch = "wasm32", target_os = "ios", target_os = "macos"))]
impl Media {
    pub(crate) fn new() -> Self {
        Self
    }

    pub fn prepare(&mut self) -> Result<(), String> {
        Ok(())
    }

    pub fn enter_picture_in_picture(&mut self, _width: i32, _height: i32) -> Result<(), String> {
        Ok(())
    }

    pub fn set_orientation(&mut self, _orientation: impl Into<String>) -> Result<(), String> {
        Ok(())
    }

    pub fn set_playback_active(
        &mut self,
        _active: bool,
        _title: impl Into<String>,
    ) -> Result<(), String> {
        Ok(())
    }
}
