import AppKit
import ApplicationServices
import Dispatch
import Foundation
import UITestCore

private final class LaunchResult: @unchecked Sendable {
    private let lock = NSLock()
    private var app: NSRunningApplication?
    private var error: Error?

    func set(app: NSRunningApplication?, error: Error?) {
        lock.lock()
        self.app = app
        self.error = error
        lock.unlock()
    }

    func get() -> (NSRunningApplication?, Error?) {
        lock.lock()
        defer { lock.unlock() }
        return (app, error)
    }
}

final class AppLifecycle {
    private let appPath: URL
    private var app: NSRunningApplication?
    private var launched = false
    private var launching: LaunchResult?
    private var dataMoved = false
    private var detail: String?

    init(appPath: URL) { self.appPath = appPath }

    func launch(keepData: Bool, runDir: URL) throws(DriverError) -> String {
        if !launched {
            try terminateExisting()
            if !keepData && !dataMoved {
                let source = FileManager.default.homeDirectoryForCurrentUser
                    .appendingPathComponent("Library/Application Support/com.app.scribular.tauri", isDirectory: true)
                do { _ = try moveDataDir(from: source, intoRunDir: runDir) }
                catch { throw .fatal("could not move app data: \(error.localizedDescription)") }
                dataMoved = true
            }
            do {
                let result = LaunchResult()
                launching = result
                let completed = DispatchSemaphore(value: 0)
                let configuration = NSWorkspace.OpenConfiguration()
                configuration.createsNewApplicationInstance = true
                NSWorkspace.shared.openApplication(at: appPath, configuration: configuration) { application, error in
                    result.set(app: application, error: error)
                    completed.signal()
                }
                let deadline = Date().addingTimeInterval(30)
                var didComplete = false
                while Date() < deadline {
                    if completed.wait(timeout: .now()) == .success { didComplete = true; break }
                    _ = RunLoop.current.run(mode: .default, before: Date().addingTimeInterval(0.05))
                }
                guard didComplete else { throw NSError(domain: "uitest.launch", code: 1, userInfo: [NSLocalizedDescriptionKey: "app launch timed out"]) }
                let (opened, error) = result.get()
                if let error { throw error }
                guard let opened else { throw NSError(domain: "uitest.launch", code: 2, userInfo: [NSLocalizedDescriptionKey: "app did not start"]) }
                app = opened
            } catch {
                throw .fatal("could not launch app: \(error.localizedDescription)")
            }
            launched = true
            let executable = Bundle(url: appPath)?.executableURL ?? appPath
            let built = (try? executable.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? nil
            let date = built.map { ISO8601DateFormatter().string(from: $0) } ?? "unknown"
            detail = "\(appPath.path) (built \(date))"
        }
        return detail ?? appPath.path
    }

    func quit() {
        // a launch interrupted mid-wait has no `app` yet; pick up whatever its completion handler delivered
        guard let running = app ?? launching?.get().0 else { return }
        running.terminate()
        if !waitForTermination(running, seconds: 5) {
            running.forceTerminate()
            _ = waitForTermination(running, seconds: 1)
        }
        app = nil
        launching = nil
        launched = false
    }

    private func terminateExisting() throws(DriverError) {
        guard let existing = NSRunningApplication.runningApplications(withBundleIdentifier: "com.app.scribular.tauri").first else { return }
        existing.terminate()
        if waitForTermination(existing, seconds: 5) { return }
        existing.forceTerminate()
        guard waitForTermination(existing, seconds: 1) else { throw .fatal("could not terminate the existing app") }
    }

    private func waitForTermination(_ app: NSRunningApplication, seconds: Double) -> Bool {
        let deadline = Date().addingTimeInterval(seconds)
        while !app.isTerminated && Date() < deadline { Thread.sleep(forTimeInterval: 0.05) }
        return app.isTerminated
    }
}
