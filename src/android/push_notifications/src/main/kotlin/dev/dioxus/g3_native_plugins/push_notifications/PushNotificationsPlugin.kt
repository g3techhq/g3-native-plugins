package dev.dioxus.g3_native_plugins.push_notifications

import android.app.Activity
import android.content.Context
import android.content.Intent
import androidx.activity.ComponentActivity
import com.google.firebase.FirebaseApp
import com.google.firebase.FirebaseOptions
import com.google.firebase.messaging.FirebaseMessaging
import org.json.JSONObject

/**
 * Remote push through Firebase Cloud Messaging.
 *
 * Firebase is normally configured by the `google-services` Gradle plugin
 * reading `google-services.json` into resources at build time. A Dioxus app's
 * Gradle project is generated, so that step is not available to it; instead
 * the four values FCM needs are passed in at run time and Firebase is
 * initialized from them. An app that does manage to include the resources is
 * detected and used as is.
 *
 * A tap on a notification FCM displayed opens the launcher Activity with the
 * message's data as Intent extras — the launch Intent on a cold start, a new
 * Intent on a warm one — read here the same way the deep-links plugin reads
 * its links.
 *
 * Commands answer null, or a JSON object carrying `error`.
 */
class PushNotificationsPlugin(private val activity: Activity) {
    companion object {
        private var listenerRegistered = false
        private var launchIntentConsumed = false
        private const val MESSAGE_ID = "google.message_id"
    }

    private val context: Context = activity.applicationContext

    // Named errorJson, not error: kotlin.error() throws, and a member named
    // the same would be a very quiet trap for whoever edits this next.
    private fun errorJson(message: String): String =
        JSONObject().put("error", message).toString()

    fun prepareFromRust() {
        activity.runOnUiThread {
            if (!launchIntentConsumed) {
                launchIntentConsumed = true
                consume(activity.intent)
            }
            val owner = activity as? ComponentActivity ?: return@runOnUiThread
            if (listenerRegistered) return@runOnUiThread
            owner.addOnNewIntentListener { intent -> consume(intent) }
            listenerRegistered = true
        }
    }

    /**
     * FCM mixes its own bookkeeping into the extras beside the message data.
     * Only the data is the sender's, so the rest is left out.
     */
    private fun consume(intent: Intent?) {
        val extras = intent?.extras ?: return
        if (extras.getString(MESSAGE_ID) == null) return
        val data = JSONObject()
        for (key in extras.keySet()) {
            if (key.startsWith("google.") || key.startsWith("gcm.") ||
                key == "from" || key == "collapse_key"
            ) {
                continue
            }
            // Bundle.get is deprecated for typed getters, but FCM's extras are
            // an open set of keys whose types are not known in advance.
            @Suppress("DEPRECATION")
            val value = extras.get(key)
            if (value is String) data.put(key, value)
        }
        val messageId = extras.getString(MESSAGE_ID)
        intent.removeExtra(MESSAGE_ID)
        PushStore.push(
            context,
            JSONObject()
                .put("type", "opened")
                .put("actionId", "tap")
                .put("messageId", messageId)
                .put("data", data),
        )
    }

    private fun ensureFirebase(configJson: String) {
        if (FirebaseApp.getApps(context).isNotEmpty()) return
        val config = JSONObject(configJson)
        val firebase = config.optJSONObject("firebase")
            ?: throw IllegalStateException(
                "Firebase is not configured: pass FirebaseOptions to register()",
            )
        val options = FirebaseOptions.Builder()
            .setApplicationId(firebase.getString("applicationId"))
            .setApiKey(firebase.getString("apiKey"))
            .setProjectId(firebase.getString("projectId"))
            .setGcmSenderId(firebase.getString("senderId"))
            .build()
        FirebaseApp.initializeApp(context, options)
    }

    /**
     * Start FCM and ask for this install's token. The token arrives later, as
     * a `token` event and through [tokenFromRust]; a failure arrives as a
     * `registrationFailed` event.
     */
    fun registerFromRust(configJson: String): String? = try {
        ensureFirebase(configJson)
        val messaging = FirebaseMessaging.getInstance()
        messaging.isAutoInitEnabled = true
        messaging.token.addOnCompleteListener { task ->
            if (task.isSuccessful && task.result != null) {
                PushStore.saveToken(context, task.result)
            } else {
                PushStore.push(
                    context,
                    JSONObject()
                        .put("type", "registrationFailed")
                        .put("error", task.exception?.message ?: "FCM returned no token"),
                )
            }
        }
        null
    } catch (failure: Exception) {
        errorJson("Could not register for push: ${failure.message ?: failure.javaClass.simpleName}")
    }

    /** Invalidate the token, so the server can no longer reach this install. */
    fun unregisterFromRust(): String? = try {
        if (FirebaseApp.getApps(context).isNotEmpty()) {
            val messaging = FirebaseMessaging.getInstance()
            messaging.isAutoInitEnabled = false
            messaging.deleteToken()
        }
        PushStore.saveToken(context, null)
        null
    } catch (failure: Exception) {
        errorJson("Could not unregister from push: ${failure.message}")
    }

    fun tokenFromRust(): String? = PushStore.token(context)

    fun takeEventFromRust(): String? = PushStore.take(context)
}
