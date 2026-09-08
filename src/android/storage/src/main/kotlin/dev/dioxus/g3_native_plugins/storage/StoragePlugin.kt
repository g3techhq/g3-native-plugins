package dev.dioxus.g3_native_plugins.storage

import android.app.Activity
import android.content.Context
import android.content.SharedPreferences
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import org.json.JSONArray
import org.json.JSONObject

/**
 * Key-value storage encrypted under a key the device will not hand over.
 *
 * Values are sealed with AES-GCM using a key generated in the AndroidKeyStore,
 * which on most hardware lives in a secure element the app can only ask to
 * perform operations, never to export. The sealed bytes then go into ordinary
 * `SharedPreferences`, which is where `EncryptedSharedPreferences` puts them
 * too — this does the same work without pulling Tink in behind a library Google
 * has stopped maintaining.
 *
 * GCM needs a nonce that is never reused under the same key, so the cipher is
 * asked to generate one per encryption rather than being handed one, and it is
 * stored alongside the ciphertext. Reusing a nonce here would not merely weaken
 * the encryption, it would leak the plaintext.
 *
 * **Key names are stored in the clear.** Only values are encrypted, so a key
 * name is a label, not a place for anything worth hiding.
 *
 * Errors come back as a JSON object carrying `error`, which is how the Rust
 * side tells a failure from a stored value.
 */
class StoragePlugin(private val activity: Activity) {
    companion object {
        private const val PREFERENCES = "g3_native_plugins.storage"
        private const val KEY_ALIAS = "g3_native_plugins.storage.key"
        private const val TRANSFORMATION = "AES/GCM/NoPadding"
        private const val TAG_BITS = 128
        private const val KEYSTORE = "AndroidKeyStore"
    }

    private val preferences: SharedPreferences =
        activity.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)

    private fun secretKey(): SecretKey {
        val store = KeyStore.getInstance(KEYSTORE).apply { load(null) }
        (store.getEntry(KEY_ALIAS, null) as? KeyStore.SecretKeyEntry)?.let {
            return it.secretKey
        }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE)
        generator.init(
            KeyGenParameterSpec.Builder(
                KEY_ALIAS,
                KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
            )
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                // No user authentication requirement: the app must be able to
                // read its own session while the screen is off.
                .setUserAuthenticationRequired(false)
                .build(),
        )
        return generator.generateKey()
    }

    private fun seal(value: String): String {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        // Let the cipher pick the nonce. Supplying one invites reuse, and a
        // reused GCM nonce gives away the plaintext.
        cipher.init(Cipher.ENCRYPT_MODE, secretKey())
        val nonce = cipher.iv
        val sealed = cipher.doFinal(value.toByteArray(Charsets.UTF_8))
        val combined = ByteArray(1 + nonce.size + sealed.size)
        combined[0] = nonce.size.toByte()
        nonce.copyInto(combined, 1)
        sealed.copyInto(combined, 1 + nonce.size)
        return Base64.encodeToString(combined, Base64.NO_WRAP)
    }

    private fun open(stored: String): String {
        val combined = Base64.decode(stored, Base64.NO_WRAP)
        val nonceSize = combined[0].toInt()
        val nonce = combined.copyOfRange(1, 1 + nonceSize)
        val sealed = combined.copyOfRange(1 + nonceSize, combined.size)
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.DECRYPT_MODE, secretKey(), GCMParameterSpec(TAG_BITS, nonce))
        return String(cipher.doFinal(sealed), Charsets.UTF_8)
    }

    // Named errorJson, not error: kotlin.error() throws, and a member named
    // the same would be a very quiet trap for whoever edits this next.
    private fun errorJson(message: String): String =
        JSONObject().put("error", message).toString()

    fun getFromRust(key: String): String? {
        val stored = preferences.getString(key, null) ?: return null
        return try {
            open(stored)
        } catch (failure: Exception) {
            // A value that will not decrypt is not coming back: the Keystore key
            // is gone, which happens when the user adds or removes a device
            // lock. Say so rather than reporting the key as empty.
            errorJson("Stored value for '$key' could not be decrypted: ${failure.message}")
        }
    }

    fun setFromRust(key: String, value: String): String? = try {
        preferences.edit().putString(key, seal(value)).apply()
        null
    } catch (failure: Exception) {
        errorJson("Could not store '$key': ${failure.message}")
    }

    fun removeFromRust(key: String): String? = try {
        preferences.edit().remove(key).apply()
        null
    } catch (failure: Exception) {
        errorJson("Could not remove '$key': ${failure.message}")
    }

    fun clearFromRust(): String? = try {
        preferences.edit().clear().apply()
        null
    } catch (failure: Exception) {
        errorJson("Could not clear storage: ${failure.message}")
    }

    fun keysFromRust(): String = try {
        JSONArray(preferences.all.keys.toList()).toString()
    } catch (failure: Exception) {
        errorJson("Could not list stored keys: ${failure.message}")
    }
}
