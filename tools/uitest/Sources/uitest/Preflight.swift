import ApplicationServices
import CoreGraphics
import Foundation

struct PreflightError: Error {
    let message: String
}

enum Preflight {
    static func check(appPath: URL) throws {
        // the permission belongs to the app hosting the terminal, not to the uitest binary
        let host = ProcessInfo.processInfo.environment["TERM_PROGRAM"] ?? "the terminal app running uitest"
        guard AXIsProcessTrusted() else {
            throw PreflightError(message: "grant Accessibility to \(host) in System Settings → Privacy & Security → Accessibility")
        }
        guard CGPreflightScreenCaptureAccess() else {
            throw PreflightError(message: "grant Screen Recording to \(host) in System Settings → Privacy & Security → Screen Recording")
        }
        guard executable("ffmpeg") != nil else { throw PreflightError(message: "install ffmpeg and ensure it is on PATH") }
        guard FileManager.default.fileExists(atPath: appPath.path) else {
            throw PreflightError(message: "build first: APPLE_SIGNING_IDENTITY=\"Scribular Local Dev\" npm run tauri -- build --bundles app")
        }
        let session = CGSessionCopyCurrentDictionary() as? [String: Any]
        let locked = session?["CGSSessionScreenIsLocked"] as? Bool ?? false
        guard !locked, CGDisplayIsAsleep(CGMainDisplayID()) == 0 else {
            throw PreflightError(message: "unlock the display")
        }
    }

    static func executable(_ name: String) -> URL? {
        for directory in ProcessInfo.processInfo.environment["PATH", default: ""].split(separator: ":") {
            let path = URL(fileURLWithPath: String(directory)).appendingPathComponent(name)
            if FileManager.default.isExecutableFile(atPath: path.path) { return path }
        }
        return nil
    }
}
