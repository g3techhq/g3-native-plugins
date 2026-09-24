package dev.dioxus.g3_native_plugins.notifications

import android.app.ActivityManager
import android.app.AlarmManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import org.json.JSONArray
import org.json.JSONObject

internal object Keys {
    const val PREFERENCES = "g3_native_plugins.notifications"
    const val SCHEDULED = "g3_native_plugins.notifications.scheduled"
    const val EXTRA_PAYLOAD = "g3_native_plugins.notification"
    const val EXTRA_ACTION = "g3_native_plugins.notification.action"
    const val EXTRA_ID = "g3_native_plugins.notification.id"
    const val REMOTE_INPUT = "g3_native_plugins.notification.input"
    const val DEFAULT_CHANNEL = "default"
    const val TAP = "tap"
}

/**
 * `optString` answers a JSON null with the four letters "null", which would
 * put exactly that on someone's lock screen. Every optional string goes
 * through here instead.
 */
internal fun JSONObject.stringOrNull(key: String): String? =
    if (has(key) && !isNull(key)) getString(key) else null

/**
 * Whether the app is what the user is looking at. Deliveries are reported to
 * Rust only then, matching iOS, where only a foreground delivery reaches the
 * app at all.
 */
internal fun isForeground(): Boolean {
    val info = ActivityManager.RunningAppProcessInfo()
    ActivityManager.getMyMemoryState(info)
    return info.importance == ActivityManager.RunningAppProcessInfo.IMPORTANCE_FOREGROUND
}

/**
 * Taps, button presses, and foreground deliveries, waiting for Rust to ask.
 *
 * Kept in preferences rather than memory because a button that does not open
 * the app is handled by [ActionReceiver], which may run in a process started
 * just for it and gone again before the app next opens. A queue in memory
 * would lose that press.
 */
internal object EventQueue {
    private const val KEY = "events"
    private const val LIMIT = 50

    @Synchronized
    fun push(context: Context, event: JSONObject) {
        val preferences = context.getSharedPreferences(Keys.PREFERENCES, Context.MODE_PRIVATE)
        val queue = JSONArray(preferences.getString(KEY, "[]"))
        queue.put(event)
        // An app that never drains its queue should not grow it forever. The
        // oldest events are the least likely to still matter.
        while (queue.length() > LIMIT) queue.remove(0)
        preferences.edit().putString(KEY, queue.toString()).commit()
    }

    @Synchronized
    fun take(context: Context): String? {
        val preferences = context.getSharedPreferences(Keys.PREFERENCES, Context.MODE_PRIVATE)
        val queue = JSONArray(preferences.getString(KEY, "[]"))
        if (queue.length() == 0) return null
        val first = queue.getJSONObject(0)
        queue.remove(0)
        preferences.edit().putString(KEY, queue.toString()).commit()
        return first.toString()
    }

    fun received(context: Context, notification: JSONObject) = push(
        context,
        JSONObject()
            .put("type", "received")
            .put("notification", NotificationPoster.summary(notification)),
    )
}

/** The button sets notifications refer to by `actionTypeId`. */
internal object ActionTypes {
    private const val KEY = "actionTypes"

    fun save(context: Context, types: JSONArray) {
        context.getSharedPreferences(Keys.PREFERENCES, Context.MODE_PRIVATE)
            .edit().putString(KEY, types.toString()).apply()
    }

    fun find(context: Context, id: String): JSONObject? {
        val stored = context.getSharedPreferences(Keys.PREFERENCES, Context.MODE_PRIVATE)
            .getString(KEY, null) ?: return null
        val types = JSONArray(stored)
        for (index in 0 until types.length()) {
            val type = types.getJSONObject(index)
            if (type.getString("id") == id) return type
        }
        return null
    }
}

/**
 * Scheduled notifications, and the alarms that fire them.
 *
 * Android has no scheduled-notification API: an alarm wakes [AlarmReceiver],
 * which posts. Alarms are forgotten on reboot and on app update, so every
 * scheduled notification is also written down here for [BootReceiver] to
 * re-arm, and so [pending] has something to answer from.
 *
 * Alarms are exact only when the app may set exact alarms, which from Android
 * 12 needs `SCHEDULE_EXACT_ALARM` and from 14 is denied by default. Otherwise
 * they are inexact and Android may deliver them some minutes late, which is
 * the trade Android intends for anything that is not an alarm clock.
 */
internal object ScheduleStore {
    fun all(context: Context): List<JSONObject> {
        val preferences = context.getSharedPreferences(Keys.SCHEDULED, Context.MODE_PRIVATE)
        return preferences.all.values.mapNotNull { value ->
            (value as? String)?.let { runCatching { JSONObject(it) }.getOrNull() }
        }
    }

    fun get(context: Context, id: Int): JSONObject? =
        context.getSharedPreferences(Keys.SCHEDULED, Context.MODE_PRIVATE)
            .getString(id.toString(), null)
            ?.let { runCatching { JSONObject(it) }.getOrNull() }

    private fun put(context: Context, notification: JSONObject) {
        context.getSharedPreferences(Keys.SCHEDULED, Context.MODE_PRIVATE).edit()
            .putString(notification.getInt("id").toString(), notification.toString())
            .commit()
    }

    fun remove(context: Context, id: Int) {
        context.getSharedPreferences(Keys.SCHEDULED, Context.MODE_PRIVATE).edit()
            .remove(id.toString()).commit()
    }

    /** Milliseconds between firings of a repeating schedule. */
    private fun period(schedule: JSONObject): Long {
        val unit = when (schedule.getString("interval")) {
            "minute" -> 60_000L
            "hour" -> 3_600_000L
            "day" -> 86_400_000L
            else -> 604_800_000L
        }
        return unit * schedule.getInt("count").coerceAtLeast(1)
    }

    /** Record and arm a notification that has a schedule. */
    fun schedule(context: Context, notification: JSONObject) {
        val schedule = notification.getJSONObject("schedule")
        val now = System.currentTimeMillis()
        val at = if (schedule.getString("kind") == "at") {
            schedule.getLong("atMs")
        } else {
            now + period(schedule)
        }
        notification.put("nextAtMs", at)
        put(context, notification)
        arm(context, notification)
    }

    /**
     * After a repeating notification fires, or after a reboot: move it to its
     * next firing and arm that, or report that a one-off is done.
     */
    fun advance(context: Context, notification: JSONObject): Boolean {
        val schedule = notification.getJSONObject("schedule")
        if (schedule.getString("kind") != "every") {
            remove(context, notification.getInt("id"))
            return false
        }
        val period = period(schedule)
        val now = System.currentTimeMillis()
        var next = notification.optLong("nextAtMs", now) + period
        // Firings missed while the device was off are skipped, not replayed
        // in a burst.
        while (next <= now) next += period
        notification.put("nextAtMs", next)
        put(context, notification)
        arm(context, notification)
        return true
    }

    private fun alarmIntent(context: Context, id: Int): PendingIntent {
        val intent = Intent(context, AlarmReceiver::class.java).putExtra(Keys.EXTRA_ID, id)
        return PendingIntent.getBroadcast(
            context,
            id,
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
    }

    fun arm(context: Context, notification: JSONObject) {
        val alarms = context.getSystemService(AlarmManager::class.java) ?: return
        val id = notification.getInt("id")
        val at = notification.getLong("nextAtMs")
        val schedule = notification.getJSONObject("schedule")
        val whileIdle = schedule.optBoolean("allowWhileIdle", false)
        val exact = Build.VERSION.SDK_INT < Build.VERSION_CODES.S || alarms.canScheduleExactAlarms()
        val operation = alarmIntent(context, id)
        when {
            exact && whileIdle ->
                alarms.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, operation)
            exact -> alarms.setExact(AlarmManager.RTC_WAKEUP, at, operation)
            whileIdle -> alarms.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, operation)
            else -> alarms.set(AlarmManager.RTC_WAKEUP, at, operation)
        }
    }

    fun disarm(context: Context, id: Int) {
        context.getSystemService(AlarmManager::class.java)?.cancel(alarmIntent(context, id))
        remove(context, id)
    }
}
