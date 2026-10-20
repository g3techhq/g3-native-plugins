package dev.dioxus.g3_native_plugins.app_info

import android.app.Activity
import android.os.Build
import org.json.JSONObject

class AppInfoPlugin(private val activity: Activity) {
    @Suppress("DEPRECATION")
    fun appInfoFromRust(): String? {
        return try {
            val info = activity.packageManager.getPackageInfo(activity.packageName, 0)
            val version = info.versionName ?: return null
            val code = if (Build.VERSION.SDK_INT >= 28) info.longVersionCode else info.versionCode.toLong()
            JSONObject().put("identifier", activity.packageName)
                .put("version", version).put("build", code.toString()).toString()
        } catch (_: Exception) {
            null
        }
    }
}
