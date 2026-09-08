import Foundation
import UIKit
import WebKit

/**
 * Turns the system back gesture into a history navigation instead of nothing.
 *
 * The gesture iOS users perform to go back is the swipe in from the left edge.
 * UIKit only gives it meaning inside a `UINavigationController`, and a web app
 * hosted in a bare `WKWebView` has no such stack, so the swipe lands on nothing
 * however the app is written. Attaching a screen-edge recognizer to the WebView
 * takes the gesture and gives it somewhere to go.
 *
 * The gesture is forwarded as the same `g3nativeback` DOM event the Android
 * plugin dispatches, so one web-side listener serves both platforms. As on
 * Android it is not `history.back()`: the Rust binary runs outside the WebView
 * and the router keeps its history there, so the WebView's own history is not
 * the app's — going back on it navigates nothing.
 *
 * Whether to intercept at all is left to the caller via
 * [setInterceptingFromRust]. Only the app knows if there is anywhere to go
 * back to, and a recognizer left enabled at the root of the stack would eat
 * edge swipes that should do nothing.
 */
@objc(BackButtonPlugin)
public class BackButtonPlugin: NSObject {

    /// Named for the crate rather than any one app, since the plugin does not
    /// know who is listening. Kept byte-identical to the Android script.
    static let backEventScript =
        "window.dispatchEvent(new Event('g3nativeback', { cancelable: true }))"

    /// A swipe shorter and slower than this reads as a stray touch near the
    /// bezel rather than a request to navigate. UIKit's own pop gesture
    /// commits on either distance or throw velocity; matching that keeps the
    /// gesture feeling native instead of stubborn.
    private static let commitTranslation: CGFloat = 40
    private static let commitVelocity: CGFloat = 300

    private var recognizer: UIScreenEdgePanGestureRecognizer?
    private var intercepting = false
    private var installAttempts = 0

    /// Whether the edge-swipe back gesture is taken by the app.
    @objc
    public func setInterceptingFromRust(_ intercepting: Bool) {
        NSLog("[BackButtonPlugin] setInterceptingFromRust \(intercepting)")
        // Gesture recognizers are UIKit state; Rust/Dioxus effects run on a
        // native worker thread.
        DispatchQueue.main.async {
            self.intercepting = intercepting
            self.installAttempts = 0
            self.applyIntercepting()
        }
    }

    /// Pass one intercepted gesture back to the platform.
    ///
    /// On Android the equivalent hands the press to the next Back handler,
    /// which closes the app. iOS has no such default — an edge swipe with
    /// nowhere to go is simply inert, and an app may not exit itself — so
    /// there is nothing to hand back and the gesture is dropped.
    @objc
    public func fallThroughFromRust() {
        NSLog("[BackButtonPlugin] fallThroughFromRust: iOS has no default back handler, dropping")
    }

    private func applyIntercepting() {
        guard let recognizer = ensureRecognizer() else {
            scheduleInstallRetry()
            return
        }
        recognizer.isEnabled = intercepting
    }

    private func scheduleInstallRetry() {
        // A route effect can ask to intercept before the host has finished
        // building its WKWebView. Retry briefly rather than silently dropping
        // the request and leaving the gesture dead for the session.
        guard installAttempts < 20 else {
            NSLog("[BackButtonPlugin] no WKWebView found to attach the back gesture to")
            return
        }
        installAttempts += 1
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) { [weak self] in
            self?.applyIntercepting()
        }
    }

    private func ensureRecognizer() -> UIScreenEdgePanGestureRecognizer? {
        if let existing = recognizer {
            return existing
        }
        guard let webView = Self.findWebView() else {
            return nil
        }
        // WebKit's own back/forward swipes would navigate the WebView's
        // history, which is not the app's. Ours replaces them.
        webView.allowsBackForwardNavigationGestures = false
        let created = UIScreenEdgePanGestureRecognizer(
            target: self,
            action: #selector(handleEdgePan(_:))
        )
        created.edges = .left
        created.isEnabled = false
        webView.addGestureRecognizer(created)
        // A horizontally scrollable page would otherwise consume the swipe
        // before the edge recognizer decides, the same way UIKit gives a
        // navigation controller's pop gesture priority over content.
        webView.scrollView.panGestureRecognizer.require(toFail: created)
        NSLog("[BackButtonPlugin] attached edge-pan recognizer to WKWebView")
        recognizer = created
        return created
    }

    @objc
    private func handleEdgePan(_ gesture: UIScreenEdgePanGestureRecognizer) {
        guard gesture.state == .ended, let view = gesture.view else {
            return
        }
        let translation = gesture.translation(in: view).x
        let velocity = gesture.velocity(in: view).x
        guard translation >= Self.commitTranslation || velocity >= Self.commitVelocity else {
            NSLog("[BackButtonPlugin] edge pan below commit threshold, ignoring")
            return
        }
        dispatchBackEvent()
    }

    private func dispatchBackEvent() {
        guard let webView = Self.findWebView() else {
            NSLog("[BackButtonPlugin] back gesture had no WKWebView to notify")
            return
        }
        NSLog("[BackButtonPlugin] dispatching g3nativeback")
        webView.evaluateJavaScript(Self.backEventScript, completionHandler: nil)
    }

    private static func findWebView() -> WKWebView? {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        let scene = scenes.first { $0.activationState == .foregroundActive } ?? scenes.first
        guard let window = scene?.windows.first(where: { $0.isKeyWindow }) ?? scene?.windows.first
        else {
            return nil
        }
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
