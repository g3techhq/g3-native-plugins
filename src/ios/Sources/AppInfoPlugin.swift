import Foundation

@objc(AppInfoPlugin)
public class AppInfoPlugin: NSObject {
    @objc
    public func appInfoFromRust() -> String? {
        let bundle = Bundle.main
        guard let identifier = bundle.bundleIdentifier,
              let version = bundle.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String,
              let build = bundle.object(forInfoDictionaryKey: "CFBundleVersion") as? String else {
            return nil
        }
        let value = ["identifier": identifier, "version": version, "build": build]
        guard let bytes = try? JSONSerialization.data(withJSONObject: value) else { return nil }
        return String(data: bytes, encoding: .utf8)
    }
}
