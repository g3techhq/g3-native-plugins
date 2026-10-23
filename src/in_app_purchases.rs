use serde::{Deserialize, Serialize};
/// Declared for its side effect: this is what tells `dx` to build and bundle
/// the Kotlin module. The calls go through [`crate::android_bridge`] instead,
/// because the macro's own bindings resolve classes against the calling
/// thread's loader and Dioxus does not call from a thread that has one.
#[cfg(target_os = "android")]
#[manganis::ffi("src/android/in_app_purchases")]
unsafe extern "Kotlin" {
    pub type InAppPurchasesPlugin;
}
#[cfg(target_os = "android")]
const IN_APP_PURCHASES_CLASS: &str =
    "dev.dioxus.g3_native_plugins.in_app_purchases.InAppPurchasesPlugin";
#[cfg(target_os = "android")]
type InAppPurchasesHandle = crate::android_bridge::AndroidPlugin;
#[cfg(target_os = "ios")]
type InAppPurchasesHandle = InAppPurchasesPlugin;
#[cfg(target_os = "ios")]
#[manganis::ffi("src/ios")]
unsafe extern "Swift" {
    pub type InAppPurchasesPlugin;
    pub fn prepareFromRust(this: &InAppPurchasesPlugin);
    pub fn startProductsRequestFromRust(this: &InAppPurchasesPlugin, idsJson: String);
    pub fn takeProductsFromRust(this: &InAppPurchasesPlugin) -> Option<String>;
    pub fn startPurchaseFromRust(this: &InAppPurchasesPlugin, payloadJson: String);
    pub fn startRestoreFromRust(this: &InAppPurchasesPlugin);
    pub fn takeEntitlementsFromRust(this: &InAppPurchasesPlugin) -> Option<String>;
    pub fn finishFromRust(this: &InAppPurchasesPlugin, payloadJson: String);
    pub fn getPurchaseStateFromRust(this: &InAppPurchasesPlugin) -> Option<String>;
}
/// What kind of thing a product is, which decides how it is finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProductKind {
    /// Bought repeatedly and used up. Must be consumed after being granted, or
    /// the store will not sell it again.
    Consumable,
    /// Bought once and owned forever.
    #[default]
    NonConsumable,
    /// Renews until cancelled.
    Subscription,
}
/// A subscription's billing period.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionPeriod {
    /// `day`, `week`, `month`, or `year`.
    pub unit: String,
    /// How many of [`unit`](SubscriptionPeriod::unit) make up one period.
    pub count: u32,
}
/// A specific deal on a subscription: the standard price, an introductory
/// rate, or a free trial.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionOffer {
    /// Pass this to `InAppPurchases::start_purchase` to buy this deal
    /// specifically. On Android it identifies a base plan and offer; on iOS it
    /// is `introductory` for the introductory offer.
    pub id: String,
    /// Localized, formatted, and ready to show. Never build your own from
    /// [`price_micros`](SubscriptionOffer::price_micros): stores format prices
    /// per storefront and some do not put the symbol where you would.
    pub display_price: String,
    /// The price in millionths of a currency unit, for comparing offers.
    pub price_micros: i64,
    /// How long this rate lasts before the standard price takes over.
    pub period: SubscriptionPeriod,
    /// Whether this offer costs nothing for its period.
    pub is_free_trial: bool,
}
/// Something the app can sell.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Product {
    /// The identifier configured in App Store Connect or the Play Console.
    pub id: String,
    /// Whether it is consumable, permanent, or a subscription.
    pub kind: ProductKind,
    /// The store's display name.
    pub title: String,
    /// The store's description.
    pub description: String,
    /// Localized, formatted, and ready to show.
    pub display_price: String,
    /// The price in millionths of a currency unit.
    pub price_micros: i64,
    /// ISO 4217 code for the storefront the price is in.
    pub currency_code: String,
    /// How often a subscription renews. `None` for one-time products.
    pub subscription_period: Option<SubscriptionPeriod>,
    /// Deals available on a subscription. Empty for one-time products.
    pub offers: Vec<SubscriptionOffer>,
}
/// Something the user owns.
#[derive(Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entitlement {
    /// Which product was bought.
    pub product_id: String,
    /// Pass this to `InAppPurchases::finish`. StoreKit's transaction id on
    /// iOS; the purchase token on Android, which is what that platform
    /// acknowledges against.
    pub transaction_id: String,
    /// The proof to send to your own server to verify the purchase. The signed
    /// JWS on iOS, the purchase token on Android.
    ///
    /// Treat everything else on this struct as display state. It comes from the
    /// device, and a device is not a thing to take financial claims from — check
    /// this against Apple's or Google's servers before granting anything that
    /// costs you money.
    pub purchase_token: String,
    /// Milliseconds since the Unix epoch.
    pub purchased_at_ms: u64,
    /// When a subscription lapses, in milliseconds since the Unix epoch.
    /// `None` for one-time products.
    pub expires_at_ms: Option<u64>,
    /// Whether it is usable now: not expired, not refunded, not revoked.
    pub is_active: bool,
    /// Whether a subscription is set to renew. `false` once the user cancels,
    /// while [`is_active`](Entitlement::is_active) stays true until it lapses.
    pub will_auto_renew: bool,
    /// Whether `InAppPurchases::finish` still has to be called.
    ///
    /// This matters more than it looks on Android, which refunds any purchase
    /// left unacknowledged for three days.
    pub needs_finishing: bool,
}
impl std::fmt::Debug for Entitlement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Entitlement")
            .field("product_id", &self.product_id)
            .field("is_active", &self.is_active)
            .field("will_auto_renew", &self.will_auto_renew)
            .field("needs_finishing", &self.needs_finishing)
            .finish_non_exhaustive()
    }
}

/// Store ownership with the optional app-account UUID supplied at purchase.
/// This is an untrusted routing hint; verify the store's signed/server response
/// and the expected account on your server before granting or finishing.
#[derive(Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributedEntitlement {
    /// Existing ownership/proof data, with the same wire fields as `Entitlement`.
    #[serde(flatten)]
    pub entitlement: Entitlement,
    /// The original app account's opaque UUID. `None` for older/unattributed
    /// purchases. Never substitute the currently signed-in account for `None`.
    #[serde(default)]
    pub app_account_token: Option<String>,
}
impl std::fmt::Debug for AttributedEntitlement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AttributedEntitlement")
            .field("entitlement", &self.entitlement)
            .field(
                "app_account_token",
                &self.app_account_token.as_ref().map(|_| "[redacted]"),
            )
            .finish()
    }
}

#[cfg(any(target_os = "android", target_os = "ios", test))]
fn account_purchase_payload(
    product: &str,
    offer: Option<&str>,
    account: &str,
) -> Result<String, String> {
    let bytes = account.as_bytes();
    let canonical = bytes.len() == 36
        && bytes.iter().enumerate().all(|(index, byte)| {
            if [8, 13, 18, 23].contains(&index) {
                *byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
        && bytes.iter().any(|byte| *byte != b'-' && *byte != b'0');
    if !canonical {
        return Err("The purchase account must be a nonzero UUID.".into());
    }
    serde_json::to_string(&serde_json::json!({"productId":product,"offerId":offer.unwrap_or_default(),"appAccountToken":account.to_ascii_lowercase()}))
        .map_err(|_| "Could not encode the purchase account.".into())
}

/// Google Play's short-lived attribution for one external content-link visit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalContentLinkToken {
    /// Send this to the payment backend and later report it to Google Play.
    pub external_transaction_token: String,
}
/// Whether a purchase is on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PurchaseState {
    /// Nothing in flight.
    #[default]
    Idle,
    /// The store's purchase sheet is up, or the result is still settling.
    Purchasing,
}
/// Selling things through the platform's own store.
///
/// Both stores are asynchronous and neither will let a purchase be awaited
/// inline, so this starts work and polls for the result, the same as
/// [`crate::Geolocation`] and [`crate::Auth`]. StoreKit 2 is `async` Swift that
/// cannot cross an FFI boundary, and Play Billing answers on listeners, so
/// there is no version of this that returns a purchase from the call that
/// started it.
///
/// ```rust,ignore
/// let mut plugins = use_context::<NativePlugins>();
///
/// // Once, at startup: connects the store and starts the transaction listener.
/// plugins.in_app_purchases.write().prepare()?;
///
/// // Ask for the catalogue, then poll for it.
/// plugins.in_app_purchases.write()
///     .start_products_request(&["pro.monthly".to_string()])?;
/// if let Ok(Some(products)) = plugins.in_app_purchases.write().poll_products() {
///     // Show them.
/// }
///
/// // Buy, then poll for what the user now owns.
/// plugins.in_app_purchases.write().start_purchase("pro.monthly", None)?;
/// if let Ok(Some(entitlements)) = plugins.in_app_purchases.write().poll_entitlements() {
///     for entitlement in entitlements {
///         // Verify entitlement.purchase_token on your server, then grant.
///         if entitlement.needs_finishing {
///             plugins.in_app_purchases.write().finish(&entitlement.transaction_id, false)?;
///         }
///     }
/// }
/// ```
///
/// [`poll_entitlements`](InAppPurchases::poll_entitlements) is the single place
/// ownership arrives from, whether it came from a purchase, a restore, or the
/// background listener that catches purchases finished outside the app. Poll it
/// for the life of the app, not only around a purchase.
///
/// Nothing here verifies a purchase. Both platforms hand back a token meant to
/// be checked server-side against the store, and a plugin running on the user's
/// device is not in a position to do that credibly.
///
/// The web and macOS builds are inert. Callers need no cfg of their own.
#[cfg(any(target_os = "android", target_os = "ios"))]
pub struct InAppPurchases {
    plugin: Option<InAppPurchasesHandle>,
}
/// The inert store facade: see the native [`InAppPurchases`] for the contract.
#[cfg(any(target_arch = "wasm32", target_os = "macos"))]
pub struct InAppPurchases;
#[cfg(any(target_os = "android", target_os = "ios"))]
impl InAppPurchases {
    pub(crate) fn new() -> Self {
        Self { plugin: None }
    }
    fn get_plugin(&mut self) -> Result<&InAppPurchasesHandle, String> {
        if self.plugin.is_none() {
            #[cfg(target_os = "android")]
            let created = InAppPurchasesHandle::new(IN_APP_PURCHASES_CLASS)?;
            #[cfg(target_os = "ios")]
            let created = InAppPurchasesPlugin::new()
                .map_err(|error| format!("Failed to create InAppPurchasesPlugin: {error:?}"))?;
            self.plugin = Some(created);
        }
        Ok(self.plugin.as_ref().unwrap())
    }
    /// Connect to the store and start listening for transactions.
    ///
    /// Call this at startup rather than at the point of sale. Both platforms
    /// deliver purchases that completed while the app was closed — an
    /// interrupted payment, a family-shared buy, a subscription renewal — and
    /// only a listener that is already running catches them.
    pub fn prepare(&mut self) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        plugin.call_unit("prepareFromRust")?;
        #[cfg(target_os = "ios")]
        prepareFromRust(plugin)?;
        Ok(())
    }
    /// Look up products by their store identifiers.
    pub fn start_products_request(&mut self, ids: &[String]) -> Result<(), String> {
        let ids = serde_json::to_string(ids)
            .map_err(|error| format!("Failed to encode product ids: {error}"))?;
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        plugin.call_unit_str("startProductsRequestFromRust", &ids)?;
        #[cfg(target_os = "ios")]
        startProductsRequestFromRust(plugin, ids)?;
        Ok(())
    }
    /// Take the catalogue once it arrives, or `Ok(None)` while it is still
    /// loading. Handed over once, then cleared.
    ///
    /// Products the store did not recognize are absent rather than an error, so
    /// compare against what was asked for. An id missing here usually means it
    /// is not approved yet, not that the code is wrong.
    pub fn poll_products(&mut self) -> Result<Option<Vec<Product>>, String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        let taken = plugin.call_string("takeProductsFromRust")?;
        #[cfg(target_os = "ios")]
        let taken = takeProductsFromRust(plugin)?;
        let Some(json) = taken else {
            return Ok(None);
        };
        Self::decode(&json, "products")
    }
    /// Put the store's purchase sheet on screen.
    ///
    /// `offer_id` picks a specific [`SubscriptionOffer`]; `None` buys at the
    /// standard price. It is ignored on iOS, where redeeming a promotional
    /// offer needs a signature from your server that this plugin does not
    /// produce — an introductory offer there is applied by StoreKit on its own
    /// if the user qualifies.
    ///
    /// The result arrives through
    /// [`poll_entitlements`](InAppPurchases::poll_entitlements), including when
    /// the user cancels, which is not an error.
    pub fn start_purchase(
        &mut self,
        product_id: &str,
        offer_id: Option<&str>,
    ) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        plugin.call_unit_str_str(
            "startPurchaseFromRust",
            product_id,
            offer_id.unwrap_or_default(),
        )?;
        #[cfg(target_os = "ios")]
        {
            let payload = serde_json::to_string(&serde_json::json!({
                "productId": product_id,
                "offerId": offer_id.unwrap_or_default(),
            }))
            .map_err(|error| format!("Failed to encode purchase request: {error}"))?;
            startPurchaseFromRust(plugin, payload)?;
        }
        Ok(())
    }
    /// Start a purchase bound to an opaque app-account UUID from your backend.
    /// iOS passes it to StoreKit's `appAccountToken`; Android uses the obfuscated
    /// account ID. Do not pass an email or a raw database identifier. The UUID
    /// must be stable for that account and never reused for a different person.
    /// Results arrive through `poll_attributed_entitlements`; verify attribution
    /// against the provider on your backend before delivery/finish.
    pub fn start_purchase_for_account(
        &mut self,
        product_id: &str,
        offer_id: Option<&str>,
        account_token: &str,
    ) -> Result<(), String> {
        let payload = account_purchase_payload(product_id, offer_id, account_token)?;
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        plugin.call_unit_str("startPurchaseForAccountFromRust", &payload)?;
        #[cfg(target_os = "ios")]
        startPurchaseFromRust(plugin, payload)?;
        Ok(())
    }

    /// Re-read what the user owns from the store.
    ///
    /// Needed on a new device or after a reinstall, and on iOS this is the call
    /// that can prompt for an App Store password, so put it behind a button the
    /// user pressed rather than running it at launch.
    pub fn start_restore(&mut self) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        plugin.call_unit("startRestoreFromRust")?;
        #[cfg(target_os = "ios")]
        startRestoreFromRust(plugin)?;
        Ok(())
    }
    /// Take everything the user owns, whenever there is news.
    ///
    /// `Ok(None)` when nothing has changed since the last poll. A batch is the
    /// complete current picture, not a delta, so it is safe to treat as the
    /// whole truth about what is owned.
    pub fn poll_entitlements(&mut self) -> Result<Option<Vec<Entitlement>>, String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        let taken = plugin.call_string("takeEntitlementsFromRust")?;
        #[cfg(target_os = "ios")]
        let taken = takeEntitlementsFromRust(plugin)?;
        let Some(json) = taken else {
            return Ok(None);
        };
        Self::decode(&json, "entitlements")
    }
    /// Take the ownership batch together with original app-account hints.
    /// Choose this or `poll_entitlements`: both consume the same shared slot.
    /// Missing attribution stays `None`; it never inherits a launch/current user.
    pub fn poll_attributed_entitlements(
        &mut self,
    ) -> Result<Option<Vec<AttributedEntitlement>>, String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        let taken = plugin.call_string("takeEntitlementsFromRust")?;
        #[cfg(target_os = "ios")]
        let taken = takeEntitlementsFromRust(plugin)?;
        let Some(json) = taken else { return Ok(None) };
        Self::decode(&json, "attributed entitlements")
    }

    /// Tell the store the purchase has been delivered.
    ///
    /// Call this only after the entitlement is actually granted, because it is
    /// the store's record that the user got what they paid for. Set `consume`
    /// for a [`ProductKind::Consumable`] so it can be bought again; leave it
    /// false for anything else. It has no effect on iOS, where finishing a
    /// consumable is the same call as finishing anything else.
    ///
    /// Android refunds purchases left unfinished for three days.
    pub fn finish(&mut self, transaction_id: &str, consume: bool) -> Result<(), String> {
        let plugin = self.get_plugin()?;
        #[cfg(target_os = "android")]
        plugin.call_unit_str_bool("finishFromRust", transaction_id, consume)?;
        #[cfg(target_os = "ios")]
        {
            let payload = serde_json::to_string(&serde_json::json!({
                "transactionId": transaction_id,
                "consume": consume,
            }))
            .map_err(|error| format!("Failed to encode finish request: {error}"))?;
            finishFromRust(plugin, payload)?;
        }
        Ok(())
    }
    /// Ask Google Play for a fresh token for one external checkout visit.
    ///
    /// Android only. The result is asynchronous and must not be cached or
    /// reused; call this immediately before every link-out.
    pub fn start_external_content_link_token(&mut self) -> Result<(), String> {
        #[cfg(target_os = "android")]
        self.get_plugin()?
            .call_unit("startExternalContentLinkTokenFromRust")?;
        Ok(())
    }
    /// Take the fresh Google Play external-transaction token once available.
    pub fn poll_external_content_link_token(
        &mut self,
    ) -> Result<Option<ExternalContentLinkToken>, String> {
        #[cfg(target_os = "android")]
        {
            let taken = self
                .get_plugin()?
                .call_string("takeExternalContentLinkTokenFromRust")?;
            let Some(json) = taken else {
                return Ok(None);
            };
            return Self::decode(&json, "external content link token");
        }
        #[cfg(target_os = "ios")]
        Ok(None)
    }
    /// Open the checkout URL through Google Play's external-link flow.
    pub fn launch_external_content_link(&mut self, url: &str) -> Result<(), String> {
        #[cfg(target_os = "android")]
        self.get_plugin()?
            .call_unit_str("launchExternalContentLinkFromRust", url)?;
        Ok(())
    }
    /// Take confirmation that Google Play launched the external URL.
    pub fn poll_external_content_link_launch(&mut self) -> Result<Option<()>, String> {
        #[cfg(target_os = "android")]
        {
            let taken = self
                .get_plugin()?
                .call_string("takeExternalContentLinkLaunchResultFromRust")?;
            let Some(json) = taken else {
                return Ok(None);
            };
            let _: Option<serde_json::Value> = Self::decode(&json, "external content link launch")?;
            return Ok(Some(()));
        }
        #[cfg(target_os = "ios")]
        Ok(None)
    }
    /// Whether a purchase is on screen, for disabling a buy button.
    pub fn purchase_state(&mut self) -> PurchaseState {
        let Ok(plugin) = self.get_plugin() else {
            return PurchaseState::Idle;
        };
        #[cfg(target_os = "android")]
        let state = plugin
            .call_string("getPurchaseStateFromRust")
            .ok()
            .flatten();
        #[cfg(target_os = "ios")]
        let state = getPurchaseStateFromRust(plugin).ok().flatten();
        match state.as_deref() {
            Some("purchasing") => PurchaseState::Purchasing,
            _ => PurchaseState::Idle,
        }
    }
    fn decode<T: serde::de::DeserializeOwned>(json: &str, what: &str) -> Result<Option<T>, String> {
        let value: serde_json::Value = serde_json::from_str(json)
            .map_err(|error| format!("Failed to read {what}: {error}"))?;
        if let Some(error) = value.get("error").and_then(|error| error.as_str()) {
            return Err(error.to_string());
        }
        serde_json::from_value(value)
            .map(Some)
            .map_err(|error| format!("Failed to read {what}: {error}"))
    }
}
#[cfg(any(target_arch = "wasm32", target_os = "macos"))]
impl InAppPurchases {
    pub(crate) fn new() -> Self {
        Self
    }
    /// No-op: there is no store to connect to on these targets.
    pub fn prepare(&mut self) -> Result<(), String> {
        Ok(())
    }
    /// No-op: no catalogue is ever fetched.
    pub fn start_products_request(&mut self, _ids: &[String]) -> Result<(), String> {
        Ok(())
    }
    /// Always `None`.
    pub fn poll_products(&mut self) -> Result<Option<Vec<Product>>, String> {
        Ok(None)
    }
    /// No-op: nothing can be bought on these targets.
    pub fn start_purchase(
        &mut self,
        _product_id: &str,
        _offer_id: Option<&str>,
    ) -> Result<(), String> {
        Ok(())
    }
    /// No-op: no account-attributed store exists on these targets.
    pub fn start_purchase_for_account(
        &mut self,
        _product_id: &str,
        _offer_id: Option<&str>,
        _account_token: &str,
    ) -> Result<(), String> {
        Ok(())
    }
    /// Always `None`: no account-attributed store exists on these targets.
    pub fn poll_attributed_entitlements(
        &mut self,
    ) -> Result<Option<Vec<AttributedEntitlement>>, String> {
        Ok(None)
    }
    /// No-op.
    pub fn start_restore(&mut self) -> Result<(), String> {
        Ok(())
    }
    /// Always `None`: nothing is owned through a store here.
    pub fn poll_entitlements(&mut self) -> Result<Option<Vec<Entitlement>>, String> {
        Ok(None)
    }
    /// No-op: there is no transaction to finish.
    pub fn finish(&mut self, _transaction_id: &str, _consume: bool) -> Result<(), String> {
        Ok(())
    }
    /// No-op outside Android.
    pub fn start_external_content_link_token(&mut self) -> Result<(), String> {
        Ok(())
    }
    /// No external-link token exists outside Android.
    pub fn poll_external_content_link_token(
        &mut self,
    ) -> Result<Option<ExternalContentLinkToken>, String> {
        Ok(None)
    }
    /// No-op outside Android.
    pub fn launch_external_content_link(&mut self, _url: &str) -> Result<(), String> {
        Ok(())
    }
    /// No launch result exists outside Android.
    pub fn poll_external_content_link_launch(&mut self) -> Result<Option<()>, String> {
        Ok(None)
    }
    /// Always [`PurchaseState::Idle`].
    pub fn purchase_state(&mut self) -> PurchaseState {
        PurchaseState::Idle
    }
}

#[cfg(test)]
mod account_attribution_tests {
    use super::*;
    #[test]
    fn account_payload_is_opaque_validated_and_serialized_as_data() {
        let account = "B91F6307-5CBE-4145-9042-45ACDB7C2544";
        let payload =
            account_purchase_payload("product\"private", Some("offer\\value"), account).unwrap();
        let value: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(value["appAccountToken"], account.to_ascii_lowercase());
        assert_eq!(value["productId"], "product\"private");
        assert_eq!(value["offerId"], "offer\\value");
        for bad in [
            "",
            "private@example.invalid",
            "user:private",
            "00000000-0000-0000-0000-000000000000",
            "b91f6307-5cbe-4145-9042-45acdb7c254z",
        ] {
            assert!(account_purchase_payload("product", None, bad).is_err());
        }
    }
    #[test]
    fn old_batches_keep_missing_attribution_and_debug_never_prints_proofs_or_ids() {
        let mut item = AttributedEntitlement {
            entitlement: Entitlement {
                transaction_id: "private-transaction-canary".into(),
                purchase_token: "private-proof-canary".into(),
                ..Default::default()
            },
            app_account_token: Some("private-account-canary".into()),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("private-proof-canary"));
        let decoded: AttributedEntitlement = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, item);
        let old: Entitlement = serde_json::from_str(&json).unwrap();
        assert_eq!(old, item.entitlement);
        let debug = format!("{item:?}");
        for secret in [
            "private-transaction-canary",
            "private-proof-canary",
            "private-account-canary",
        ] {
            assert!(!debug.contains(secret));
        }
        item.app_account_token = None;
        let mut old = serde_json::to_value(&item).unwrap();
        old.as_object_mut().unwrap().remove("appAccountToken");
        assert!(
            serde_json::from_value::<AttributedEntitlement>(old)
                .unwrap()
                .app_account_token
                .is_none()
        );
    }
}
