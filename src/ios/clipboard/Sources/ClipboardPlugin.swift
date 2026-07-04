import Foundation
import UIKit

@objc(ClipboardPlugin)
public class ClipboardPlugin: NSObject {

    @objc
    public func copyToClipboardFromRust(_ text: String) {
        DispatchQueue.main.async {
            UIPasteboard.general.string = text
        }
    }

    @objc
    public func shareFromRust(_ text: String) {
        DispatchQueue.main.async {
            guard let vc = Self.topViewController() else { return }
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
    }

    private static func topViewController() -> UIViewController? {
        let windowScene = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .first { $0.activationState == .foregroundActive }
        let root = windowScene?.windows.first { $0.isKeyWindow }?.rootViewController
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
