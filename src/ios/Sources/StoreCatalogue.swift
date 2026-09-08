import Foundation
import StoreKit

/**
 * Turns StoreKit's product and transaction shapes into the JSON the Rust side
 * reads.
 *
 * Kept apart from the purchase flow because it is pure translation, and because
 * the mapping is where this platform's differences from Play have to be decided
 * rather than papered over.
 */
enum StoreCatalogue {

    static func json(_ value: Any) -> String? {
        guard let data = try? JSONSerialization.data(withJSONObject: value) else { return nil }
        return String(data: data, encoding: .utf8)
    }

    static func error(_ message: String) -> String {
        json(["error": message]) ?? "{\"error\":\"The store request failed.\"}"
    }

    static func period(_ period: Product.SubscriptionPeriod) -> [String: Any] {
        let unit: String
        switch period.unit {
        case .day: unit = "day"
        case .week: unit = "week"
        case .month: unit = "month"
        case .year: unit = "year"
        @unknown default: unit = "month"
        }
        return ["unit": unit, "count": period.value]
    }

    /// Prices cross as millionths of a currency unit, matching what Play
    /// reports, so the two platforms compare on the same scale.
    static func micros(_ price: Decimal) -> Int64 {
        NSDecimalNumber(decimal: price * 1_000_000).int64Value
    }

    static func kind(_ type: Product.ProductType) -> String {
        switch type {
        case .consumable: return "consumable"
        case .nonConsumable: return "non-consumable"
        // A non-renewing subscription is still a subscription to the app, even
        // though StoreKit tracks it as a one-time purchase underneath.
        case .autoRenewable, .nonRenewable: return "subscription"
        default: return "non-consumable"
        }
    }

    static func offer(_ offer: Product.SubscriptionOffer, id: String) -> [String: Any] {
        [
            "id": id,
            "displayPrice": offer.displayPrice,
            "priceMicros": micros(offer.price),
            "period": period(offer.period),
            "isFreeTrial": offer.paymentMode == .freeTrial,
        ]
    }

    static func product(_ product: Product) -> [String: Any] {
        var encoded: [String: Any] = [
            "id": product.id,
            "kind": kind(product.type),
            "title": product.displayName,
            "description": product.description,
            "displayPrice": product.displayPrice,
            "priceMicros": micros(product.price),
            "currencyCode": product.priceFormatStyle.currencyCode,
            "offers": [],
        ]
        guard let subscription = product.subscription else {
            encoded["subscriptionPeriod"] = NSNull()
            return encoded
        }
        encoded["subscriptionPeriod"] = period(subscription.subscriptionPeriod)
        // Only the introductory offer is listed. A promotional offer cannot be
        // redeemed without a signature from the developer's own server, which
        // this plugin has no way to produce, so advertising one here would
        // offer the user a deal the app could not then honour.
        if let introductory = subscription.introductoryOffer {
            encoded["offers"] = [offer(introductory, id: "introductory")]
        }
        return encoded
    }

    static func entitlement(
        _ transaction: Transaction,
        jws: String,
        needsFinishing: Bool,
        willAutoRenew: Bool
    ) -> [String: Any] {
        let expiry = transaction.expirationDate
        let active = transaction.revocationDate == nil
            && (expiry == nil || expiry! > Date())
        return [
            "productId": transaction.productID,
            "transactionId": String(transaction.id),
            "purchaseToken": jws,
            "purchasedAtMs": Int(transaction.purchaseDate.timeIntervalSince1970 * 1000),
            "expiresAtMs": expiry.map { Int($0.timeIntervalSince1970 * 1000) } ?? NSNull(),
            "isActive": active,
            "willAutoRenew": willAutoRenew,
            "needsFinishing": needsFinishing,
        ]
    }
}
