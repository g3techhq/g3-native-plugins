package dev.dioxus.g3_native_plugins.media

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.os.Build
import android.os.IBinder

/// Holds the process in the foreground while media plays. The notification
/// is built by `MediaPlugin`, which owns the media session it points at; this
/// only shows it.
class PlaybackService : Service() {
    companion object {
        const val EXTRA_NOTIFICATION = "notification"
        /// Paused: leave the notification up, stop being a foreground service.
        const val ACTION_DETACH = "dev.dioxus.g3_native_plugins.media.DETACH"
        const val CHANNEL_ID = "tawny_playback"
        const val NOTIFICATION_ID = 7021
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val notification = intent?.let(::notificationExtra)
        if (intent?.action == ACTION_DETACH) {
            if (notification != null) {
                getSystemService(NotificationManager::class.java).notify(NOTIFICATION_ID, notification)
            }
            stopForeground(STOP_FOREGROUND_DETACH)
            stopSelf()
            return START_NOT_STICKY
        }
        startForeground(NOTIFICATION_ID, notification ?: fallbackNotification())
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

    override fun onTaskRemoved(rootIntent: Intent?) {
        stopSelf()
        super.onTaskRemoved(rootIntent)
    }

    override fun onBind(intent: Intent?): IBinder? = null
}
