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
    private let noTrust: Bool
    private var app: NSRunningApplication?
    private var launched = false
    private var launching: LaunchResult?
    private var dataMoved = false
    private(set) var usesFreshProfile = false
    private var detail: String?

    init(appPath: URL, noTrust: Bool = false) { self.appPath = appPath; self.noTrust = noTrust }

    func launch(keepData: Bool, runDir: URL, unavailableHotkey: Bool = false) throws(DriverError) -> String {
        if !launched {
            let trusted = try trustBuild(runDir: runDir)
            try terminateExisting()
            let home = FileManager.default.homeDirectoryForCurrentUser
            let source = home.appendingPathComponent("Library/Application Support/com.app.scribular.tauri", isDirectory: true)
            let cache = home.appendingPathComponent("Library/Caches/scribular-uitest/models", isDirectory: true)
            if !keepData && !dataMoved {
                // seed the cache from a real models dir first, so 1.85 GB of models never moves into the run folder
                if FileManager.default.fileExists(atPath: source.path) { try linkCache(source, cache) }
                do { _ = try moveDataDir(from: source, intoRunDir: runDir) }
                catch { throw .fatal("could not move app data: \(error.localizedDescription)") }
                dataMoved = true
                usesFreshProfile = true
            }
            try linkCache(source, cache)
            do {
                let result = LaunchResult()
                launching = result
                let completed = DispatchSemaphore(value: 0)
                let configuration = NSWorkspace.OpenConfiguration()
                configuration.createsNewApplicationInstance = true
                // remote-llm test builds send summaries to this host llama-server (README "Host llama-server (VM)").
                if let url = ProcessInfo.processInfo.environment["SCRIBULAR_LLM_URL"], !url.isEmpty {
                    configuration.environment = ["SCRIBULAR_LLM_URL": url]
                }
                if let skip = ProcessInfo.processInfo.environment["SCRIBULAR_SKIP_LICENSE"], !skip.isEmpty {
                    configuration.environment["SCRIBULAR_SKIP_LICENSE"] = skip
                }
                if unavailableHotkey {
                    configuration.environment["SCRIBULAR_UITEST_UNAVAILABLE_HOTKEY"] = "1"
                }
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
            detail = "\(appPath.path) (built \(date))" + (trusted.isEmpty ? "" : "\n  " + trusted.replacingOccurrences(of: "\n", with: "\n  "))
        }
        return detail ?? appPath.path
    }

    func reopen() throws(DriverError) {
        guard let app, !app.isTerminated, !processHasExited(app.processIdentifier) else {
            throw .fatal("reopen-app requires a running app launched by this script")
        }
        guard NSWorkspace.shared.open(appPath) else { throw .fatal("could not reopen the app bundle") }
    }

    /// Runs trust-build.sh so a rebuilt (new cdhash) app reads its keychain key without a prompt.
    private func trustBuild(runDir: URL) throws(DriverError) -> String {
        let log = runDir.appendingPathComponent("trust-build.log")
        func appendLog(_ data: Data) throws(DriverError) {
            do {
                let handle = try FileHandle(forWritingTo: log)
                try handle.seekToEnd()
                try handle.write(contentsOf: data)
                try handle.close()
            } catch { throw .fatal("could not save trust-build.log: \(error.localizedDescription)") }
        }
        if noTrust {
            let message = "trust-build skipped (--no-trust)\n"
            try appendLog(Data(message.utf8))
            return message.trimmingCharacters(in: .newlines)
        }
        let script = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("trust-build.sh")
        let process = Process()
        let pipe = Pipe()
        process.executableURL = URL(fileURLWithPath: "/bin/bash")
        process.arguments = [script.path, appPath.path]
        process.standardOutput = pipe
        process.standardError = pipe
        do { try process.run() } catch { throw .fatal("could not run trust-build.sh: \(error.localizedDescription)") }
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        try appendLog(data)
        let output = String(decoding: data, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
        guard process.terminationStatus == 0 else { throw .fatal("trust-build.sh failed: \(output)") }
        return output
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
        usesFreshProfile = false
    }

    /// Kills the app with no chance to clean up, like a crash, so a recording in progress is left interrupted.
    func forceQuit() {
        guard let running = app ?? launching?.get().0 else { return }
        kill(running.processIdentifier, SIGKILL)
        _ = waitForTermination(running, seconds: 5)
        app = nil
        launching = nil
        launched = false
        usesFreshProfile = false
    }

    private func linkCache(_ appDataDir: URL, _ cache: URL) throws(DriverError) {
        do { try linkModelCache(appDataDir: appDataDir, cache: cache) }
        catch { throw .fatal("could not link model cache: \(error.localizedDescription)") }
    }

    private func terminateExisting() throws(DriverError) {
        guard let existing = NSRunningApplication.runningApplications(withBundleIdentifier: "com.app.scribular.tauri").first else { return }
        existing.terminate()
        if waitForTermination(existing, seconds: 5) { return }
        existing.forceTerminate()
        guard waitForTermination(existing, seconds: 1) else { throw .fatal("could not terminate the existing app") }
    }

    private func waitForTermination(_ app: NSRunningApplication, seconds: Double) -> Bool {
        let pid = app.processIdentifier
        let deadline = Date().addingTimeInterval(seconds)
        // AppKit can cache isTerminated while the caller is waiting outside the main run loop.
        while !app.isTerminated && !processHasExited(pid) && Date() < deadline {
            Thread.sleep(forTimeInterval: 0.05)
        }
        return app.isTerminated || processHasExited(pid)
    }
}
