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
//! The Activity comes from `manganis::android::with_activity`, not from
//! `ndk_context`. On the Dioxus fork `ndk_context` holds the Application, and
//! the plugins need a real Activity: they present dialogs, register Back and
//! new-Intent callbacks, and change the window. Android can also recreate the
//! Activity while the process lives on — a configuration change the manifest
//! does not declare it handles, or the system reclaiming a backgrounded
//! Activity — and `dioxus-desktop` reports each new one to manganis. The
//! generated manifest handles rotation and dark mode itself, so this is rare,
//! but it happens. A Kotlin plugin keeps the Activity it was constructed
//! with, so one built for a destroyed Activity would act on a dead window.
//! Each call therefore checks the plugin against the current Activity and
//! rebuilds it when the Activity has changed, then repeats the setup calls
//! made through the `keep_*` methods, so the new Activity is set up the way
//! the old one was. A request in flight on the old plugin (a sign-in, a
//! location fix) is lost with it; it was tied to the destroyed Activity anyway.
//!
//! Plugins still declare a `#[manganis::ffi]` block holding only the type, with
//! no functions in it. That block is what tells `dx` to build and bundle the
//! Kotlin module; the calls themselves are made here.
use std::sync::{Mutex, PoisonError};

use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JClass, JObject, JString, JValue},
};

/// A Kotlin plugin object, rebuilt whenever the Activity it was built for is
/// replaced.
pub(crate) struct AndroidPlugin {
    class_name: String,
    vm: JavaVM,
    built: Mutex<Built>,
    /// Setup calls to repeat on a plugin rebuilt for a new Activity, in the
    /// order first made, with the latest argument for each method.
    kept: Mutex<Vec<(String, Setup)>>,
}

/// One plugin instance and the Activity it was constructed with.
struct Built {
    activity: GlobalRef,
    instance: GlobalRef,
}

/// The argument of a kept setup call.
#[derive(Clone, Copy)]
enum Setup {
    Unit,
    Bool(bool),
}

impl AndroidPlugin {
    /// Construct `class_name` with the current Activity, through the app's own
    /// loader.
    pub(crate) fn new(class_name: &str) -> Result<Self, String> {
        let android = ndk_context::android_context();
        let vm = unsafe { JavaVM::from_raw(android.vm().cast()) }
            .map_err(|error| format!("Failed to access Android VM: {error}"))?;
        let built = with_activity(&vm, |env, activity| Built::new(env, activity, class_name))?;
        Ok(Self {
            class_name: class_name.to_owned(),
            vm,
            built: Mutex::new(built),
            kept: Mutex::new(Vec::new()),
        })
    }

    /// Run `call` against a plugin built for the current Activity.
    fn with_instance<R>(
        &self,
        call: impl FnOnce(&mut JNIEnv<'_>, &JObject<'_>) -> Result<R, String>,
    ) -> Result<R, String> {
        with_activity(&self.vm, |env, activity| {
            let instance = self.instance_for(env, activity)?;
            call(env, instance.as_obj())
        })
    }

    /// The plugin for `activity`, rebuilt and set up again if the one held was
    /// built for an earlier Activity.
    fn instance_for(
        &self,
        env: &mut JNIEnv<'_>,
        activity: &JObject<'_>,
    ) -> Result<GlobalRef, String> {
        let mut built = self.built.lock().unwrap_or_else(PoisonError::into_inner);
        let current = env
            .is_same_object(built.activity.as_obj(), activity)
            .map_err(|error| format!("Failed to compare Android activities: {error}"))?;
        if !current {
            // Stored before replaying, so a failed replay still leaves a
            // plugin for the live Activity rather than the destroyed one.
            *built = Built::new(env, activity, &self.class_name)?;
            let kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
            for (method, setup) in kept.iter() {
                setup.call(env, built.instance.as_obj(), method)?;
            }
        }
        Ok(built.instance.clone())
    }

    /// Remember a setup call so a plugin rebuilt for a new Activity gets it
    /// too. Recorded after the call succeeds, so a rebuild that call triggers
    /// replays the earlier value and the call itself then applies the new one.
    fn keep(&self, method: &str, setup: Setup) {
        let mut kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        match kept.iter_mut().find(|(name, _)| name == method) {
            Some(entry) => entry.1 = setup,
            None => kept.push((method.to_owned(), setup)),
        }
    }

    /// Call a `fun name()` that sets up the Activity, and call it again on any
    /// later Activity.
    pub(crate) fn keep_unit(&self, method: &str) -> Result<(), String> {
        self.call_unit(method)?;
        self.keep(method, Setup::Unit);
        Ok(())
    }

    /// Call a `fun name(value: Boolean)` that sets up the Activity, and call it
    /// again with the latest value on any later Activity.
    pub(crate) fn keep_unit_bool(&self, method: &str, value: bool) -> Result<(), String> {
        self.call_unit_bool(method, value)?;
        self.keep(method, Setup::Bool(value));
        Ok(())
    }

    /// Call a `fun name()` returning nothing.
    pub(crate) fn call_unit(&self, method: &str) -> Result<(), String> {
        self.with_instance(|env, instance| Setup::Unit.call(env, instance, method))
    }

    /// Call a `fun name(value: Boolean)` returning nothing.
    pub(crate) fn call_unit_bool(&self, method: &str, value: bool) -> Result<(), String> {
        self.with_instance(|env, instance| Setup::Bool(value).call(env, instance, method))
    }

    /// Call a `fun name(first: Int, second: Int)` returning nothing.
    pub(crate) fn call_unit_int_int(
        &self,
        method: &str,
        first: i32,
        second: i32,
    ) -> Result<(), String> {
        self.with_instance(|env, instance| {
            env.call_method(
                instance,
                method,
                "(II)V",
                &[JValue::Int(first), JValue::Int(second)],
            )
            .map_err(|error| format!("Failed to call {method}: {error}"))?;
            Ok(())
        })
    }

    /// Call a `fun name(value: String)` returning nothing.
    pub(crate) fn call_unit_str(&self, method: &str, value: &str) -> Result<(), String> {
        self.with_instance(|env, instance| {
            let argument = Self::string(env, value)?;
            env.call_method(
                instance,
                method,
                "(Ljava/lang/String;)V",
                &[JValue::Object(&argument)],
            )
            .map_err(|error| format!("Failed to call {method}: {error}"))?;
            Ok(())
        })
    }

    /// Call a `fun name(first: String, second: String)` returning nothing.
    pub(crate) fn call_unit_str_str(
        &self,
        method: &str,
        first: &str,
        second: &str,
    ) -> Result<(), String> {
        self.with_instance(|env, instance| {
            let first = Self::string(env, first)?;
            let second = Self::string(env, second)?;
            env.call_method(
                instance,
                method,
                "(Ljava/lang/String;Ljava/lang/String;)V",
                &[JValue::Object(&first), JValue::Object(&second)],
            )
            .map_err(|error| format!("Failed to call {method}: {error}"))?;
            Ok(())
        })
    }

    /// Call a `fun name(value: String, flag: Boolean)` returning nothing.
    pub(crate) fn call_unit_str_bool(
        &self,
        method: &str,
        value: &str,
        flag: bool,
    ) -> Result<(), String> {
        self.with_instance(|env, instance| {
            let argument = Self::string(env, value)?;
            env.call_method(
                instance,
                method,
                "(Ljava/lang/String;Z)V",
                &[JValue::Object(&argument), JValue::Bool(flag.into())],
            )
            .map_err(|error| format!("Failed to call {method}: {error}"))?;
            Ok(())
        })
    }

    /// Call a `fun name(flag: Boolean, value: String)` returning nothing.
    pub(crate) fn call_unit_bool_str(
        &self,
        method: &str,
        flag: bool,
        value: &str,
    ) -> Result<(), String> {
        self.with_instance(|env, instance| {
            let argument = Self::string(env, value)?;
            env.call_method(
                instance,
                method,
                "(ZLjava/lang/String;)V",
                &[JValue::Bool(flag.into()), JValue::Object(&argument)],
            )
            .map_err(|error| format!("Failed to call {method}: {error}"))?;
            Ok(())
        })
    }

    /// Call a `fun name(): String?`.
    pub(crate) fn call_string(&self, method: &str) -> Result<Option<String>, String> {
        self.with_instance(|env, instance| {
            let value = env
                .call_method(instance, method, "()Ljava/lang/String;", &[])
                .map_err(|error| format!("Failed to call {method}: {error}"))?;
            Self::read_string(env, value, method)
        })
    }

    /// Call a `fun name(value: String): String?`.
    pub(crate) fn call_string_str(
        &self,
        method: &str,
        value: &str,
    ) -> Result<Option<String>, String> {
        self.with_instance(|env, instance| {
            let argument = Self::string(env, value)?;
            let value = env
                .call_method(
                    instance,
                    method,
                    "(Ljava/lang/String;)Ljava/lang/String;",
                    &[JValue::Object(&argument)],
                )
                .map_err(|error| format!("Failed to call {method}: {error}"))?;
            Self::read_string(env, value, method)
        })
    }

    /// Call a `fun name(first: String, second: String): String?`.
    pub(crate) fn call_string_str_str(
        &self,
        method: &str,
        first: &str,
        second: &str,
    ) -> Result<Option<String>, String> {
        self.with_instance(|env, instance| {
            let first = Self::string(env, first)?;
            let second = Self::string(env, second)?;
            let value = env
                .call_method(
                    instance,
                    method,
                    "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                    &[JValue::Object(&first), JValue::Object(&second)],
                )
                .map_err(|error| format!("Failed to call {method}: {error}"))?;
            Self::read_string(env, value, method)
        })
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

impl Built {
    /// Construct `class_name` with `activity`, through the app's own loader.
    fn new(env: &mut JNIEnv<'_>, activity: &JObject<'_>, class_name: &str) -> Result<Self, String> {
        let loader = env
            .call_method(activity, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
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
                &[JValue::Object(activity)],
            )
            .map_err(|error| format!("Failed to create {class_name}: {error}"))?;
        let instance = env
            .new_global_ref(instance)
            .map_err(|error| format!("Failed to retain {class_name}: {error}"))?;
        let activity = env
            .new_global_ref(activity)
            .map_err(|error| format!("Failed to retain the Activity: {error}"))?;
        Ok(Self { activity, instance })
    }
}

impl Setup {
    fn call(
        self,
        env: &mut JNIEnv<'_>,
        instance: &JObject<'_>,
        method: &str,
    ) -> Result<(), String> {
        match self {
            Self::Unit => env.call_method(instance, method, "()V", &[]),
            Self::Bool(value) => {
                env.call_method(instance, method, "(Z)V", &[JValue::Bool(value.into())])
            }
        }
        .map_err(|error| format!("Failed to call {method}: {error}"))?;
        Ok(())
    }
}

/// Run `call` with the current Activity.
///
/// The thread is attached for good first: `with_activity` attaches only for
/// the length of its closure, and Dioxus calls plugins from the same few
/// worker threads over and over, so detaching after each call would only buy
/// another attach on the next one.
fn with_activity<R>(
    vm: &JavaVM,
    call: impl FnOnce(&mut JNIEnv<'_>, &JObject<'_>) -> Result<R, String>,
) -> Result<R, String> {
    vm.attach_current_thread_permanently()
        .map_err(|error| format!("Failed to attach plugin thread: {error}"))?;
    manganis::android::with_activity(|env, activity| Some(call(env, activity)))
        .ok_or_else(|| "No Android Activity is available".to_string())?
}
