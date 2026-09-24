package dev.dioxus.g3_native_plugins.notifications

import android.Manifest
import android.app.Activity
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.RemoteInput
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.media.AudioAttributes
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.activity.ComponentActivity
import org.json.JSONArray
import org.json.JSONObject

/**
 * Local notifications: posted now or scheduled, with buttons and replies.
 *
 * Taps come back to the app as Intents — the launch Intent on a cold start,
 * a new Intent through `ComponentActivity`'s listener on a warm one — the same
 * two routes the deep-links plugin reads, for the same reason: neither reaches
 * the WebView, and `onNewIntent` cannot be overridden from a library. Button
 * presses that do not open the app arrive at [ActionReceiver] instead. All of
 * them land in one persisted queue that Rust drains.
 *
 * Commands answer null, or a JSON object carrying `error`.
 */
class NotificationsPlugin(private val activity: Activity) {
    companion object {
        private const val REQUEST_PERMISSION = 7403
        private const val ASKED = "asked"
        private var listenerRegistered = false
        private var launchIntentConsumed = false
    }

    private val context: Context = activity.applicationContext
    private val manager: NotificationManager = NotificationPoster.manager(context)

    // Named errorJson, not error: kotlin.error() throws, and a member named
    // the same would be a very quiet trap for whoever edits this next.
    private fun errorJson(message: String): String =
        JSONObject().put("error", message).toString()

    private inline fun command(what: String, block: () -> Unit): String? = try {
        block()
        null
    } catch (failure: Exception) {
        errorJson("Could not $what: ${failure.message ?: failure.javaClass.simpleName}")
    }

    fun prepareFromRust() {
        // Intent state and listener registration belong on the main thread;
        // Rust calls from a native thread.
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
     * Record a tap carried by an Intent, then strip it off, so nothing that
     * reads the Intent again — a second prepare, a recreated Activity — reports
     * the same tap twice.
     */
    private fun consume(intent: Intent?) {
        val payload = intent?.getStringExtra(Keys.EXTRA_PAYLOAD) ?: return
        val action = intent.getStringExtra(Keys.EXTRA_ACTION) ?: Keys.TAP
        val input = RemoteInput.getResultsFromIntent(intent)
            ?.getCharSequence(Keys.REMOTE_INPUT)
            ?.toString()
        intent.removeExtra(Keys.EXTRA_PAYLOAD)
        intent.removeExtra(Keys.EXTRA_ACTION)
        val notification = runCatching { JSONObject(payload) }.getOrNull() ?: return
        EventQueue.push(
            context,
            JSONObject()
                .put("type", "action")
                .put("actionId", action)
                .put("input", input ?: JSONObject.NULL)
                .put("notification", notification),
        )
        // A tap dismisses its notification by itself; a button does not.
        if (action != Keys.TAP) manager.cancel(notification.getInt("id"))
    }

    fun takeEventFromRust(): String? = EventQueue.take(context)

    /**
     * Android cannot tell "never asked" from "refused for good": both report
     * denied with no rationale wanted. Remembering that a request was made
     * tells them apart, the same as the camera plugin, and here it is kept
     * across launches because a notification prompt is usually asked once.
     */
    fun checkPermissionsFromRust(): String {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
            // Granted at install; the user can still turn the app off.
            return if (manager.areNotificationsEnabled()) "granted" else "denied"
        }
        val permission = Manifest.permission.POST_NOTIFICATIONS
        return when {
            activity.checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED ->
                if (manager.areNotificationsEnabled()) "granted" else "denied"
            activity.shouldShowRequestPermissionRationale(permission) -> "prompt-with-rationale"
            context.getSharedPreferences(Keys.PREFERENCES, Context.MODE_PRIVATE)
                .getBoolean(ASKED, false) -> "denied"
            else -> "prompt"
        }
    }

    fun requestPermissionsFromRust() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        val permission = Manifest.permission.POST_NOTIFICATIONS
        activity.runOnUiThread {
            if (activity.checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED) {
                return@runOnUiThread
            }
            // The answer lands on onRequestPermissionsResult, which a library
            // cannot override, so Rust polls checkPermissions instead.
            activity.requestPermissions(arrayOf(permission), REQUEST_PERMISSION)
            context.getSharedPreferences(Keys.PREFERENCES, Context.MODE_PRIVATE)
                .edit().putBoolean(ASKED, true).apply()
        }
    }

    fun openSettingsFromRust() {
        activity.runOnUiThread {
            val intent = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
                    .putExtra(Settings.EXTRA_APP_PACKAGE, activity.packageName)
            } else {
                Intent(
                    Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
                    Uri.fromParts("package", activity.packageName, null),
                )
            }.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            try {
                activity.startActivity(intent)
            } catch (_: Exception) {
                // No settings screen to open is not something the app can fix.
            }
        }
    }

    fun showFromRust(notificationJson: String): String? = command("show the notification") {
        val notification = JSONObject(notificationJson)
        if (notification.has("schedule") && !notification.isNull("schedule")) {
            ScheduleStore.schedule(context, notification)
            return@command
        }
        // notify() drops a notification the app may not post without a word,
        // which reads as a bug in the app. Say why instead.
        if (!manager.areNotificationsEnabled()) {
            throw IllegalStateException("notifications are turned off for this app")
        }
        NotificationPoster.post(context, notification)
        if (isForeground()) EventQueue.received(context, notification)
    }

    fun pendingFromRust(): String = try {
        val pending = JSONArray()
        for (notification in ScheduleStore.all(context)) {
            pending.put(
                JSONObject()
                    .put("id", notification.getInt("id"))
                    .put("title", notification.optString("title", ""))
                    .put("body", notification.stringOrNull("body") ?: JSONObject.NULL)
                    .put("schedule", notification.getJSONObject("schedule")),
            )
        }
        pending.toString()
    } catch (failure: Exception) {
        errorJson("Could not list scheduled notifications: ${failure.message}")
    }

    private fun ids(idsJson: String): List<Int> {
        val array = JSONArray(idsJson)
        return (0 until array.length()).map { array.getInt(it) }
    }

    fun cancelFromRust(idsJson: String): String? = command("cancel notifications") {
        for (id in ids(idsJson)) ScheduleStore.disarm(context, id)
    }

    fun cancelAllFromRust(): String? = command("cancel notifications") {
        for (notification in ScheduleStore.all(context)) {
            ScheduleStore.disarm(context, notification.getInt("id"))
        }
    }

    /**
     * Only notifications this plugin posted: they carry its payload. The app's
     * others — a media player's, a push the system displayed — belong to
     * whoever posted them.
     */
    private fun ours() = manager.activeNotifications.filter {
        it.notification.extras?.getString(Keys.EXTRA_PAYLOAD) != null
    }

    fun activeFromRust(): String = try {
        val active = JSONArray()
        for (posted in ours()) {
            active.put(JSONObject(posted.notification.extras.getString(Keys.EXTRA_PAYLOAD)!!))
        }
        active.toString()
    } catch (failure: Exception) {
        errorJson("Could not list notifications: ${failure.message}")
    }

    fun removeActiveFromRust(idsJson: String): String? = command("remove notifications") {
        for (id in ids(idsJson)) manager.cancel(id)
    }

    fun removeAllActiveFromRust(): String? = command("remove notifications") {
        for (posted in ours()) manager.cancel(posted.tag, posted.id)
    }

    private fun importance(name: String): Int = when (name) {
        "none" -> NotificationManager.IMPORTANCE_NONE
        "min" -> NotificationManager.IMPORTANCE_MIN
        "low" -> NotificationManager.IMPORTANCE_LOW
        "high" -> NotificationManager.IMPORTANCE_HIGH
        else -> NotificationManager.IMPORTANCE_DEFAULT
    }

    private fun importanceName(value: Int): String = when (value) {
        NotificationManager.IMPORTANCE_NONE -> "none"
        NotificationManager.IMPORTANCE_MIN -> "min"
        NotificationManager.IMPORTANCE_LOW -> "low"
        NotificationManager.IMPORTANCE_HIGH, NotificationManager.IMPORTANCE_MAX -> "high"
        else -> "default"
    }

    private fun visibility(name: String): Int = when (name) {
        "public" -> android.app.Notification.VISIBILITY_PUBLIC
        "secret" -> android.app.Notification.VISIBILITY_SECRET
        else -> android.app.Notification.VISIBILITY_PRIVATE
    }

    private fun visibilityName(value: Int): String = when (value) {
        android.app.Notification.VISIBILITY_PUBLIC -> "public"
        android.app.Notification.VISIBILITY_SECRET -> "secret"
        else -> "private"
    }

    /** Channels exist from Android 8; before that the calls have nothing to do. */
    fun createChannelFromRust(channelJson: String): String? = command("create the channel") {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return@command
        val json = JSONObject(channelJson)
        val channel = NotificationChannel(
            json.getString("id"),
            json.getString("name"),
            importance(json.optString("importance", "default")),
        )
        json.stringOrNull("description")?.let { channel.description = it }
        channel.lockscreenVisibility = visibility(json.optString("visibility", "private"))
        channel.enableVibration(json.optBoolean("vibration", false))
        json.stringOrNull("sound")?.let { sound ->
            val uri = Uri.parse("android.resource://${context.packageName}/raw/$sound")
            val attributes = AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_NOTIFICATION)
                .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                .build()
            channel.setSound(uri, attributes)
        }
        manager.createNotificationChannel(channel)
    }

    fun deleteChannelFromRust(id: String): String? = command("delete the channel") {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) manager.deleteNotificationChannel(id)
    }

    fun channelsFromRust(): String = try {
        val channels = JSONArray()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            for (channel in manager.notificationChannels) {
                channels.put(
                    JSONObject()
                        .put("id", channel.id)
                        .put("name", channel.name.toString())
                        .put("description", channel.description ?: JSONObject.NULL)
                        .put("importance", importanceName(channel.importance))
                        .put("visibility", visibilityName(channel.lockscreenVisibility))
                        .put("vibration", channel.shouldVibrate())
                        .put("sound", channel.sound?.lastPathSegment ?: JSONObject.NULL),
                )
            }
        }
        channels.toString()
    } catch (failure: Exception) {
        errorJson("Could not list channels: ${failure.message}")
    }

    fun registerActionTypesFromRust(typesJson: String): String? =
        command("register the action types") {
            ActionTypes.save(context, JSONArray(typesJson))
        }
}
