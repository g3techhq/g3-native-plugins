package dev.dioxus.g3_native_plugins.safe_area

import android.app.Activity
import android.os.Build
import android.view.View
import android.view.ViewGroup
import android.view.WindowInsets
import android.view.WindowInsetsController
import android.webkit.WebView

/**
 * Publishes the system bars' insets to the page as CSS custom properties.
 *
 * Android 15 draws apps edge to edge, so the status bar, a camera cutout and
 * the gesture or button bar sit over the page. A page is meant to read those
 * through `env(safe-area-inset-*)`, but Android WebViews before Chrome's
 * edge-to-edge support report every one as 0. This reads the insets from the
 * window instead and sets `--g3-safe-area-top`, `-right`, `-bottom` and
 * `-left` on the root element, in CSS pixels, whenever they change. g3-ui
 * prefers those variables and falls back to `env()`.
 *
 * Hidden bars report no inset, so a page in immersive mode fills the screen.
 */
class SafeAreaPlugin(private val activity: Activity) {
    private var last: String? = null

    fun startFromRust() {
        activity.runOnUiThread {
            val webView = findWebView(activity.window.decorView) ?: return@runOnUiThread
            webView.setOnApplyWindowInsetsListener { view, insets ->
                publish(view as WebView, insets)
                // The WebView's own handling still runs: a WebView that does
                // support `env()` keeps working, and the variables win anyway.
                view.onApplyWindowInsets(insets)
            }
            // Publish what applies now, then again on every change. Forget the
            // last value first, so a page that reloaded gets it again.
            last = null
            webView.requestApplyInsets()
        }
    }

    /**
     * Hides or shows the status and navigation bars. Hidden, a swipe from the
     * edge shows them for a moment; the page's insets drop to 0 meanwhile.
     */
    fun setBarsHiddenFromRust(hidden: Boolean) {
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
                window.decorView.systemUiVisibility = if (hidden) {
                    View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY or
                        View.SYSTEM_UI_FLAG_FULLSCREEN or
                        View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                } else {
                    View.SYSTEM_UI_FLAG_VISIBLE
                }
            }
        }
    }

    private fun publish(webView: WebView, insets: WindowInsets) {
        val (top, right, bottom, left) = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val bars = insets.getInsets(
                WindowInsets.Type.systemBars() or WindowInsets.Type.displayCutout()
            )
            listOf(bars.top, bars.right, bars.bottom, bars.left)
        } else {
            @Suppress("DEPRECATION")
            listOf(
                insets.systemWindowInsetTop,
                insets.systemWindowInsetRight,
                insets.systemWindowInsetBottom,
                insets.systemWindowInsetLeft,
            )
        }
        val density = activity.resources.displayMetrics.density
        fun css(px: Int) = "${(px / density).toInt()}px"
        val script = "(function(s){" +
            "s.setProperty('--g3-safe-area-top','${css(top)}');" +
            "s.setProperty('--g3-safe-area-right','${css(right)}');" +
            "s.setProperty('--g3-safe-area-bottom','${css(bottom)}');" +
            "s.setProperty('--g3-safe-area-left','${css(left)}');" +
            "})(document.documentElement.style)"
        if (script == last) return
        last = script
        webView.evaluateJavascript(script, null)
    }

    private fun findWebView(view: View): WebView? {
        if (view is WebView) return view
        if (view is ViewGroup) {
            for (index in 0 until view.childCount) {
                findWebView(view.getChildAt(index))?.let { return it }
            }
        }
        return null
    }
}
