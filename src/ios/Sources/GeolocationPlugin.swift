import CoreLocation
import Foundation

/**
 * The device's location, as a request you start and a result you collect.
 *
 * Nothing here blocks. `CLLocationManager` delivers to its delegate on the main
 * queue, so a call that waited for a fix on the calling thread would wait for
 * something that can never arrive there — the Rust side starts a request and
 * polls for the answer instead.
 *
 * The permission itself is declared by the Dioxus CLI from `[permissions]` in
 * `Dioxus.toml`, which writes `NSLocationWhenInUseUsageDescription`. Without
 * that string in the Info.plist iOS refuses the request outright rather than
 * showing a prompt.
 */
@objc(GeolocationPlugin)
public class GeolocationPlugin: NSObject, CLLocationManagerDelegate {

    private static let stateIdle = "idle"
    private static let stateLocating = "locating"

    private let lock = NSLock()
    private var locating = false
    private var result: String?

    /// Read from any thread, written only on the main one. `CLLocationManager`
    /// is main-thread state, and a synchronous hop from the Rust thread to read
    /// it could deadlock against a main thread waiting on that same call, so
    /// the status is mirrored here instead. The first poll after launch can see
    /// the pre-launch default; the second sees the truth.
    private var cachedStatus: CLAuthorizationStatus = .notDetermined

    private var manager: CLLocationManager?
    private var timeoutWork: DispatchWorkItem?
    private var pendingHighAccuracy = false

    override public init() {
        super.init()
        // Get the manager and its authorization status on screen early, so the
        // first permission poll from Rust already has a real answer.
        DispatchQueue.main.async { self.ensureManager() }
    }

    // MARK: - Permissions

    @objc
    public func checkPermissionsFromRust() -> String {
        let state: String
        switch cachedStatus {
        case .notDetermined: state = "prompt"
        case .restricted, .denied: state = "denied"
        case .authorizedAlways, .authorizedWhenInUse: state = "granted"
        @unknown default: state = "prompt"
        }
        // iOS grants location as one thing; it has no separate coarse grant the
        // way Android does, so both aliases report the same state.
        let payload = ["location": state, "coarseLocation": state]
        guard let data = try? JSONSerialization.data(withJSONObject: payload),
              let json = String(data: data, encoding: .utf8) else {
            return "{\"location\":\"prompt\",\"coarseLocation\":\"prompt\"}"
        }
        return json
    }

    @objc
    public func requestPermissionsFromRust() {
        DispatchQueue.main.async {
            guard let manager = self.ensureManager() else { return }
            // iOS shows this once per install. After a refusal it does nothing
            // at all, which is why the Rust side treats denied as final.
            manager.requestWhenInUseAuthorization()
        }
    }

    // MARK: - Position

    @objc
    public func startPositionRequestFromRust(_ optionsJson: String) {
        var highAccuracy = false
        var timeoutMs: Double = 10_000
        var maximumAgeMs: Double = 0
        if let data = optionsJson.data(using: .utf8),
           let options = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] {
            highAccuracy = options["enableHighAccuracy"] as? Bool ?? false
            timeoutMs = options["timeout"] as? Double ?? 10_000
            maximumAgeMs = options["maximumAge"] as? Double ?? 0
        }

        DispatchQueue.main.async {
            self.cancelInFlight()
            self.lock.lock()
            self.result = nil
            self.locating = true
            self.lock.unlock()

            guard let manager = self.ensureManager() else {
                self.finish(Self.errorJson("Location services are unavailable."))
                return
            }

            if maximumAgeMs > 0, let cached = manager.location,
               Date().timeIntervalSince(cached.timestamp) * 1000 <= maximumAgeMs {
                self.finish(Self.positionJson(cached))
                return
            }

            manager.desiredAccuracy = highAccuracy
                ? kCLLocationAccuracyBest
                : kCLLocationAccuracyHundredMeters

            let expire = DispatchWorkItem { [weak self] in
                self?.finish(Self.errorJson("Timed out waiting for a location."))
            }
            self.timeoutWork = expire
            DispatchQueue.main.asyncAfter(
                deadline: .now() + max(timeoutMs / 1000, 0.1),
                execute: expire
            )

            if self.cachedStatus == .notDetermined {
                // Asking now and requesting on the authorization callback,
                // rather than firing a request iOS would silently drop.
                self.pendingHighAccuracy = true
                manager.requestWhenInUseAuthorization()
                return
            }
            guard self.cachedStatus == .authorizedAlways
                || self.cachedStatus == .authorizedWhenInUse else {
                self.finish(Self.errorJson("Location permission has not been granted."))
                return
            }
            manager.requestLocation()
        }
    }

    @objc
    public func getLocationStateFromRust() -> String {
        lock.lock()
        defer { lock.unlock() }
        return locating ? Self.stateLocating : Self.stateIdle
    }

    @objc
    public func takePositionFromRust() -> String? {
        lock.lock()
        defer { lock.unlock() }
        let taken = result
        result = nil
        return taken
    }

    // MARK: - Delegate

    public func locationManager(
        _ manager: CLLocationManager,
        didUpdateLocations locations: [CLLocation]
    ) {
        guard let location = locations.last else { return }
        finish(Self.positionJson(location))
    }

    public func locationManager(_ manager: CLLocationManager, didFailWithError error: Error) {
        finish(Self.errorJson("Could not get a location: \(error.localizedDescription)"))
    }

    public func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        cachedStatus = manager.authorizationStatus
        guard pendingHighAccuracy else { return }
        switch cachedStatus {
        case .notDetermined:
            // Still on screen; wait for the user to answer.
            return
        case .authorizedAlways, .authorizedWhenInUse:
            pendingHighAccuracy = false
            manager.requestLocation()
        default:
            pendingHighAccuracy = false
            finish(Self.errorJson("Location permission was refused."))
        }
    }

    // MARK: - Internals, all on the main thread

    @discardableResult
    private func ensureManager() -> CLLocationManager? {
        if let existing = manager {
            cachedStatus = existing.authorizationStatus
            return existing
        }
        let created = CLLocationManager()
        created.delegate = self
        manager = created
        cachedStatus = created.authorizationStatus
        return created
    }

    /// Settle the request, whichever way it went, and stop listening.
    private func finish(_ json: String) {
        cancelInFlight()
        lock.lock()
        result = json
        locating = false
        lock.unlock()
    }

    private func cancelInFlight() {
        timeoutWork?.cancel()
        timeoutWork = nil
        pendingHighAccuracy = false
    }

    private static func errorJson(_ message: String) -> String {
        guard let data = try? JSONSerialization.data(withJSONObject: ["error": message]),
              let json = String(data: data, encoding: .utf8) else {
            return "{\"error\":\"Could not get a location.\"}"
        }
        return json
    }

    private static func positionJson(_ location: CLLocation) -> String {
        var coords: [String: Any] = [
            "latitude": location.coordinate.latitude,
            "longitude": location.coordinate.longitude,
            // A negative accuracy means the reading is invalid rather than
            // very precise, so report it as unknown rather than as a number.
            "accuracy": max(location.horizontalAccuracy, 0),
        ]
        if location.verticalAccuracy >= 0 {
            coords["altitude"] = location.altitude
            coords["altitudeAccuracy"] = location.verticalAccuracy
        }
        if location.speed >= 0 {
            coords["speed"] = location.speed
        }
        if location.course >= 0 {
            coords["heading"] = location.course
        }
        let payload: [String: Any] = [
            "timestamp": Int(location.timestamp.timeIntervalSince1970 * 1000),
            "coords": coords,
        ]
        guard let data = try? JSONSerialization.data(withJSONObject: payload),
              let json = String(data: data, encoding: .utf8) else {
            return errorJson("Could not encode the location.")
        }
        return json
    }
}
