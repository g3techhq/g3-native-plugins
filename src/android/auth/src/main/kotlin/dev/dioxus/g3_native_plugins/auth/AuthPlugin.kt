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
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.launch

class AuthPlugin(private val activity: Activity) {
    fun startGoogleAuthFromRust(): String? {
        val componentActivity = activity as? ComponentActivity ?: return null
        val latch = CountDownLatch(1)
        var credential: String? = null

        componentActivity.runOnUiThread {
            componentActivity.lifecycleScope.launch {
                credential = startGoogleAuthAsync(componentActivity)
                latch.countDown()
            }
        }

        latch.await(2, TimeUnit.MINUTES)
        return credential
    }

    private suspend fun startGoogleAuthAsync(activity: ComponentActivity): String? {
        try {
            val credentialManager = CredentialManager.create(activity)
            val googleIdOption = GetGoogleIdOption.Builder()
                .setFilterByAuthorizedAccounts(false)
                .setServerClientId("670494631267-ig0mogebnadg4badthak02cilpb57ufv.apps.googleusercontent.com")
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
