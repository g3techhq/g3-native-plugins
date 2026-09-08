//! One card per plugin: what it proves, how it did, and what you can poke.
//!
//! Each card says in a line what the plugin is actually for, because the point
//! of a test bed that doubles as a demo is that someone who has not read the
//! crate can tell whether the thing works.

use crate::report::Reporter;
use crate::status::{Check, Status};
use dioxus::prelude::*;
use g3_native_plugins::{NativePlugins, PositionOptions};

/// A titled card with a one-line explanation of what the plugin is for.
#[component]
fn Card(title: String, what: String, children: Element) -> Element {
    rsx! {
        section {
            div { class: "section-head",
                h2 { "{title}" }
                span { class: "what", "{what}" }
            }
            div { class: "card", {children} }
        }
    }
}

#[component]
fn Action(label: String, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button { onclick: move |event| onclick.call(event), "{label}" }
    }
}

#[component]
pub fn StorageCard() -> Element {
    let plugins = use_context::<NativePlugins>();
    let reporter = use_context::<Reporter>();
    rsx! {
        Card { title: "Storage", what: "Keystore-encrypted key/value",
            Check { area: "storage", label: "Round trip on launch" }
            Check { area: "storage.manual", label: "Manual write and read" }
            Check { area: "storage.keys", label: "List keys" }
            Check { area: "storage.clear", label: "Clear" }
            div { class: "row",
                Action { label: "Write and read", onclick: move |_| {
                    let mut plugins = plugins;
                    let result = (|| -> Result<String, String> {
                        let mut storage = plugins.storage.write();
                        storage.set("manual", "hello from the test bed")?;
                        Ok(format!("{:?}", storage.get("manual")?))
                    })();
                    reporter.result("storage.manual", result);
                } }
                Action { label: "List keys", onclick: move |_| {
                    let mut plugins = plugins;
                    let result = plugins.storage.write().keys();
                    reporter.result("storage.keys", result);
                } }
                Action { label: "Clear", onclick: move |_| {
                    let mut plugins = plugins;
                    let result = plugins.storage.write().clear();
                    reporter.result("storage.clear", result);
                } }
            }
        }
    }
}

#[component]
pub fn BackButtonCard() -> Element {
    let plugins = use_context::<NativePlugins>();
    let reporter = use_context::<Reporter>();
    rsx! {
        Card { title: "Back button", what: "System back as an app event",
            Check { area: "back-button", label: "Plugin ready" }
            Check { area: "back-button.intercept", label: "Interception" }
            Check { area: "back-button.event", label: "Back reached the page" }
            div { class: "row",
                Action { label: "Intercept on", onclick: move |_| {
                    let mut plugins = plugins;
                    let result = plugins.back_button.write().set_intercepting(true)
                        .map(|_| "on: back now stays in the app");
                    reporter.result("back-button.intercept", result);
                } }
                Action { label: "Intercept off", onclick: move |_| {
                    let mut plugins = plugins;
                    let result = plugins.back_button.write().set_intercepting(false)
                        .map(|_| "off: back now leaves the app");
                    reporter.result("back-button.intercept", result);
                } }
            }
            p { class: "hint", "Turn it on, then press back (adb shell input keyevent 4)" }
        }
    }
}

#[component]
pub fn DeepLinksCard() -> Element {
    rsx! {
        Card { title: "Deep links", what: "The URL that opened the app",
            Check { area: "deep-links", label: "Listening" }
            Check { area: "deep-links.received", label: "Link received" }
            p { class: "hint",
                "adb shell am start -a android.intent.action.VIEW -d \"g3testbed://open/hello\""
            }
        }
    }
}

#[component]
pub fn GeolocationCard() -> Element {
    let plugins = use_context::<NativePlugins>();
    let reporter = use_context::<Reporter>();
    rsx! {
        Card { title: "Geolocation", what: "Permission and a position fix",
            Check { area: "geolocation.permissions", label: "Permission state" }
            Check { area: "geolocation.position", label: "Position" }
            div { class: "row",
                Action { label: "Request permission", onclick: move |_| {
                    let mut plugins = plugins;
                    let result = plugins.geolocation.write().request_permissions()
                        .map(|_| "prompt shown; poll the check below");
                    reporter.result("geolocation.request", result);
                } }
                Action { label: "Check permission", onclick: move |_| {
                    let mut plugins = plugins;
                    let result = plugins.geolocation.write().check_permissions();
                    reporter.result("geolocation.permissions", result);
                } }
                Action { label: "Get position", onclick: move |_| {
                    let mut plugins = plugins;
                    match plugins.geolocation.write()
                        .start_position_request(PositionOptions::default()) {
                        Ok(()) => reporter.waiting("geolocation.position", "looking for a fix"),
                        Err(error) => reporter.result::<()>("geolocation.position", Err(error)),
                    }
                } }
            }
            p { class: "hint", "On an emulator: adb emu geo fix -122.084 37.4220" }
        }
    }
}

/// Attach a live camera to the preview element.
///
/// The stream is parked on `window` so the stop button can find it again; a
/// stream nobody holds a reference to keeps the camera light on.
fn start_preview(reporter: Reporter, facing: &'static str) {
    reporter.waiting("camera-microphone.getUserMedia", "opening the camera");
    spawn(async move {
        let script = format!(
            r#"
            (async () => {{
              const video = document.getElementById('g3-camera-preview');
              try {{
                if (window.__g3stream) {{
                  window.__g3stream.getTracks().forEach(track => track.stop());
                }}
                const stream = await navigator.mediaDevices.getUserMedia({{
                  video: {{ facingMode: '{facing}' }}, audio: false
                }});
                window.__g3stream = stream;
                video.srcObject = stream;
                video.autoplay = true;
                video.muted = true;
                video.playsInline = true;
                await video.play();
                const track = stream.getVideoTracks()[0];
                const size = track.getSettings();
                dioxus.send('live ' + size.width + 'x' + size.height
                  + ' from ' + (track.label || 'camera'));
              }} catch (error) {{
                dioxus.send('ERR ' + error.name);
              }}
            }})();
            "#
        );
        let mut preview = document::eval(&script);
        if let Ok(outcome) = preview.recv::<String>().await {
            let result = match outcome.strip_prefix("ERR ") {
                Some(error) => Err(error.to_string()),
                None => Ok(outcome),
            };
            reporter.result("camera-microphone.getUserMedia", result);
        }
    });
}

const STOP_PREVIEW: &str = r#"
    if (window.__g3stream) {
      window.__g3stream.getTracks().forEach(track => track.stop());
      window.__g3stream = null;
    }
    const video = document.getElementById('g3-camera-preview');
    if (video) { video.srcObject = null; }
    dioxus.send('stopped');
"#;

#[component]
pub fn CaptureCard() -> Element {
    let plugins = use_context::<NativePlugins>();
    let reporter = use_context::<Reporter>();
    // The video element stays in the DOM whether or not it is showing, so the
    // script above can always find it; only its visibility changes.
    let mut streaming = use_signal(|| false);
    rsx! {
        Card { title: "Camera and microphone", what: "Permission around getUserMedia",
            Check { area: "camera-microphone.permissions", label: "Permission state" }
            Check { area: "camera-microphone.getUserMedia", label: "getUserMedia" }
            div {
                class: if streaming() { "preview-wrap on" } else { "preview-wrap" },
                video { id: "g3-camera-preview", class: "preview" }
            }
            div { class: "row",
                Action { label: "Check", onclick: move |_| {
                    let mut plugins = plugins;
                    let result = plugins.camera_microphone.write().check_permissions();
                    reporter.result("camera-microphone.permissions", result);
                } }
                Action { label: "Request camera", onclick: move |_| {
                    let mut plugins = plugins;
                    let result = plugins.camera_microphone.write().request_camera()
                        .map(|_| "prompt shown");
                    reporter.result("camera-microphone.request", result);
                } }
                Action { label: "Request mic", onclick: move |_| {
                    let mut plugins = plugins;
                    let result = plugins.camera_microphone.write().request_microphone()
                        .map(|_| "prompt shown");
                    reporter.result("camera-microphone.request", result);
                } }
                Action { label: "Open settings", onclick: move |_| {
                    let mut plugins = plugins;
                    let result = plugins.camera_microphone.write().open_settings()
                        .map(|_| "left the app for Settings");
                    reporter.result("camera-microphone.settings", result);
                } }
                // Plain web code: wry bridges the WebView permission itself.
                // An emulator has no real camera, but it does have synthetic
                // ones — a navigable 3D room on the back lens and a moving test
                // pattern on the front — so a preview here shows real frames.
                Action { label: "Back camera", onclick: move |_| {
                    streaming.set(true);
                    start_preview(reporter, "environment");
                } }
                Action { label: "Front camera", onclick: move |_| {
                    streaming.set(true);
                    start_preview(reporter, "user");
                } }
                Action { label: "Stop", onclick: move |_| {
                    streaming.set(false);
                    spawn(async move {
                        let mut stop = document::eval(STOP_PREVIEW);
                        let _ = stop.recv::<String>().await;
                    });
                    reporter.result::<&str>(
                        "camera-microphone.getUserMedia",
                        Ok("stopped"),
                    );
                } }
            }
        }
    }
}

#[component]
pub fn ClipboardCard() -> Element {
    let plugins = use_context::<NativePlugins>();
    let reporter = use_context::<Reporter>();
    rsx! {
        Card { title: "Clipboard", what: "Copy and the native share sheet",
            Check { area: "clipboard.copy", label: "Copy" }
            Check { area: "clipboard.share", label: "Share sheet" }
            div { class: "row",
                Action { label: "Copy text", onclick: move |_| {
                    let mut plugins = plugins;
                    let result = plugins.clipboard.write()
                        .copy_to_clipboard("g3-testbed-clipboard".to_string())
                        .map(|_| "copied g3-testbed-clipboard to the system clipboard");
                    reporter.result("clipboard.copy", result);
                } }
                Action { label: "Share sheet", onclick: move |_| {
                    let mut plugins = plugins;
                    let result = plugins.clipboard.write()
                        .share("g3-testbed-share".to_string())
                        .map(|_| "chooser opened");
                    reporter.result("clipboard.share", result);
                } }
            }
        }
    }
}

#[component]
pub fn MediaCard() -> Element {
    let plugins = use_context::<NativePlugins>();
    let reporter = use_context::<Reporter>();
    rsx! {
        Card { title: "Media", what: "Background playback and orientation",
            Check { area: "media", label: "Plugin ready" }
            Check { area: "media.playback", label: "Foreground service" }
            Check { area: "media.orientation", label: "Orientation" }
            div { class: "row",
                Action { label: "Playback on", onclick: move |_| {
                    let mut plugins = plugins;
                    let result = plugins.media.write()
                        .set_playback_active(true, "Testbed track")
                        .map(|_| "service started, check the notification");
                    reporter.result("media.playback", result);
                } }
                Action { label: "Playback off", onclick: move |_| {
                    let mut plugins = plugins;
                    let result = plugins.media.write()
                        .set_playback_active(false, "")
                        .map(|_| "service stopped");
                    reporter.result("media.playback", result);
                } }
                Action { label: "Landscape", onclick: move |_| {
                    let mut plugins = plugins;
                    let result = plugins.media.write().set_orientation("landscape")
                        .map(|_| "locked to landscape");
                    reporter.result("media.orientation", result);
                } }
                Action { label: "Unlock", onclick: move |_| {
                    let mut plugins = plugins;
                    let result = plugins.media.write().set_orientation("unspecified")
                        .map(|_| "orientation unlocked");
                    reporter.result("media.orientation", result);
                } }
            }
        }
    }
}

#[component]
pub fn ExternalUrlCard() -> Element {
    let plugins = use_context::<NativePlugins>();
    let reporter = use_context::<Reporter>();
    rsx! {
        Card { title: "External URL", what: "Hand a link to the system browser",
            Check { area: "external-url", label: "Open in browser" }
            div { class: "row",
                Action { label: "Open example.com", onclick: move |_| {
                    let mut plugins = plugins;
                    let result = plugins.external_url.write().open("https://example.com")
                        .map(|_| "browser opened; come back with the back gesture");
                    reporter.result("external-url", result);
                } }
            }
        }
    }
}

#[component]
pub fn PurchasesCard() -> Element {
    let plugins = use_context::<NativePlugins>();
    let reporter = use_context::<Reporter>();
    rsx! {
        Card { title: "In-app purchases", what: "Store connection, products, ownership",
            Check { area: "in-app-purchases", label: "Store connected" }
            Check { area: "in-app-purchases.products", label: "Products" }
            Check { area: "in-app-purchases.entitlements", label: "Owned" }
            div { class: "row",
                Action { label: "Load products", onclick: move |_| {
                    let mut plugins = plugins;
                    match plugins.in_app_purchases.write()
                        .start_products_request(&["testbed.subscription".to_string()]) {
                        Ok(()) => reporter.waiting("in-app-purchases.products", "asking the store"),
                        Err(error) => {
                            reporter.result::<()>("in-app-purchases.products", Err(error))
                        }
                    }
                } }
                Action { label: "Restore", onclick: move |_| {
                    let mut plugins = plugins;
                    match plugins.in_app_purchases.write().start_restore() {
                        Ok(()) => {
                            reporter.waiting("in-app-purchases.entitlements", "re-reading the store")
                        }
                        Err(error) => {
                            reporter.result::<()>("in-app-purchases.entitlements", Err(error))
                        }
                    }
                } }
                Action { label: "Buy subscription", onclick: move |_| {
                    let mut plugins = plugins;
                    match plugins.in_app_purchases.write()
                        .start_purchase("testbed.subscription", None) {
                        Ok(()) => reporter.waiting("in-app-purchases.entitlements", "checkout"),
                        Err(error) => {
                            reporter.result::<()>("in-app-purchases.entitlements", Err(error))
                        }
                    }
                } }
            }
            p { class: "hint", "Needs products in the store console and a licensed tester" }
        }
    }
}

/// The Google Cloud web client id to sign in against.
///
/// It lives here rather than in the crate because it names one project, and a
/// library carrying one could only ever authenticate the app it was written
/// for. This is Greenside Partee's, borrowed to exercise the call path — it is
/// registered against that app's package name and signing certificate, not this
/// test bed's, so Credential Manager is expected to refuse it. That still
/// proves the plugin reports a refusal cleanly instead of hanging.
#[cfg(target_os = "android")]
const GOOGLE_SERVER_CLIENT_ID: &str =
    "670494631267-ig0mogebnadg4badthak02cilpb57ufv.apps.googleusercontent.com";

#[component]
pub fn AuthCard() -> Element {
    let plugins = use_context::<NativePlugins>();
    let reporter = use_context::<Reporter>();
    rsx! {
        Card { title: "Auth", what: "Google on Android, Apple on iOS",
            Check { area: "auth", label: "Sign-in" }
            div { class: "row",
                Action { label: "Start sign-in", onclick: move |_| {
                    let mut plugins = plugins;
                    #[cfg(target_os = "android")]
                    let started = plugins.auth.write()
                        .start_google_auth(GOOGLE_SERVER_CLIENT_ID);
                    #[cfg(any(target_os = "ios", target_os = "macos"))]
                    let started = plugins.auth.write().start_apple_auth();
                    if let Err(error) = started {
                        reporter.record("auth", Status::Fail, error);
                        return;
                    }
                    reporter.waiting("auth", "native account UI opened");
                    spawn(async move {
                        // Apple dispatches the start to its main queue, so give
                        // it one turn before observing the awaiting flag.
                        crate::sleep_ms(100).await;
                        loop {
                            match plugins.auth.write().poll_auth_result() {
                                Ok(Some(credential)) => {
                                    reporter.record(
                                        "auth",
                                        Status::Pass,
                                        format!("credential received ({} bytes)", credential.len()),
                                    );
                                    break;
                                }
                                Err(error) => {
                                    reporter.record("auth", Status::Fail, error);
                                    break;
                                }
                                Ok(None) => {}
                            }
                            if !plugins.auth.write().is_auth_awaiting() {
                                reporter.record(
                                    "auth",
                                    Status::Fail,
                                    "sign-in ended without a credential",
                                );
                                break;
                            }
                            crate::sleep_ms(200).await;
                        }
                    });
                } }
            }
            p { class: "hint",
                "Needs an account on the device, and this app registered against the client id"
            }
        }
    }
}
