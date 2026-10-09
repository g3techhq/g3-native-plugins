package dev.dioxus.g3_native_plugins.appearance

import android.app.Activity
import android.content.res.Configuration
import android.content.res.Resources

@Suppress("UNUSED_PARAMETER")
class AppearancePlugin(activity: Activity) {
    /// The system's night mode. Read from the system's own configuration, not
    /// the Activity's: the Activity theme is what the WebView reports, and
    /// AppCompat may override the Activity's `uiMode` to match it.
    fun isDarkFromRust(): String {
        val night = Resources.getSystem().configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK
        return (night == Configuration.UI_MODE_NIGHT_YES).toString()
    }
}
