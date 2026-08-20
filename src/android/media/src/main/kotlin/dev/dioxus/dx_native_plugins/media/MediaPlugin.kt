package dev.dioxus.dx_native_plugins.media

import android.app.Activity
import android.app.PendingIntent
import android.app.PictureInPictureParams
import android.app.RemoteAction
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.ActivityInfo
import android.graphics.drawable.Icon
import android.os.Build
import android.util.Rational
import android.view.View
import android.view.ViewGroup
import android.view.WindowInsets
import android.webkit.WebView
import androidx.activity.ComponentActivity

class MediaPlugin(private val activity: Activity) {
    companion object {
        private const val ACTION_PIP_COMMAND =
            "dev.dioxus.dx_native_plugins.media.PIP_COMMAND"
        private const val EXTRA_COMMAND = "command"
        private const val COMMAND_REWIND = "rewind"
        private const val COMMAND_TOGGLE = "toggle"
        private const val COMMAND_FORWARD = "forward"
    }

    private var commandReceiver: BroadcastReceiver? = null
    private var pipListenerRegistered = false
    private var pipWidth = 16
    private var pipHeight = 9
    private var isPlaying = true

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
        val script = if (active) {
            "document.documentElement.dataset.androidPip='true'"
        } else {
            "delete document.documentElement.dataset.androidPip"
        }
        findWebView(activity.window.decorView)?.evaluateJavascript(script, null)
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

    private fun runPictureInPictureCommand(command: String) {
        val webView = findWebView(activity.window.decorView) ?: return
        val script = when (command) {
            COMMAND_REWIND ->
                "(() => { const v=document.querySelector('#tawny-player video');" +
                    "if(!v)return 'missing';v.currentTime=Math.max(0,v.currentTime-10);return 'rewound';})()"
            COMMAND_FORWARD ->
                "(() => { const v=document.querySelector('#tawny-player video');" +
                    "if(!v)return 'missing';const end=Number.isFinite(v.duration)?v.duration:v.currentTime+10;" +
                    "v.currentTime=Math.min(end,v.currentTime+10);return 'forwarded';})()"
            COMMAND_TOGGLE ->
                "(() => { const v=document.querySelector('#tawny-player video');" +
                    "if(!v)return 'missing';if(v.paused){v.play().catch(()=>{});return 'playing';}" +
                    "v.pause();return 'paused';})()"
            else -> return
        }
        webView.evaluateJavascript(script) { result ->
            if (command != COMMAND_TOGGLE) return@evaluateJavascript
            when (result?.trim('"')) {
                "playing" -> isPlaying = true
                "paused" -> isPlaying = false
                else -> return@evaluateJavascript
            }
            updatePictureInPictureParams()
        }
    }

    private fun ensurePictureInPictureCallbacks() {
        if (commandReceiver == null) {
            val receiver = object : BroadcastReceiver() {
                override fun onReceive(context: Context?, intent: Intent?) {
                    runPictureInPictureCommand(intent?.getStringExtra(EXTRA_COMMAND).orEmpty())
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

    fun setPlaybackActiveFromRust(active: Boolean, title: String) {
        activity.runOnUiThread {
            isPlaying = active
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && activity.isInPictureInPictureMode) {
                updatePictureInPictureParams()
            }
            val intent = Intent(activity, PlaybackService::class.java).apply {
                putExtra(PlaybackService.EXTRA_TITLE, title)
            }
            if (active) {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                    activity.startForegroundService(intent)
                } else {
                    activity.startService(intent)
                }
                // Wry pauses the WebView with the Activity. Keeping its media
                // clock resumed lets the foreground media service do its job.
                findWebView(activity.window.decorView)?.let {
                    it.onResume()
                    it.resumeTimers()
                }
            } else {
                activity.stopService(intent)
            }
        }
    }
}
