import Foundation
import UIKit

@objc(ExternalUrlPlugin)
public class ExternalUrlPlugin: NSObject {
    @objc
    public func openExternalUrlFromRust(_ url: String) {
        guard let parsed = URL(string: url) else { return }
        DispatchQueue.main.async {
            UIApplication.shared.open(parsed)
        }
    }
}
