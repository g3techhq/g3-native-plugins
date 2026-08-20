package dev.dioxus.dx_native_plugins.media

import android.app.Activity
import android.app.PictureInPictureParams
import android.content.Intent
import android.content.pm.ActivityInfo
import android.graphics.Rect
import android.os.Build
import android.util.Rational
import android.view.View
import android.view.ViewGroup
import android.view.WindowInsets
import android.webkit.WebView

class MediaPlugin(private val activity: Activity) {
    private fun findWebView(view: View): WebView? {
        if (view is WebView) return view
        if (view is ViewGroup) {
            for (index in 0 until view.childCount) {
                findWebView(view.getChildAt(index))?.let { return it }
            }
        }
        return null
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
        }
    }

    fun enterPictureInPictureFromRust(width: Int, height: Int) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        activity.runOnUiThread {
            val safeWidth = width.coerceAtLeast(1)
            val safeHeight = height.coerceAtLeast(1)
            val ratio = Rational(safeWidth, safeHeight)
            val params = PictureInPictureParams.Builder()
                .setAspectRatio(ratio)
                .setSourceRectHint(Rect(0, 0, safeWidth, safeHeight))
                .build()
            activity.enterPictureInPictureMode(params)
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
