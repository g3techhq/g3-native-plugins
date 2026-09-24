#[cfg(any(target_os = "android", target_os = "ios", test))]
use serde::Deserialize;
use serde::Serialize;
/// Declared for its side effect: this is what tells `dx` to build and bundle
/// the Kotlin module. The calls go through [`crate::android_bridge`] instead,
/// because the macro's own bindings resolve classes against the calling
/// thread's loader and Dioxus does not call from a thread that has one.
#[cfg(target_os = "android")]
#[manganis::ffi("src/android/push_notifications")]
unsafe extern "Kotlin" {
    pub type PushNotificationsPlugin;
}
#[cfg(target_os = "android")]
const PUSH_NOTIFICATIONS_CLASS: &str =
    "dev.dioxus.g3_native_plugins.push_notifications.PushNotificationsPlugin";
#[cfg(target_os = "android")]
type PushNotificationsHandle = crate::android_bridge::AndroidPlugin;
#[cfg(target_os = "ios")]
type PushNotificationsHandle = PushNotificationsPlugin;
#[cfg(target_os = "ios")]
#[manganis::ffi("src/ios")]
unsafe extern "Swift" {
    pub type PushNotificationsPlugin;
    pub fn prepareFromRust(this: &PushNotificationsPlugin);
    pub fn registerFromRust(this: &PushNotificationsPlugin, configJson: String) -> Option<String>;
    pub fn unregisterFromRust(this: &PushNotificationsPlugin) -> Option<String>;
    pub fn tokenFromRust(this: &PushNotificationsPlugin) -> Option<String>;
    pub fn takeEventFromRust(this: &PushNotificationsPlugin) -> Option<String>;
}
/// The four values Firebase Cloud Messaging needs on Android, from the
/// Firebase console's Android app settings or its `google-services.json`.
/// Ignored on iOS, which receives push from APNs directly.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FirebaseOptions {
    /// `mobilesdk_app_id`, shaped like `1:1234567890:android:abc123`.
    pub application_id: String,
    /// `current_key` under `api_key`.
    pub api_key: String,
    /// `project_id`.
    pub project_id: String,
    /// `project_number`, which FCM calls the sender id.
    pub sender_id: String,
}
/// Which service a [`PushToken`] addresses, and so which API the server sends
/// through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PushService {
    /// Apple Push Notification service: an iOS device token, hex encoded.
    Apns,
    /// Firebase Cloud Messaging: an Android registration token.
    Fcm,
}
/// The address a server pushes to for this install.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct PushToken {
    /// The service the token belongs to.
    pub service: PushService,
    /// The token itself. Send it to your server; it changes from time to time,
    /// and each change arrives as a [`PushEvent::Token`].
    pub token: String,
}
/// Something that happened with remote push.
#[derive(Debug, Clone, PartialEq)]
pub enum PushEvent {
    /// This install has a token, new or changed. Send it to the server.
    Token(PushToken),
    /// Registering with APNs or FCM failed.
    RegistrationFailed(String),
    /// A push arrived that the system did not display: any push while the app
    /// is in the foreground, and data-only pushes in the background. Show it
    /// yourself with [`crate::Notifications::show`] if it should be seen.
    Message {
        /// The push's custom data. On iOS the whole payload, `aps` included.
        data: serde_json::Map<String, serde_json::Value>,
        /// The notification title, if the push had one.
        title: Option<String>,
        /// The notification body, if the push had one.
        body: Option<String>,
    },
    /// The user tapped a push the system displayed.
    Opened {
        /// `"tap"` for the notification itself, otherwise the button's id.
        action_id: String,
        /// The push's custom data.
        data: serde_json::Map<String, serde_json::Value>,
    },
}
/// How events cross the bridge. Kept apart from [`PushEvent`] so the public
/// type can carry the service a token belongs to, which only Rust knows.
#[cfg(any(target_os = "android", target_os = "ios", test))]
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum WireEvent {
    Token {
        token: String,
    },
    RegistrationFailed {
        error: String,
    },
    Message {
        #[serde(default)]
        data: serde_json::Map<String, serde_json::Value>,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        body: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Opened {
        #[serde(default = "tap")]
        action_id: String,
        #[serde(default)]
        data: serde_json::Map<String, serde_json::Value>,
    },
}
#[cfg(any(target_os = "android", target_os = "ios", test))]
fn tap() -> String {
    crate::notifications::TAP_ACTION.to_string()
}
#[cfg(any(target_os = "android", target_os = "ios", test))]
impl WireEvent {
    fn into_event(self, service: PushService) -> PushEvent {
        match self {
            WireEvent::Token { token } => PushEvent::Token(PushToken { service, token }),
            WireEvent::RegistrationFailed { error } => PushEvent::RegistrationFailed(error),
            WireEvent::Message { data, title, body } => PushEvent::Message { data, title, body },
            WireEvent::Opened { action_id, data } => PushEvent::Opened { action_id, data },
        }
    }
}
/// Remote push: APNs on iOS, Firebase Cloud Messaging on Android.
///
/// ```rust,ignore
/// fn main() {
///     // Early, so a push that launched the app is not missed.
///     let _ = Notifications::new().prepare();
///     let _ = PushNotifications::new().prepare();
///     dioxus::launch(App);
/// }
///
/// let mut plugins = use_context::<NativePlugins>();
/// plugins.notifications.write().request_permissions()?;
/// plugins.push_notifications.write().register(Some(&FIREBASE_OPTIONS))?;
///
/// while let Ok(Some(event)) = plugins.push_notifications.write().take_event() {
///     match event {
///         PushEvent::Token(token) => { /* send token.token to the server */ }
///         PushEvent::Opened { data, .. } => { /* route on data */ }
///         _ => {}
///     }
/// }
/// ```
///
/// A push can only be *seen* with the notification permission, which is
/// [`crate::Notifications`]'s to ask for — that is why this feature turns that
/// one on. Registering and receiving data pushes need no permission at all.
///
/// Neither platform lets a library do this alone:
///
/// - **iOS** needs the Push Notifications capability — an `aps-environment`
///   entitlement in the app's provisioning profile — and, for data-only
///   pushes, the `remote-notification` background mode. Without the
///   entitlement registration fails, which arrives as
///   [`PushEvent::RegistrationFailed`].
/// - **Android** needs a Firebase project with this app registered in it.
///   Pass its [`FirebaseOptions`] to [`register`](PushNotifications::register).
///
/// On iOS the token goes to APNs directly; there is no Firebase on that side.
/// A server that sends through FCM for both platforms can still address the
/// APNs token through FCM's APNs support, or send to APNs itself.
///
/// The web and macOS builds are inert, so callers need no cfg of their own.
#[cfg(any(target_os = "android", target_os = "ios"))]
pub struct PushNotifications {
    plugin: Option<PushNotificationsHandle>,
}
/// The inert push facade: see the native [`PushNotifications`] for the
/// contract.
#[cfg(any(target_arch = "wasm32", target_os = "macos"))]
pub struct PushNotifications;
#[cfg(target_os = "android")]
const SERVICE: PushService = PushService::Fcm;
#[cfg(target_os = "ios")]
const SERVICE: PushService = PushService::Apns;
#[cfg(any(target_os = "android", target_os = "ios"))]
fn check(reported: Option<String>) -> Result<(), String> {
    let Some(raw) = reported else {
        return Ok(());
    };
    match serde_json::from_str::<serde_json::Value>(&raw) {
        Ok(value) => match value.get("error").and_then(|error| error.as_str()) {
            Some(error) => Err(error.to_string()),
            None => Ok(()),
        },
        Err(_) => Ok(()),
    }
}
#[cfg(any(target_os = "android", target_os = "ios"))]
impl PushNotifications {
    /// Create the facade without touching the platform.
    ///
    /// Public so an app can [`prepare`](PushNotifications::prepare) in `main`.
    /// Events are queued natively and shared across instances.
    pub fn new() -> Self {
        Self { plugin: None }
    }
    fn get_plugin(&mut self) -> Result<&PushNotificationsHandle, String> {
        if self.plugin.is_none() {
            #[cfg(target_os = "android")]
            let created = PushNotificationsHandle::new(PUSH_NOTIFICATIONS_CLASS)?;
            #[cfg(target_os = "ios")]
            let created = PushNotificationsPlugin::new()
                .map_err(|error| format!("Failed to create PushNotificationsPlugin: {error:?}"))?;
            self.plugin = Some(created);
        }
        Ok(self.plugin.as_ref().unwrap())
    }
    /// Start listening for taps on pushes and, on iOS, for the token. Call it
    /// in `main`: a push that launched the app is delivered during launch.
    pub fn prepare(&mut self) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        plugin.call_unit("prepareFromRust")?;
        #[cfg(target_os = "ios")]
        prepareFromRust(plugin)?;
        Ok(())
    }
    /// Register with APNs or FCM. Returns at once; the token arrives as a
    /// [`PushEvent::Token`] and from [`token`](PushNotifications::token).
    ///
    /// `firebase` is required on Android unless the app bundles its own
    /// Firebase configuration, and ignored on iOS.
    pub fn register(&mut self, firebase: Option<&FirebaseOptions>) -> Result<(), String> {
        let config = serde_json::json!({ "firebase": firebase }).to_string();
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        return check(plugin.call_string_str("registerFromRust", &config)?);
        #[cfg(target_os = "ios")]
        return check(registerFromRust(plugin, config)?);
    }
    /// Stop receiving pushes: the token is invalidated, and a later
    /// [`register`](PushNotifications::register) gets a new one.
    pub fn unregister(&mut self) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        return check(plugin.call_string("unregisterFromRust")?);
        #[cfg(target_os = "ios")]
        return check(unregisterFromRust(plugin)?);
    }
    /// The last token this install received, if any.
    pub fn token(&mut self) -> Result<Option<PushToken>, String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        let token = plugin.call_string("tokenFromRust")?;
        #[cfg(target_os = "ios")]
        let token = tokenFromRust(plugin)?;
        Ok(token.map(|token| PushToken {
            service: SERVICE,
            token,
        }))
    }
    /// The oldest event not yet handled, or `None` when none are waiting.
    /// Drain in a loop until it returns `None`.
    pub fn take_event(&mut self) -> Result<Option<PushEvent>, String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        let reported = plugin.call_string("takeEventFromRust")?;
        #[cfg(target_os = "ios")]
        let reported = takeEventFromRust(plugin)?;
        let Some(raw) = reported else {
            return Ok(None);
        };
        let wire: WireEvent = serde_json::from_str(&raw)
            .map_err(|error| format!("Failed to read a push event: {error}"))?;
        Ok(Some(wire.into_event(SERVICE)))
    }
}
#[cfg(any(target_arch = "wasm32", target_os = "macos"))]
impl PushNotifications {
    /// Create the inert facade.
    pub fn new() -> Self {
        Self
    }
    /// No-op.
    pub fn prepare(&mut self) -> Result<(), String> {
        Ok(())
    }
    /// No-op: no token will arrive.
    pub fn register(&mut self, _firebase: Option<&FirebaseOptions>) -> Result<(), String> {
        Ok(())
    }
    /// No-op.
    pub fn unregister(&mut self) -> Result<(), String> {
        Ok(())
    }
    /// Always `None`.
    pub fn token(&mut self) -> Result<Option<PushToken>, String> {
        Ok(None)
    }
    /// Always `None`.
    pub fn take_event(&mut self) -> Result<Option<PushEvent>, String> {
        Ok(None)
    }
}
#[cfg(any(
    target_arch = "wasm32",
    target_os = "android",
    target_os = "ios",
    target_os = "macos"
))]
impl Default for PushNotifications {
    fn default() -> Self {
        Self::new()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn event(json: &str, service: PushService) -> PushEvent {
        serde_json::from_str::<WireEvent>(json)
            .expect("wire event should parse")
            .into_event(service)
    }
    #[test]
    fn events_from_either_platform_parse_into_the_public_shape() {
        assert_eq!(
            event(r#"{"type":"token","token":"abc"}"#, PushService::Fcm),
            PushEvent::Token(PushToken {
                service: PushService::Fcm,
                token: "abc".to_string(),
            })
        );
        let PushEvent::Message { data, title, .. } = event(
            r#"{"type":"message","messageId":"1","title":"Hi","body":null,"data":{"game":"7"}}"#,
            PushService::Fcm,
        ) else {
            panic!("a message");
        };
        assert_eq!(title.as_deref(), Some("Hi"));
        assert_eq!(data["game"], "7");
        // iOS leaves the action out of a plain tap on a push.
        assert_eq!(
            event(r#"{"type":"opened","data":{}}"#, PushService::Apns),
            PushEvent::Opened {
                action_id: "tap".to_string(),
                data: serde_json::Map::new(),
            }
        );
        assert_eq!(
            event(
                r#"{"type":"registrationFailed","error":"no entitlement"}"#,
                PushService::Apns
            ),
            PushEvent::RegistrationFailed("no entitlement".to_string())
        );
    }
    #[test]
    fn firebase_options_reach_kotlin_under_the_names_it_reads() {
        let options = FirebaseOptions {
            application_id: "1:2:android:3".to_string(),
            api_key: "key".to_string(),
            project_id: "project".to_string(),
            sender_id: "2".to_string(),
        };
        let config = serde_json::json!({ "firebase": options });
        for key in ["applicationId", "apiKey", "projectId", "senderId"] {
            assert!(config["firebase"].get(key).is_some(), "{key}");
        }
    }
}
