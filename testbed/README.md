# Test bed

A Dioxus app wiring up every plugin in this crate. It reads two ways: as a demo
someone can look at, and as something a terminal can drive.

On screen, each plugin is a card saying in one line what it is for, with a
pass/fail badge per check and the last result underneath. A running tally sits
in a sticky header, so the answer to "does this all work" is visible without
scrolling. Untried checks read `NOT RUN` rather than anything alarming.

The same results go to the system log, so nothing has to be read off a screen
to be useful.

It is a standalone package (its own empty `[workspace]`), so `cargo test` and
`cargo package` at the repo root neither build nor ship it.

## Run it

```bash
cd testbed
dx serve --android          # or: dx build --android, then adb install
```

`dx serve` prints `ERROR dx and dioxus versions are incompatible!` when the CLI
is 0.7.9 and Cargo has resolved dioxus to 0.7.10. It is a version comparison,
not a build failure — the build completes and the app launches. Pin
`dioxus = "=0.7.9"` if the noise is not wanted.

Everything that can be checked without a person runs on launch and reports with
a `G3TESTBED` tag:

```bash
adb logcat -c
adb shell monkey -p dev.dioxus.g3nativeplugins.testbed -c android.intent.category.LAUNCHER 1
adb logcat -d | grep G3TESTBED
```

A healthy run looks like:

```text
G3TESTBED run start self-test beginning
G3TESTBED storage ok "set, get, keys, remove all agreed"
G3TESTBED deep-links.prepare ok ()
G3TESTBED geolocation.permissions ok PermissionStatus { location: Prompt, ... }
G3TESTBED camera-microphone.permissions ok CapturePermissions { camera: Prompt, ... }
G3TESTBED media.prepare ok ()
G3TESTBED in-app-purchases.prepare ok ()
G3TESTBED back-button.prepare ok ()
G3TESTBED run ready self-test done, polling for async results
G3TESTBED in-app-purchases.entitlements ok []
```

`storage ok` is the one line that proves a whole plugin end to end on its own:
it writes through the Keystore, reads back, lists, and deletes, and complains if
any step disagrees.

## Driving the rest from adb

Anything needing a tap or an external event is a button, and most can be reached
without touching the screen:

```bash
# Deep link. Arrives as deep-links.received.
adb shell am start -a android.intent.action.VIEW -d "g3testbed://open/hello"

# Back button: tap "Intercept on" first, then
adb shell input keyevent 4      # expect back-button.event, and the app stays up

# Permissions, without the dialog
adb shell pm grant dev.dioxus.g3nativeplugins.testbed android.permission.CAMERA
adb shell pm grant dev.dioxus.g3nativeplugins.testbed android.permission.RECORD_AUDIO
```

Buttons are tapped by coordinate, which means screenshotting first:

```bash
adb shell screencap -p /sdcard/tb.png && adb pull /sdcard/tb.png
adb shell input tap <x> <y>
```

## Seeing a camera feed

An emulator has no camera, but it can synthesise one, so the camera card shows
a live preview rather than just reporting that `getUserMedia` resolved. The AVD
picks a mode per lens in `config.ini`:

```ini
hw.camera.back=virtualscene   # a 3D room you can walk around with WASD
hw.camera.front=emulated      # a moving test pattern
```

Other modes, settable there or as `emulator -camera-back <mode>`:

| Mode | What the app sees |
| --- | --- |
| `virtualscene` | A navigable 3D room. Extended Controls → Camera can hang your own images on its walls. |
| `emulated` | A synthetic moving pattern. |
| `videoplayback` | **A video file of your choosing**, which is the closest thing to feeding real footage in. |
| `webcam0` | The host machine's webcam. |
| `none` | No camera at all, for testing the refusal path. |

**The iOS Simulator has no camera in any of these senses.** It exposes no
capture device, so `getUserMedia` for video fails there however it is
configured — camera work on iOS needs a real device.

## What still needs a real account or device

- **In-app purchases** beyond connecting: products and a purchase flow need
  items configured in the Play Console and a licensed test account.
- **Auth**: needs a Google client id or an Apple capability.
- **Geolocation position**: the emulator reports no fix until one is set, via
  Extended Controls or `adb emu geo fix <lon> <lat>`.
- **Share sheet, external URL, media playback**: these leave the app or start a
  service, so confirm them by eye or with `adb shell dumpsys`.
