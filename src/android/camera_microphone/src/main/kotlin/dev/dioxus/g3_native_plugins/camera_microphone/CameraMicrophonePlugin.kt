package dev.dioxus.g3_native_plugins.camera_microphone

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.provider.Settings
import org.json.JSONObject

/**
 * The permission state around `getUserMedia`, not a capture API of its own.
 *
 * Capture itself already works: wry's `WebChromeClient` handles the WebView's
 * `onPermissionRequest` and asks Android for `CAMERA` and `RECORD_AUDIO` when
 * the page calls `getUserMedia`. Nothing here touches that client, because
 * replacing it would take wry's file chooser and JS dialogs with it.
 *
 * What this adds is what the page cannot reach: reading the state before
 * capture is attempted, prompting at a moment the app chooses rather than
 * mid-stream, and opening the Settings page — the only way back once the user
 * has refused twice and Android stops showing the dialog at all.
 *
 * The permissions themselves are declared by the Dioxus CLI from
 * `[permissions]` in `Dioxus.toml`. Without the manifest entry Android refuses
 * immediately and no prompt is ever shown.
 */
class CameraMicrophonePlugin(private val activity: Activity) {
    companion object {
        private const val REQUEST_CAMERA = 7401
        private const val REQUEST_MICROPHONE = 7402
    }

    private fun stateOf(permission: String): String = when {
        activity.checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED -> "granted"
        // Android raises this only after a refusal, so it doubles as "asked and
        // declined once, and asking again will still show the dialog".
        activity.shouldShowRequestPermissionRationale(permission) -> "prompt-with-rationale"
        else -> "prompt"
    }

    /**
     * Android cannot distinguish "never asked" from "refused for good": both
     * report denied with no rationale wanted. The two are told apart by
     * remembering that a request was made, so a second look after a refusal
     * reports denied rather than inviting a prompt that will not appear.
     */
    private var askedCamera = false
    private var askedMicrophone = false

    private fun reportedState(permission: String, asked: Boolean): String {
        val state = stateOf(permission)
        return if (state == "prompt" && asked) "denied" else state
    }

    fun checkPermissionsFromRust(): String = JSONObject()
        .put("camera", reportedState(Manifest.permission.CAMERA, askedCamera))
        .put("microphone", reportedState(Manifest.permission.RECORD_AUDIO, askedMicrophone))
        .toString()

    fun requestCameraFromRust() {
        request(Manifest.permission.CAMERA, REQUEST_CAMERA)
        askedCamera = true
    }

    fun requestMicrophoneFromRust() {
        // MODIFY_AUDIO_SETTINGS goes with it, matching what wry asks for when
        // the page requests audio capture, so the two paths agree.
        request(Manifest.permission.RECORD_AUDIO, REQUEST_MICROPHONE)
        askedMicrophone = true
    }

    private fun request(permission: String, code: Int) {
        activity.runOnUiThread {
            if (activity.checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED) {
                return@runOnUiThread
            }
            val permissions = if (permission == Manifest.permission.RECORD_AUDIO) {
                arrayOf(permission, Manifest.permission.MODIFY_AUDIO_SETTINGS)
            } else {
                arrayOf(permission)
            }
            // The answer lands on onRequestPermissionsResult, an Activity
            // callback a library cannot override from outside, so the Rust side
            // polls checkPermissions instead of waiting on a hook.
            activity.requestPermissions(permissions, code)
        }
    }

    fun openSettingsFromRust() {
        activity.runOnUiThread {
            val intent = Intent(
                Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
                Uri.fromParts("package", activity.packageName, null),
            ).apply { addFlags(Intent.FLAG_ACTIVITY_NEW_TASK) }
            try {
                activity.startActivity(intent)
            } catch (_: Exception) {
                // A device with no settings activity to open is not something
                // the app can do anything about.
            }
        }
    }

    /** No-op: Android has no audio session to rearrange for capture. */
    fun setCapturingFromRust(capturing: Boolean) = Unit
}
