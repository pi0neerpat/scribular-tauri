import AppKit
import Darwin
import Dispatch
import Foundation
import UITestCore

private final class Cleanup {
    private let lifecycle: AppLifecycle
    private let recorder: Recorder?
    private let player: Player
    private let lock = NSLock()
    private var finished = false

    init(lifecycle: AppLifecycle, recorder: Recorder?, player: Player) {
        self.lifecycle = lifecycle
        self.recorder = recorder
        self.player = player
    }

    func run() {
        lock.lock()
        defer { lock.unlock() }  // a concurrent caller blocks until the first finishes cleaning
        guard !finished else { return }
        finished = true
        player.stop()  // runs at run end (pass or fail) and on SIGINT/SIGTERM, so afplay never outlives uitest
        recorder?.stop()
        lifecycle.quit()
    }
}

private func runCLI() -> Int32 {
    setvbuf(stdout, nil, _IOLBF, 0)  // one line per step as it happens, even when piped to a log or agent
    let args = Array(CommandLine.arguments.dropFirst())
    let cli: UITestOptions
    do { cli = try UITestOptions.parse(args) }
    catch {
        print("usage: uitest <script> [--app <path to .app>] [--record <name>] [--record-dir <dir>] [--pace <duration>]")
        print("argument error: \(error.localizedDescription)")
        return 2
    }
    var appPath = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
        .appendingPathComponent("src-tauri/target/release/bundle/macos/Scribular.app")
    if let value = cli.appPath { appPath = URL(fileURLWithPath: value) }

    let steps: [Step]
    do {
        let source = try String(contentsOf: URL(fileURLWithPath: cli.scriptPath), encoding: .utf8)
        steps = try parseScript(source)
    } catch let error as ScriptError {
        print("parse error line \(error.line): \(error.message)")
        return 2
    } catch {
        print("parse error: \(error.localizedDescription)")
        return 2
    }

    let stamp = DateFormatter()
    stamp.dateFormat = "yyyy-MM-dd-HHmmss-SSS"
    let runName = stamp.string(from: Date())
    let runDir = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
        .appendingPathComponent("runs/\(runName)", isDirectory: true)
    let recordingsBase = recordingDirectory(cli.recordingDirectory)
    Recorder.reapOrphans(in: [URL(fileURLWithPath: FileManager.default.currentDirectoryPath).appendingPathComponent("runs", isDirectory: true), recordingsBase])
    let finalizer = cli.recordingName.map {
        RecordingFinalizer(name: $0, directory: recordingsBase,
                           scenario: URL(fileURLWithPath: cli.scriptPath).lastPathComponent,
                           appBuildTime: appBuildTime(appPath), stepCount: steps.count)
    }
    let recordingURL = finalizer?.recordingURL
    do {
        try FileManager.default.createDirectory(at: runDir, withIntermediateDirectories: true)
        if finalizer != nil {
            try FileManager.default.createDirectory(at: recordingsBase, withIntermediateDirectories: true)
            try finalizer?.validateAvailable()
        }
        try Preflight.check(appPath: appPath)
    } catch let error as PreflightError {
        print("preflight error: \(error.message)")
        return 2
    } catch {
        print("preflight error: \(error.localizedDescription)")
        return 2
    }

    let recorder = finalizer.map { Recorder(output: $0.temporaryURL) }
    if let recorder {
        do { try recorder.start() }
        catch {
            print("preflight error: \(error.localizedDescription)")
            return 2
        }
    }

    let lifecycle = AppLifecycle(appPath: appPath)
    let player = Player()
    let cleanup = Cleanup(lifecycle: lifecycle, recorder: recorder, player: player)
    let start = Date()
    let finalizeLock = NSLock()
    var finalized = false
    func finalizeOnce(_ result: String) {
        finalizeLock.lock()
        defer { finalizeLock.unlock() }
        guard !finalized else { return }
        finalized = true
        guard let result = RecordingResult(rawValue: result) else { return }
        guard let finalizer else {
            if result != .pass { print("recording: no recording") }
            return
        }
        do {
            if let output = try finalizer.finish(result: result, duration: Date().timeIntervalSince(start)), result != .pass {
                print("recording: \(output.path)")
            } else if result != .pass {
                print("recording: no recording")
            }
        } catch { print("warning: could not finalize recording: \(error.localizedDescription)") }
    }
    signal(SIGINT, SIG_IGN)
    signal(SIGTERM, SIG_IGN)
    let signals = [SIGINT, SIGTERM].map { value -> DispatchSourceSignal in
        let source = DispatchSource.makeSignalSource(signal: value, queue: .global())
        source.setEventHandler {
            cleanup.run()
            print("FAIL interrupted — recording run stopped by signal")
            finalizeOnce("interrupted")
            Darwin.exit(value == SIGINT ? 130 : 143)
        }
        source.resume()
        return source
    }
    defer {
        signals.forEach { $0.cancel() }
        cleanup.run()
    }

    let driver = AXDriver(lifecycle: lifecycle, runDir: runDir, keychainPassword: ProcessInfo.processInfo.environment["UITEST_KEYCHAIN_PASSWORD"], player: player)
    let passed = runSteps(steps, driver: driver,
                          options: RunOptions(runDir: runDir, recordingPath: recordingURL?.path ?? "no recording",
                                              secrets: [ProcessInfo.processInfo.environment["UITEST_KEYCHAIN_PASSWORD"]].compactMap { $0 },
                                              pace: cli.pace,
                                              onStep: { step, date in finalizer?.record(step, at: date) }),
                          out: { print($0) })
    if recorder != nil { Thread.sleep(forTimeInterval: 3) }
    cleanup.run()
    finalizeOnce(passed ? "pass" : "fail")
    return passed ? 0 : 1
}

private func appBuildTime(_ appPath: URL) -> String {
    let executable = Bundle(url: appPath)?.executableURL ?? appPath
    let date = (try? executable.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? nil
    return date.map { ISO8601DateFormatter().string(from: $0) } ?? "unknown"
}

private func recordingDirectory(_ requested: String?) -> URL {
    if let requested { return URL(fileURLWithPath: requested, relativeTo: URL(fileURLWithPath: FileManager.default.currentDirectoryPath)).standardizedFileURL }
    if let environment = ProcessInfo.processInfo.environment["CWF_RECORDINGS_DIR"], !environment.isEmpty {
        return URL(fileURLWithPath: environment, relativeTo: URL(fileURLWithPath: FileManager.default.currentDirectoryPath)).standardizedFileURL
    }
    let cwd = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
    let git = Process()
    git.executableURL = URL(fileURLWithPath: "/usr/bin/git")
    git.arguments = ["-C", cwd.path, "remote"]
    git.standardOutput = Pipe()
    git.standardError = FileHandle.nullDevice
    if (try? git.run()) != nil {
        git.waitUntilExit()
        let remotes = (git.standardOutput as? Pipe).map { String(decoding: $0.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self) } ?? ""
        let primary = cwd.deletingLastPathComponent().appendingPathComponent("scribular-tauri/runs", isDirectory: true)
        if !remotes.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
           FileManager.default.fileExists(atPath: primary.deletingLastPathComponent().appendingPathComponent(".git").path) { return primary }
    }
    return cwd.appendingPathComponent("runs", isDirectory: true)
}

Darwin.exit(runCLI())
