package dev.dioxus.g3_native_plugins.media

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.content.Context
import android.net.wifi.WifiManager
import android.os.PowerManager
import android.os.Build
import android.os.IBinder

/// Holds the process in the foreground while media plays. The notification
/// is built by `MediaPlugin`, which owns the media session it points at; this
/// shows it and holds the locks needed to refill a screen-off player.
class PlaybackService : Service() {
    companion object {
        const val EXTRA_NOTIFICATION = "notification"
        const val EXTRA_PLAYING = "playing"
        const val ACTION_UPDATE_PLAYBACK = "dev.dioxus.g3_native_plugins.media.UPDATE_PLAYBACK"
        /// Paused: leave the notification up, stop being a foreground service.
        const val ACTION_DETACH = "dev.dioxus.g3_native_plugins.media.DETACH"
        const val CHANNEL_ID = "tawny_playback"
        const val NOTIFICATION_ID = 7021
    }

    private var cpuLock: PowerManager.WakeLock? = null
    private var wifiLock: WifiManager.WifiLock? = null

    /// Audio output can hold its own wake lock, but loses it when the buffer
    /// empties. Keep segment requests and the local relay running through that
    /// gap, even with the screen off. A foreground service alone cannot do so.
    @Suppress("DEPRECATION")
    private fun setPlaybackLocks(playing: Boolean) {
        if (!playing) {
            cpuLock?.let { if (it.isHeld) it.release() }
            wifiLock?.let { if (it.isHeld) it.release() }
            return
        }
        val cpu = cpuLock ?: getSystemService(PowerManager::class.java)
            .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "g3-native-plugins:playback")
            .apply { setReferenceCounted(false) }
            .also { cpuLock = it }
        if (!cpu.isHeld) cpu.acquire()
        val wifi = wifiLock ?: (applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager)
            .createWifiLock(WifiManager.WIFI_MODE_FULL_HIGH_PERF, "g3-native-plugins:playback")
            .apply { setReferenceCounted(false) }
            .also { wifiLock = it }
        if (!wifi.isHeld) wifi.acquire()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val notification = intent?.let(::notificationExtra)
        if (intent?.action == ACTION_DETACH) {
            setPlaybackLocks(false)
            if (notification != null) {
                getSystemService(NotificationManager::class.java).notify(NOTIFICATION_ID, notification)
            }
            stopForeground(STOP_FOREGROUND_DETACH)
            stopSelf()
            return START_NOT_STICKY
        }
        if (intent?.action == ACTION_UPDATE_PLAYBACK) {
            if (notification != null) {
                getSystemService(NotificationManager::class.java).notify(NOTIFICATION_ID, notification)
            }
        } else {
            startForeground(NOTIFICATION_ID, notification ?: fallbackNotification())
        }
        // This is playback intent, including buffering, rather than whether
        // Chromium is producing audio right now. Pause releases immediately;
        // the foreground grace period must not keep the phone awake.
        setPlaybackLocks(intent?.getBooleanExtra(EXTRA_PLAYING, false) == true)
        return START_NOT_STICKY
    }

    private fun notificationExtra(intent: Intent): Notification? =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            intent.getParcelableExtra(EXTRA_NOTIFICATION, Notification::class.java)
        } else {
            @Suppress("DEPRECATION")
            intent.getParcelableExtra(EXTRA_NOTIFICATION)
        }

    /// Only reached if the service is restarted without its notification;
    /// `startForeground` still has to be given one.
    private fun fallbackNotification(): Notification {
        val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            getSystemService(NotificationManager::class.java).createNotificationChannel(
                NotificationChannel(CHANNEL_ID, "Video playback", NotificationManager.IMPORTANCE_LOW),
            )
            Notification.Builder(this, CHANNEL_ID)
        } else {
            @Suppress("DEPRECATION")
            Notification.Builder(this)
        }
        return builder
            .setSmallIcon(android.R.drawable.ic_media_play)
            .setContentTitle("Playing video")
            .setOngoing(true)
            .setCategory(Notification.CATEGORY_TRANSPORT)
            .build()
    }

    override fun onDestroy() {
        setPlaybackLocks(false)
        super.onDestroy()
    }

    override fun onTaskRemoved(rootIntent: Intent?) {
        setPlaybackLocks(false)
        stopSelf()
        super.onTaskRemoved(rootIntent)
    }

    override fun onBind(intent: Intent?): IBinder? = null
}
