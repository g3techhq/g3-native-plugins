package dev.dioxus.dx_native_plugins.back_button

import android.app.Activity
import android.view.View
import android.view.ViewGroup
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.activity.ComponentActivity

/**
 * Turns the system back gesture into a history navigation instead of an exit.
 *
 * Android delivers back to the Activity, never to the WebView, so a web app
 * hosted this way closes on the first back press no matter what it does in
 * JavaScript. Registering a callback on the dispatcher takes that press before
 * the default handler sees it.
 *
 * The press is forwarded as `history.back()` rather than through a Rust
 * callback, because that is the same event a browser's own back button
 * produces: whatever the app already does for a traversal keeps working, with
 * no second path to maintain.
 *
 * Whether to intercept at all is left to the caller via [setInterceptingFromRust].
 * Only the app knows if there is anywhere to go back to, and a callback that
 * stays enabled at the root would trap the user in the app.
 */
class BackButtonPlugin(private val activity: Activity) {
    private var callback: OnBackPressedCallback? = null

    private fun findWebView(view: View): WebView? {
        if (view is WebView) return view
        if (view is ViewGroup) {
            for (index in 0 until view.childCount) {
                findWebView(view.getChildAt(index))?.let { return it }
            }
        }
        return null
    }

    private fun ensureCallback(): OnBackPressedCallback? {
        val owner = activity as? ComponentActivity ?: return null
        callback?.let { return it }
        val created = object : OnBackPressedCallback(false) {
            override fun handleOnBackPressed() {
                val webView = findWebView(activity.window.decorView)
                if (webView == null) {
                    // Nothing to navigate: step aside so the press behaves the
                    // way the user expects rather than being swallowed.
                    isEnabled = false
                    activity.onBackPressedDispatcher.onBackPressed()
                    return
                }
                webView.evaluateJavascript("window.history.back()", null)
            }
        }
        owner.onBackPressedDispatcher.addCallback(owner, created)
        callback = created
        return created
    }

    fun setInterceptingFromRust(intercepting: Boolean) {
        val callback = ensureCallback() ?: return
        activity.runOnUiThread { callback.isEnabled = intercepting }
    }
}
