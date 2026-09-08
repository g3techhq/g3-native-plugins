package dev.dioxus.g3_native_plugins.in_app_purchases

import com.android.billingclient.api.ProductDetails
import org.json.JSONArray
import org.json.JSONObject

/**
 * Turns Play's product and purchase shapes into the JSON the Rust side reads.
 *
 * Kept apart from the billing plumbing because it is pure translation, and
 * because the mapping is where this platform's differences from StoreKit have
 * to be decided rather than papered over.
 */
internal object Catalogue {

    /**
     * Play does not label a one-time product as consumable or not. That is
     * decided by whether the app consumes the purchase, so everything one-time
     * is reported as non-consumable and the caller says which it meant when it
     * finishes the transaction.
     */
    private const val KIND_NON_CONSUMABLE = "non-consumable"
    private const val KIND_SUBSCRIPTION = "subscription"

    /** Parse an ISO 8601 billing period, the `P1M` and `P7D` Play reports. */
    fun period(iso: String?): JSONObject {
        val fallback = JSONObject().put("unit", "month").put("count", 1)
        if (iso.isNullOrEmpty() || iso[0] != 'P') return fallback
        val count = iso.drop(1).takeWhile { it.isDigit() }
        val unit = when (iso.lastOrNull()) {
            'D' -> "day"
            'W' -> "week"
            'M' -> "month"
            'Y' -> "year"
            else -> return fallback
        }
        return JSONObject()
            .put("unit", unit)
            .put("count", count.toIntOrNull() ?: 1)
    }

    /**
     * An offer's id, as the Rust side will hand it back to buy that deal.
     *
     * A base plan on its own is identified by its own id; a discount or trial
     * attached to one needs both parts to be unambiguous.
     */
    fun offerId(offer: ProductDetails.SubscriptionOfferDetails): String =
        if (offer.offerId.isNullOrEmpty()) offer.basePlanId else "${offer.basePlanId}:${offer.offerId}"

    fun product(details: ProductDetails): JSONObject {
        val json = JSONObject()
            .put("id", details.productId)
            .put("title", details.title)
            .put("description", details.description)
            .put("offers", JSONArray())

        details.oneTimePurchaseOfferDetails?.let { one ->
            return json
                .put("kind", KIND_NON_CONSUMABLE)
                .put("displayPrice", one.formattedPrice)
                .put("priceMicros", one.priceAmountMicros)
                .put("currencyCode", one.priceCurrencyCode)
                .put("subscriptionPeriod", JSONObject.NULL)
        }

        val offers = details.subscriptionOfferDetails.orEmpty()
        json.put("kind", KIND_SUBSCRIPTION)

        // The standing price is the last phase of the plainest offer: an
        // introductory phase comes first and runs out, and what is left is what
        // the user actually pays from then on.
        val standing = offers
            .firstOrNull { it.offerId.isNullOrEmpty() }
            ?.pricingPhases?.pricingPhaseList?.lastOrNull()
            ?: offers.firstOrNull()?.pricingPhases?.pricingPhaseList?.lastOrNull()
        json
            .put("displayPrice", standing?.formattedPrice.orEmpty())
            .put("priceMicros", standing?.priceAmountMicros ?: 0L)
            .put("currencyCode", standing?.priceCurrencyCode.orEmpty())
            .put("subscriptionPeriod", period(standing?.billingPeriod))

        val encoded = JSONArray()
        for (offer in offers) {
            // The first phase is the deal being advertised; later phases are
            // what it settles into.
            val phase = offer.pricingPhases.pricingPhaseList.firstOrNull() ?: continue
            encoded.put(
                JSONObject()
                    .put("id", offerId(offer))
                    .put("displayPrice", phase.formattedPrice)
                    .put("priceMicros", phase.priceAmountMicros)
                    .put("period", period(phase.billingPeriod))
                    .put("isFreeTrial", phase.priceAmountMicros == 0L),
            )
        }
        return json.put("offers", encoded)
    }

    fun error(message: String): String = JSONObject().put("error", message).toString()
}
