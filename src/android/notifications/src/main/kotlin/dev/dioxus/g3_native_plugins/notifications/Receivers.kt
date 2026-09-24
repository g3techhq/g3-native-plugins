package dev.dioxus.g3_native_plugins.notifications

import android.app.RemoteInput
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import org.json.JSONObject

/** Fires a scheduled notification, then arms its next firing if it repeats. */
class AlarmReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val id = intent.getIntExtra(Keys.EXTRA_ID, Int.MIN_VALUE)
        if (id == Int.MIN_VALUE) return
        val notification = ScheduleStore.get(context, id) ?: return
        NotificationPoster.post(context, notification)
        if (isForeground()) EventQueue.received(context, notification)
        ScheduleStore.advance(context, notification)
    }
}

/**
 * A button that does not open the app. The press is recorded for Rust to
 * collect on its next poll, and the notification is taken down, since Android
 * leaves it up after a button press and the user has already answered it.
 */
class ActionReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val payload = intent.getStringExtra(Keys.EXTRA_PAYLOAD) ?: return
        val action = intent.getStringExtra(Keys.EXTRA_ACTION) ?: return
        val notification = JSONObject(payload)
        val input = RemoteInput.getResultsFromIntent(intent)
            ?.getCharSequence(Keys.REMOTE_INPUT)
            ?.toString()
        EventQueue.push(
            context,
            JSONObject()
                .put("type", "action")
                .put("actionId", action)
                .put("input", input ?: JSONObject.NULL)
                .put("notification", notification),
        )
        NotificationPoster.manager(context).cancel(notification.getInt("id"))
    }
}

/**
 * Alarms do not survive a reboot or an app update, so every scheduled
 * notification is armed again from the record kept of it. A one-off whose
 * moment passed while the device was off is shown now rather than dropped:
 * a reminder late is better than a reminder lost.
 */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != Intent.ACTION_BOOT_COMPLETED &&
            intent.action != Intent.ACTION_MY_PACKAGE_REPLACED
        ) {
            return
        }
        val now = System.currentTimeMillis()
        for (notification in ScheduleStore.all(context)) {
            val schedule = notification.getJSONObject("schedule")
            val due = notification.optLong("nextAtMs", now)
            when {
                schedule.getString("kind") == "every" && due <= now ->
                    ScheduleStore.advance(context, notification)
                due <= now -> {
                    NotificationPoster.post(context, notification)
                    ScheduleStore.remove(context, notification.getInt("id"))
                }
                else -> ScheduleStore.arm(context, notification)
            }
        }
    }
}
