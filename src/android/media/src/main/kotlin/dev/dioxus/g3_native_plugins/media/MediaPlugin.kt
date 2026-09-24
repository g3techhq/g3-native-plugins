package dev.dioxus.g3_native_plugins.media

import android.app.Activity
import android.app.Application
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.PictureInPictureParams
import android.app.RemoteAction
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.ActivityInfo
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.drawable.Icon
import android.media.MediaMetadata
import android.media.session.MediaSession
import android.media.session.PlaybackState
import android.os.Build
import android.os.Bundle
import android.os.SystemClock
import android.util.Rational
import android.view.View
import android.view.ViewGroup
import android.view.ViewTreeObserver
import android.view.WindowInsets
import android.view.WindowInsetsController
import android.webkit.WebView
import androidx.activity.ComponentActivity
import java.net.HttpURLConnection
import java.net.URL
import org.json.JSONObject

class MediaPlugin(private val activity: Activity) {
    companion object {
        private const val ACTION_PIP_COMMAND =
            "dev.dioxus.g3_native_plugins.media.PIP_COMMAND"
        private const val EXTRA_COMMAND = "command"
        private const val COMMAND_REWIND = "rewind"
        private const val COMMAND_TOGGLE = "toggle"
        private const val COMMAND_FORWARD = "forward"
        private const val COMMAND_PLAY = "play"
        private const val COMMAND_PAUSE = "pause"
        private const val COMMAND_SEEK = "seek"
        private const val ARTWORK_MAX_PX = 512

        /// Reads what the system media controls show straight from the player,
        /// so position and duration are the element's own rather than a copy
        /// that drifts. Title, channel and artwork come from the stage's data
        /// attributes.
        private const val PLAYER_STATE_SCRIPT =
            "(() => { const v=document.querySelector('#tawny-player video');" +
                "if(!v)return null;const s=document.getElementById('tawny-player');" +
                "const d=(s&&s.dataset)||{};let art='';" +
                "try{if(d.thumbnail)art=new URL(d.thumbnail,location.href).href;}catch(e){}" +
                "return {position:v.currentTime||0," +
                "duration:Number.isFinite(v.duration)?v.duration:0," +
                "rate:v.playbackRate||1,title:d.title||'',artist:d.channel||'',artwork:art};})()"
    }

    private var commandReceiver: BroadcastReceiver? = null
    private var mediaSession: MediaSession? = null
    private var serviceForeground = false
    private var artworkUrl = ""
    private var artwork: Bitmap? = null
    private var pipListenerRegistered = false
    private var pipWidth = 16
    private var pipHeight = 9
    private var isPlaying = true
    private var playbackActive = false
    private var playbackTitle = ""
    private var lifecycleCallbacks: Application.ActivityLifecycleCallbacks? = null
    private var pendingWebPlaybackResume: Runnable? = null

    private fun findWebView(view: View): WebView? {
        if (view is WebView) return view
        if (view is ViewGroup) {
            for (index in 0 until view.childCount) {
                findWebView(view.getChildAt(index))?.let { return it }
            }
        }
        return null
    }

    private fun setPictureInPictureDomState(active: Boolean) {
        val state = if (active) "true" else "false"
        val stateUpdate = if (active) {
            "document.documentElement.dataset.androidPip='true';"
        } else {
            "delete document.documentElement.dataset.androidPip;"
        }
        val script = stateUpdate +
            "window.dispatchEvent(new CustomEvent('tawnynativepictureinpicturechange'," +
            "{detail:{active:$state}}))"
        findWebView(activity.window.decorView)?.evaluateJavascript(script, null)
    }

    private fun notifyWebPlaybackResume() {
        findWebView(activity.window.decorView)?.evaluateJavascript(
            "window.dispatchEvent(new Event('tawnynativeplaybackresume'))",
            null,
        )
    }

    private fun scheduleWebPlaybackResume() {
        val root = activity.window.decorView
        pendingWebPlaybackResume?.let(root::removeCallbacks)
        val resume = Runnable {
            pendingWebPlaybackResume = null
            if (!playbackActive) return@Runnable
            // The host pauses its WebView with the Activity. Resume it only at
            // that real lifecycle boundary; doing this for every HTML play
            // event repeatedly reset Chromium's audio clock and timers.
            findWebView(root)?.let { webView ->
                webView.onResume()
                webView.resumeTimers()
            }
            notifyWebPlaybackResume()
        }
        pendingWebPlaybackResume = resume
        root.post(resume)
    }

    /// Switching to another app hides the window, and Chromium pauses any
    /// video with an audio track once its WebView reports a hidden window.
    /// Turning the screen off leaves the window visible, which is why only the
    /// app switch stopped playback. While playback is active, hand the WebView
    /// a visible window straight back so the video keeps its audio running.
    /// A dismissed PiP window also hides it, and that one should stop.
    private val windowVisibilityListener =
        ViewTreeObserver.OnWindowVisibilityChangeListener { visibility ->
            if (visibility == View.VISIBLE || !playbackActive) return@OnWindowVisibilityChangeListener
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && activity.isInPictureInPictureMode) {
                return@OnWindowVisibilityChangeListener
            }
            findWebView(activity.window.decorView)?.dispatchWindowVisibilityChanged(View.VISIBLE)
        }
    private var windowVisibilityObserver: ViewTreeObserver? = null

    private fun ensureWindowVisibilityListener() {
        val observer = activity.window.decorView.viewTreeObserver
        if (observer === windowVisibilityObserver && observer.isAlive) return
        observer.addOnWindowVisibilityChangeListener(windowVisibilityListener)
        windowVisibilityObserver = observer
    }

    /// Once playback stops in the background, give the WebView the window's
    /// real visibility so the page stops rendering and its timers slow down.
    private fun restoreWebViewWindowVisibility() {
        val root = activity.window.decorView
        if (root.windowVisibility == View.VISIBLE) return
        findWebView(root)?.dispatchWindowVisibilityChanged(root.windowVisibility)
    }

    private fun ensureActivityLifecycleCallbacks() {
        if (lifecycleCallbacks != null) return
        val callbacks = object : Application.ActivityLifecycleCallbacks {
            override fun onActivityPaused(paused: Activity) {
                if (paused === activity && playbackActive) scheduleWebPlaybackResume()
            }

            override fun onActivityStopped(stopped: Activity) {
                if (stopped === activity && playbackActive) scheduleWebPlaybackResume()
            }

            override fun onActivityResumed(resumed: Activity) {
                if (resumed === activity && playbackActive) {
                    activity.window.decorView.post { notifyWebPlaybackResume() }
                }
            }

            override fun onActivityDestroyed(destroyed: Activity) {
                if (destroyed !== activity) return
                pendingWebPlaybackResume?.let(activity.window.decorView::removeCallbacks)
                pendingWebPlaybackResume = null
                releasePlayback()
                windowVisibilityObserver?.takeIf { it.isAlive }
                    ?.removeOnWindowVisibilityChangeListener(windowVisibilityListener)
                windowVisibilityObserver = null
                activity.application.unregisterActivityLifecycleCallbacks(this)
                lifecycleCallbacks = null
            }

            override fun onActivityCreated(created: Activity, state: Bundle?) = Unit
            override fun onActivityStarted(started: Activity) = Unit
            override fun onActivitySaveInstanceState(activity: Activity, state: Bundle) = Unit
        }
        activity.application.registerActivityLifecycleCallbacks(callbacks)
        lifecycleCallbacks = callbacks
    }

    private fun commandPendingIntent(command: String, requestCode: Int): PendingIntent {
        val intent = Intent(ACTION_PIP_COMMAND).apply {
            setPackage(activity.packageName)
            putExtra(EXTRA_COMMAND, command)
        }
        return PendingIntent.getBroadcast(
            activity,
            requestCode,
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
    }

    private fun pipActions(): List<RemoteAction> = listOf(
        RemoteAction(
            Icon.createWithResource(activity, android.R.drawable.ic_media_rew),
            "Back 10 seconds",
            "Back 10 seconds",
            commandPendingIntent(COMMAND_REWIND, 7101),
        ),
        RemoteAction(
            Icon.createWithResource(
                activity,
                if (isPlaying) android.R.drawable.ic_media_pause else android.R.drawable.ic_media_play,
            ),
            if (isPlaying) "Pause" else "Play",
            if (isPlaying) "Pause" else "Play",
            commandPendingIntent(COMMAND_TOGGLE, 7102),
        ),
        RemoteAction(
            Icon.createWithResource(activity, android.R.drawable.ic_media_ff),
            "Forward 10 seconds",
            "Forward 10 seconds",
            commandPendingIntent(COMMAND_FORWARD, 7103),
        ),
    )

    private fun buildPictureInPictureParams(): PictureInPictureParams {
        val builder = PictureInPictureParams.Builder()
            .setAspectRatio(Rational(pipWidth.coerceAtLeast(1), pipHeight.coerceAtLeast(1)))
            .setActions(pipActions())
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            builder.setSeamlessResizeEnabled(true)
        }
        return builder.build()
    }

    private fun updatePictureInPictureParams() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        activity.setPictureInPictureParams(buildPictureInPictureParams())
    }

    private fun runPlaybackCommand(command: String, positionMs: Long = 0L) {
        val root = activity.window.decorView
        val webView = findWebView(root) ?: return
        if (command == COMMAND_PLAY || command == COMMAND_TOGGLE) {
            // Pausing in the background gave the WebView its hidden window back,
            // and Chromium will not start a video in a hidden page. A headset or
            // the system controls asking to play have to show it first.
            if (root.windowVisibility != View.VISIBLE) {
                webView.dispatchWindowVisibilityChanged(View.VISIBLE)
            }
        }
        val script = when (command) {
            COMMAND_PLAY ->
                "(() => { const v=document.querySelector('#tawny-player video');" +
                    "if(!v)return 'missing';v.__tawnyPlaybackIntent=true;" +
                    "v.play().catch(()=>{});return 'playing';})()"
            COMMAND_PAUSE ->
                "(() => { const v=document.querySelector('#tawny-player video');" +
                    "if(!v)return 'missing';v.__tawnyPlaybackIntent=false;v.pause();return 'paused';})()"
            COMMAND_SEEK ->
                "(() => { const v=document.querySelector('#tawny-player video');" +
                    "if(!v)return 'missing';v.currentTime=${positionMs / 1000.0};return 'sought';})()"
            COMMAND_REWIND ->
                "(() => { const v=document.querySelector('#tawny-player video');" +
                    "if(!v)return 'missing';v.currentTime=Math.max(0,v.currentTime-10);return 'rewound';})()"
            COMMAND_FORWARD ->
                "(() => { const v=document.querySelector('#tawny-player video');" +
                    "if(!v)return 'missing';const end=Number.isFinite(v.duration)?v.duration:v.currentTime+10;" +
                    "v.currentTime=Math.min(end,v.currentTime+10);return 'forwarded';})()"
            COMMAND_TOGGLE ->
                "(() => { const v=document.querySelector('#tawny-player video');" +
                    "if(!v)return 'missing';if(v.paused){v.__tawnyPlaybackIntent=true;" +
                    "v.play().catch(()=>{});return 'playing';}" +
                    "v.__tawnyPlaybackIntent=false;v.pause();return 'paused';})()"
            else -> return
        }
        webView.evaluateJavascript(script) { result ->
            when (result?.trim('"')) {
                "playing" -> isPlaying = true
                "paused" -> isPlaying = false
                else -> return@evaluateJavascript
            }
            updatePictureInPictureParams()
        }
    }

    private fun ensureMediaSession(): MediaSession {
        mediaSession?.let { return it }
        // The session is what the system media controls and a headset's
        // buttons talk to. Without one, Android hands a Play/Pause press to
        // whichever other app last had a session.
        val session = MediaSession(activity, "G3NativeMedia")
        session.setCallback(object : MediaSession.Callback() {
            override fun onPlay() = runPlaybackCommand(COMMAND_PLAY)
            override fun onPause() = runPlaybackCommand(COMMAND_PAUSE)
            override fun onStop() = runPlaybackCommand(COMMAND_PAUSE)
            override fun onSeekTo(pos: Long) = runPlaybackCommand(COMMAND_SEEK, pos)
            override fun onFastForward() = runPlaybackCommand(COMMAND_FORWARD)
            override fun onRewind() = runPlaybackCommand(COMMAND_REWIND)
        })
        session.setSessionActivity(launchPendingIntent())
        mediaSession = session
        return session
    }

    private fun launchPendingIntent(): PendingIntent? {
        val launch = activity.packageManager.getLaunchIntentForPackage(activity.packageName)
            ?: return null
        return PendingIntent.getActivity(
            activity,
            0,
            launch,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
    }

    /// Reads the player and republishes the session, the notification and the
    /// foreground service to match it. Called for every play, pause, seek and
    /// rate change, which are rare enough that re-reading is cheaper than
    /// tracking.
    private fun publishPlaybackState() {
        val webView = findWebView(activity.window.decorView) ?: return
        webView.evaluateJavascript(PLAYER_STATE_SCRIPT) { result ->
            val session = mediaSession ?: return@evaluateJavascript
            val state = result?.takeIf { it != "null" }
                ?.let { runCatching { JSONObject(it) }.getOrNull() }
                ?: return@evaluateJavascript
            val playing = playbackActive
            val title = state.optString("title").ifBlank { playbackTitle }
            val artist = state.optString("artist")
            loadArtwork(state.optString("artwork"))

            val metadata = MediaMetadata.Builder()
                .putString(MediaMetadata.METADATA_KEY_TITLE, title)
                .putString(MediaMetadata.METADATA_KEY_ARTIST, artist)
                .putLong(
                    MediaMetadata.METADATA_KEY_DURATION,
                    (state.optDouble("duration", 0.0) * 1000).toLong(),
                )
            artwork?.let { metadata.putBitmap(MediaMetadata.METADATA_KEY_ART, it) }
            session.setMetadata(metadata.build())

            val actions = PlaybackState.ACTION_PLAY_PAUSE or
                PlaybackState.ACTION_SEEK_TO or
                PlaybackState.ACTION_FAST_FORWARD or
                PlaybackState.ACTION_REWIND or
                PlaybackState.ACTION_STOP or
                if (playing) PlaybackState.ACTION_PAUSE else PlaybackState.ACTION_PLAY
            session.setPlaybackState(
                PlaybackState.Builder()
                    .setActions(actions)
                    .setState(
                        if (playing) PlaybackState.STATE_PLAYING else PlaybackState.STATE_PAUSED,
                        (state.optDouble("position", 0.0) * 1000).toLong(),
                        if (playing) state.optDouble("rate", 1.0).toFloat() else 0f,
                        SystemClock.elapsedRealtime(),
                    )
                    .build(),
            )
            session.isActive = true
            postPlaybackNotification(session, title, artist, playing)
        }
    }

    private fun loadArtwork(url: String) {
        if (url.isBlank() || url == artworkUrl) return
        artworkUrl = url
        artwork = null
        Thread {
            val bitmap = runCatching {
                val connection = URL(url).openConnection() as HttpURLConnection
                connection.connectTimeout = 5000
                connection.readTimeout = 5000
                connection.inputStream.use(BitmapFactory::decodeStream)
            }.getOrNull()?.let(::scaleArtwork) ?: return@Thread
            activity.runOnUiThread {
                // Only if the video has not changed while this was loading.
                if (artworkUrl != url || mediaSession == null) return@runOnUiThread
                artwork = bitmap
                publishPlaybackState()
            }
        }.start()
    }

    /// Metadata and notifications cross a binder, and a full-size thumbnail
    /// can blow its transaction limit.
    private fun scaleArtwork(bitmap: Bitmap): Bitmap {
        val largest = maxOf(bitmap.width, bitmap.height)
        if (largest <= ARTWORK_MAX_PX) return bitmap
        val scale = ARTWORK_MAX_PX.toFloat() / largest
        return Bitmap.createScaledBitmap(
            bitmap,
            (bitmap.width * scale).toInt().coerceAtLeast(1),
            (bitmap.height * scale).toInt().coerceAtLeast(1),
            true,
        )
    }

    private fun ensureNotificationChannel() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val channel = NotificationChannel(
            PlaybackService.CHANNEL_ID,
            "Video playback",
            NotificationManager.IMPORTANCE_LOW,
        ).apply {
            description = "Keeps video audio playing while the app is in the background"
            setShowBadge(false)
        }
        activity.getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }

    private fun notificationAction(icon: Int, title: String, command: String, requestCode: Int) =
        Notification.Action.Builder(
            Icon.createWithResource(activity, icon),
            title,
            commandPendingIntent(command, requestCode),
        ).build()

    private fun postPlaybackNotification(
        session: MediaSession,
        title: String,
        artist: String,
        playing: Boolean,
    ) {
        ensureNotificationChannel()
        val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Notification.Builder(activity, PlaybackService.CHANNEL_ID)
        } else {
            @Suppress("DEPRECATION")
            Notification.Builder(activity)
        }
        // Android 13 and later draw these controls from the session itself;
        // the actions are for older releases, which read them off the
        // notification.
        val notification = builder
            .setSmallIcon(android.R.drawable.ic_media_play)
            .setContentTitle(title.ifBlank { "Playing video" })
            .setContentText(artist)
            .setLargeIcon(artwork)
            .setContentIntent(launchPendingIntent())
            .setOngoing(playing)
            .setOnlyAlertOnce(true)
            .setVisibility(Notification.VISIBILITY_PUBLIC)
            .setCategory(Notification.CATEGORY_TRANSPORT)
            .addAction(notificationAction(android.R.drawable.ic_media_rew, "Back 10 seconds", COMMAND_REWIND, 7201))
            .addAction(
                if (playing) {
                    notificationAction(android.R.drawable.ic_media_pause, "Pause", COMMAND_PAUSE, 7202)
                } else {
                    notificationAction(android.R.drawable.ic_media_play, "Play", COMMAND_PLAY, 7203)
                },
            )
            .addAction(notificationAction(android.R.drawable.ic_media_ff, "Forward 10 seconds", COMMAND_FORWARD, 7204))
            .setStyle(
                Notification.MediaStyle()
                    .setMediaSession(session.sessionToken)
                    .setShowActionsInCompactView(0, 1, 2),
            )
            .build()

        val intent = Intent(activity, PlaybackService::class.java)
            .putExtra(PlaybackService.EXTRA_NOTIFICATION, notification)
        val notifications = activity.getSystemService(NotificationManager::class.java)
        when {
            playing && !serviceForeground -> {
                // Starting an already-running foreground service and forcing
                // the WebView lifecycle on every duplicate `play` event causes
                // audible clock resets, so only the first play starts it and
                // later ones just update the notification.
                serviceForeground = try {
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                        activity.startForegroundService(intent)
                    } else {
                        activity.startService(intent)
                    }
                    true
                } catch (error: IllegalStateException) {
                    // Android 12 refuses a foreground start from the background
                    // unless an exemption applies. The session still works;
                    // only the process's protection from being killed is lost.
                    notifications.notify(PlaybackService.NOTIFICATION_ID, notification)
                    false
                }
            }
            !playing && serviceForeground -> {
                // Paused: keep the controls on screen so Play is one tap
                // away, but let go of the foreground service.
                serviceForeground = false
                try {
                    activity.startService(intent.setAction(PlaybackService.ACTION_DETACH))
                } catch (error: IllegalStateException) {
                    notifications.notify(PlaybackService.NOTIFICATION_ID, notification)
                }
            }
            else -> notifications.notify(PlaybackService.NOTIFICATION_ID, notification)
        }
    }

    private fun releasePlayback() {
        playbackActive = false
        playbackTitle = ""
        if (serviceForeground) {
            activity.stopService(Intent(activity, PlaybackService::class.java))
            serviceForeground = false
        }
        activity.getSystemService(NotificationManager::class.java)
            .cancel(PlaybackService.NOTIFICATION_ID)
        mediaSession?.let {
            it.isActive = false
            it.release()
        }
        mediaSession = null
        artworkUrl = ""
        artwork = null
    }

    private fun ensurePictureInPictureCallbacks() {
        if (commandReceiver == null) {
            val receiver = object : BroadcastReceiver() {
                override fun onReceive(context: Context?, intent: Intent?) {
                    runPlaybackCommand(intent?.getStringExtra(EXTRA_COMMAND).orEmpty())
                }
            }
            val filter = IntentFilter(ACTION_PIP_COMMAND)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                activity.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED)
            } else {
                @Suppress("DEPRECATION")
                activity.registerReceiver(receiver, filter)
            }
            commandReceiver = receiver
        }

        if (!pipListenerRegistered) {
            val owner = activity as? ComponentActivity
            owner?.addOnPictureInPictureModeChangedListener { info ->
                activity.runOnUiThread {
                    setPictureInPictureDomState(info.isInPictureInPictureMode)
                    if (info.isInPictureInPictureMode) updatePictureInPictureParams()
                }
            }
            pipListenerRegistered = owner != null
        }
    }

    fun prepareFromRust() {
        activity.runOnUiThread {
            val root = activity.window.decorView
            root.setOnApplyWindowInsetsListener { view, insets ->
                val top = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                    insets.getInsets(WindowInsets.Type.statusBars()).top
                } else {
                    @Suppress("DEPRECATION")
                    insets.systemWindowInsetTop
                }
                val cssTop = (top / activity.resources.displayMetrics.density).toInt()
                findWebView(view)?.evaluateJavascript(
                    "document.documentElement.dataset.androidApp='true';" +
                        "document.documentElement.style.setProperty('--android-status-bar-inset', '${cssTop}px')",
                    null,
                )
                insets
            }
            root.requestApplyInsets()
            findWebView(root)?.settings?.mediaPlaybackRequiresUserGesture = false
            ensurePictureInPictureCallbacks()
            ensureActivityLifecycleCallbacks()
            ensureWindowVisibilityListener()
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                setPictureInPictureDomState(activity.isInPictureInPictureMode)
            }
        }
    }

    fun enterPictureInPictureFromRust(width: Int, height: Int) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        activity.runOnUiThread {
            pipWidth = width.coerceAtLeast(1)
            pipHeight = height.coerceAtLeast(1)
            ensurePictureInPictureCallbacks()
            // A 16x9 source rectangle describes a tiny region in the top-left
            // of the Activity, not the video. Omitting that hint lets Android
            // animate and center the full WebView media surface correctly.
            activity.enterPictureInPictureMode(buildPictureInPictureParams())
        }
    }

    fun setOrientationFromRust(orientation: String) {
        activity.runOnUiThread {
            activity.requestedOrientation = when (orientation) {
                "landscape" -> ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
                "portrait" -> ActivityInfo.SCREEN_ORIENTATION_SENSOR_PORTRAIT
                else -> ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
            }
        }
    }

    /// A WebView's `requestFullscreen()` only fills the WebView: the status and
    /// navigation bars belong to the window, so fullscreen playback has to hide
    /// them here. A swipe from the edge shows them transiently, as in other
    /// video players.
    fun setSystemBarsHiddenFromRust(hidden: Boolean) {
        activity.runOnUiThread {
            val window = activity.window
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                val controller = window.insetsController ?: return@runOnUiThread
                if (hidden) {
                    controller.systemBarsBehavior =
                        WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
                    controller.hide(WindowInsets.Type.systemBars())
                } else {
                    controller.show(WindowInsets.Type.systemBars())
                }
            } else {
                @Suppress("DEPRECATION")
                val immersiveFlags = View.SYSTEM_UI_FLAG_FULLSCREEN or
                    View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or
                    View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                @Suppress("DEPRECATION")
                window.decorView.systemUiVisibility = if (hidden) {
                    window.decorView.systemUiVisibility or immersiveFlags
                } else {
                    window.decorView.systemUiVisibility and immersiveFlags.inv()
                }
            }
        }
    }

    /// Playing or paused. Pausing keeps the media session and its controls,
    /// so the system player and a headset can resume; `clearPlaybackFromRust`
    /// is what removes them.
    fun setPlaybackActiveFromRust(active: Boolean, title: String) {
        activity.runOnUiThread {
            val wasActive = playbackActive
            playbackActive = active
            if (title.isNotBlank()) playbackTitle = title
            isPlaying = active
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && activity.isInPictureInPictureMode) {
                updatePictureInPictureParams()
            }
            if (active) {
                ensureWindowVisibilityListener()
                ensureMediaSession()
            } else if (wasActive) {
                pendingWebPlaybackResume?.let(activity.window.decorView::removeCallbacks)
                pendingWebPlaybackResume = null
                restoreWebViewWindowVisibility()
            }
            // A pause before anything ever played has no controls to update.
            if (mediaSession != null) publishPlaybackState()
        }
    }

    /// The player went away: drop the session, its notification and the
    /// foreground service.
    fun clearPlaybackFromRust() {
        activity.runOnUiThread {
            val wasActive = playbackActive
            releasePlayback()
            isPlaying = false
            if (wasActive) {
                pendingWebPlaybackResume?.let(activity.window.decorView::removeCallbacks)
                pendingWebPlaybackResume = null
                restoreWebViewWindowVisibility()
            }
        }
    }
}
