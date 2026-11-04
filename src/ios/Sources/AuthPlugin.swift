import Foundation
import AuthenticationServices
import CryptoKit
import UIKit

@objc(AuthPlugin)
public class AuthPlugin: NSObject {
    // Main-thread only: UIKit presents and owns them.
    private var coordinator: AppleSignInCoordinator?
    private var controller: ASAuthorizationController?

    // Read and written from the Rust caller's thread and the main thread, so
    // only under `lock`, as the in-app purchases plugin does.
    private let lock = NSLock()
    private var pendingResult: String?
    private var isAwaiting = false

    @objc
    public func startAppleAuthFromRust(_ requestJson: String) -> String? {
        guard let data = requestJson.data(using: .utf8),
              let request = try? JSONDecoder().decode(AppleAuthRequest.self, from: data),
              !request.state.isEmpty,
              !request.nonce.isEmpty else {
            setPendingResult(nil)
            return nil
        }

        // Awaiting from this call on, not from when the main queue gets to
        // it: Rust polls right after this returns, and an "idle" answer then
        // reads as a cancelled sign-in while the sheet is still opening. The
        // Android plugin sets its flag the same way.
        lock.lock()
        pendingResult = nil
        isAwaiting = true
        lock.unlock()
        DispatchQueue.main.async { [weak self] in
            self?.startAppleAuthOnMainThread(request)
        }
        return nil
    }

    @objc
    public func getPendingResult() -> String? {
        lock.lock()
        defer { lock.unlock() }
        let result = pendingResult
        pendingResult = nil
        return result
    }

    @objc
    public func getAuthState() -> String? {
        lock.lock()
        defer { lock.unlock() }
        return isAwaiting ? "awaiting" : "idle"
    }

    func setPendingResult(_ result: String?) {
        lock.lock()
        pendingResult = result
        isAwaiting = false
        lock.unlock()
        DispatchQueue.main.async {
            self.controller = nil
            self.coordinator = nil
        }
    }

    private func startAppleAuthOnMainThread(_ authRequest: AppleAuthRequest) {
        let provider = ASAuthorizationAppleIDProvider()
        let request = provider.createRequest()
        request.requestedScopes = [.fullName, .email]
        request.state = authRequest.state
        request.nonce = Self.sha256(authRequest.nonce)

        let controller = ASAuthorizationController(authorizationRequests: [request])
        let coordinator = AppleSignInCoordinator(plugin: self)
        self.controller = controller
        self.coordinator = coordinator
        controller.delegate = coordinator
        controller.presentationContextProvider = coordinator
        controller.performRequests()
    }

    private static func sha256(_ value: String) -> String {
        SHA256.hash(data: Data(value.utf8)).map { String(format: "%02x", $0) }.joined()
    }
}

private struct AppleAuthRequest: Decodable {
    let state: String
    let nonce: String
}

private final class AppleSignInCoordinator: NSObject, ASAuthorizationControllerDelegate,
    ASAuthorizationControllerPresentationContextProviding
{
    private let plugin: AuthPlugin

    init(plugin: AuthPlugin) {
        self.plugin = plugin
    }

    func presentationAnchor(for controller: ASAuthorizationController) -> ASPresentationAnchor {
        for scene in UIApplication.shared.connectedScenes {
            guard let windowScene = scene as? UIWindowScene,
                  windowScene.activationState == .foregroundActive else {
                continue
            }
            if let window = windowScene.windows.first(where: \.isKeyWindow) {
                return window
            }
            if let window = windowScene.windows.first {
                return window
            }
        }

        for scene in UIApplication.shared.connectedScenes {
            if let window = (scene as? UIWindowScene)?.windows.first {
                return window
            }
        }

        return ASPresentationAnchor()
    }

    func authorizationController(
        controller: ASAuthorizationController,
        didCompleteWithAuthorization authorization: ASAuthorization
    ) {
        guard let credential = authorization.credential as? ASAuthorizationAppleIDCredential,
              let tokenData = credential.identityToken,
              let identityToken = String(data: tokenData, encoding: .utf8),
              let codeData = credential.authorizationCode,
              let authorizationCode = String(data: codeData, encoding: .utf8),
              let state = credential.state,
              !state.isEmpty else {
            plugin.setPendingResult(nil)
            return
        }

        var payload: [String: String] = [
            "identity_token": identityToken,
            "authorization_code": authorizationCode,
            "state": state,
        ]

        if let email = credential.email, !email.isEmpty {
            payload["email"] = email
        }

        let name = [credential.fullName?.givenName, credential.fullName?.familyName]
            .compactMap { $0 }
            .joined(separator: " ")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        if !name.isEmpty {
            payload["display_name"] = name
        }

        guard let data = try? JSONSerialization.data(withJSONObject: payload),
              let json = String(data: data, encoding: .utf8) else {
            plugin.setPendingResult(nil)
            return
        }
        plugin.setPendingResult(json)
    }

    func authorizationController(
        controller: ASAuthorizationController,
        didCompleteWithError error: Error
    ) {
        plugin.setPendingResult(nil)
    }
}
