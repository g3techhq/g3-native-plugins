import Foundation
import Security

/**
 * Key-value storage in the Keychain.
 *
 * Each entry is a generic-password item scoped to this app's bundle id, which
 * is what keeps one app's values out of another's. The Keychain, not the app,
 * holds the encryption key, and on a device with a Secure Enclave it never
 * leaves it.
 *
 * Items are written `kSecAttrAccessibleAfterFirstUnlock`: readable in the
 * background and across a reboot once the user has unlocked the device at least
 * once, and never before that. The stricter `WhenUnlocked` would lock the app
 * out of its own session whenever the screen was off, which is wrong for
 * anything a background task needs.
 *
 * **Key names are stored in the clear.** They are Keychain account names, which
 * are attributes rather than protected content.
 *
 * Errors come back as a JSON object carrying `error`, which is how the Rust
 * side tells a failure from a stored value.
 */
@objc(StoragePlugin)
public class StoragePlugin: NSObject {

    /// Scoping every item to the bundle id keeps this app's values separate
    /// from anything else in the keychain access group.
    private static var service: String {
        Bundle.main.bundleIdentifier ?? "dev.dioxus.g3_native_plugins.storage"
    }

    private static func query(for key: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key,
        ]
    }

    private static func errorJson(_ message: String) -> String {
        guard let data = try? JSONSerialization.data(withJSONObject: ["error": message]),
              let json = String(data: data, encoding: .utf8) else {
            return "{\"error\":\"The keychain request failed.\"}"
        }
        return json
    }

    @objc
    public func getFromRust(_ key: String) -> String? {
        var query = Self.query(for: key)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne

        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        if status == errSecItemNotFound {
            return nil
        }
        guard status == errSecSuccess else {
            return Self.errorJson("Could not read '\(key)' from the keychain: \(status)")
        }
        guard let data = item as? Data, let value = String(data: data, encoding: .utf8) else {
            return Self.errorJson("Stored value for '\(key)' is not readable text.")
        }
        return value
    }

    @objc
    public func setFromRust(_ entryJson: String) -> String? {
        guard let data = entryJson.data(using: .utf8),
              let entry = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let key = entry["key"] as? String,
              let value = entry["value"] as? String else {
            return Self.errorJson("Could not decode the keychain entry.")
        }
        guard let data = value.data(using: .utf8) else {
            return Self.errorJson("Could not encode the value for '\(key)'.")
        }
        let query = Self.query(for: key)
        // The Keychain has no upsert: adding over an existing item fails, so
        // update what is there and fall back to adding when there is nothing.
        let update: [String: Any] = [kSecValueData as String: data]
        let updated = SecItemUpdate(query as CFDictionary, update as CFDictionary)
        if updated == errSecSuccess {
            return nil
        }
        guard updated == errSecItemNotFound else {
            return Self.errorJson("Could not update '\(key)' in the keychain: \(updated)")
        }

        var insert = query
        insert[kSecValueData as String] = data
        insert[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlock
        let added = SecItemAdd(insert as CFDictionary, nil)
        guard added == errSecSuccess else {
            return Self.errorJson("Could not store '\(key)' in the keychain: \(added)")
        }
        return nil
    }

    @objc
    public func removeFromRust(_ key: String) -> String? {
        let status = SecItemDelete(Self.query(for: key) as CFDictionary)
        // Deleting something that was never there is the outcome the caller
        // wanted, not a failure.
        guard status == errSecSuccess || status == errSecItemNotFound else {
            return Self.errorJson("Could not remove '\(key)' from the keychain: \(status)")
        }
        return nil
    }

    @objc
    public func clearFromRust() -> String? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            // Scoped to this service, so an app's other keychain items survive.
            kSecAttrService as String: Self.service,
        ]
        let status = SecItemDelete(query as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            return Self.errorJson("Could not clear the keychain: \(status)")
        }
        return nil
    }

    @objc
    public func keysFromRust() -> String {
        var query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: Self.service,
        ]
        query[kSecReturnAttributes as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitAll

        var items: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &items)
        if status == errSecItemNotFound {
            return "[]"
        }
        guard status == errSecSuccess, let entries = items as? [[String: Any]] else {
            return Self.errorJson("Could not list keychain entries: \(status)")
        }
        let keys = entries.compactMap { $0[kSecAttrAccount as String] as? String }
        guard let data = try? JSONSerialization.data(withJSONObject: keys),
              let json = String(data: data, encoding: .utf8) else {
            return Self.errorJson("Could not encode the stored keys.")
        }
        return json
    }
}
