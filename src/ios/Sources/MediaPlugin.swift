import AVFoundation
import Foundation
import MediaPlayer
import UIKit
import WebKit

/**
 * Keeps WebView playback alive when the app is not on screen, and puts it on
 * the lock screen.
 *
 * iOS suspends an app's audio the moment it stops being frontmost unless the
 * app has claimed a playback audio session, and it shows nothing on the lock
 * screen unless the app publishes Now Playing metadata. Neither happens for a
 * `<video>` inside a `WKWebView` on its own, so a web app hosted this way goes
 * silent on the home gesture and offers no controls when the screen locks.
 *
 * This plugin claims the session, publishes the metadata, and wires the
 * resulting remote commands back into the page — the same commands Android
 * puts on its picture-in-picture window, driving the same element with the
 * same script, so both platforms share one player contract.
 */
@objc(MediaPlugin)
public class MediaPlugin: NSObject {
    /// The element the host app plays through. Matched to the Android plugin's
    /// selector so a single player serves both.
    private static let videoSelector = "#tawny-player video"

    /// Android's picture-in-picture actions use ten-second steps; the lock
    /// screen and Control Center use the same ones here.
    private static let skipInterval: TimeInterval = 10

    private var commandsRegistered = false
    private var sessionConfigured = false
    private var playbackActive = false
    private var playbackTitle = ""

    /// Claim the audio session and mark the DOM as running inside the iOS app.
    ///
    /// The session is configured but not activated: activating it here would
    /// interrupt whatever the user is already listening to, before this app
    /// has anything of its own to play.
    @objc
    public func prepareFromRust() {
        DispatchQueue.main.async {
            self.configureAudioSession()
            // Android reports its status-bar inset through a CSS variable
            // because it has no other way to publish one. iOS already exposes
            // `env(safe-area-inset-*)` to the WebView, so the page only needs
            // to know which platform it is on.
            Self.findWebView()?.evaluateJavaScript(
                "document.documentElement.dataset.iosApp='true'",
                completionHandler: nil
            )
        }
    }

    /// Hand the video to the system picture-in-picture window.
    ///
    /// `width` and `height` are accepted for a call site shared with Android
    /// and ignored: AVKit takes the aspect ratio from the video track itself
    /// rather than from a caller-supplied hint.
    ///
    /// The host must build its `WKWebView` with
    /// `allowsPictureInPictureMediaPlayback` enabled; that configuration is
    /// immutable once the WebView exists, so the plugin cannot set it.
    @objc
    public func enterPictureInPictureFromRust(_ dimensionsJson: String) {
        let dimensions = dimensionsJson.data(using: .utf8)
            .flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }
        let width = dimensions?["width"] as? NSNumber
        let height = dimensions?["height"] as? NSNumber
        NSLog("[MediaPlugin] enterPictureInPictureFromRust \(width?.intValue ?? 0)x\(height?.intValue ?? 0), ratio taken from the track")
        DispatchQueue.main.async {
            let script = """
            (() => { const v = document.querySelector('\(Self.videoSelector)');
            if (!v || typeof v.webkitSetPresentationMode !== 'function') return 'missing';
            v.webkitSetPresentationMode('picture-in-picture'); return 'entered'; })()
            """
            Self.findWebView()?.evaluateJavaScript(script) { result, _ in
                NSLog("[MediaPlugin] picture-in-picture request: \(result ?? "no result")")
            }
        }
    }

    /// Ask the window to rotate.
    ///
    /// iOS only honors this within the orientations the host allows, so the
    /// app's `Info.plist` and root view controller must permit the orientation
    /// being requested; the request is otherwise refused with no effect.
    @objc
    public func setOrientationFromRust(_ orientation: String) {
        DispatchQueue.main.async {
            if #available(iOS 16.0, *) {
                let mask: UIInterfaceOrientationMask
                switch orientation {
                case "landscape": mask = .landscape
                case "portrait": mask = .portrait
                default: mask = .all
                }
                Self.topViewController()?.setNeedsUpdateOfSupportedInterfaceOrientations()
                Self.activeScene()?.requestGeometryUpdate(.iOS(interfaceOrientations: mask)) { error in
                    NSLog("[MediaPlugin] orientation request refused: \(error.localizedDescription)")
                }
            } else {
                let value: Int
                switch orientation {
                case "landscape": value = UIInterfaceOrientation.landscapeRight.rawValue
                case "portrait": value = UIInterfaceOrientation.portrait.rawValue
                default: return
                }
                UIDevice.current.setValue(value, forKey: "orientation")
                UIViewController.attemptRotationToDeviceOrientation()
            }
        }
    }

    /// Start or stop background playback and its lock-screen presence.
    @objc
    public func setPlaybackActiveFromRust(_ playbackJson: String) {
        guard let data = playbackJson.data(using: .utf8),
              let playback = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let active = playback["active"] as? Bool,
              let title = playback["title"] as? String else {
            NSLog("[MediaPlugin] playback state could not be decoded")
            return
        }
        DispatchQueue.main.async {
            let wasActive = self.playbackActive
            let titleChanged = active && title != self.playbackTitle
            self.playbackActive = active
            self.playbackTitle = active ? title : ""

            if active {
                // Re-activating a live session and re-registering commands on
                // every duplicate `play` event is audible as a stutter, so do
                // it only when the state or the metadata actually changed.
                if !wasActive || titleChanged {
                    self.configureAudioSession()
                    self.activateAudioSession(true)
                    self.registerRemoteCommands()
                    self.updateNowPlaying()
                }
            } else if wasActive {
                MPNowPlayingInfoCenter.default().nowPlayingInfo = nil
                self.activateAudioSession(false)
            }
        }
    }

    // MARK: - Audio session

    private func configureAudioSession() {
        if sessionConfigured { return }
        do {
            try AVAudioSession.sharedInstance().setCategory(.playback, mode: .moviePlayback)
            sessionConfigured = true
            NSLog("[MediaPlugin] audio session category set to playback")
        } catch {
            NSLog("[MediaPlugin] failed to set audio session category: \(error.localizedDescription)")
        }
    }

    private func activateAudioSession(_ active: Bool) {
        do {
            // Telling other apps on the way out lets whatever was playing
            // before resume instead of staying ducked.
            try AVAudioSession.sharedInstance().setActive(
                active,
                options: active ? [] : [.notifyOthersOnDeactivation]
            )
        } catch {
            NSLog("[MediaPlugin] failed to set audio session active=\(active): \(error.localizedDescription)")
        }
    }

    // MARK: - Remote commands

    private func registerRemoteCommands() {
        if commandsRegistered { return }
        // Without this the command center is registered but never delivered to.
        UIApplication.shared.beginReceivingRemoteControlEvents()

        let center = MPRemoteCommandCenter.shared()
        _ = center.playCommand.addTarget { [weak self] _ in
            self?.run(script: Self.playScript) ?? .commandFailed
        }
        _ = center.pauseCommand.addTarget { [weak self] _ in
            self?.run(script: Self.pauseScript) ?? .commandFailed
        }
        _ = center.togglePlayPauseCommand.addTarget { [weak self] _ in
            self?.run(script: Self.toggleScript) ?? .commandFailed
        }
        center.skipBackwardCommand.preferredIntervals = [NSNumber(value: Self.skipInterval)]
        _ = center.skipBackwardCommand.addTarget { [weak self] _ in
            self?.run(script: Self.seekScript(by: -Self.skipInterval)) ?? .commandFailed
        }
        center.skipForwardCommand.preferredIntervals = [NSNumber(value: Self.skipInterval)]
        _ = center.skipForwardCommand.addTarget { [weak self] _ in
            self?.run(script: Self.seekScript(by: Self.skipInterval)) ?? .commandFailed
        }
        commandsRegistered = true
        NSLog("[MediaPlugin] remote commands registered")
    }

    private func run(script: String) -> MPRemoteCommandHandlerStatus {
        guard playbackActive, let webView = Self.findWebView() else {
            return .noSuchContent
        }
        webView.evaluateJavaScript(script) { [weak self] _, _ in
            self?.updateNowPlaying()
        }
        return .success
    }

    // The `__tawnyPlaybackIntent` flag mirrors the Android plugin: it records
    // that the user, not the page, asked for this state change.
    private static let playScript = """
    (() => { const v = document.querySelector('\(videoSelector)'); if (!v) return 'missing';
    v.__tawnyPlaybackIntent = true; v.play().catch(() => {}); return 'playing'; })()
    """

    private static let pauseScript = """
    (() => { const v = document.querySelector('\(videoSelector)'); if (!v) return 'missing';
    v.__tawnyPlaybackIntent = false; v.pause(); return 'paused'; })()
    """

    private static let toggleScript = """
    (() => { const v = document.querySelector('\(videoSelector)'); if (!v) return 'missing';
    if (v.paused) { v.__tawnyPlaybackIntent = true; v.play().catch(() => {}); return 'playing'; }
    v.__tawnyPlaybackIntent = false; v.pause(); return 'paused'; })()
    """

    private static func seekScript(by seconds: TimeInterval) -> String {
        // A live or still-loading stream reports a non-finite duration, so
        // clamp against it only once there is one to clamp against.
        let outcome = seconds < 0 ? "rewound" : "forwarded"
        return """
        (() => { const v = document.querySelector('\(videoSelector)'); if (!v) return 'missing';
        const target = Math.max(0, v.currentTime + \(seconds));
        v.currentTime = Number.isFinite(v.duration) ? Math.min(v.duration, target) : target;
        return '\(outcome)'; })()
        """
    }

    // MARK: - Now Playing

    private static let progressScript = """
    (() => { const v = document.querySelector('\(videoSelector)'); if (!v) return '';
    return JSON.stringify({ position: v.currentTime,
    duration: Number.isFinite(v.duration) ? v.duration : 0, paused: v.paused }); })()
    """

    /// Publish the lock-screen entry, reading position from the page.
    ///
    /// The times come from the video element rather than being tracked here:
    /// the page owns playback, and a scrubber fed by a guess drifts out of
    /// step with what the user hears.
    private func updateNowPlaying() {
        guard playbackActive else { return }
        let title = playbackTitle
        guard let webView = Self.findWebView() else {
            MPNowPlayingInfoCenter.default().nowPlayingInfo = [MPMediaItemPropertyTitle: title]
            return
        }
        webView.evaluateJavaScript(Self.progressScript) { result, _ in
            var info: [String: Any] = [MPMediaItemPropertyTitle: title]
            if let json = result as? String,
               let data = json.data(using: .utf8),
               let parsed = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] {
                let paused = parsed["paused"] as? Bool ?? false
                info[MPNowPlayingInfoPropertyElapsedPlaybackTime] = parsed["position"] as? Double ?? 0
                info[MPMediaItemPropertyPlaybackDuration] = parsed["duration"] as? Double ?? 0
                info[MPNowPlayingInfoPropertyPlaybackRate] = paused ? 0.0 : 1.0
            }
            MPNowPlayingInfoCenter.default().nowPlayingInfo = info
        }
    }

    // MARK: - View lookup

    private static func activeScene() -> UIWindowScene? {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        return scenes.first { $0.activationState == .foregroundActive } ?? scenes.first
    }

    private static func keyWindow() -> UIWindow? {
        let scene = activeScene()
        return scene?.windows.first(where: { $0.isKeyWindow }) ?? scene?.windows.first
    }

    private static func topViewController() -> UIViewController? {
        var current = keyWindow()?.rootViewController
        while let presented = current?.presentedViewController {
            current = presented
        }
        return current
    }

    private static func findWebView() -> WKWebView? {
        guard let window = keyWindow() else { return nil }
        return findWebView(in: window)
    }

    private static func findWebView(in view: UIView) -> WKWebView? {
        if let webView = view as? WKWebView {
            return webView
        }
        for subview in view.subviews {
            if let found = findWebView(in: subview) {
                return found
            }
        }
        return nil
    }
}
