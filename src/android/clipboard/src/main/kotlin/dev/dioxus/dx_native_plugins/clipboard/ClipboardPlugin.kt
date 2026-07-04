package dev.dioxus.dx_native_plugins.clipboard

import android.app.Activity
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent

class ClipboardPlugin(private val activity: Activity) {
    fun copyToClipboardFromRust(text: String) {
        val clipboard = activity.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        clipboard.setPrimaryClip(ClipData.newPlainText("Copied From Greenside Partee", text))
    }

    fun shareFromRust(text: String) {
        val intent = Intent().apply {
            action = Intent.ACTION_SEND
            putExtra(Intent.EXTRA_TEXT, text)
            type = "text/plain"
        }
        activity.startActivity(Intent.createChooser(intent, "Share To:"))
    }
}
