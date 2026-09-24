//! A test bed and demo for every plugin in `g3-native-plugins`.
//!
//! Two audiences, one set of facts. On screen each plugin is a card with a
//! pass/fail badge and its last result, so a person can see at a glance whether
//! the thing works. In the system log the same results appear under a
//! `G3TESTBED` tag, so a terminal can drive and read the whole run:
//!
//! ```text
//! adb logcat -s RustStdoutStderr:* | grep G3TESTBED
//! ```
//!
//! Everything that can be checked without a person runs on launch. The rest —
//! a share sheet, a purchase, a permission dialog — needs a tap, so those are
//! buttons.

mod cards;
mod report;
mod status;
mod style;

use dioxus::prelude::*;
use g3_native_plugins::{
    NativePlugins, NativePluginsProvider, NotificationEvent, Notifications, PushEvent,
    PushNotifications, Updater, UpdaterConfig, UpdaterState,
};
use report::Reporter;
use status::{Results, Status, Summary};
use std::collections::BTreeMap;

/// A throwaway minisign key whose secret half is a published test fixture in
/// the crate's updater tests. Fine for proving the plumbing; a real app ships
/// the public half of a key nobody else has.
const UPDATER_TEST_KEY: &str = "RWQBI0VniavN7wOhB7/zzhC+HXDdGOdLwJln5NYwm6UNXx3chmQSVTG4";

fn main() {
    dioxus::logger::initialize_default();
    // Before launch, where they belong: a notification tap that starts the
    // app is delivered during launch, and the updater installs or rolls back
    // before anything reads a bundle file.
    let _ = Notifications::new().prepare();
    let _ = PushNotifications::new().prepare();
    let _ = Updater::new().launch(
        UpdaterConfig::new(UPDATER_TEST_KEY, "0.1.0")
            .endpoint("https://g3-testbed.example.com/updates/{{target}}/{{current_version}}"),
    );
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        NativePluginsProvider { Harness {} }
    }
}

#[component]
fn Harness() -> Element {
    let plugins = use_context::<NativePlugins>();
    let log = use_signal(Vec::<String>::new);
    let results: Results = use_signal(BTreeMap::new);
    use_context_provider(|| results);
    let reporter = Reporter { log, results };
    use_context_provider(|| reporter);

    // The automatic half of the run: everything that either succeeds or fails
    // on its own, with nobody waiting to tap anything.
    use_effect(move || {
        let mut plugins = plugins;

        // Storage is the one plugin provable end to end without a store
        // account, a fix, or a tap: write, read back, list, delete, and
        // complain if any step disagrees.
        let round_trip = (|| -> Result<String, String> {
            let mut storage = plugins.storage.write();
            storage.prepare()?;
            storage.set("testbed.key", "round-trip-value")?;
            let read = storage.get("testbed.key")?;
            if read.as_deref() != Some("round-trip-value") {
                return Err(format!("read back {read:?}, expected what was written"));
            }
            let keys = storage.keys()?;
            if !keys.iter().any(|key| key == "testbed.key") {
                return Err(format!("keys() omitted the key just written: {keys:?}"));
            }
            storage.remove("testbed.key")?;
            if storage.get("testbed.key")?.is_some() {
                return Err("value survived remove()".to_string());
            }
            Ok("wrote, read, listed and deleted through the Keystore".to_string())
        })();
        reporter.result("storage", round_trip);

        reporter.result("deep-links", plugins.deep_links.write().prepare());
        reporter.result(
            "geolocation.permissions",
            plugins.geolocation.write().check_permissions(),
        );
        reporter.result(
            "camera-microphone.permissions",
            plugins.camera_microphone.write().check_permissions(),
        );
        reporter.result("media", plugins.media.write().prepare());
        reporter.result(
            "in-app-purchases",
            plugins.in_app_purchases.write().prepare(),
        );
        reporter.result("back-button", plugins.back_button.write().prepare());
        reporter.result(
            "notifications.permissions",
            plugins.notifications.write().check_permissions(),
        );
        // Reaching this effect is the proof a bundle works, so confirm it.
        let serving = plugins.updater.write().current_version();
        reporter.result(
            "updater",
            plugins.updater.write().notify_ready().map(|_| {
                format!(
                    "serving {}",
                    serving.unwrap_or_else(|| "nothing: launch failed".to_string())
                )
            }),
        );

        // These three used to go through manganis-generated Android calls,
        // whose FindClass fails specifically from this Dioxus effect thread.
        // Keep an automatic call here so that regression is exercised on a
        // device rather than hidden by the manual buttons' main-thread calls.
        reporter.result(
            "auth.effect",
            plugins
                .auth
                .write()
                .poll_auth_result()
                .map(|_| "constructed and polled from an effect"),
        );
        reporter.result(
            "clipboard.effect",
            plugins
                .clipboard
                .write()
                .copy_to_clipboard("g3-testbed-effect-clipboard".to_string()),
        );
        reporter.result(
            "external-url.effect",
            plugins.external_url.write().prepare(),
        );
    });

    // The back event only exists inside the page, so the page has to be the
    // one to notice it and pass it back out.
    use_future(move || async move {
        let mut listener = document::eval(
            "window.addEventListener('g3nativeback', () => { dioxus.send('back'); });",
        );
        while listener.recv::<String>().await.is_ok() {
            reporter.record(
                "back-button.event",
                Status::Pass,
                "page received g3nativeback",
            );
        }
    });

    // Anything that resolves later: a deep link arriving, a fix landing, a
    // purchase settling.
    use_future(move || async move {
        let mut plugins = plugins;
        // The updater reports a state, not events, so only a change is news.
        let mut last_updater_state = UpdaterState::Idle;
        loop {
            if let Ok(Some(url)) = plugins.deep_links.write().take_link() {
                reporter.record("deep-links.received", Status::Pass, url);
            }
            match plugins.geolocation.write().poll_position() {
                Ok(Some(position)) => reporter.record(
                    "geolocation.position",
                    Status::Pass,
                    format!(
                        "{:.5}, {:.5}  ±{:.0}m",
                        position.coords.latitude,
                        position.coords.longitude,
                        position.coords.accuracy
                    ),
                ),
                Err(error) => reporter.record("geolocation.position", Status::Fail, error),
                Ok(None) => {}
            }
            match plugins.in_app_purchases.write().poll_products() {
                Ok(Some(products)) => reporter.record(
                    "in-app-purchases.products",
                    Status::Pass,
                    if products.is_empty() {
                        "store returned no matching products".to_string()
                    } else {
                        products
                            .iter()
                            .map(|product| format!("{} {}", product.id, product.display_price))
                            .collect::<Vec<_>>()
                            .join(", ")
                    },
                ),
                Err(error) => reporter.record("in-app-purchases.products", Status::Fail, error),
                Ok(None) => {}
            }
            match plugins.in_app_purchases.write().poll_entitlements() {
                Ok(Some(owned)) => reporter.record(
                    "in-app-purchases.entitlements",
                    Status::Pass,
                    if owned.is_empty() {
                        "store reached, nothing owned".to_string()
                    } else {
                        owned
                            .iter()
                            .map(|item| item.product_id.clone())
                            .collect::<Vec<_>>()
                            .join(", ")
                    },
                ),
                Err(error) => reporter.record("in-app-purchases.entitlements", Status::Fail, error),
                Ok(None) => {}
            }
            while let Ok(Some(event)) = plugins.notifications.write().take_event() {
                let detail = match event {
                    NotificationEvent::Received { notification } => {
                        format!("delivered in foreground: {}", notification.title)
                    }
                    NotificationEvent::Action {
                        action_id,
                        input,
                        notification,
                    } => format!("{action_id} on {} {input:?}", notification.title),
                };
                reporter.record("notifications.event", Status::Pass, detail);
            }
            while let Ok(Some(event)) = plugins.push_notifications.write().take_event() {
                match event {
                    PushEvent::Token(token) => reporter.record(
                        "push.token",
                        Status::Pass,
                        format!("{:?} {}", token.service, token.token),
                    ),
                    PushEvent::RegistrationFailed(error) => {
                        reporter.record("push.token", Status::Fail, error)
                    }
                    other => reporter.record("push.event", Status::Pass, format!("{other:?}")),
                }
            }
            let updater_state = plugins.updater.write().state();
            if updater_state != last_updater_state {
                match &updater_state {
                    UpdaterState::Checking | UpdaterState::Idle => {}
                    UpdaterState::Failed(error) => {
                        reporter.record("updater.check", Status::Fail, error)
                    }
                    state => reporter.record("updater.check", Status::Pass, format!("{state:?}")),
                }
                last_updater_state = updater_state;
            }
            sleep_ms(400).await;
        }
    });

    rsx! {
        style { {style::STYLE} }
        main {
            header {
                h1 { "g3-native-plugins" }
                p { class: "sub", "Every plugin, on this device. Log tag: G3TESTBED" }
                Summary {}
            }
            cards::StorageCard {}
            cards::BackButtonCard {}
            cards::DeepLinksCard {}
            cards::GeolocationCard {}
            cards::CaptureCard {}
            cards::ClipboardCard {}
            cards::MediaCard {}
            cards::ExternalUrlCard {}
            cards::PurchasesCard {}
            cards::AuthCard {}
            cards::NotificationsCard {}
            cards::PushCard {}
            cards::UpdaterCard {}

            details {
                summary { "Full log ({log.read().len()} lines)" }
                pre { class: "log",
                    for line in log.read().iter().rev() {
                        "{line}\n"
                    }
                }
            }
        }
    }
}

/// Small sleep, borrowed from the page rather than a timer crate.
///
/// The app is inside a WebView on every target it builds for, so the one timer
/// guaranteed to be there is the browser's.
async fn sleep_ms(millis: u32) {
    let mut timer = document::eval(&format!("setTimeout(() => dioxus.send('tick'), {millis});"));
    let _ = timer.recv::<String>().await;
}
