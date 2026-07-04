import Foundation
import AuthenticationServices
import UIKit

@objc(AuthPlugin)
public class AuthPlugin: NSObject {

    private var coordinator: AppleSignInCoordinator?
    private var controller: ASAuthorizationController?
    private var _pendingResult: String?
    private var _isAwaiting: Bool = false

    @objc
    public func startAppleAuthFromRust() -> String? {
        NSLog("[AuthPlugin] ========== START startAppleAuthFromRust ==========")
        NSLog("[AuthPlugin] Thread: \(Thread.isMainThread ? "main" : "background")")
        DispatchQueue.main.async {
            self.startAppleAuthOnMainThread()
        }
        NSLog("[AuthPlugin] Returning nil (fire-and-forget)")
        return nil
    }

    @objc
    public func getPendingResult() -> String? {
        NSLog("[AuthPlugin] getPendingResult called, value: \(_pendingResult ?? "nil")")
        return _pendingResult
    }

    @objc
    public func getAuthState() -> String? {
        let state = _isAwaiting ? "awaiting" : "idle"
        NSLog("[AuthPlugin] getAuthState called, returning: \(state)")
        return state
    }

    // Called by AppleSignInCoordinator when auth completes
    func setPendingResult(_ result: String?) {
        DispatchQueue.main.async {
            NSLog("[AuthPlugin] setPendingResult called with: \(result ?? "nil")")
            self._pendingResult = result
            NSLog("[AuthPlugin] _isAwaiting set to false")
            self._isAwaiting = false
            self.controller = nil
            self.coordinator = nil
        }
    }

    private func startAppleAuthOnMainThread() {
        NSLog("[AuthPlugin] startAppleAuthOnMainThread")
        _pendingResult = nil
        _isAwaiting = true

        let provider = ASAuthorizationAppleIDProvider()
        let request = provider.createRequest()
        request.requestedScopes = [.fullName, .email]

        let controller = ASAuthorizationController(authorizationRequests: [request])
        let coordinator = AppleSignInCoordinator(plugin: self)

        self.controller = controller
        self.coordinator = coordinator

        controller.delegate = coordinator
        controller.presentationContextProvider = coordinator
        controller.performRequests()
    }
}

private final class AppleSignInCoordinator: NSObject, ASAuthorizationControllerDelegate, ASAuthorizationControllerPresentationContextProviding {
    private let plugin: AuthPlugin

    init(plugin: AuthPlugin) {
        self.plugin = plugin
        NSLog("[AuthPlugin.Coordinator] Coordinator initialized")
    }

    func presentationAnchor(for controller: ASAuthorizationController) -> ASPresentationAnchor {
        NSLog("[AuthPlugin.Coordinator] ========== presentationAnchor called ==========")
        NSLog("[AuthPlugin.Coordinator] connectedScenes count: \(UIApplication.shared.connectedScenes.count)")

        // Get the key window from the active window scene
        for scene in UIApplication.shared.connectedScenes {
            NSLog("[AuthPlugin.Coordinator] Checking scene: activationState=\(scene.activationState)")
            if let windowScene = scene as? UIWindowScene {
                NSLog("[AuthPlugin.Coordinator] UIWindowScene found, activationState=\(windowScene.activationState.rawValue)")
                if windowScene.activationState == .foregroundActive {
                    NSLog("[AuthPlugin.Coordinator] Scene is foregroundActive, checking windows")
                    NSLog("[AuthPlugin.Coordinator] WindowScene windows count: \(windowScene.windows.count)")
                    for window in windowScene.windows {
                        NSLog("[AuthPlugin.Coordinator] Checking window, isKeyWindow=\(window.isKeyWindow)")
                        if window.isKeyWindow {
                            NSLog("[AuthPlugin.Coordinator] Found key window, returning as anchor")
                            return window
                        }
                    }
                    if let w = windowScene.windows.first {
                        NSLog("[AuthPlugin.Coordinator] No key window found, returning first window")
                        return w
                    }
                } else {
                    NSLog("[AuthPlugin.Coordinator] Scene is NOT foregroundActive, skipping")
                }
            } else {
                NSLog("[AuthPlugin.Coordinator] Scene is not a UIWindowScene")
            }
        }

        // Fallback: try any connected scene
        NSLog("[AuthPlugin.Coordinator] No active scene found, trying fallback")
        for scene in UIApplication.shared.connectedScenes {
            if let windowScene = scene as? UIWindowScene {
                for window in windowScene.windows {
                    NSLog("[AuthPlugin.Coordinator] Fallback: found window from non-active scene")
                    return window
                }
            }
        }

        NSLog("[AuthPlugin.Coordinator] WARNING: No valid presentation anchor found, returning empty ASPresentationAnchor")
        return ASPresentationAnchor()
    }

    func authorizationController(controller: ASAuthorizationController, didCompleteWithAuthorization authorization: ASAuthorization) {
        NSLog("[AuthPlugin.Coordinator] ========== didCompleteWithAuthorization called ==========")
        
        guard let credential = authorization.credential as? ASAuthorizationAppleIDCredential else {
            NSLog("[AuthPlugin.Coordinator] ERROR: authorization.credential is not ASAuthorizationAppleIDCredential")
            NSLog("[AuthPlugin.Coordinator] credential type: \(type(of: authorization.credential))")
            plugin.setPendingResult(nil)
            return
        }
        NSLog("[AuthPlugin.Coordinator] Got ASAuthorizationAppleIDCredential")

        guard let tokenData = credential.identityToken,
              let identityToken = String(data: tokenData, encoding: .utf8) else {
            NSLog("[AuthPlugin.Coordinator] ERROR: credential has no identityToken or encoding failed")
            NSLog("[AuthPlugin.Coordinator] identityToken present: \(credential.identityToken != nil)")
            plugin.setPendingResult(nil)
            return
        }
        NSLog("[AuthPlugin.Coordinator] identityToken received")

        var payload: [String: String] = ["identity_token": identityToken]
        NSLog("[AuthPlugin.Coordinator] Base payload created with identity_token")

        if let email = credential.email, !email.isEmpty {
            payload["email"] = email
            NSLog("[AuthPlugin.Coordinator] Email found: \(email)")
        } else {
            NSLog("[AuthPlugin.Coordinator] No email in credential")
        }

        let name = [credential.fullName?.givenName, credential.fullName?.familyName]
            .compactMap { $0 }
            .joined(separator: " ")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        if !name.isEmpty {
            payload["display_name"] = name
            NSLog("[AuthPlugin.Coordinator] Display name: \(name)")
        } else {
            NSLog("[AuthPlugin.Coordinator] No display name in credential")
        }

        do {
            NSLog("[AuthPlugin.Coordinator] Encoding payload to JSON")
            let data = try JSONSerialization.data(withJSONObject: payload)
            let json = String(data: data, encoding: .utf8)
            NSLog("[AuthPlugin.Coordinator] JSON payload encoded")
            plugin.setPendingResult(json)
        } catch {
            NSLog("[AuthPlugin.Coordinator] ERROR: JSON serialization failed: \(error)")
            plugin.setPendingResult(nil)
        }

        NSLog("[AuthPlugin.Coordinator] ========== didCompleteWithAuthorization complete ==========")
    }

    func authorizationController(controller: ASAuthorizationController, didCompleteWithError error: Error) {
        NSLog("[AuthPlugin.Coordinator] ========== didCompleteWithError called ==========")
        NSLog("[AuthPlugin.Coordinator] Error domain: \((error as NSError).domain)")
        NSLog("[AuthPlugin.Coordinator] Error code: \((error as NSError).code)")
        NSLog("[AuthPlugin.Coordinator] Error description: \(error.localizedDescription)")
        NSLog("[AuthPlugin.Coordinator] Error user info: \((error as NSError).userInfo)")
        plugin.setPendingResult(nil)
        NSLog("[AuthPlugin.Coordinator] ========== didCompleteWithError complete ==========")
    }
}
