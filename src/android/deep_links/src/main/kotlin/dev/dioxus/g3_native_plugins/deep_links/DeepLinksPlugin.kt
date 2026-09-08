package dev.dioxus.g3_native_plugins.deep_links

import android.app.Activity
import android.content.Intent
import androidx.activity.ComponentActivity
import java.util.ArrayDeque

/**
 * Collects the URLs an App Link or custom scheme launches the app with.
 *
 * Android delivers a link as an `ACTION_VIEW` Intent, in one of two ways
 * depending on whether the app was already running: as the Activity's launch
 * Intent on a cold start, or through `onNewIntent` on a warm one. Neither
 * reaches the WebView, and `onNewIntent` is a lifecycle method a library cannot
 * override from outside, so both are picked up here instead — the launch Intent
 * directly, warm links through `ComponentActivity`'s new-intent listener.
 *
 * Links are queued rather than pushed. A link can arrive before the app has
 * rendered anything that could receive it, and unlike a Back press it is a
 * value that must not be dropped, so the Rust side drains the queue when it is
 * ready. Nothing here is thrown away for want of a listener.
 *
 * The queue is static: a link belongs to the app, not to whichever instance of
 * this plugin happened to exist when it arrived. That lets the host call
 * [prepareFromRust] early, before its plugin provider exists, without the
 * result being stranded on a discarded instance.
 *
 * For warm links to arrive at all, the host Activity needs a launch mode that
 * reuses the existing instance — `singleTask` or `singleTop`. Under the default
 * `standard` mode Android starts a second Activity instead of calling
 * `onNewIntent`, and this plugin sees the link as that Activity's launch Intent.
 */
class DeepLinksPlugin(private val activity: Activity) {
    companion object {
        private val pending = ArrayDeque<String>()
        private var listenerRegistered = false
        private var launchIntentConsumed = false

        private fun enqueue(intent: Intent?) {
            if (intent == null || intent.action != Intent.ACTION_VIEW) return
            val url = intent.dataString ?: return
            synchronized(pending) { pending.addLast(url) }
        }

        private fun take(): String? = synchronized(pending) { pending.pollFirst() }
    }

    fun prepareFromRust() {
        // Intent state and listener registration belong on Android's main
        // thread. Rust/Dioxus effects execute on a native worker thread.
        activity.runOnUiThread {
            if (!launchIntentConsumed) {
                // Read once per process. The launch Intent stays on the
                // Activity for its whole life, so a second read would hand the
                // same link back after the app had already acted on it.
                launchIntentConsumed = true
                enqueue(activity.intent)
            }
            val owner = activity as? ComponentActivity ?: return@runOnUiThread
            if (listenerRegistered) return@runOnUiThread
            owner.addOnNewIntentListener { intent -> enqueue(intent) }
            listenerRegistered = true
        }
    }

    /** The oldest link not yet handed to Rust, or null when the queue is empty. */
    fun takeLinkFromRust(): String? = take()
}
