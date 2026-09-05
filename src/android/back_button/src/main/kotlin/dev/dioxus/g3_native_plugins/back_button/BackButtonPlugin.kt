package dev.dioxus.g3_native_plugins.back_button

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
 * The press is forwarded as a DOM event rather than as `history.back()`. On
 * this platform the Rust binary runs outside the WebView and the router keeps
 * its history there, so the WebView's own history is not the app's — calling
 * back on it navigates nothing. Dispatching an event lets the Rust side pick
 * it up over the same bridge it already uses to talk to the page, and act on
 * the history that actually exists.
 *
 * Whether to intercept at all is left to the caller via [setInterceptingFromRust].
 * Only the app knows if there is anywhere to go back to, and a callback that
 * stays enabled at the root would trap the user in the app.
 */
class BackButtonPlugin(private val activity: Activity) {
    companion object {
        /// Named for the crate rather than any one app, since the plugin does
        /// not know who is listening.
        const val BACK_EVENT_SCRIPT =
            "window.dispatchEvent(new Event('g3nativeback', { cancelable: true }))"
    }

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
                webView.evaluateJavascript(BACK_EVENT_SCRIPT, null)
            }
        }
        owner.onBackPressedDispatcher.addCallback(owner, created)
        callback = created
        return created
    }

    fun setInterceptingFromRust(intercepting: Boolean) {
        // Registration as well as mutation belongs on Android's main thread.
        // Rust/Dioxus effects execute on a native worker thread.
        activity.runOnUiThread {
            val callback = ensureCallback() ?: return@runOnUiThread
            callback.isEnabled = intercepting
        }
    }

    fun fallThroughFromRust() {
        activity.runOnUiThread {
            val owner = activity as? ComponentActivity ?: return@runOnUiThread
            val current = callback
            current?.isEnabled = false
            owner.onBackPressedDispatcher.onBackPressed()
            if (!activity.isFinishing && !activity.isDestroyed) {
                current?.isEnabled = true
            }
        }
    }
}
