import Foundation
import UIKit

@objc(ClipboardPlugin)
public class ClipboardPlugin: NSObject {

    @objc
    public func copyToClipboardFromRust(_ text: String) -> String {
        NSLog("[ClipboardPlugin] copyToClipboardFromRust received text length: \(text.count)")
        DispatchQueue.main.async {
            NSLog("[ClipboardPlugin] setting UIPasteboard.general.string")
            UIPasteboard.general.string = text
        }
        return "copy scheduled"
    }

    @objc
    public func shareFromRust(_ text: String) -> String {
        NSLog("[ClipboardPlugin] shareFromRust received text length: \(text.count)")
        DispatchQueue.main.async {
            guard let vc = Self.topViewController() else {
                NSLog("[ClipboardPlugin] share failed: no top view controller")
                return
            }
            NSLog("[ClipboardPlugin] presenting UIActivityViewController from \(type(of: vc))")
            let shareVC = UIActivityViewController(
                activityItems: [text],
                applicationActivities: nil
            )
            if let popover = shareVC.popoverPresentationController {
                popover.sourceView = vc.view
                popover.sourceRect = CGRect(
                    x: vc.view.bounds.midX,
                    y: vc.view.bounds.midY,
                    width: 0,
                    height: 0
                )
                popover.permittedArrowDirections = []
            }
            vc.present(shareVC, animated: true)
        }
        return "share scheduled"
    }

    private static func topViewController() -> UIViewController? {
        let windowScene = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .first { $0.activationState == .foregroundActive }
        let root = windowScene?.windows.first { $0.isKeyWindow }?.rootViewController
        if windowScene == nil {
            NSLog("[ClipboardPlugin] no foreground active UIWindowScene")
        }
        if root == nil {
            NSLog("[ClipboardPlugin] no key window rootViewController")
        }
        return topViewController(from: root)
    }

    private static func topViewController(from root: UIViewController?) -> UIViewController? {
        if let presented = root?.presentedViewController {
            return topViewController(from: presented)
        }
        if let navigation = root as? UINavigationController {
            return topViewController(from: navigation.visibleViewController)
        }
        if let tab = root as? UITabBarController {
            return topViewController(from: tab.selectedViewController)
        }
        return root
    }
}
