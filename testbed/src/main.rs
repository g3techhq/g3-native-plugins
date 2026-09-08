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
use g3_native_plugins::{NativePlugins, NativePluginsProvider};
use report::Reporter;
use status::{Results, Status, Summary};
use std::collections::BTreeMap;

fn main() {
    dioxus::logger::initialize_default();
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
