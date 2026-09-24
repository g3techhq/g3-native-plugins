package dev.dioxus.g3_native_plugins.push_notifications

import android.content.Context
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import org.json.JSONArray
import org.json.JSONObject

internal object PushStore {
    const val PREFERENCES = "g3_native_plugins.push_notifications"
    private const val EVENTS = "events"
    private const val TOKEN = "token"
    private const val LIMIT = 50

    /**
     * Persisted rather than held in memory: FCM starts the app's process just
     * to run [PushMessagingService], and a message delivered to a process that
     * is gone before the app opens would otherwise be lost with it.
     */
    @Synchronized
    fun push(context: Context, event: JSONObject) {
        val preferences = context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
        val queue = JSONArray(preferences.getString(EVENTS, "[]"))
        queue.put(event)
        while (queue.length() > LIMIT) queue.remove(0)
        preferences.edit().putString(EVENTS, queue.toString()).commit()
    }

    @Synchronized
    fun take(context: Context): String? {
        val preferences = context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
        val queue = JSONArray(preferences.getString(EVENTS, "[]"))
        if (queue.length() == 0) return null
        val first = queue.getJSONObject(0)
        queue.remove(0)
        preferences.edit().putString(EVENTS, queue.toString()).commit()
        return first.toString()
    }

    fun token(context: Context): String? =
        context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE).getString(TOKEN, null)

    /** Store a token and announce it, but only when it actually changed. */
    fun saveToken(context: Context, token: String?) {
        val preferences = context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
        if (preferences.getString(TOKEN, null) == token) return
        preferences.edit().putString(TOKEN, token).commit()
        if (token != null) push(context, JSONObject().put("type", "token").put("token", token))
    }
}

/**
 * Where FCM hands over what it does not display itself: every data message,
 * and notification messages that arrive while the app is in the foreground.
 * A notification message that arrives in the background is displayed by FCM
 * and reaches the app only if tapped, as an Intent the plugin reads.
 */
class PushMessagingService : FirebaseMessagingService() {
    override fun onNewToken(token: String) {
        PushStore.saveToken(applicationContext, token)
    }

    override fun onMessageReceived(message: RemoteMessage) {
        val data = JSONObject()
        for ((key, value) in message.data) data.put(key, value)
        val notification = message.notification
        PushStore.push(
            applicationContext,
            JSONObject()
                .put("type", "message")
                .put("messageId", message.messageId ?: JSONObject.NULL)
                .put("title", notification?.title ?: JSONObject.NULL)
                .put("body", notification?.body ?: JSONObject.NULL)
                .put("data", data),
        )
    }
}
