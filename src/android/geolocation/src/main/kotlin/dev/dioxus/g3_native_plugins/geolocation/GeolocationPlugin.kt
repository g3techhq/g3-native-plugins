package dev.dioxus.g3_native_plugins.geolocation

import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.pm.PackageManager
import android.location.Location
import android.location.LocationListener
import android.location.LocationManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import org.json.JSONObject

/**
 * The device's location, as a request you start and a result you collect.
 *
 * A fix takes seconds and arrives on a callback, so nothing here blocks: the
 * Rust side starts a request and polls for the answer. Blocking the calling
 * thread would stall a Dioxus effect for as long as the fix took, and the
 * listener needs a live Looper to be delivered on anyway.
 *
 * Fixes come from the platform [LocationManager] rather than Google's fused
 * provider. The fused provider gives better results for less battery, but only
 * at the cost of forcing play-services-location on everyone who depends on this
 * crate, which is not a trade a library gets to make for its consumers.
 *
 * Requests listen to every enabled provider at once and take the first fix.
 * See [providers] for why picking only the best-looking one is a trap.
 *
 * The permission itself is declared by the Dioxus CLI from `[permissions]` in
 * `Dioxus.toml`. This asks the user for it at runtime, which Android requires
 * separately from declaring it.
 */
class GeolocationPlugin(private val activity: Activity) {
    companion object {
        private const val REQUEST_CODE = 7301
        private const val STATE_IDLE = "idle"
        private const val STATE_LOCATING = "locating"
    }

    private val manager: LocationManager? =
        activity.getSystemService(Context.LOCATION_SERVICE) as? LocationManager

    private val lock = Object()
    private var locating = false
    private var result: String? = null
    private var listener: LocationListener? = null
    private var timeout: Runnable? = null
    private val handler = Handler(Looper.getMainLooper())

    // ---- Permissions ----

    private fun stateOf(permission: String): String = when {
        activity.checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED -> "granted"
        // Android says to explain first only once the user has already refused,
        // so this doubles as "asked and declined, but not permanently".
        activity.shouldShowRequestPermissionRationale(permission) -> "prompt-with-rationale"
        else -> "prompt"
    }

    fun checkPermissionsFromRust(): String = JSONObject()
        .put("location", stateOf(Manifest.permission.ACCESS_FINE_LOCATION))
        .put("coarseLocation", stateOf(Manifest.permission.ACCESS_COARSE_LOCATION))
        .toString()

    fun requestPermissionsFromRust() {
        activity.runOnUiThread {
            val wanted = arrayOf(
                Manifest.permission.ACCESS_FINE_LOCATION,
                Manifest.permission.ACCESS_COARSE_LOCATION,
            ).filter {
                activity.checkSelfPermission(it) != PackageManager.PERMISSION_GRANTED
            }
            if (wanted.isEmpty()) return@runOnUiThread
            // The result arrives on onRequestPermissionsResult, an Activity
            // callback a library cannot override from outside. The Rust side
            // polls checkPermissions instead, which is cheap and needs no hook.
            activity.requestPermissions(wanted.toTypedArray(), REQUEST_CODE)
        }
    }

    private fun hasAnyLocationPermission(): Boolean =
        activity.checkSelfPermission(Manifest.permission.ACCESS_FINE_LOCATION) ==
            PackageManager.PERMISSION_GRANTED ||
            activity.checkSelfPermission(Manifest.permission.ACCESS_COARSE_LOCATION) ==
            PackageManager.PERMISSION_GRANTED

    // ---- Position ----

    fun startPositionRequestFromRust(optionsJson: String) {
        val options = try {
            JSONObject(optionsJson)
        } catch (error: Exception) {
            JSONObject()
        }
        val highAccuracy = options.optBoolean("enableHighAccuracy", false)
        val timeoutMs = options.optLong("timeout", 10_000L).coerceAtLeast(1L)
        val maximumAgeMs = options.optLong("maximumAge", 0L)

        activity.runOnUiThread {
            // Registration, cancellation and delivery all belong on a thread
            // with a Looper; Rust effects run on a native worker thread.
            cancelInFlight()
            synchronized(lock) {
                result = null
                locating = true
            }

            if (!hasAnyLocationPermission()) {
                finish(errorJson("Location permission has not been granted."))
                return@runOnUiThread
            }
            val manager = manager
            if (manager == null) {
                finish(errorJson("Location services are unavailable."))
                return@runOnUiThread
            }

            cachedLocation(manager, maximumAgeMs)?.let {
                finish(positionJson(it))
                return@runOnUiThread
            }

            val providers = providers(manager, highAccuracy)
            if (providers.isEmpty()) {
                finish(errorJson("No location provider is enabled."))
                return@runOnUiThread
            }

            val created = LocationListener { location ->
                // First fix wins, whichever provider produced it. finish()
                // removes this listener from all of them.
                finish(positionJson(location))
            }
            listener = created
            val expire = Runnable { finish(errorJson("Timed out waiting for a location.")) }
            timeout = expire
            handler.postDelayed(expire, timeoutMs)

            var listening = 0
            var lastFailure: String? = null
            for (provider in providers) {
                try {
                    @Suppress("MissingPermission")
                    manager.requestLocationUpdates(
                        provider,
                        0L,
                        0f,
                        created,
                        Looper.getMainLooper(),
                    )
                    listening += 1
                } catch (error: SecurityException) {
                    lastFailure = "Location permission was revoked: ${error.message}"
                } catch (error: Exception) {
                    lastFailure = "Could not start location updates: ${error.message}"
                }
            }
            if (listening == 0) {
                finish(errorJson(lastFailure ?: "Could not start location updates."))
            }
        }
    }

    fun getLocationStateFromRust(): String =
        synchronized(lock) { if (locating) STATE_LOCATING else STATE_IDLE }

    fun takePositionFromRust(): String? = synchronized(lock) {
        val taken = result
        result = null
        taken
    }

    // ---- Internals, all on the main thread ----

    /** Settle the request, whichever way it went, and stop listening. */
    private fun finish(json: String) {
        cancelInFlight()
        synchronized(lock) {
            result = json
            locating = false
        }
    }

    private fun cancelInFlight() {
        listener?.let { current ->
            try {
                manager?.removeUpdates(current)
            } catch (_: Exception) {
                // Removing an already-removed listener is not worth reporting.
            }
        }
        listener = null
        timeout?.let(handler::removeCallbacks)
        timeout = null
    }

    /**
     * Every provider worth listening to, rather than the single best one.
     *
     * A provider can report itself enabled and still never produce a fix:
     * network location with nothing to work from, or an emulator where only GPS
     * is fed. Betting a whole request on one provider turns that into a timeout
     * while another provider had an answer sitting ready — which is exactly what
     * happened the first time this ran on a device. Registering on all of them
     * and taking whichever answers first costs nothing and cannot do worse.
     *
     * High accuracy is the exception. Asking for GPS-grade precision and then
     * returning a coarse network fix because it arrived first would not be the
     * thing that was asked for, so that case listens to GPS alone.
     */
    private fun providers(manager: LocationManager, highAccuracy: Boolean): List<String> {
        val enabled = listOf(LocationManager.GPS_PROVIDER, LocationManager.NETWORK_PROVIDER)
            .filter { name ->
                try {
                    manager.isProviderEnabled(name)
                } catch (_: Exception) {
                    false
                }
            }
        if (highAccuracy && enabled.contains(LocationManager.GPS_PROVIDER)) {
            return listOf(LocationManager.GPS_PROVIDER)
        }
        return enabled
    }

    private fun cachedLocation(manager: LocationManager, maximumAgeMs: Long): Location? {
        if (maximumAgeMs <= 0L) return null
        val oldestAcceptable = System.currentTimeMillis() - maximumAgeMs
        return manager.allProviders
            .mapNotNull { name ->
                try {
                    @Suppress("MissingPermission")
                    manager.getLastKnownLocation(name)
                } catch (_: Exception) {
                    null
                }
            }
            .filter { it.time >= oldestAcceptable }
            .maxByOrNull { it.time }
    }

    private fun errorJson(message: String): String =
        JSONObject().put("error", message).toString()

    private fun positionJson(location: Location): String {
        val coords = JSONObject()
            .put("latitude", location.latitude)
            .put("longitude", location.longitude)
            .put("accuracy", location.accuracy.toDouble())
        if (location.hasAltitude()) coords.put("altitude", location.altitude)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O &&
            location.hasVerticalAccuracy()
        ) {
            coords.put("altitudeAccuracy", location.verticalAccuracyMeters.toDouble())
        }
        if (location.hasSpeed()) coords.put("speed", location.speed.toDouble())
        if (location.hasBearing()) coords.put("heading", location.bearing.toDouble())
        return JSONObject()
            .put("timestamp", location.time)
            .put("coords", coords)
            .toString()
    }
}
