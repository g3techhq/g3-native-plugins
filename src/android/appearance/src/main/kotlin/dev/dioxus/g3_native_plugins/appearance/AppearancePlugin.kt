package dev.dioxus.g3_native_plugins.appearance

import android.app.Activity
import android.content.res.Configuration
import android.content.res.Resources
import android.os.Build
import android.view.View
import android.view.WindowInsetsController

class AppearancePlugin(private val activity: Activity) {
    /// The system's night mode. Read from the system's own configuration, not
    /// the Activity's: the Activity theme is what the WebView reports, and
    /// AppCompat may override the Activity's `uiMode` to match it.
    fun isDarkFromRust(): String {
        val night = Resources.getSystem().configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK
        return (night == Configuration.UI_MODE_NIGHT_YES).toString()
    }

    /// Dark status- and navigation-bar icons (for a light page) or light ones
    /// (for a dark page). Android 15 draws the app edge to edge, so the page
    /// is what sits behind the bars, and the icon color follows the Activity
    /// theme rather than the page unless it is set here.
    fun setDarkBarIconsFromRust(dark: Boolean) {
        activity.runOnUiThread {
            val window = activity.window
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                val mask = WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS or
                    WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS
                window.insetsController?.setSystemBarsAppearance(if (dark) mask else 0, mask)
            } else {
                val decor = window.decorView
                var flags = View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                    flags = flags or View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR
                }
                @Suppress("DEPRECATION")
                decor.systemUiVisibility = if (dark) {
                    decor.systemUiVisibility or flags
                } else {
                    decor.systemUiVisibility and flags.inv()
                }
            }
        }
    }
}
