package dev.dioxus.g3_native_plugins.updater_install

import android.app.Activity
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageInstaller
import android.net.Uri
import android.os.Build
import android.provider.Settings
import java.io.File
import org.json.JSONObject

/**
 * Installing an APK the updater downloaded and Rust verified, through
 * `PackageInstaller`.
 *
 * Its own module because of its permissions: `REQUEST_INSTALL_PACKAGES` is
 * one Google Play restricts, so only an app built with `updater-install`
 * (one distributed outside Play) gets this module and its manifest.
 *
 * Errors come back as a JSON object carrying `error`.
 */
class UpdaterInstallPlugin(private val activity: Activity) {
    companion object {
        private const val ACTION_STATUS = "dev.dioxus.g3_native_plugins.updater.INSTALL_STATUS"

        // Static, so a status that arrives after the Activity was recreated
        // still reaches the plugin Rust polls.
        @Volatile private var status: String = "none"
        @Volatile private var message: String? = null
        @Volatile private var receiverRegistered = false
    }


    private fun errorJson(text: String): String = JSONObject().put("error", text).toString()

    /** Whether the user has let this app install apps. */
    fun canInstallFromRust(): String =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            activity.packageManager.canRequestPackageInstalls().toString()
        } else {
            "true"
        }

    /** The system screen where the user lets this app install apps. */
    fun openInstallSettingsFromRust() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val intent = Intent(
            Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
            Uri.parse("package:${activity.packageName}"),
        )
        activity.runOnUiThread { activity.startActivity(intent) }
    }

    /**
     * Hand the APK, already verified by Rust, to the system installer. The
     * outcome arrives later as a broadcast; Rust polls [installStatusFromRust].
     */
    fun installFromRust(path: String): String {
        val file = File(path)
        if (!file.isFile) return errorJson("The downloaded APK is missing")
        return try {
            registerReceiver()
            val installer = activity.packageManager.packageInstaller
            val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL)
            params.setAppPackageName(activity.packageName)
            params.setSize(file.length())
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                // Honoured only when the app updates itself, declares
                // UPDATE_PACKAGES_WITHOUT_USER_ACTION and targets a recent
                // SDK; otherwise the system asks the user, which the receiver
                // handles.
                params.setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_NOT_REQUIRED)
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                params.setPackageSource(PackageInstaller.PACKAGE_SOURCE_DOWNLOADED_FILE)
            }
            val sessionId = installer.createSession(params)
            installer.openSession(sessionId).use { session ->
                file.inputStream().use { input ->
                    session.openWrite("base.apk", 0, file.length()).use { output ->
                        input.copyTo(output)
                        session.fsync(output)
                    }
                }
                // Explicit to this package, which Android 14 requires of a
                // mutable PendingIntent; mutable so the system can add the
                // status extras.
                val intent = Intent(ACTION_STATUS).setPackage(activity.packageName)
                var flags = PendingIntent.FLAG_UPDATE_CURRENT
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) flags = flags or PendingIntent.FLAG_MUTABLE
                val pending = PendingIntent.getBroadcast(activity, sessionId, intent, flags)
                status = "pending"
                message = null
                session.commit(pending.intentSender)
            }
            // The session holds its own copy now.
            file.delete()
            "{}"
        } catch (failure: Exception) {
            status = "none"
            errorJson("Could not start the install: ${failure.message ?: failure.javaClass.simpleName}")
        }
    }

    fun installStatusFromRust(): String {
        val answer = JSONObject().put("status", status)
        message?.let { answer.put("message", it) }
        return answer.toString()
    }

    private fun registerReceiver() {
        if (receiverRegistered) return
        val receiver = object : BroadcastReceiver() {
            override fun onReceive(context: Context, intent: Intent) {
                when (val code = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, -999)) {
                    PackageInstaller.STATUS_PENDING_USER_ACTION -> {
                        val confirm = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                            intent.getParcelableExtra(Intent.EXTRA_INTENT, Intent::class.java)
                        } else {
                            @Suppress("DEPRECATION")
                            intent.getParcelableExtra(Intent.EXTRA_INTENT)
                        }
                        if (confirm == null) {
                            status = "failed"
                            message = "The system asked to confirm the install but gave no screen to do it"
                        } else {
                            confirm.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                            context.startActivity(confirm)
                        }
                    }
                    PackageInstaller.STATUS_SUCCESS -> status = "success"
                    PackageInstaller.STATUS_FAILURE_ABORTED -> status = "aborted"
                    else -> {
                        status = "failed"
                        message = intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE)
                            ?: "The install failed (status $code)"
                    }
                }
            }
        }
        val filter = IntentFilter(ACTION_STATUS)
        val context = activity.applicationContext
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            context.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED)
        } else {
            @Suppress("UnspecifiedRegisterReceiverFlag")
            context.registerReceiver(receiver, filter)
        }
        receiverRegistered = true
    }
}
