import Foundation
import UserNotifications

/**
 * The one `UNUserNotificationCenter` delegate, shared by the local and push
 * plugins.
 *
 * iOS has a single delegate slot for both kinds of notification, and it is
 * the only way to learn that one was tapped or arrived in the foreground. So
 * both plugins install this same object, and it sorts what it hears by origin:
 * a notification with a push trigger goes to the push queue, everything else
 * to the local one.
 *
 * A tap that launches the app is delivered while launching, and only to a
 * delegate already in place — Apple's documentation asks for it to be set
 * before launching finishes. That is why the plugins install it synchronously
 * from `prepare`, which the Rust side documents calling in `main`.
 *
 * It only ever takes an empty slot. A host that installed its own delegate
 * keeps it, and the plugins log rather than displacing it.
 */
final class NotificationCenterRelay: NSObject, UNUserNotificationCenterDelegate {
    static let shared = NotificationCenterRelay()

    private static let lock = NSLock()
    private static var installed = false
    private static var localEvents: [String] = []
    private static var pushEvents: [String] = []
    /// An app that never drains a queue should not grow it forever.
    private static let limit = 50

    static func install() {
        lock.lock()
        defer { lock.unlock() }
        if installed { return }
        installed = true
        let center = UNUserNotificationCenter.current()
        if let existing = center.delegate, !(existing === shared) {
            NSLog("[NotificationCenterRelay] \(type(of: existing)) is already the notification delegate; leaving it alone")
            return
        }
        center.delegate = shared
    }

    // MARK: - Queues

    private static func encode(_ object: [String: Any]) -> String? {
        guard let data = try? JSONSerialization.data(withJSONObject: jsonSafe(object)),
              let text = String(data: data, encoding: .utf8) else { return nil }
        return text
    }

    static func enqueueLocal(_ event: [String: Any]) {
        guard let text = encode(event) else { return }
        lock.lock()
        defer { lock.unlock() }
        localEvents.append(text)
        if localEvents.count > limit { localEvents.removeFirst() }
    }

    static func enqueuePush(_ event: [String: Any]) {
        guard let text = encode(event) else { return }
        lock.lock()
        defer { lock.unlock() }
        pushEvents.append(text)
        if pushEvents.count > limit { pushEvents.removeFirst() }
    }

    static func takeLocal() -> String? {
        lock.lock()
        defer { lock.unlock() }
        return localEvents.isEmpty ? nil : localEvents.removeFirst()
    }

    static func takePush() -> String? {
        lock.lock()
        defer { lock.unlock() }
        return pushEvents.isEmpty ? nil : pushEvents.removeFirst()
    }

    // MARK: - Shapes

    /**
     * A push payload is `[AnyHashable: Any]` and may hold anything; JSON holds
     * strings, numbers, booleans, arrays, and string-keyed objects. Anything
     * else becomes its description rather than failing the whole event.
     */
    static func jsonSafe(_ value: Any) -> Any {
        switch value {
        case let dictionary as [AnyHashable: Any]:
            var object: [String: Any] = [:]
            for (key, inner) in dictionary { object["\(key)"] = jsonSafe(inner) }
            return object
        case let array as [Any]:
            return array.map(jsonSafe)
        case is String, is NSNumber, is NSNull:
            return value
        default:
            return "\(value)"
        }
    }

    /// JSON null for an empty string, which is how iOS spells "not set".
    /// A function rather than a ternary: the two branches have different
    /// types, and Swift will not always join them to `Any` unasked.
    static func nullIfEmpty(_ text: String) -> Any {
        if text.isEmpty { return NSNull() }
        return text
    }

    static func orNull(_ value: Any?) -> Any {
        if let value = value { return value }
        return NSNull()
    }

    static func isRemote(_ notification: UNNotification) -> Bool {
        notification.request.trigger is UNPushNotificationTrigger
    }

    /// The part of a local notification handed back to Rust.
    static func summary(_ request: UNNotificationRequest) -> [String: Any] {
        let content = request.content
        return [
            "id": Int(request.identifier) ?? 0,
            "title": content.title,
            "body": nullIfEmpty(content.body),
            "group": nullIfEmpty(content.threadIdentifier),
            "actionTypeId": nullIfEmpty(content.categoryIdentifier),
            "extra": content.userInfo[NotificationsPlugin.extraKey] ?? [String: Any](),
        ]
    }

    private static func pushData(_ content: UNNotificationContent) -> [String: Any] {
        jsonSafe(content.userInfo) as? [String: Any] ?? [:]
    }

    // MARK: - UNUserNotificationCenterDelegate

    /**
     * A notification arriving while the app is frontmost. A local one is
     * shown as a banner, as it would be in the background, since the app
     * asked for it. A push is not: Android does not display a push that
     * arrives in the foreground either, so on both platforms the app gets
     * the message and decides.
     */
    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        let content = notification.request.content
        if Self.isRemote(notification) {
            Self.enqueuePush([
                "type": "message",
                "title": Self.nullIfEmpty(content.title),
                "body": Self.nullIfEmpty(content.body),
                "data": Self.pushData(content),
            ])
            completionHandler([])
            return
        }
        Self.enqueueLocal([
            "type": "received",
            "notification": Self.summary(notification.request),
        ])
        if #available(iOS 14.0, *) {
            completionHandler([.banner, .list, .sound, .badge])
        } else {
            completionHandler([.alert, .sound, .badge])
        }
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        defer { completionHandler() }
        var action = response.actionIdentifier
        if action == UNNotificationDefaultActionIdentifier { action = "tap" }
        if action == UNNotificationDismissActionIdentifier { action = "dismiss" }
        let notification = response.notification
        if Self.isRemote(notification) {
            Self.enqueuePush([
                "type": "opened",
                "actionId": action,
                "data": Self.pushData(notification.request.content),
            ])
            return
        }
        let input = (response as? UNTextInputNotificationResponse)?.userText
        Self.enqueueLocal([
            "type": "action",
            "actionId": action,
            "input": Self.orNull(input),
            "notification": Self.summary(notification.request),
        ])
    }
}
