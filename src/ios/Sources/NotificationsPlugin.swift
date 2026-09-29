import Foundation
import UIKit
import UserNotifications

/**
 * Local notifications through `UNUserNotificationCenter`: shown now or on a
 * trigger, with buttons and text replies.
 *
 * Several of the center's queries only answer through a completion handler.
 * Those are waited for here, with a timeout, which is safe on any thread:
 * the center runs its completions on a queue of its own, never the one that
 * is waiting. Commands answer nil, or a JSON object carrying `error`.
 */
@objc(NotificationsPlugin)
public class NotificationsPlugin: NSObject {
    /// Where a notification's `extra` rides in its `userInfo`.
    static let extraKey = "g3.extra"
    /// Where its schedule rides, so `pending()` can report it back as given.
    static let scheduleKey = "g3.schedule"

    private static let timeout = DispatchTimeInterval.seconds(5)

    private var center: UNUserNotificationCenter { UNUserNotificationCenter.current() }

    private static func json(_ object: Any) -> String {
        guard JSONSerialization.isValidJSONObject(object),
              let data = try? JSONSerialization.data(withJSONObject: object),
              let text = String(data: data, encoding: .utf8) else {
            return errorJson("The notification answer could not be encoded.")
        }
        return text
    }

    private static func errorJson(_ message: String) -> String {
        guard let data = try? JSONSerialization.data(withJSONObject: ["error": message]),
              let text = String(data: data, encoding: .utf8) else {
            return "{\"error\":\"The notification request failed.\"}"
        }
        return text
    }

    private static func decode(_ text: String) -> Any? {
        guard let data = text.data(using: .utf8) else { return nil }
        return try? JSONSerialization.jsonObject(with: data)
    }

    /// Wait for a completion-handler API, for at most [timeout].
    private static func wait<T>(_ start: (@escaping (T) -> Void) -> Void) -> T? {
        let done = DispatchSemaphore(value: 0)
        let box = Box<T>()
        start { value in
            box.value = value
            done.signal()
        }
        guard done.wait(timeout: .now() + timeout) == .success else { return nil }
        return box.value
    }

    @objc
    public func prepareFromRust() {
        NotificationCenterRelay.install()
    }

    @objc
    public func takeEventFromRust() -> String? {
        NotificationCenterRelay.takeLocal()
    }

    @objc
    public func checkPermissionsFromRust() -> String? {
        guard let settings = Self.wait({ self.center.getNotificationSettings(completionHandler: $0) }) else {
            return nil
        }
        switch settings.authorizationStatus {
        case .notDetermined:
            return "prompt"
        case .denied:
            return "denied"
        default:
            // Authorized, provisional, and ephemeral all let the app post.
            return "granted"
        }
    }

    @objc
    public func requestPermissionsFromRust() {
        center.requestAuthorization(options: [.alert, .sound, .badge]) { _, error in
            if let error = error {
                NSLog("[NotificationsPlugin] permission request failed: \(error)")
            }
        }
    }

    @objc
    public func openSettingsFromRust() {
        DispatchQueue.main.async {
            let target: String
            if #available(iOS 16.0, *) {
                target = UIApplication.openNotificationSettingsURLString
            } else {
                target = UIApplication.openSettingsURLString
            }
            guard let url = URL(string: target) else { return }
            UIApplication.shared.open(url)
        }
    }

    private static func seconds(_ interval: String) -> TimeInterval {
        switch interval {
        case "minute": return 60
        case "hour": return 3_600
        case "day": return 86_400
        default: return 604_800
        }
    }

    /// Nil for "now"; a trigger otherwise. Throws for a schedule iOS refuses.
    private static func trigger(_ schedule: [String: Any]?) throws -> UNNotificationTrigger? {
        guard let schedule = schedule else { return nil }
        if schedule["kind"] as? String == "every" {
            let count = max(1, schedule["count"] as? Int ?? 1)
            let every = seconds(schedule["interval"] as? String ?? "day") * Double(count)
            guard every >= 60 else {
                throw PluginError("iOS will not repeat a notification more often than once a minute.")
            }
            return UNTimeIntervalNotificationTrigger(timeInterval: every, repeats: true)
        }
        let at = (schedule["atMs"] as? Double ?? 0) / 1000
        let delay = at - Date().timeIntervalSince1970
        // A moment already past fires now. Zero is not a valid interval.
        guard delay >= 1 else { return nil }
        return UNTimeIntervalNotificationTrigger(timeInterval: delay, repeats: false)
    }

    @objc
    public func showFromRust(_ notificationJson: String) -> String? {
        guard let notification = Self.decode(notificationJson) as? [String: Any],
              let id = notification["id"] as? Int else {
            return Self.errorJson("Could not read the notification.")
        }
        let content = UNMutableNotificationContent()
        content.title = notification["title"] as? String ?? ""
        if let body = notification["body"] as? String { content.body = body }
        if let group = notification["group"] as? String { content.threadIdentifier = group }
        if let type = notification["actionTypeId"] as? String { content.categoryIdentifier = type }
        if let badge = notification["badge"] as? Int { content.badge = NSNumber(value: badge) }
        if notification["silent"] as? Bool != true {
            if let sound = notification["sound"] as? String {
                content.sound = UNNotificationSound(named: UNNotificationSoundName(sound))
            } else {
                content.sound = .default
            }
        }
        var userInfo: [String: Any] = [Self.extraKey: notification["extra"] ?? [String: Any]()]
        let schedule = notification["schedule"] as? [String: Any]
        if let schedule = schedule { userInfo[Self.scheduleKey] = schedule }
        content.userInfo = userInfo

        let trigger: UNNotificationTrigger?
        do {
            trigger = try Self.trigger(schedule)
        } catch {
            return Self.errorJson("\(error)")
        }
        let request = UNNotificationRequest(
            identifier: String(id),
            content: content,
            trigger: trigger
        )
        let added: Error?? = Self.wait { done in self.center.add(request) { done($0) } }
        guard let outcome = added else {
            return Self.errorJson("The notification center did not answer.")
        }
        if let error = outcome {
            return Self.errorJson("Could not show the notification: \(error.localizedDescription)")
        }
        return nil
    }

    @objc
    public func pendingFromRust() -> String? {
        guard let requests = Self.wait({ self.center.getPendingNotificationRequests(completionHandler: $0) }) else {
            return Self.errorJson("The notification center did not answer.")
        }
        let pending: [[String: Any]] = requests.compactMap { request -> [String: Any]? in
            guard let id = Int(request.identifier) else { return nil }
            return [
                "id": id,
                "title": request.content.title,
                "body": NotificationCenterRelay.nullIfEmpty(request.content.body),
                "schedule": request.content.userInfo[Self.scheduleKey] ?? NSNull(),
            ]
        }
        return Self.json(pending)
    }

    private static func identifiers(_ idsJson: String) -> [String]? {
        (decode(idsJson) as? [Int])?.map(String.init)
    }

    @objc
    public func cancelFromRust(_ idsJson: String) -> String? {
        guard let ids = Self.identifiers(idsJson) else {
            return Self.errorJson("Could not read the notification ids.")
        }
        center.removePendingNotificationRequests(withIdentifiers: ids)
        return nil
    }

    @objc
    public func cancelAllFromRust() -> String? {
        center.removeAllPendingNotificationRequests()
        return nil
    }

    /// Only this plugin's: a push the system displayed has no numeric id.
    @objc
    public func activeFromRust() -> String? {
        guard let delivered = Self.wait({ self.center.getDeliveredNotifications(completionHandler: $0) }) else {
            return Self.errorJson("The notification center did not answer.")
        }
        let active = delivered
            .filter { !NotificationCenterRelay.isRemote($0) && Int($0.request.identifier) != nil }
            .map { NotificationCenterRelay.jsonSafe(NotificationCenterRelay.summary($0.request)) }
        return Self.json(active)
    }

    @objc
    public func removeActiveFromRust(_ idsJson: String) -> String? {
        guard let ids = Self.identifiers(idsJson) else {
            return Self.errorJson("Could not read the notification ids.")
        }
        center.removeDeliveredNotifications(withIdentifiers: ids)
        return nil
    }

    @objc
    public func removeAllActiveFromRust() -> String? {
        center.removeAllDeliveredNotifications()
        return nil
    }

    /// iOS has no channels: sound and interruption are per notification.
    @objc
    public func createChannelFromRust(_ channelJson: String) -> String? { nil }

    @objc
    public func deleteChannelFromRust(_ id: String) -> String? { nil }

    @objc
    public func channelsFromRust() -> String? { "[]" }

    @objc
    public func registerActionTypesFromRust(_ typesJson: String) -> String? {
        guard let types = Self.decode(typesJson) as? [[String: Any]] else {
            return Self.errorJson("Could not read the action types.")
        }
        var categories = Set<UNNotificationCategory>()
        for type in types {
            guard let id = type["id"] as? String else { continue }
            let specs = type["actions"] as? [[String: Any]] ?? []
            let actions: [UNNotificationAction] = specs.compactMap { action -> UNNotificationAction? in
                guard let actionId = action["id"] as? String,
                      let title = action["title"] as? String else { return nil }
                var options: UNNotificationActionOptions = []
                if action["foreground"] as? Bool == true { options.insert(.foreground) }
                if action["destructive"] as? Bool == true { options.insert(.destructive) }
                if action["requiresAuthentication"] as? Bool == true {
                    options.insert(.authenticationRequired)
                }
                if action["input"] as? Bool == true {
                    return UNTextInputNotificationAction(
                        identifier: actionId,
                        title: title,
                        options: options,
                        textInputButtonTitle: action["inputButtonTitle"] as? String ?? "Send",
                        textInputPlaceholder: action["inputPlaceholder"] as? String ?? ""
                    )
                }
                return UNNotificationAction(identifier: actionId, title: title, options: options)
            }
            categories.insert(UNNotificationCategory(
                identifier: id,
                actions: actions,
                intentIdentifiers: [],
                options: []
            ))
        }
        center.setNotificationCategories(categories)
        return nil
    }
}

/// Carries a completion's value out to the thread waiting on it.
private final class Box<T> {
    var value: T?
}

private struct PluginError: Error, CustomStringConvertible {
    let description: String
    init(_ description: String) { self.description = description }
}
