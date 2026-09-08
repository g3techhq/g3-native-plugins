//! Constructing and calling Kotlin plugin objects from a Dioxus thread.
//!
//! JNI's `FindClass` resolves against the class loader of the thread it is
//! called on. On a thread the JVM created that is the app's loader and
//! everything works; on a thread attached from native code — which is every
//! thread Dioxus runs effects on — it is the system loader, which has never
//! heard of the app's classes. The failure looks like the plugin was never
//! packaged:
//!
//! ```text
//! ClassNotFoundException: Didn't find class "…DeepLinksPlugin"
//!   on path: DexPathList[[directory "."], …]
//! ```
//!
//! The way out is to stop asking the thread and ask the Activity instead: it
//! knows its own loader, and `ClassLoader.loadClass` finds what `FindClass`
//! cannot. Every plugin here goes through that route.
//!
//! Plugins still declare a `#[manganis::ffi]` block holding only the type, with
//! no functions in it. That block is what tells `dx` to build and bundle the
//! Kotlin module; the calls themselves are made here.
use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JClass, JObject, JString, JValue},
};
/// A constructed Kotlin plugin object, with the VM needed to call into it.
pub(crate) struct AndroidPlugin {
    instance: GlobalRef,
    vm: JavaVM,
}
impl AndroidPlugin {
    /// Construct `class_name` with the Activity, through the app's own loader.
    pub(crate) fn new(class_name: &str) -> Result<Self, String> {
        let android = ndk_context::android_context();
        let vm = unsafe { JavaVM::from_raw(android.vm().cast()) }
            .map_err(|error| format!("Failed to access Android VM: {error}"))?;
        let mut env = vm
            .attach_current_thread_permanently()
            .map_err(|error| format!("Failed to attach plugin thread: {error}"))?;
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
            .new_string(class_name)
            .map_err(|error| format!("Failed to create plugin class name: {error}"))?;
        let name_object = JObject::from(name);
        let class_object = env
            .call_method(
                loader,
                "loadClass",
                "(Ljava/lang/String;)Ljava/lang/Class;",
                &[JValue::Object(&name_object)],
            )
            .and_then(|value| value.l())
            .map_err(|error| format!("Failed to load {class_name}: {error}"))?;
        let class = JClass::from(class_object);
        let instance = env
            .new_object(
                class,
                "(Landroid/app/Activity;)V",
                &[JValue::Object(&activity)],
            )
            .map_err(|error| format!("Failed to create {class_name}: {error}"))?;
        let instance = env
            .new_global_ref(instance)
            .map_err(|error| format!("Failed to retain {class_name}: {error}"))?;
        // The Activity reference belongs to the host, not to this JObject
        // wrapper, so it must not be released when the wrapper goes out of
        // scope.
        std::mem::forget(activity);
        Ok(Self { instance, vm })
    }
    fn env(&self) -> Result<JNIEnv<'_>, String> {
        self.vm
            .attach_current_thread_permanently()
            .map_err(|error| format!("Failed to attach plugin thread: {error}"))
    }
    /// Call a `fun name()` returning nothing.
    pub(crate) fn call_unit(&self, method: &str) -> Result<(), String> {
        let mut env = self.env()?;
        env.call_method(self.instance.as_obj(), method, "()V", &[])
            .map_err(|error| format!("Failed to call {method}: {error}"))?;
        Ok(())
    }
    /// Call a `fun name(value: Boolean)` returning nothing.
    pub(crate) fn call_unit_bool(&self, method: &str, value: bool) -> Result<(), String> {
        let mut env = self.env()?;
        env.call_method(
            self.instance.as_obj(),
            method,
            "(Z)V",
            &[JValue::Bool(value.into())],
        )
        .map_err(|error| format!("Failed to call {method}: {error}"))?;
        Ok(())
    }
    /// Call a `fun name(value: String)` returning nothing.
    pub(crate) fn call_unit_str(&self, method: &str, value: &str) -> Result<(), String> {
        let mut env = self.env()?;
        let argument = Self::string(&mut env, value)?;
        env.call_method(
            self.instance.as_obj(),
            method,
            "(Ljava/lang/String;)V",
            &[JValue::Object(&argument)],
        )
        .map_err(|error| format!("Failed to call {method}: {error}"))?;
        Ok(())
    }
    /// Call a `fun name(first: String, second: String)` returning nothing.
    pub(crate) fn call_unit_str_str(
        &self,
        method: &str,
        first: &str,
        second: &str,
    ) -> Result<(), String> {
        let mut env = self.env()?;
        let first = Self::string(&mut env, first)?;
        let second = Self::string(&mut env, second)?;
        env.call_method(
            self.instance.as_obj(),
            method,
            "(Ljava/lang/String;Ljava/lang/String;)V",
            &[JValue::Object(&first), JValue::Object(&second)],
        )
        .map_err(|error| format!("Failed to call {method}: {error}"))?;
        Ok(())
    }
    /// Call a `fun name(value: String, flag: Boolean)` returning nothing.
    pub(crate) fn call_unit_str_bool(
        &self,
        method: &str,
        value: &str,
        flag: bool,
    ) -> Result<(), String> {
        let mut env = self.env()?;
        let argument = Self::string(&mut env, value)?;
        env.call_method(
            self.instance.as_obj(),
            method,
            "(Ljava/lang/String;Z)V",
            &[JValue::Object(&argument), JValue::Bool(flag.into())],
        )
        .map_err(|error| format!("Failed to call {method}: {error}"))?;
        Ok(())
    }
    /// Call a `fun name(): String?`.
    pub(crate) fn call_string(&self, method: &str) -> Result<Option<String>, String> {
        let mut env = self.env()?;
        let value = env
            .call_method(self.instance.as_obj(), method, "()Ljava/lang/String;", &[])
            .map_err(|error| format!("Failed to call {method}: {error}"))?;
        Self::read_string(&mut env, value, method)
    }
    /// Call a `fun name(value: String): String?`.
    pub(crate) fn call_string_str(
        &self,
        method: &str,
        value: &str,
    ) -> Result<Option<String>, String> {
        let mut env = self.env()?;
        let argument = Self::string(&mut env, value)?;
        let value = env
            .call_method(
                self.instance.as_obj(),
                method,
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&argument)],
            )
            .map_err(|error| format!("Failed to call {method}: {error}"))?;
        Self::read_string(&mut env, value, method)
    }
    /// Call a `fun name(first: String, second: String): String?`.
    pub(crate) fn call_string_str_str(
        &self,
        method: &str,
        first: &str,
        second: &str,
    ) -> Result<Option<String>, String> {
        let mut env = self.env()?;
        let first = Self::string(&mut env, first)?;
        let second = Self::string(&mut env, second)?;
        let value = env
            .call_method(
                self.instance.as_obj(),
                method,
                "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&first), JValue::Object(&second)],
            )
            .map_err(|error| format!("Failed to call {method}: {error}"))?;
        Self::read_string(&mut env, value, method)
    }
    fn string<'a>(env: &mut JNIEnv<'a>, value: &str) -> Result<JObject<'a>, String> {
        env.new_string(value)
            .map(JObject::from)
            .map_err(|error| format!("Failed to pass a string to Kotlin: {error}"))
    }
    fn read_string(
        env: &mut JNIEnv<'_>,
        value: jni::objects::JValueGen<JObject<'_>>,
        method: &str,
    ) -> Result<Option<String>, String> {
        let object = value
            .l()
            .map_err(|error| format!("{method} did not return an object: {error}"))?;
        if object.is_null() {
            return Ok(None);
        }
        // Bound rather than inlined: `get_string` borrows the JString, so a
        // temporary would be dropped before the text is read out of it.
        let string = JString::from(object);
        let text = env
            .get_string(&string)
            .map_err(|error| format!("{method} returned unreadable text: {error}"))?;
        Ok(Some(text.into()))
    }
}
