package dev.dioxus.g3_native_plugins.external_url

import android.app.Activity
import android.content.Intent
import android.net.Uri

class ExternalUrlPlugin(private val activity: Activity) {
    fun openExternalUrlFromRust(url: String) {
        activity.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
    }
}
