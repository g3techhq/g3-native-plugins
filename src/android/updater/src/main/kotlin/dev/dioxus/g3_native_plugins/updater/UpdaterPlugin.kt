package dev.dioxus.g3_native_plugins.updater

import android.app.Activity
import android.content.Intent
import android.net.Uri
import java.io.File
import java.net.HttpURLConnection
import java.net.URI
import org.json.JSONObject

/**
 * The platform half of the app updater: a download folder, an HTTP client,
 * and opening a store page. Installing an APK is [UpdaterInstallPlugin]'s, in
 * the `updater_install` module, so a Play build never declares the install
 * permission.
 *
 * Everything that decides anything — which release, and whether its
 * signature holds — is in Rust, where it is the same code on every platform
 * and is tested on the host. This side moves bytes.
 *
 * [downloadFromRust] blocks, deliberately. The Rust side only calls it from
 * a worker thread of its own, never from a Dioxus thread or the main thread
 * (where Android would refuse network access outright).
 *
 * Errors come back as a JSON object carrying `error`.
 */
class UpdaterPlugin(private val activity: Activity) {
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

    /**
     * Open a store page (Google Play) or any link with whatever the system
     * has for it, for an update this app does not install itself.
     */
    fun openUrlFromRust(url: String): String = try {
        val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        activity.runOnUiThread { activity.startActivity(intent) }
        "{}"
    } catch (failure: Exception) {
        errorJson("Could not open ${url}: ${failure.message ?: failure.javaClass.simpleName}")
    }
}
