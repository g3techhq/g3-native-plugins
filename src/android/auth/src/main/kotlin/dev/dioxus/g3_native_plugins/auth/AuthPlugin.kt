package dev.dioxus.g3_native_plugins.auth

import android.app.Activity
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.credentials.CredentialManager
import androidx.credentials.GetCredentialRequest
import androidx.credentials.exceptions.GetCredentialException
import androidx.lifecycle.lifecycleScope
import com.google.android.libraries.identity.googleid.GetGoogleIdOption
import com.google.android.libraries.identity.googleid.GoogleIdTokenCredential
import kotlinx.coroutines.launch

/**
 * Google sign-in through Credential Manager.
 *
 * The server client id comes from the caller rather than being baked in here.
 * It identifies one project in the Google Cloud console, so a library that
 * carried its own would only ever work for the app it was written for.
 *
 * Credential Manager suspends while its UI is open. The Rust caller therefore
 * starts the flow and polls [getPendingResult] and [getAuthState] instead of
 * blocking a Dioxus worker thread while the user decides what to do.
 */
class AuthPlugin(private val activity: Activity) {
    companion object {
        private const val STATE_IDLE = "idle"
        private const val STATE_AWAITING = "awaiting"
    }

    private val lock = Object()
    private var pendingResult: String? = null
    private var awaiting = false
    private var requestGeneration = 0L

    fun startGoogleAuthFromRust(serverClientId: String) {
        if (serverClientId.isEmpty()) {
            Log.e("Auth", "No server client id was supplied for Google sign-in")
            return
        }
        val componentActivity = activity as? ComponentActivity
        if (componentActivity == null) {
            Log.e("Auth", "Google sign-in requires a ComponentActivity")
            return
        }
        val generation = synchronized(lock) {
            requestGeneration += 1
            pendingResult = null
            awaiting = true
            requestGeneration
        }

        componentActivity.runOnUiThread {
            componentActivity.lifecycleScope.launch {
                val credential = startGoogleAuthAsync(componentActivity, serverClientId)
                synchronized(lock) {
                    // If another request started while this one was open, its
                    // state owns the polling slot and this result is stale.
                    if (generation == requestGeneration) {
                        pendingResult = credential
                        awaiting = false
                    }
                }
            }
        }
    }

    fun getPendingResult(): String? = synchronized(lock) {
        val taken = pendingResult
        pendingResult = null
        taken
    }

    fun getAuthState(): String =
        synchronized(lock) { if (awaiting) STATE_AWAITING else STATE_IDLE }

    private suspend fun startGoogleAuthAsync(
        activity: ComponentActivity,
        serverClientId: String,
    ): String? {
        try {
            val credentialManager = CredentialManager.create(activity)
            val googleIdOption = GetGoogleIdOption.Builder()
                .setFilterByAuthorizedAccounts(false)
                .setServerClientId(serverClientId)
                .build()
            val request = GetCredentialRequest.Builder()
                .addCredentialOption(googleIdOption)
                .build()
            val result = credentialManager.getCredential(
                request = request,
                context = activity,
            )
            val googleIdTokenCredential = GoogleIdTokenCredential.createFrom(result.credential.data)
            return googleIdTokenCredential.idToken
        } catch (e: GetCredentialException) {
            Log.e("Auth", "GetCredentialException: ${e.type}")
            return null
        } catch (e: Exception) {
            Log.e("Auth", "Unexpected error: ${e.message}")
            return null
        }
    }
}
