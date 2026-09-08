import AVFoundation
import Foundation
import UIKit

/**
 * The permission state around `getUserMedia`, not a capture API of its own.
 *
 * Capture itself already works: wry's `WKUIDelegate` answers WebKit's
 * `requestMediaCapturePermissionForOrigin` with a grant, so WebKit raises the
 * system prompt from the Info.plist usage strings. Nothing here touches that
 * delegate, because replacing it would take wry's file chooser and JS dialogs
 * with it.
 *
 * What this adds is what the page cannot reach: reading the state before
 * capture is attempted, prompting at a moment the app chooses rather than
 * mid-stream, and opening the Settings page — the only way back on iOS, which
 * shows its prompt exactly once per install and silently refuses after that.
 *
 * The usage strings themselves are written by the Dioxus CLI from
 * `[permissions]` in `Dioxus.toml`. Without them iOS does not prompt, it
 * terminates the app.
 */
@objc(CameraMicrophonePlugin)
public class CameraMicrophonePlugin: NSObject {

    private static func state(for media: AVMediaType) -> String {
        switch AVCaptureDevice.authorizationStatus(for: media) {
        case .notDetermined: return "prompt"
        // Restricted means parental controls or an MDM profile decided, which
        // the user cannot undo from Settings, but it is a refusal either way.
        case .denied, .restricted: return "denied"
        case .authorized: return "granted"
        // iOS has no rationale state; that variant only ever comes from Android.
        @unknown default: return "prompt"
        }
    }

    @objc
    public func checkPermissionsFromRust() -> String {
        let payload = [
            "camera": Self.state(for: .video),
            "microphone": Self.state(for: .audio),
        ]
        guard let data = try? JSONSerialization.data(withJSONObject: payload),
              let json = String(data: data, encoding: .utf8) else {
            return "{\"camera\":\"prompt\",\"microphone\":\"prompt\"}"
        }
        return json
    }

    @objc
    public func requestCameraFromRust() {
        // iOS shows this once per install and does nothing on later calls, so
        // the Rust side treats a refusal as final and points at Settings.
        AVCaptureDevice.requestAccess(for: .video) { granted in
            NSLog("[CameraMicrophonePlugin] camera access granted: \(granted)")
        }
    }

    @objc
    public func requestMicrophoneFromRust() {
        AVCaptureDevice.requestAccess(for: .audio) { granted in
            NSLog("[CameraMicrophonePlugin] microphone access granted: \(granted)")
        }
    }

    @objc
    public func openSettingsFromRust() {
        DispatchQueue.main.async {
            guard let url = URL(string: UIApplication.openSettingsURLString) else { return }
            UIApplication.shared.open(url)
        }
    }

    /**
     * Move the audio session between playing and recording.
     *
     * Only matters when something else has already claimed the session. The
     * media plugin here sets `.playback`, which has no input at all, so a
     * microphone opened under it returns silence. `.playAndRecord` is the
     * category that allows both.
     *
     * Routing changes with the category: `.playAndRecord` sends audio to the
     * earpiece by default, which is wrong for anything but a phone call, so it
     * asks for the speaker and allows a Bluetooth headset explicitly.
     */
    @objc
    public func setCapturingFromRust(_ capturing: Bool) {
        DispatchQueue.main.async {
            let session = AVAudioSession.sharedInstance()
            do {
                if capturing {
                    try session.setCategory(
                        .playAndRecord,
                        mode: .videoChat,
                        options: [.defaultToSpeaker, .allowBluetooth]
                    )
                } else {
                    try session.setCategory(.playback, mode: .moviePlayback)
                }
                try session.setActive(true)
            } catch {
                NSLog(
                    "[CameraMicrophonePlugin] could not set the audio session: "
                        + error.localizedDescription
                )
            }
        }
    }
}
