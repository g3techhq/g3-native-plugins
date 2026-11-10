package dev.dioxus.g3_native_plugins.updater

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
import java.net.HttpURLConnection
import java.net.URI
import org.json.JSONObject

/**
 * The platform half of the app updater: a download folder, an HTTP client,
 * and `PackageInstaller`.
 *
 * Everything that decides anything — which release, and whether its
 * signature holds — is in Rust, where it is the same code on every platform
 * and is tested on the host. This side moves bytes and hands a verified APK
 * to the system.
 *
 * [downloadFromRust] blocks, deliberately. The Rust side only calls it from
 * a worker thread of its own, never from a Dioxus thread or the main thread
 * (where Android would refuse network access outright).
 *
 * Errors come back as a JSON object carrying `error`.
 */
class UpdaterPlugin(private val activity: Activity) {
    companion object {
        private const val ACTION_STATUS = "dev.dioxus.g3_native_plugins.updater.INSTALL_STATUS"

        // Static, so a status that arrives after the Activity was recreated
        // still reaches the plugin Rust polls.
        @Volatile private var status: String = "none"
        @Volatile private var message: String? = null
        @Volatile private var receiverRegistered = false
    }

    // Named errorJson, not error: kotlin.error() throws, and a member named
    // the same would be a very quiet trap for whoever edits this next.
    private fun errorJson(text: String): String = JSONObject().put("error", text).toString()

    /**
     * Under the cache: a downloaded APK is worth nothing once installed, and
     * the system may clear it whenever it likes.
     */
    fun dataDirectoryFromRust(): String {
        val directory = File(activity.cacheDir, "g3_native_plugins/updater")
        directory.mkdirs()
        return directory.absolutePath
    }

    /**
     * Fetch a URL to a file and report the status. The body is only written
     * for a 2xx other than 204, and only ever by renaming a finished partial
     * file into place. The size from the headers goes beside it, as
     * `<destination>.length`, for Rust to report progress against.
     */
    fun downloadFromRust(requestJson: String): String {
        val request = try {
            JSONObject(requestJson)
        } catch (failure: Exception) {
            return errorJson("Could not read the download request: ${failure.message}")
        }
        val destination = File(request.getString("destination"))
        val partial = File(destination.path + ".part")
        val length = File(destination.path + ".length")
        var connection: HttpURLConnection? = null
        return try {
            connection = URI(request.getString("url")).toURL().openConnection() as HttpURLConnection
            val timeout = request.optInt("timeoutMs", 30_000)
            connection.connectTimeout = timeout
            connection.readTimeout = timeout
            // Redirects are followed within a scheme; HttpURLConnection never
            // follows HTTPS down to HTTP, which is the behavior wanted here.
            connection.instanceFollowRedirects = true
            connection.useCaches = false
            request.optJSONObject("headers")?.let { headers ->
                for (name in headers.keys()) {
                    connection.setRequestProperty(name, headers.getString(name))
                }
            }
            val code = connection.responseCode
            if (code in 200..299 && code != 204) {
                destination.parentFile?.mkdirs()
                val size = connection.contentLengthLong
                if (size >= 0) length.writeText(size.toString())
                connection.inputStream.use { input ->
                    partial.outputStream().use { output -> input.copyTo(output) }
                }
                if (destination.exists()) destination.delete()
                if (!partial.renameTo(destination)) {
                    return errorJson("Could not move the download into place")
                }
            }
            JSONObject().put("status", code).toString()
        } catch (failure: Exception) {
            partial.delete()
            errorJson("Download failed: ${failure.message ?: failure.javaClass.simpleName}")
        } finally {
            connection?.disconnect()
        }
    }

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
