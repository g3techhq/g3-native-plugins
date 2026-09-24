#[cfg(any(
    target_arch = "wasm32",
    target_os = "android",
    target_os = "ios",
    target_os = "macos"
))]
use crate::permissions::PermissionState;
use serde::{Deserialize, Serialize};
/// Declared for its side effect: this is what tells `dx` to build and bundle
/// the Kotlin module. The calls go through [`crate::android_bridge`] instead,
/// because the macro's own bindings resolve classes against the calling
/// thread's loader and Dioxus does not call from a thread that has one.
#[cfg(target_os = "android")]
#[manganis::ffi("src/android/notifications")]
unsafe extern "Kotlin" {
    pub type NotificationsPlugin;
}
#[cfg(target_os = "android")]
const NOTIFICATIONS_CLASS: &str = "dev.dioxus.g3_native_plugins.notifications.NotificationsPlugin";
#[cfg(target_os = "android")]
type NotificationsHandle = crate::android_bridge::AndroidPlugin;
#[cfg(target_os = "ios")]
type NotificationsHandle = NotificationsPlugin;
#[cfg(target_os = "ios")]
#[manganis::ffi("src/ios")]
unsafe extern "Swift" {
    pub type NotificationsPlugin;
    pub fn prepareFromRust(this: &NotificationsPlugin);
    pub fn checkPermissionsFromRust(this: &NotificationsPlugin) -> Option<String>;
    pub fn requestPermissionsFromRust(this: &NotificationsPlugin);
    pub fn openSettingsFromRust(this: &NotificationsPlugin);
    pub fn showFromRust(this: &NotificationsPlugin, notificationJson: String) -> Option<String>;
    pub fn pendingFromRust(this: &NotificationsPlugin) -> Option<String>;
    pub fn cancelFromRust(this: &NotificationsPlugin, idsJson: String) -> Option<String>;
    pub fn cancelAllFromRust(this: &NotificationsPlugin) -> Option<String>;
    pub fn activeFromRust(this: &NotificationsPlugin) -> Option<String>;
    pub fn removeActiveFromRust(this: &NotificationsPlugin, idsJson: String) -> Option<String>;
    pub fn removeAllActiveFromRust(this: &NotificationsPlugin) -> Option<String>;
    pub fn createChannelFromRust(this: &NotificationsPlugin, channelJson: String)
    -> Option<String>;
    pub fn deleteChannelFromRust(this: &NotificationsPlugin, id: String) -> Option<String>;
    pub fn channelsFromRust(this: &NotificationsPlugin) -> Option<String>;
    pub fn registerActionTypesFromRust(
        this: &NotificationsPlugin,
        typesJson: String,
    ) -> Option<String>;
    pub fn takeEventFromRust(this: &NotificationsPlugin) -> Option<String>;
}
/// The action id reported when the user taps the notification itself rather
/// than one of its buttons.
pub const TAP_ACTION: &str = "tap";
/// A notification to show now or later.
///
/// ```rust,ignore
/// let reminder = Notification::new(7, "Tee time in 30 minutes")
///     .body("Hole 1, Pine Valley")
///     .schedule(Schedule::At { at_ms: tee_time_ms - 30 * 60_000, allow_while_idle: true });
/// plugins.notifications.write().show(&reminder)?;
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    /// Identifies the notification for [`Notifications::cancel`] and
    /// [`Notifications::remove_active`]. Showing another with the same id
    /// replaces it.
    pub id: i32,
    /// The bold first line.
    pub title: String,
    /// The text under the title.
    pub body: Option<String>,
    /// Android: the channel to post to, which decides its sound, importance,
    /// and whether it makes any noise at all. A `default` channel is created on
    /// first use when this is `None`. Ignored on iOS.
    pub channel_id: Option<String>,
    /// Groups related notifications together: Android's group key, iOS's
    /// thread identifier.
    pub group: Option<String>,
    /// Which [`ActionType`] supplies this notification's buttons.
    pub action_type_id: Option<String>,
    /// iOS: a sound file in the app bundle to play instead of the default.
    /// Android 8+ takes sound from the channel instead; see [`Channel`].
    pub sound: Option<String>,
    /// Play no sound. On Android 8+ the channel decides, so use a channel with
    /// [`Importance::Low`] for quiet notifications there.
    pub silent: bool,
    /// iOS: the number to show on the app icon. Android: the count some
    /// launchers show on the notification.
    pub badge: Option<u32>,
    /// Android: a drawable resource name for the status-bar icon, which must
    /// be a white-on-transparent silhouette. Falls back to the app icon, which
    /// Android renders as a white square. Ignored on iOS.
    pub icon: Option<String>,
    /// Anything the app wants back when the notification is delivered or
    /// tapped: a route, a record id.
    pub extra: serde_json::Map<String, serde_json::Value>,
    /// When to show it. `None` shows it now.
    pub schedule: Option<Schedule>,
}
impl Notification {
    /// A notification with only an id and a title.
    pub fn new(id: i32, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            ..Self::default()
        }
    }
    /// Set the text under the title.
    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }
    /// Show it later rather than now.
    pub fn schedule(mut self, schedule: Schedule) -> Self {
        self.schedule = Some(schedule);
        self
    }
    /// Post to an Android channel created with [`Notifications::create_channel`].
    pub fn channel(mut self, channel_id: impl Into<String>) -> Self {
        self.channel_id = Some(channel_id.into());
        self
    }
    /// Give it the buttons of a registered [`ActionType`].
    pub fn action_type(mut self, action_type_id: impl Into<String>) -> Self {
        self.action_type_id = Some(action_type_id.into());
        self
    }
    /// Attach a value the app gets back in [`NotificationEvent`]s.
    pub fn extra(mut self, key: impl Into<String>, value: impl Into<serde_json::Value>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }
}
/// When a scheduled notification fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Schedule {
    /// Once, at a moment in time. A moment already past fires immediately.
    #[serde(rename_all = "camelCase")]
    At {
        /// Milliseconds since the Unix epoch.
        at_ms: i64,
        /// Android: fire even in Doze. Costs battery, so only for things the
        /// user would be upset to get late. iOS always delivers on time.
        allow_while_idle: bool,
    },
    /// Repeatedly, every `count` intervals, starting one period from now.
    #[serde(rename_all = "camelCase")]
    Every {
        /// The unit.
        interval: ScheduleInterval,
        /// How many units between firings. iOS refuses repeats more often
        /// than once a minute.
        count: u32,
    },
}
/// The unit of a repeating [`Schedule`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScheduleInterval {
    /// Sixty seconds.
    Minute,
    /// Sixty minutes.
    Hour,
    /// Twenty-four hours.
    Day,
    /// Seven days.
    Week,
}
/// A scheduled notification that has not fired yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingNotification {
    /// The id it was shown with.
    pub id: i32,
    /// Its title.
    pub title: String,
    /// Its body.
    pub body: Option<String>,
    /// When it will fire.
    pub schedule: Option<Schedule>,
}
/// A notification as it comes back from the system: one on screen, or one in
/// a [`NotificationEvent`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DeliveredNotification {
    /// The id it was shown with.
    pub id: i32,
    /// Its title.
    pub title: String,
    /// Its body.
    pub body: Option<String>,
    /// Its group or thread.
    pub group: Option<String>,
    /// The action type that supplied its buttons.
    pub action_type_id: Option<String>,
    /// The [`Notification::extra`] it was shown with.
    pub extra: serde_json::Map<String, serde_json::Value>,
}
/// Something that happened to a notification this app showed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum NotificationEvent {
    /// Delivered while the app was in the foreground.
    Received {
        /// What was delivered.
        notification: DeliveredNotification,
    },
    /// The user tapped the notification or one of its buttons.
    #[serde(rename_all = "camelCase")]
    Action {
        /// [`TAP_ACTION`] for the notification itself, otherwise the
        /// [`Action::id`] of the button.
        action_id: String,
        /// What the user typed, for an [`Action`] with `input`.
        input: Option<String>,
        /// What was tapped.
        notification: DeliveredNotification,
    },
}
/// How insistently an Android channel interrupts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Importance {
    /// Never shown.
    None,
    /// In the shade only, collapsed.
    Min,
    /// Shown, silently.
    Low,
    /// Shown, with sound.
    #[default]
    Default,
    /// Shown, with sound, and peeks onto the screen.
    High,
}
/// What an Android channel shows on the lock screen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Visibility {
    /// That a notification arrived, not what it says.
    #[default]
    Private,
    /// All of it.
    Public,
    /// Nothing.
    Secret,
}
/// An Android notification channel: the unit the user turns on and off in
/// Settings, and the owner of a notification's sound and importance on
/// Android 8 and later. iOS has no channels, and the channel calls do nothing
/// there.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Channel {
    /// Stable identifier, used by [`Notification::channel_id`].
    pub id: String,
    /// Name shown in Settings.
    pub name: String,
    /// Description shown in Settings.
    pub description: Option<String>,
    /// How insistently it interrupts.
    pub importance: Importance,
    /// What it shows on the lock screen.
    pub visibility: Visibility,
    /// Whether it vibrates.
    pub vibration: bool,
    /// A sound in `res/raw` to play instead of the default, by resource name.
    pub sound: Option<String>,
}
/// A set of buttons a notification can carry, registered once and referred to
/// by [`Notification::action_type_id`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionType {
    /// Identifier for [`Notification::action_type_id`].
    pub id: String,
    /// The buttons, in order. Android shows at most three.
    pub actions: Vec<Action>,
}
/// One button on a notification.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Action {
    /// Reported as [`NotificationEvent::Action::action_id`].
    pub id: String,
    /// The button label.
    pub title: String,
    /// Bring the app to the foreground when tapped. Otherwise the event is
    /// recorded without opening the app and waits for the next poll.
    pub foreground: bool,
    /// iOS: show it in red.
    pub destructive: bool,
    /// iOS: require the device to be unlocked first.
    pub requires_authentication: bool,
    /// Let the user type a reply, reported as
    /// [`NotificationEvent::Action::input`].
    pub input: bool,
    /// The placeholder in the reply field.
    pub input_placeholder: Option<String>,
    /// iOS: the send button's label.
    pub input_button_title: Option<String>,
}
/// Local notifications: shown now or scheduled, with buttons, on both
/// platforms — the counterpart of Tauri's notification plugin.
///
/// ```rust,ignore
/// let mut plugins = use_context::<NativePlugins>();
///
/// if plugins.notifications.write().check_permissions()? != PermissionState::Granted {
///     plugins.notifications.write().request_permissions()?;
/// }
/// plugins.notifications.write().show(&Notification::new(1, "Round saved").body("18 holes, 72"))?;
///
/// // Wherever the app polls:
/// while let Ok(Some(event)) = plugins.notifications.write().take_event() {
///     if let NotificationEvent::Action { notification, .. } = event {
///         // Route on notification.extra.
///     }
/// }
/// ```
///
/// Taps and foreground deliveries arrive on platform callbacks, often before
/// the app has rendered anything, so they are queued natively and polled with
/// [`take_event`](Notifications::take_event), the same contract as deep
/// links. A tap that launched the app is only caught if
/// [`prepare`](Notifications::prepare) ran before launching finished, so call
/// it in `main` — [`Notifications::new`] is public for that.
///
/// Android 13 and later asks for the `POST_NOTIFICATIONS` permission at run
/// time, which is what [`request_permissions`](Notifications::request_permissions)
/// raises; older Android grants it at install. iOS always asks. As with
/// geolocation, the answer is read by polling
/// [`check_permissions`](Notifications::check_permissions).
///
/// The web and macOS builds are inert, so callers need no cfg of their own.
#[cfg(any(target_os = "android", target_os = "ios"))]
pub struct Notifications {
    plugin: Option<NotificationsHandle>,
}
/// The inert notifications facade: see the native [`Notifications`] for the
/// contract.
#[cfg(any(target_arch = "wasm32", target_os = "macos"))]
pub struct Notifications;
/// The native side answers a command with nothing, or with a JSON object
/// carrying `error`.
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
/// Read a JSON answer, turning an `error` object into `Err`.
#[cfg(any(target_os = "android", target_os = "ios"))]
fn parse<T: serde::de::DeserializeOwned>(
    reported: Option<String>,
    what: &str,
) -> Result<T, String> {
    let raw = reported.ok_or_else(|| format!("The platform did not report {what}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|error| format!("Failed to read {what}: {error}"))?;
    if let Some(error) = value.get("error").and_then(|error| error.as_str()) {
        return Err(error.to_string());
    }
    serde_json::from_value(value).map_err(|error| format!("Failed to read {what}: {error}"))
}
#[cfg(any(target_os = "android", target_os = "ios"))]
fn encode<T: Serialize>(value: &T, what: &str) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| format!("Failed to encode {what}: {error}"))
}
#[cfg(any(target_os = "android", target_os = "ios"))]
impl Notifications {
    /// Create the facade without touching the platform.
    ///
    /// Public, unlike most plugins here, so an app can
    /// [`prepare`](Notifications::prepare) in `main`. The event queue lives on
    /// the native side, so an early instance and the provider's own see the
    /// same events.
    pub fn new() -> Self {
        Self { plugin: None }
    }
    fn get_plugin(&mut self) -> Result<&NotificationsHandle, String> {
        if self.plugin.is_none() {
            #[cfg(target_os = "android")]
            let created = NotificationsHandle::new(NOTIFICATIONS_CLASS)?;
            #[cfg(target_os = "ios")]
            let created = NotificationsPlugin::new()
                .map_err(|error| format!("Failed to create NotificationsPlugin: {error:?}"))?;
            self.plugin = Some(created);
        }
        Ok(self.plugin.as_ref().unwrap())
    }
    /// Start listening for taps and deliveries. Call it as early as the app
    /// can: a tap that launched the app is delivered during launch, and only
    /// to a listener already in place. Calling it again is harmless.
    pub fn prepare(&mut self) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        plugin.call_unit("prepareFromRust")?;
        #[cfg(target_os = "ios")]
        prepareFromRust(plugin)?;
        Ok(())
    }
    /// Whether the app may show notifications.
    pub fn check_permissions(&mut self) -> Result<PermissionState, String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        let reported = plugin.call_string("checkPermissionsFromRust")?;
        #[cfg(target_os = "ios")]
        let reported = checkPermissionsFromRust(plugin)?;
        let state = reported.ok_or("Notification permission was not reported")?;
        serde_json::from_value(serde_json::Value::String(state))
            .map_err(|error| format!("Failed to read notification permission: {error}"))
    }
    /// Show the system permission prompt. Returns as soon as it is raised;
    /// poll [`check_permissions`](Notifications::check_permissions) for the
    /// answer.
    pub fn request_permissions(&mut self) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        plugin.call_unit("requestPermissionsFromRust")?;
        #[cfg(target_os = "ios")]
        requestPermissionsFromRust(plugin)?;
        Ok(())
    }
    /// Open the app's notification settings: the only way back once the user
    /// has refused.
    pub fn open_settings(&mut self) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        plugin.call_unit("openSettingsFromRust")?;
        #[cfg(target_os = "ios")]
        openSettingsFromRust(plugin)?;
        Ok(())
    }
    /// Show a notification now, or schedule it if it has a
    /// [`schedule`](Notification::schedule).
    pub fn show(&mut self, notification: &Notification) -> Result<(), String> {
        if let Some(Schedule::Every { count: 0, .. }) = notification.schedule {
            return Err("A repeating schedule needs a count of at least one".to_string());
        }
        let json = encode(notification, "the notification")?;
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        return check(plugin.call_string_str("showFromRust", &json)?);
        #[cfg(target_os = "ios")]
        return check(showFromRust(plugin, json)?);
    }
    /// Scheduled notifications that have not fired yet.
    pub fn pending(&mut self) -> Result<Vec<PendingNotification>, String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        let reported = plugin.call_string("pendingFromRust")?;
        #[cfg(target_os = "ios")]
        let reported = pendingFromRust(plugin)?;
        parse(reported, "pending notifications")
    }
    /// Cancel scheduled notifications before they fire.
    pub fn cancel(&mut self, ids: &[i32]) -> Result<(), String> {
        let json = encode(&ids, "notification ids")?;
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        return check(plugin.call_string_str("cancelFromRust", &json)?);
        #[cfg(target_os = "ios")]
        return check(cancelFromRust(plugin, json)?);
    }
    /// Cancel every scheduled notification.
    pub fn cancel_all(&mut self) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        return check(plugin.call_string("cancelAllFromRust")?);
        #[cfg(target_os = "ios")]
        return check(cancelAllFromRust(plugin)?);
    }
    /// Notifications from this app on screen now.
    pub fn active(&mut self) -> Result<Vec<DeliveredNotification>, String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        let reported = plugin.call_string("activeFromRust")?;
        #[cfg(target_os = "ios")]
        let reported = activeFromRust(plugin)?;
        parse(reported, "active notifications")
    }
    /// Take notifications off the screen.
    pub fn remove_active(&mut self, ids: &[i32]) -> Result<(), String> {
        let json = encode(&ids, "notification ids")?;
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        return check(plugin.call_string_str("removeActiveFromRust", &json)?);
        #[cfg(target_os = "ios")]
        return check(removeActiveFromRust(plugin, json)?);
    }
    /// Take every notification from this app off the screen.
    pub fn remove_all_active(&mut self) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        return check(plugin.call_string("removeAllActiveFromRust")?);
        #[cfg(target_os = "ios")]
        return check(removeAllActiveFromRust(plugin)?);
    }
    /// Create or update an Android channel. Once created, Android lets the app
    /// change only its name and description; the rest belongs to the user.
    pub fn create_channel(&mut self, channel: &Channel) -> Result<(), String> {
        let json = encode(channel, "the channel")?;
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        return check(plugin.call_string_str("createChannelFromRust", &json)?);
        #[cfg(target_os = "ios")]
        return check(createChannelFromRust(plugin, json)?);
    }
    /// Delete an Android channel.
    pub fn delete_channel(&mut self, id: &str) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        return check(plugin.call_string_str("deleteChannelFromRust", id)?);
        #[cfg(target_os = "ios")]
        return check(deleteChannelFromRust(plugin, id.to_string())?);
    }
    /// The app's Android channels. Always empty on iOS.
    pub fn channels(&mut self) -> Result<Vec<Channel>, String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        let reported = plugin.call_string("channelsFromRust")?;
        #[cfg(target_os = "ios")]
        let reported = channelsFromRust(plugin)?;
        parse(reported, "notification channels")
    }
    /// Register the button sets notifications can use, replacing any
    /// registered before.
    pub fn register_action_types(&mut self, types: &[ActionType]) -> Result<(), String> {
        let json = encode(&types, "the action types")?;
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        return check(plugin.call_string_str("registerActionTypesFromRust", &json)?);
        #[cfg(target_os = "ios")]
        return check(registerActionTypesFromRust(plugin, json)?);
    }
    /// The oldest event not yet handled, or `None` when none are waiting.
    /// Drain in a loop until it returns `None`.
    pub fn take_event(&mut self) -> Result<Option<NotificationEvent>, String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        let reported = plugin.call_string("takeEventFromRust")?;
        #[cfg(target_os = "ios")]
        let reported = takeEventFromRust(plugin)?;
        match reported {
            None => Ok(None),
            Some(_) => parse(reported, "a notification event").map(Some),
        }
    }
}
#[cfg(any(target_arch = "wasm32", target_os = "macos"))]
impl Notifications {
    /// Create the inert facade.
    pub fn new() -> Self {
        Self
    }
    /// No-op.
    pub fn prepare(&mut self) -> Result<(), String> {
        Ok(())
    }
    /// Always [`PermissionState::Prompt`]: nothing here shows notifications.
    pub fn check_permissions(&mut self) -> Result<PermissionState, String> {
        Ok(PermissionState::Prompt)
    }
    /// No-op.
    pub fn request_permissions(&mut self) -> Result<(), String> {
        Ok(())
    }
    /// No-op.
    pub fn open_settings(&mut self) -> Result<(), String> {
        Ok(())
    }
    /// No-op: nothing is shown.
    pub fn show(&mut self, _notification: &Notification) -> Result<(), String> {
        Ok(())
    }
    /// Always empty.
    pub fn pending(&mut self) -> Result<Vec<PendingNotification>, String> {
        Ok(Vec::new())
    }
    /// No-op.
    pub fn cancel(&mut self, _ids: &[i32]) -> Result<(), String> {
        Ok(())
    }
    /// No-op.
    pub fn cancel_all(&mut self) -> Result<(), String> {
        Ok(())
    }
    /// Always empty.
    pub fn active(&mut self) -> Result<Vec<DeliveredNotification>, String> {
        Ok(Vec::new())
    }
    /// No-op.
    pub fn remove_active(&mut self, _ids: &[i32]) -> Result<(), String> {
        Ok(())
    }
    /// No-op.
    pub fn remove_all_active(&mut self) -> Result<(), String> {
        Ok(())
    }
    /// No-op.
    pub fn create_channel(&mut self, _channel: &Channel) -> Result<(), String> {
        Ok(())
    }
    /// No-op.
    pub fn delete_channel(&mut self, _id: &str) -> Result<(), String> {
        Ok(())
    }
    /// Always empty.
    pub fn channels(&mut self) -> Result<Vec<Channel>, String> {
        Ok(Vec::new())
    }
    /// No-op.
    pub fn register_action_types(&mut self, _types: &[ActionType]) -> Result<(), String> {
        Ok(())
    }
    /// Always `None`.
    pub fn take_event(&mut self) -> Result<Option<NotificationEvent>, String> {
        Ok(None)
    }
}
#[cfg(any(
    target_arch = "wasm32",
    target_os = "android",
    target_os = "ios",
    target_os = "macos"
))]
impl Default for Notifications {
    fn default() -> Self {
        Self::new()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notifications_cross_the_bridge_in_the_shape_both_platforms_read() {
        let notification = Notification::new(7, "Tee time")
            .body("Hole 1")
            .action_type("reply")
            .extra("route", "/games/7")
            .schedule(Schedule::At {
                at_ms: 1_800_000_000_000,
                allow_while_idle: true,
            });
        let json = serde_json::to_value(&notification).unwrap();
        assert_eq!(json["actionTypeId"], "reply");
        assert_eq!(json["extra"]["route"], "/games/7");
        assert_eq!(json["schedule"]["kind"], "at");
        assert_eq!(json["schedule"]["atMs"], 1_800_000_000_000_i64);
        assert_eq!(json["schedule"]["allowWhileIdle"], true);
        let every = serde_json::to_value(Schedule::Every {
            interval: ScheduleInterval::Day,
            count: 2,
        })
        .unwrap();
        assert_eq!(
            every,
            serde_json::json!({ "kind": "every", "interval": "day", "count": 2 })
        );
    }
    #[test]
    fn events_from_either_platform_parse() {
        let tap: NotificationEvent = serde_json::from_str(
            r#"{"type":"action","actionId":"tap","input":null,
                "notification":{"id":7,"title":"Tee time","body":"Hole 1","extra":{"route":"/games/7"}}}"#,
        )
        .unwrap();
        let NotificationEvent::Action {
            action_id,
            notification,
            ..
        } = tap
        else {
            panic!("a tap is an action");
        };
        assert_eq!(action_id, TAP_ACTION);
        assert_eq!(notification.extra["route"], "/games/7");
        // Fields a platform leaves out take their defaults.
        let received: NotificationEvent =
            serde_json::from_str(r#"{"type":"received","notification":{"id":1,"title":"Hi"}}"#)
                .unwrap();
        assert!(matches!(received, NotificationEvent::Received { .. }));
    }
}
