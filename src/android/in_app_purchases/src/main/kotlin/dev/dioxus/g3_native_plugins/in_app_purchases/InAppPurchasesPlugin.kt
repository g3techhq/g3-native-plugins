package dev.dioxus.g3_native_plugins.in_app_purchases

import android.app.Activity
import android.net.Uri
import com.android.billingclient.api.AcknowledgePurchaseParams
import com.android.billingclient.api.BillingClient
import com.android.billingclient.api.BillingClientStateListener
import com.android.billingclient.api.BillingFlowParams
import com.android.billingclient.api.BillingProgramReportingDetailsParams
import com.android.billingclient.api.BillingResult
import com.android.billingclient.api.ConsumeParams
import com.android.billingclient.api.LaunchExternalLinkParams
import com.android.billingclient.api.PendingPurchasesParams
import com.android.billingclient.api.ProductDetails
import com.android.billingclient.api.Purchase
import com.android.billingclient.api.QueryProductDetailsParams
import com.android.billingclient.api.QueryPurchasesParams
import org.json.JSONArray
import org.json.JSONObject

/**
 * Google Play billing, as requests you start and results you collect.
 *
 * Play answers on listeners rather than returning anything, so nothing here
 * blocks: the Rust side starts work and polls. Ownership arrives from three
 * places — a purchase completing, a restore, and the updates listener that
 * catches purchases finished outside the app — and all three publish through
 * the same slot, so the Rust side has one place to read from.
 *
 * Two things differ from StoreKit and are not worth hiding:
 *
 * Play does not say whether a one-time product is consumable. That is decided
 * by whether the app consumes the purchase, so everything one-time reports as
 * non-consumable and the caller says which it meant when finishing.
 *
 * Play does not report a subscription's expiry to the device at all. It comes
 * from the Play Developer API, server side, which is where a subscription's
 * validity should be decided anyway. `expiresAtMs` is therefore always absent
 * here, while iOS fills it in.
 */
class InAppPurchasesPlugin(private val activity: Activity) {
    companion object {
        private const val STATE_IDLE = "idle"
        private const val STATE_PURCHASING = "purchasing"
    }

    private val lock = Object()
    private var products: String? = null
    private var entitlements: String? = null
    private var externalContentLinkToken: String? = null
    private var externalContentLinkLaunchResult: String? = null
    private var purchasing = false
    private var connected = false

    /** Kept so a purchase can be launched against the details Play gave us. */
    private val known = mutableMapOf<String, ProductDetails>()

    private val client: BillingClient = BillingClient.newBuilder(activity)
        .setListener { result, purchases -> onPurchasesUpdated(result, purchases) }
        .enableBillingProgram(BillingClient.BillingProgram.EXTERNAL_CONTENT_LINK)
        .enablePendingPurchases(
            // A pending purchase is one the user still has to complete, by cash
            // at a counter in some markets. Play requires saying up front that
            // the app can cope with seeing one.
            PendingPurchasesParams.newBuilder().enableOneTimeProducts().build(),
        )
        .build()

    // ---- Connection ----

    fun prepareFromRust() {
        connect { }
    }

    private fun connect(onReady: () -> Unit) {
        synchronized(lock) {
            if (connected && client.isReady) {
                onReady()
                return
            }
        }
        client.startConnection(object : BillingClientStateListener {
            override fun onBillingSetupFinished(result: BillingResult) {
                if (result.responseCode == BillingClient.BillingResponseCode.OK) {
                    synchronized(lock) { connected = true }
                    // Anything bought while the app was gone is waiting here.
                    refreshEntitlements()
                    onReady()
                } else {
                    publishEntitlementsError("Could not connect to Play: ${result.debugMessage}")
                }
            }

            override fun onBillingServiceDisconnected() {
                // Play reconnects on the next call rather than retrying in a
                // loop here, which would fight the backoff it already does.
                synchronized(lock) { connected = false }
            }
        })
    }

    // ---- Products ----

    fun startProductsRequestFromRust(idsJson: String) {
        val ids = mutableListOf<String>()
        try {
            val array = JSONArray(idsJson)
            for (index in 0 until array.length()) ids.add(array.getString(index))
        } catch (error: Exception) {
            publish(Catalogue.error("Could not read the product ids: ${error.message}"), isProducts = true)
            return
        }
        if (ids.isEmpty()) {
            publish(JSONArray().toString(), isProducts = true)
            return
        }
        connect {
            // Which ids are subscriptions and which are one-time is not known
            // here, and Play will not say. Ask for both and keep what comes
            // back; an id of the wrong type is simply absent from that answer.
            val collected = JSONArray()
            var remaining = 2
            val onDone = { ->
                synchronized(lock) {
                    remaining -= 1
                    if (remaining == 0) publish(collected.toString(), isProducts = true)
                }
            }
            for (type in listOf(BillingClient.ProductType.INAPP, BillingClient.ProductType.SUBS)) {
                queryProducts(ids, type, collected, onDone)
            }
        }
    }

    private fun queryProducts(
        ids: List<String>,
        type: String,
        into: JSONArray,
        onDone: () -> Unit,
    ) {
        val params = QueryProductDetailsParams.newBuilder()
            .setProductList(
                ids.map {
                    QueryProductDetailsParams.Product.newBuilder()
                        .setProductId(it)
                        .setProductType(type)
                        .build()
                },
            )
            .build()
        client.queryProductDetailsAsync(params) { _, result ->
            synchronized(lock) {
                for (item in result.productDetailsList) {
                    known[item.productId] = item
                    into.put(Catalogue.product(item))
                }
            }
            onDone()
        }
    }

    // ---- Purchase ----

    fun startPurchaseFromRust(productId: String, offerId: String) {
        connect {
            val details = synchronized(lock) { known[productId] }
            if (details == null) {
                publishEntitlementsError(
                    "Ask for $productId with a products request before buying it.",
                )
                return@connect
            }
            val builder = BillingFlowParams.ProductDetailsParams.newBuilder()
                .setProductDetails(details)
            val offers = details.subscriptionOfferDetails.orEmpty()
            if (offers.isNotEmpty()) {
                // A subscription cannot be bought without naming an offer. Use
                // the one asked for, else the plainest base plan.
                val chosen = offers.firstOrNull { Catalogue.offerId(it) == offerId }
                    ?: offers.firstOrNull { it.offerId.isNullOrEmpty() }
                    ?: offers.first()
                builder.setOfferToken(chosen.offerToken)
            }
            val params = BillingFlowParams.newBuilder()
                .setProductDetailsParamsList(listOf(builder.build()))
                .build()
            synchronized(lock) { purchasing = true }
            // Play requires the real Activity; this has to be the main thread.
            activity.runOnUiThread {
                val result = client.launchBillingFlow(activity, params)
                if (result.responseCode != BillingClient.BillingResponseCode.OK) {
                    publishEntitlementsError("Could not open checkout: ${result.debugMessage}")
                }
            }
        }
    }

    private fun onPurchasesUpdated(result: BillingResult, purchases: List<Purchase>?) {
        synchronized(lock) { purchasing = false }
        when (result.responseCode) {
            BillingClient.BillingResponseCode.OK,
            // Already owned is not a failure worth surfacing: the refresh
            // reports the thing the user owns, which is what was wanted.
            BillingClient.BillingResponseCode.ITEM_ALREADY_OWNED,
            // Backing out is a choice, not an error. Refreshing anyway leaves
            // the app with a settled answer rather than a poll that never ends.
            BillingClient.BillingResponseCode.USER_CANCELED,
            -> refreshEntitlements()
            else -> publishEntitlementsError("Purchase failed: ${result.debugMessage}")
        }
    }

    // ---- Entitlements ----

    fun startRestoreFromRust() {
        connect { refreshEntitlements() }
    }

    private fun refreshEntitlements() {
        val collected = JSONArray()
        var remaining = 2
        for (type in listOf(BillingClient.ProductType.INAPP, BillingClient.ProductType.SUBS)) {
            val params = QueryPurchasesParams.newBuilder().setProductType(type).build()
            client.queryPurchasesAsync(params) { _, purchases ->
                synchronized(lock) {
                    for (purchase in purchases) collected.put(entitlement(purchase))
                    remaining -= 1
                    if (remaining == 0) publish(collected.toString(), isProducts = false)
                }
            }
        }
    }

    private fun entitlement(purchase: Purchase): JSONObject {
        val owned = purchase.purchaseState == Purchase.PurchaseState.PURCHASED
        val json = JSONObject()
            .put("productId", purchase.products.firstOrNull().orEmpty())
            // Play acknowledges against the token, so it doubles as the id the
            // Rust side hands back to finish the transaction.
            .put("transactionId", purchase.purchaseToken)
            .put("purchaseToken", purchase.purchaseToken)
            .put("purchasedAtMs", purchase.purchaseTime)
            // Play tells the device nothing about when a subscription lapses.
            // That answer lives in the Play Developer API, server side.
            .put("expiresAtMs", JSONObject.NULL)
            .put("isActive", owned)
            .put("willAutoRenew", purchase.isAutoRenewing)
            .put("needsFinishing", owned && !purchase.isAcknowledged)
        return json
    }

    fun finishFromRust(transactionId: String, consume: Boolean) {
        connect {
            if (consume) {
                val params = ConsumeParams.newBuilder()
                    .setPurchaseToken(transactionId)
                    .build()
                client.consumeAsync(params) { _, _ -> refreshEntitlements() }
            } else {
                val params = AcknowledgePurchaseParams.newBuilder()
                    .setPurchaseToken(transactionId)
                    .build()
                client.acknowledgePurchase(params) { _ -> refreshEntitlements() }
            }
        }
    }

    // ---- External content links ----

    /**
     * Checks eligibility and creates the single-use reporting token Google
     * requires immediately before each external content link visit.
     */
    fun startExternalContentLinkTokenFromRust() {
        synchronized(lock) { externalContentLinkToken = null }
        connect {
            client.isBillingProgramAvailableAsync(
                BillingClient.BillingProgram.EXTERNAL_CONTENT_LINK,
            ) { availability, _ ->
                if (availability.responseCode != BillingClient.BillingResponseCode.OK) {
                    publishExternalContentLinkTokenError(
                        "External checkout is unavailable: ${availability.debugMessage}",
                    )
                    return@isBillingProgramAvailableAsync
                }

                val params = BillingProgramReportingDetailsParams.newBuilder()
                    .setBillingProgram(BillingClient.BillingProgram.EXTERNAL_CONTENT_LINK)
                    .build()
                client.createBillingProgramReportingDetailsAsync(params) { result, details ->
                    val token = details?.externalTransactionToken
                    if (result.responseCode != BillingClient.BillingResponseCode.OK || token.isNullOrBlank()) {
                        publishExternalContentLinkTokenError(
                            "Could not prepare external checkout: ${result.debugMessage}",
                        )
                        return@createBillingProgramReportingDetailsAsync
                    }
                    synchronized(lock) {
                        externalContentLinkToken = JSONObject()
                            .put("externalTransactionToken", token)
                            .toString()
                    }
                }
            }
        }
    }

    fun takeExternalContentLinkTokenFromRust(): String? = synchronized(lock) {
        val taken = externalContentLinkToken
        externalContentLinkToken = null
        taken
    }

    /** Opens an approved digital-content URL through Google's required UI. */
    fun launchExternalContentLinkFromRust(url: String) {
        synchronized(lock) { externalContentLinkLaunchResult = null }
        val params = LaunchExternalLinkParams.newBuilder()
            .setBillingProgram(BillingClient.BillingProgram.EXTERNAL_CONTENT_LINK)
            .setLinkUri(Uri.parse(url))
            .setLinkType(LaunchExternalLinkParams.LinkType.LINK_TO_DIGITAL_CONTENT_OFFER)
            .setLaunchMode(LaunchExternalLinkParams.LaunchMode.LAUNCH_IN_EXTERNAL_BROWSER_OR_APP)
            .build()
        activity.runOnUiThread {
            client.launchExternalLink(activity, params) { result ->
                synchronized(lock) {
                    externalContentLinkLaunchResult = if (
                        result.responseCode == BillingClient.BillingResponseCode.OK
                    ) {
                        JSONObject().put("launched", true).toString()
                    } else {
                        Catalogue.error("Could not open external checkout: ${result.debugMessage}")
                    }
                }
            }
        }
    }

    fun takeExternalContentLinkLaunchResultFromRust(): String? = synchronized(lock) {
        val taken = externalContentLinkLaunchResult
        externalContentLinkLaunchResult = null
        taken
    }

    private fun publishExternalContentLinkTokenError(message: String) {
        synchronized(lock) {
            externalContentLinkToken = Catalogue.error(message)
        }
    }

    // ---- Polling ----

    fun takeProductsFromRust(): String? = synchronized(lock) {
        val taken = products
        products = null
        taken
    }

    fun takeEntitlementsFromRust(): String? = synchronized(lock) {
        val taken = entitlements
        entitlements = null
        taken
    }

    fun getPurchaseStateFromRust(): String =
        synchronized(lock) { if (purchasing) STATE_PURCHASING else STATE_IDLE }

    private fun publish(json: String, isProducts: Boolean) {
        synchronized(lock) {
            if (isProducts) products = json else entitlements = json
        }
    }

    private fun publishEntitlementsError(message: String) {
        synchronized(lock) { purchasing = false }
        publish(Catalogue.error(message), isProducts = false)
    }
}
