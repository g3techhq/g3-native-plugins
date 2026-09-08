import Foundation
import ObjectiveC
import UIKit

/**
 * Collects the URLs a universal link or custom scheme launches the app with.
 *
 * iOS delivers a universal link as an `NSUserActivity` to
 * `application(_:continue:restorationHandler:)` and a custom scheme to
 * `application(_:open:options:)`, both on the app delegate. The delegate here
 * belongs to the windowing layer Dioxus runs on, which implements neither, so
 * the link arrives and is dropped on the floor.
 *
 * Rather than require the host to own an app delegate, this plugin adds the two
 * methods to the live delegate's class at runtime. It only ever *adds*: if the
 * delegate already implements one, that implementation is left alone and the
 * plugin logs rather than displacing it.
 *
 * A cold-start link is delivered before any of that can be installed, so it is
 * read from the launch options instead, through
 * `UIApplication.didFinishLaunchingNotification`. Catching it depends on
 * observing that notification before it is posted, which is why the Rust side
 * documents calling `prepare()` as early as it can — ideally before the app
 * launches rather than from a component effect.
 *
 * Links are queued rather than pushed. A link can arrive before the app has
 * rendered anything that could receive it, and unlike a back gesture it is a
 * value that must not be dropped, so the Rust side drains the queue when it is
 * ready.
 *
 * The queue is static: a link belongs to the app, not to whichever instance of
 * this plugin happened to exist when it arrived.
 */
@objc(DeepLinksPlugin)
public class DeepLinksPlugin: NSObject {

    private static let lock = NSLock()
    private static var pending: [String] = []
    private static var lastEnqueued: (url: String, at: Date)?
    private static var hooksInstalled = false
    private static var launchObserver: NSObjectProtocol?

    /// A cold-start link is reported twice — once in the launch options and
    /// again through the delegate callback that follows. They are the same
    /// arrival, so the second one is dropped. Two genuine opens of one URL
    /// within a second is not something a person can do.
    private static let duplicateWindow: TimeInterval = 1

    @objc
    public func prepareFromRust() {
        // The delegate and the notification center are UIKit state; Rust
        // effects run on a native worker thread.
        DispatchQueue.main.async {
            Self.observeLaunch()
            Self.installDelegateHooks()
        }
    }

    /// The oldest link not yet handed to Rust, or nil when the queue is empty.
    @objc
    public func takeLinkFromRust() -> String? {
        Self.lock.lock()
        defer { Self.lock.unlock() }
        return Self.pending.isEmpty ? nil : Self.pending.removeFirst()
    }

    fileprivate static func enqueue(_ url: String) {
        lock.lock()
        defer { lock.unlock() }
        if let last = lastEnqueued, last.url == url,
           Date().timeIntervalSince(last.at) < duplicateWindow {
            NSLog("[DeepLinksPlugin] dropping duplicate arrival of the same link")
            return
        }
        lastEnqueued = (url, Date())
        pending.append(url)
        NSLog("[DeepLinksPlugin] queued a link, \(pending.count) waiting")
    }

    // MARK: - Cold start

    private static func observeLaunch() {
        if launchObserver != nil { return }
        launchObserver = NotificationCenter.default.addObserver(
            forName: UIApplication.didFinishLaunchingNotification,
            object: nil,
            queue: .main
        ) { note in
            readLaunchOptions(note.userInfo)
            // The delegate exists by now even if it did not when the host first
            // asked, so this is the reliable moment to add the callbacks.
            installDelegateHooks()
        }
    }

    private static func readLaunchOptions(_ userInfo: [AnyHashable: Any]?) {
        guard let userInfo = userInfo else { return }
        if let url = userInfo[UIApplication.LaunchOptionsKey.url] as? URL {
            enqueue(url.absoluteString)
        }
        guard let activities = userInfo[UIApplication.LaunchOptionsKey.userActivityDictionary]
            as? [AnyHashable: Any] else { return }
        for value in activities.values {
            guard let activity = value as? NSUserActivity,
                  activity.activityType == NSUserActivityTypeBrowsingWeb,
                  let url = activity.webpageURL else { continue }
            enqueue(url.absoluteString)
        }
    }

    // MARK: - Delegate hooks

    private static func installDelegateHooks() {
        if hooksInstalled { return }
        guard let delegate = UIApplication.shared.delegate else { return }
        let cls: AnyClass = type(of: delegate)

        // Blocks used as an IMP take the receiver as their first argument and,
        // unlike a C function IMP, no selector argument.
        let continueBlock: @convention(block) (
            AnyObject, UIApplication, NSUserActivity, ([UIUserActivityRestoring]?) -> Void
        ) -> Bool = { _, _, activity, _ in
            guard activity.activityType == NSUserActivityTypeBrowsingWeb,
                  let url = activity.webpageURL else { return false }
            enqueue(url.absoluteString)
            return true
        }
        let continueAdded = class_addMethod(
            cls,
            #selector(UIApplicationDelegate.application(_:continue:restorationHandler:)),
            imp_implementationWithBlock(continueBlock),
            "B@:@@@?"
        )

        let openBlock: @convention(block) (
            AnyObject, UIApplication, URL, [UIApplication.OpenURLOptionsKey: Any]
        ) -> Bool = { _, _, url, _ in
            enqueue(url.absoluteString)
            return true
        }
        let openAdded = class_addMethod(
            cls,
            #selector(UIApplicationDelegate.application(_:open:options:)),
            imp_implementationWithBlock(openBlock),
            "B@:@@@"
        )

        if !continueAdded {
            NSLog("[DeepLinksPlugin] \(cls) already handles universal links; leaving it alone")
        }
        if !openAdded {
            NSLog("[DeepLinksPlugin] \(cls) already handles URL schemes; leaving it alone")
        }
        hooksInstalled = true
        NSLog("[DeepLinksPlugin] delegate hooks installed on \(cls)")
    }
}
