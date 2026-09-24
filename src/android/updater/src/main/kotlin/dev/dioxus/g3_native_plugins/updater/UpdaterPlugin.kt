package dev.dioxus.g3_native_plugins.updater

import android.app.Activity
import android.content.pm.PackageManager
import android.os.Build
import java.io.File
import java.net.HttpURLConnection
import java.net.URI
import org.json.JSONObject

/**
 * The platform half of the over-the-air updater: where bundles live, what the
 * app's version is, and an HTTP client.
 *
 * Everything that decides anything — signature and hash checks, staging,
 * install and rollback — is in Rust, where it is the same code on both
 * platforms and is tested on the host. This side only moves bytes.
 *
 * [downloadFromRust] blocks, deliberately. The Rust side only calls it from
 * the updater's own worker thread, never from a Dioxus thread or the main
 * thread (where Android would refuse network access outright), and a blocking
 * call keeps the whole pipeline one straight line of code over there.
 *
 * Errors come back as a JSON object carrying `error`.
 */
class UpdaterPlugin(private val activity: Activity) {
    // Named errorJson, not error: kotlin.error() throws, and a member named
    // the same would be a very quiet trap for whoever edits this next.
    private fun errorJson(message: String): String =
        JSONObject().put("error", message).toString()

    /**
     * Under `noBackupFilesDir`: a bundle is a cache of something the server
     * can always send again, and restoring one onto a device with a different
     * app build would only have it thrown away as the wrong runtime.
     */
    fun dataDirectoryFromRust(): String {
        val directory = File(activity.noBackupFilesDir, "g3_native_plugins/updater")
        directory.mkdirs()
        return directory.absolutePath
    }

    fun appVersionFromRust(): String? = try {
        val info = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            activity.packageManager.getPackageInfo(
                activity.packageName,
                PackageManager.PackageInfoFlags.of(0),
            )
        } else {
            @Suppress("DEPRECATION")
            activity.packageManager.getPackageInfo(activity.packageName, 0)
        }
        info.versionName
    } catch (_: Exception) {
        null
    }

    /**
     * Fetch a URL to a file and report the status. The body is only written
     * for a 2xx other than 204, and only ever by renaming a finished temporary
     * file into place, so a half-downloaded file never sits at the
     * destination looking complete.
     */
    fun downloadFromRust(requestJson: String): String {
        val request = try {
            JSONObject(requestJson)
        } catch (failure: Exception) {
            return errorJson("Could not read the download request: ${failure.message}")
        }
        val destination = File(request.getString("destination"))
        val partial = File(destination.path + ".part")
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
            val status = connection.responseCode
            if (status in 200..299 && status != 204) {
                destination.parentFile?.mkdirs()
                connection.inputStream.use { input ->
                    partial.outputStream().use { output -> input.copyTo(output) }
                }
                if (destination.exists()) destination.delete()
                if (!partial.renameTo(destination)) {
                    return errorJson("Could not move the download into place")
                }
            }
            JSONObject().put("status", status).toString()
        } catch (failure: Exception) {
            partial.delete()
            errorJson("Download failed: ${failure.message ?: failure.javaClass.simpleName}")
        } finally {
            connection?.disconnect()
        }
    }
}
