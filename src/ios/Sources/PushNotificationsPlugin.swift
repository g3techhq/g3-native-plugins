import Foundation
import ObjectiveC
import UIKit
import UserNotifications

/**
 * Remote push from APNs.
 *
 * APNs answers a registration by calling the app delegate —
 * `application(_:didRegisterForRemoteNotificationsWithDeviceToken:)` or its
 * failure twin — and delivers data-only pushes to
 * `application(_:didReceiveRemoteNotification:fetchCompletionHandler:)`. The
 * delegate belongs to the windowing layer Dioxus runs on and implements none
 * of them, so, like the deep-links plugin, this adds them to the live
 * delegate's class at runtime. It only ever adds: a delegate that already
 * implements one keeps it, and the plugin logs instead.
 *
 * Taps and foreground deliveries come through the notification center
 * delegate shared with the local plugin, which sorts pushes into this
 * plugin's queue.
 *
 * Registration needs the Push Notifications capability — an `aps-environment`
 * entitlement from the provisioning profile. Without it APNs refuses, and the
 * refusal arrives as a `registrationFailed` event rather than silence.
 */
@objc(PushNotificationsPlugin)
public class PushNotificationsPlugin: NSObject {
    private static let lock = NSLock()
    private static var token: String?
    private static var hooksInstalled = false
    private static var launchObserver: NSObjectProtocol?

    @objc
    public func prepareFromRust() {
        // The notification center delegate has to be in place before
        // launching finishes to hear about a tap that launched the app.
        NotificationCenterRelay.install()
        // Always deferred, even from the main thread: `prepare` is meant to be
        // called from `main`, before UIApplicationMain has created the
        // application, and touching `UIApplication.shared` then would crash.
        // A main-queue block first runs once the run loop does, by which
        // point the application exists.
        DispatchQueue.main.async { Self.prepareOnMain() }
    }

    private static func prepareOnMain() {
        installDelegateHooks()
        if hooksInstalled || launchObserver != nil { return }
        // Before launch there is no delegate to add the hooks to yet.
        launchObserver = NotificationCenter.default.addObserver(
            forName: UIApplication.didFinishLaunchingNotification,
            object: nil,
            queue: .main
        ) { _ in installDelegateHooks() }
    }

    @objc
    public func registerFromRust(_ configJson: String) -> String? {
        DispatchQueue.main.async {
            Self.installDelegateHooks()
            UIApplication.shared.registerForRemoteNotifications()
        }
        return nil
    }

    @objc
    public func unregisterFromRust() -> String? {
        DispatchQueue.main.async {
            UIApplication.shared.unregisterForRemoteNotifications()
        }
        Self.lock.lock()
        Self.token = nil
        Self.lock.unlock()
        return nil
    }

    @objc
    public func tokenFromRust() -> String? {
        Self.lock.lock()
        defer { Self.lock.unlock() }
        return Self.token
    }

    @objc
    public func takeEventFromRust() -> String? {
        NotificationCenterRelay.takePush()
    }

    fileprivate static func received(token data: Data) {
        let hex = data.map { String(format: "%02x", $0) }.joined()
        lock.lock()
        let changed = token != hex
        token = hex
        lock.unlock()
        // iOS answers every registration, usually with the same token. Only a
        // change is news for the server.
        if changed {
            NotificationCenterRelay.enqueuePush(["type": "token", "token": hex])
        }
    }

    private static func installDelegateHooks() {
        if hooksInstalled { return }
        guard let delegate = UIApplication.shared.delegate else { return }
        let cls: AnyClass = type(of: delegate)

        // Blocks used as an IMP take the receiver as their first argument and,
        // unlike a C function IMP, no selector argument.
        let registered: @convention(block) (AnyObject, UIApplication, Data) -> Void = { _, _, token in
            received(token: token)
        }
        let failed: @convention(block) (AnyObject, UIApplication, NSError) -> Void = { _, _, error in
            NotificationCenterRelay.enqueuePush([
                "type": "registrationFailed",
                "error": error.localizedDescription,
            ])
        }
        // A push with an alert is reported by the notification center delegate
        // when it arrives in the foreground, and iOS may call this one for it
        // as well. Only a push with nothing to display is reported from here,
        // so no push is reported twice.
        let data: @convention(block) (
            AnyObject, UIApplication, [AnyHashable: Any], (UIBackgroundFetchResult) -> Void
        ) -> Void = { _, _, payload, completion in
            let aps = payload["aps"] as? [AnyHashable: Any]
            if aps?["alert"] == nil {
                NotificationCenterRelay.enqueuePush([
                    "type": "message",
                    "title": NSNull(),
                    "body": NSNull(),
                    "data": NotificationCenterRelay.jsonSafe(payload),
                ])
            }
            completion(.newData)
        }

        // Added one at a time with the block's own type: a block passed on as
        // `Any` is not guaranteed to reach the runtime as a block.
        add(
            cls,
            #selector(UIApplicationDelegate.application(_:didRegisterForRemoteNotificationsWithDeviceToken:)),
            imp_implementationWithBlock(registered),
            "v@:@@"
        )
        add(
            cls,
            #selector(UIApplicationDelegate.application(_:didFailToRegisterForRemoteNotificationsWithError:)),
            imp_implementationWithBlock(failed),
            "v@:@@"
        )
        add(
            cls,
            #selector(UIApplicationDelegate.application(_:didReceiveRemoteNotification:fetchCompletionHandler:)),
            imp_implementationWithBlock(data),
            "v@:@@@?"
        )
        hooksInstalled = true
        NSLog("[PushNotificationsPlugin] delegate hooks installed on \(cls)")
    }

    private static func add(_ cls: AnyClass, _ selector: Selector, _ imp: IMP, _ types: String) {
        if !class_addMethod(cls, selector, imp, types) {
            NSLog("[PushNotificationsPlugin] \(cls) already implements \(selector); leaving it alone")
        }
    }
}
