import Foundation
import StoreKit

/**
 * StoreKit 2, as requests you start and results you collect.
 *
 * StoreKit 2 is `async` Swift, and an `async` function cannot be `@objc`, so
 * none of it can be called across the FFI boundary directly. Every entry point
 * here starts a `Task` and leaves the answer in a slot the Rust side polls.
 * That is not a workaround for the boundary so much as a match for the thing
 * itself: a purchase involves a sheet, a payment, and sometimes a parent's
 * approval days later, and no call can wait for that.
 *
 * Ownership arrives from three places — a purchase completing, a restore, and
 * the `Transaction.updates` listener that catches everything finished outside
 * the app — and all three publish through the same slot, so the Rust side has
 * one place to read from.
 *
 * Nothing here is trusted. `purchaseToken` carries the signed JWS for the app's
 * own server to check against Apple; the rest of an entitlement is display
 * state read off the device.
 */
@objc(InAppPurchasesPlugin)
public class InAppPurchasesPlugin: NSObject {

    private static let stateIdle = "idle"
    private static let statePurchasing = "purchasing"

    private let lock = NSLock()
    private var products: String?
    private var entitlements: String?
    private var purchasing = false

    private var updatesTask: Task<Void, Never>?
    /// Products already fetched, so a purchase does not have to round-trip to
    /// the store again for something the app has just shown the user.
    private var loaded: [String: Product] = [:]

    // MARK: - Connection

    @objc
    public func prepareFromRust() {
        lock.lock()
        let alreadyListening = updatesTask != nil
        lock.unlock()
        if alreadyListening { return }

        let task = Task.detached { [weak self] in
            // Renewals, interrupted payments, family sharing, and purchases
            // approved after the fact all arrive here and nowhere else. The
            // listener has to be running before they show up, which is why this
            // belongs at startup rather than at the point of sale.
            for await update in Transaction.updates {
                guard let self = self else { return }
                if case .verified = update {
                    await self.publishEntitlements()
                }
            }
        }
        lock.lock()
        updatesTask = task
        lock.unlock()

        Task { await publishEntitlements() }
    }

    // MARK: - Products

    @objc
    public func startProductsRequestFromRust(_ idsJson: String) {
        Task {
            guard let data = idsJson.data(using: .utf8),
                  let ids = (try? JSONSerialization.jsonObject(with: data)) as? [String] else {
                publish(StoreCatalogue.error("Could not read the product ids."), isProducts: true)
                return
            }
            if ids.isEmpty {
                publish("[]", isProducts: true)
                return
            }
            do {
                let fetched = try await Product.products(for: ids)
                lock.lock()
                for product in fetched { loaded[product.id] = product }
                lock.unlock()
                let encoded = fetched.map { StoreCatalogue.product($0) }
                publish(
                    StoreCatalogue.json(encoded) ?? "[]",
                    isProducts: true
                )
            } catch {
                publish(
                    StoreCatalogue.error("Could not load products: \(error.localizedDescription)"),
                    isProducts: true
                )
            }
        }
    }

    // MARK: - Purchase

    @objc
    public func startPurchaseFromRust(_ payloadJson: String) {
        guard let data = payloadJson.data(using: .utf8),
              let payload = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let productId = payload["productId"] as? String,
              let offerId = payload["offerId"] as? String else {
            publishPurchaseError("The purchase request could not be decoded.")
            return
        }
        // offerId is accepted for a call site shared with Android and ignored:
        // StoreKit applies an introductory offer itself when the user qualifies,
        // and a promotional offer needs a server signature this cannot make.
        Task {
            guard let product = await product(for: productId) else {
                publishPurchaseError("No product called \(productId) is available to buy.")
                return
            }
            setPurchasing(true)
            do {
                let result = try await product.purchase()
                switch result {
                case .success(let verification):
                    guard case .verified = verification else {
                        setPurchasing(false)
                        publishPurchaseError("The purchase could not be verified.")
                        return
                    }
                    await publishEntitlements()
                case .userCancelled, .pending:
                    // Backing out is a choice, and a pending purchase is one
                    // waiting on someone else. Neither is a failure; publish
                    // the settled picture so the app is not left polling.
                    await publishEntitlements()
                @unknown default:
                    await publishEntitlements()
                }
                setPurchasing(false)
            } catch {
                setPurchasing(false)
                publishPurchaseError("The purchase failed: \(error.localizedDescription)")
            }
        }
    }

    private func product(for id: String) async -> Product? {
        lock.lock()
        let cached = loaded[id]
        lock.unlock()
        if let cached = cached { return cached }
        guard let fetched = try? await Product.products(for: [id]).first else { return nil }
        lock.lock()
        loaded[id] = fetched
        lock.unlock()
        return fetched
    }

    // MARK: - Entitlements

    @objc
    public func startRestoreFromRust() {
        Task {
            // Asking the App Store to re-sync is what can prompt for a password,
            // which is why the Rust side documents putting this behind a button.
            try? await AppStore.sync()
            await publishEntitlements()
        }
    }

    private func publishEntitlements() async {
        var unfinished = Set<UInt64>()
        for await result in Transaction.unfinished {
            if case .verified(let transaction) = result {
                unfinished.insert(transaction.id)
            }
        }

        var encoded: [[String: Any]] = []
        for await result in Transaction.currentEntitlements {
            guard case .verified(let transaction) = result else { continue }
            var willAutoRenew = false
            if transaction.productType == .autoRenewable {
                willAutoRenew = await autoRenews(transaction.productID)
            }
            encoded.append(
                StoreCatalogue.entitlement(
                    transaction,
                    jws: result.jwsRepresentation,
                    needsFinishing: unfinished.contains(transaction.id),
                    willAutoRenew: willAutoRenew
                )
            )
        }
        publish(StoreCatalogue.json(encoded) ?? "[]", isProducts: false)
    }

    /// Whether a subscription is set to renew, which a transaction alone does
    /// not say — a cancelled subscription keeps a valid transaction until it
    /// actually lapses, so this is the only way to tell the two apart.
    private func autoRenews(_ productID: String) async -> Bool {
        guard let product = await product(for: productID),
              let statuses = try? await product.subscription?.status else { return false }
        for status in statuses ?? [] {
            if case .verified(let renewal) = status.renewalInfo {
                return renewal.willAutoRenew
            }
        }
        return false
    }

    @objc
    public func finishFromRust(_ payloadJson: String) {
        guard let data = payloadJson.data(using: .utf8),
              let payload = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let transactionId = payload["transactionId"] as? String,
              let consume = payload["consume"] as? Bool else {
            publishPurchaseError("The finish request could not be decoded.")
            return
        }
        // consume is accepted for a call site shared with Android and ignored:
        // finishing a consumable is the same call as finishing anything else.
        Task {
            for await result in Transaction.unfinished {
                guard case .verified(let transaction) = result,
                      String(transaction.id) == transactionId else { continue }
                await transaction.finish()
                break
            }
            await publishEntitlements()
        }
    }

    // MARK: - Polling

    @objc
    public func takeProductsFromRust() -> String? {
        lock.lock()
        defer { lock.unlock() }
        let taken = products
        products = nil
        return taken
    }

    @objc
    public func takeEntitlementsFromRust() -> String? {
        lock.lock()
        defer { lock.unlock() }
        let taken = entitlements
        entitlements = nil
        return taken
    }

    @objc
    public func getPurchaseStateFromRust() -> String {
        lock.lock()
        defer { lock.unlock() }
        return purchasing ? Self.statePurchasing : Self.stateIdle
    }

    // MARK: - Internals

    private func publish(_ json: String, isProducts: Bool) {
        lock.lock()
        defer { lock.unlock() }
        if isProducts {
            products = json
        } else {
            entitlements = json
        }
    }

    private func publishPurchaseError(_ message: String) {
        setPurchasing(false)
        publish(StoreCatalogue.error(message), isProducts: false)
    }

    private func setPurchasing(_ value: Bool) {
        lock.lock()
        purchasing = value
        lock.unlock()
    }
}
