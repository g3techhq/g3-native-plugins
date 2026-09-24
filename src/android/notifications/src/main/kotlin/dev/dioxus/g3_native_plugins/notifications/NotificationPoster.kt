package dev.dioxus.g3_native_plugins.notifications

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.RemoteInput
import android.content.Context
import android.content.Intent
import android.graphics.drawable.Icon
import android.os.Build
import android.os.Bundle
import org.json.JSONObject

/**
 * Turns the JSON Rust sends into a posted notification.
 *
 * An object rather than part of the plugin because the receivers post too —
 * a scheduled notification fires from [AlarmReceiver], with no Activity and
 * possibly no plugin instance in the process.
 */
internal object NotificationPoster {
    fun manager(context: Context): NotificationManager =
        context.getSystemService(NotificationManager::class.java)

    /** Android 8+ will not post without a channel, so one exists by default. */
    private fun ensureDefaultChannel(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val manager = manager(context)
        if (manager.getNotificationChannel(Keys.DEFAULT_CHANNEL) != null) return
        manager.createNotificationChannel(
            NotificationChannel(
                Keys.DEFAULT_CHANNEL,
                "Notifications",
                NotificationManager.IMPORTANCE_DEFAULT,
            ),
        )
    }

    /** The part of a notification handed back to Rust in events. */
    fun summary(notification: JSONObject): JSONObject = JSONObject()
        .put("id", notification.getInt("id"))
        .put("title", notification.optString("title", ""))
        .put("body", notification.stringOrNull("body") ?: JSONObject.NULL)
        .put("group", notification.stringOrNull("group") ?: JSONObject.NULL)
        .put("actionTypeId", notification.stringOrNull("actionTypeId") ?: JSONObject.NULL)
        .put("extra", notification.optJSONObject("extra") ?: JSONObject())

    /**
     * The status-bar icon: a drawable the app names, else the app icon.
     * Android draws the app icon as a white square here, which is why apps
     * ship a silhouette and name it.
     */
    private fun smallIcon(context: Context, name: String?): Int {
        if (name != null) {
            val found = context.resources.getIdentifier(name, "drawable", context.packageName)
            if (found != 0) return found
        }
        return context.applicationInfo.icon.takeIf { it != 0 } ?: android.R.drawable.ic_dialog_info
    }

    /**
     * A distinct request code per notification and action. PendingIntents
     * that differ only in extras are the same PendingIntent to Android, so
     * without this every notification would open with the last one's data.
     */
    private fun requestCode(id: Int, action: String) = "$id:$action".hashCode()

    /**
     * Opens the app with the notification attached. `singleTop` with
     * `newTask` brings a running app forward and hands this over as a new
     * Intent rather than starting a second Activity, which wry cannot host.
     */
    private fun activityIntent(
        context: Context,
        notification: JSONObject,
        action: String,
        mutable: Boolean,
    ): PendingIntent? {
        val intent = context.packageManager.getLaunchIntentForPackage(context.packageName)
            ?: return null
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        intent.putExtra(Keys.EXTRA_PAYLOAD, summary(notification).toString())
        intent.putExtra(Keys.EXTRA_ACTION, action)
        return PendingIntent.getActivity(
            context,
            requestCode(notification.getInt("id"), action),
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or mutability(mutable),
        )
    }

    private fun broadcastIntent(
        context: Context,
        notification: JSONObject,
        action: String,
        mutable: Boolean,
    ): PendingIntent {
        val intent = Intent(context, ActionReceiver::class.java)
            .putExtra(Keys.EXTRA_PAYLOAD, summary(notification).toString())
            .putExtra(Keys.EXTRA_ACTION, action)
        return PendingIntent.getBroadcast(
            context,
            requestCode(notification.getInt("id"), action),
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or mutability(mutable),
        )
    }

    /** A reply field only works if the system may write the reply into the Intent. */
    private fun mutability(mutable: Boolean): Int = when {
        !mutable -> PendingIntent.FLAG_IMMUTABLE
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> PendingIntent.FLAG_MUTABLE
        else -> 0
    }

    private fun addActions(
        context: Context,
        builder: Notification.Builder,
        notification: JSONObject,
        icon: Int,
    ) {
        val typeId = notification.stringOrNull("actionTypeId") ?: return
        val type = ActionTypes.find(context, typeId) ?: return
        val actions = type.getJSONArray("actions")
        // Android lays out three at most and drops the rest.
        for (index in 0 until minOf(actions.length(), 3)) {
            val action = actions.getJSONObject(index)
            val id = action.getString("id")
            val input = action.optBoolean("input", false)
            val intent = if (action.optBoolean("foreground", false)) {
                activityIntent(context, notification, id, input)
            } else {
                broadcastIntent(context, notification, id, input)
            } ?: continue
            val button = Notification.Action.Builder(
                Icon.createWithResource(context, icon),
                action.getString("title"),
                intent,
            )
            if (input) {
                button.addRemoteInput(
                    RemoteInput.Builder(Keys.REMOTE_INPUT)
                        .setLabel(action.stringOrNull("inputPlaceholder") ?: action.getString("title"))
                        .build(),
                )
            }
            builder.addAction(button.build())
        }
    }

    fun post(context: Context, notification: JSONObject) {
        val channel = notification.stringOrNull("channelId")
            ?: Keys.DEFAULT_CHANNEL.also { ensureDefaultChannel(context) }
        val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Notification.Builder(context, channel)
        } else {
            @Suppress("DEPRECATION")
            Notification.Builder(context)
        }
        val icon = smallIcon(context, notification.stringOrNull("icon"))
        val body = notification.stringOrNull("body")
        builder
            .setSmallIcon(icon)
            .setContentTitle(notification.optString("title", ""))
            .setAutoCancel(true)
            .setShowWhen(true)
            .setWhen(System.currentTimeMillis())
        if (body != null) {
            builder.setContentText(body)
            builder.setStyle(Notification.BigTextStyle().bigText(body))
        }
        notification.stringOrNull("group")?.let { builder.setGroup(it) }
        if (notification.has("badge") && !notification.isNull("badge")) {
            builder.setNumber(notification.getInt("badge"))
        }
        // Before channels, sound was per notification.
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) {
            @Suppress("DEPRECATION")
            if (notification.optBoolean("silent", false)) {
                builder.setDefaults(0)
            } else {
                builder.setDefaults(Notification.DEFAULT_ALL)
            }
        }
        activityIntent(context, notification, Keys.TAP, false)?.let { builder.setContentIntent(it) }
        addActions(context, builder, notification, icon)
        // Carried on the notification so active() can hand the same data back.
        builder.setExtras(Bundle().apply {
            putString(Keys.EXTRA_PAYLOAD, summary(notification).toString())
        })
        manager(context).notify(notification.getInt("id"), builder.build())
    }
}
