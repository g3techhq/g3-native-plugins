import Foundation
import UIKit

/**
 * The platform half of the app updater: a download folder, an HTTP client,
 * and opening the install link.
 *
 * Everything that decides anything — which release, and whether its signed
 * install manifest holds — is in Rust, where it is the same code on every
 * platform and is tested on the host. This side moves bytes and opens URLs.
 *
 * `downloadFromRust` blocks, deliberately, and it is safe to: the Rust side
 * only calls it from the updater's own worker thread, and URLSession delivers
 * the completion on its own delegate queue, never the thread that is waiting,
 * so the wait cannot deadlock whichever thread it is on.
 *
 * Errors come back as a JSON object carrying `error`.
 */
@objc(UpdaterPlugin)
public class UpdaterPlugin: NSObject {

    /// No cache and no cookies: an update check must see the server's answer
    /// now, not one an intermediate cache kept.
    private static let session: URLSession = {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.requestCachePolicy = .reloadIgnoringLocalCacheData
        return URLSession(configuration: configuration)
    }()

    private static func json(_ object: [String: Any]) -> String {
        guard let data = try? JSONSerialization.data(withJSONObject: object),
              let text = String(data: data, encoding: .utf8) else {
            return "{\"error\":\"The updater could not encode its answer.\"}"
        }
        return text
    }

    private static func errorJson(_ message: String) -> String {
        json(["error": message])
    }

    /// Under Caches, which is never backed up and which iOS may clear: what is
    /// downloaded here is only needed until the install starts.
    @objc
    public func dataDirectoryFromRust() -> String? {
        guard let caches = FileManager.default.urls(
            for: .cachesDirectory,
            in: .userDomainMask
        ).first else { return nil }
        let directory = caches
            .appendingPathComponent("g3_native_plugins", isDirectory: true)
            .appendingPathComponent("updater", isDirectory: true)
        do {
            try FileManager.default.createDirectory(
                at: directory,
                withIntermediateDirectories: true
            )
        } catch {
            NSLog("[UpdaterPlugin] could not prepare the download directory: \(error)")
            return nil
        }
        return directory.path
    }

    /// Open the install link (`itms-services://`) or store page. iOS shows
    /// its own install prompt or the store; nothing comes back to wait for.
    /// Returns an error message, or nil once the open is on its way.
    @objc
    public func openUrlFromRust(_ text: String) -> String? {
        guard let url = URL(string: text) else {
            return "The install link is not a valid URL."
        }
        DispatchQueue.main.async {
            UIApplication.shared.open(url, options: [:]) { opened in
                if !opened {
                    NSLog("[UpdaterPlugin] iOS would not open the install link")
                }
            }
        }
        return nil
    }

    /// Fetch a URL to a file and report the status. The body is only moved to
    /// the destination for a 2xx other than 204. URLSession downloads to a
    /// temporary file of its own and deletes it once the completion returns,
    /// so the move has to happen inside the completion.
    @objc
    public func downloadFromRust(_ requestJson: String) -> String? {
        guard let data = requestJson.data(using: .utf8),
              let request = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let urlText = request["url"] as? String,
              let url = URL(string: urlText),
              let destinationPath = request["destination"] as? String else {
            return Self.errorJson("Could not read the download request.")
        }
        let destination = URL(fileURLWithPath: destinationPath)
        let timeout = (request["timeoutMs"] as? Double).map { $0 / 1000 } ?? 30
        var urlRequest = URLRequest(url: url, timeoutInterval: timeout)
        for (name, value) in request["headers"] as? [String: String] ?? [:] {
            urlRequest.setValue(value, forHTTPHeaderField: name)
        }

        let done = DispatchSemaphore(value: 0)
        // A box rather than a captured `var`: the completion handler is
        // @Sendable in current SDKs, and mutating a captured local from one is
        // a compile error. The semaphore orders the write before the read.
        let answer = Answer(Self.errorJson("The download did not finish."))
        let task = Self.session.downloadTask(with: urlRequest) { location, response, error in
            defer { done.signal() }
            if let error = error {
                answer.value = Self.errorJson("Download failed: \(error.localizedDescription)")
                return
            }
            guard let http = response as? HTTPURLResponse else {
                answer.value = Self.errorJson("The server's answer was not HTTP.")
                return
            }
            let status = http.statusCode
            if (200..<300).contains(status), status != 204, let location = location {
                do {
                    let manager = FileManager.default
                    try manager.createDirectory(
                        at: destination.deletingLastPathComponent(),
                        withIntermediateDirectories: true
                    )
                    if manager.fileExists(atPath: destination.path) {
                        try manager.removeItem(at: destination)
                    }
                    try manager.moveItem(at: location, to: destination)
                } catch {
                    answer.value = Self.errorJson("Could not move the download into place: \(error)")
                    return
                }
            }
            answer.value = Self.json(["status": status])
        }
        task.resume()
        done.wait()
        return answer.value
    }
}

private final class Answer {
    var value: String
    init(_ value: String) { self.value = value }
}
