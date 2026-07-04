import Foundation
import UIKit

@objc(ExternalUrlPlugin)
public class ExternalUrlPlugin: NSObject {
    @objc
    public func openExternalUrlFromRust(_ url: String) {
        NSLog("[ExternalUrlPlugin] openExternalUrlFromRust \(url)")
        guard let parsed = URL(string: url) else {
            NSLog("[ExternalUrlPlugin] invalid URL")
            return
        }
        DispatchQueue.main.async {
            UIApplication.shared.open(parsed)
        }
    }
}
