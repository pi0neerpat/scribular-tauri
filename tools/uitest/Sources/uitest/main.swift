import AppKit
import ApplicationServices
import CoreGraphics
import Darwin
import Dispatch
import Foundation
import UITestCore

nonisolated(unsafe) private var processRunLock: RunLock?

private final class RunLock {
    private let descriptor: Int32

    private init(descriptor: Int32) { self.descriptor = descriptor }
    deinit { close(descriptor) }

    static func acquire() throws -> RunLock {
        let url = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Library/Caches/scribular-uitest", isDirectory: true)
            .appendingPathComponent("uitest.lock")
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        let descriptor = open(url.path, O_CREAT | O_RDWR, S_IRUSR | S_IWUSR)
        guard descriptor >= 0 else { throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno)) }
        guard flock(descriptor, LOCK_EX | LOCK_NB) == 0 else {
            var buffer = [CChar](repeating: 0, count: 32)
            let count = buffer.withUnsafeMutableBytes { pread(descriptor, $0.baseAddress, $0.count - 1, 0) }
            let pid = count > 0
                ? trimmedRunLockPID(String(decoding: buffer.prefix(Int(count)).prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }, as: UTF8.self))
                : "unknown"
            close(descriptor)
            throw NSError(domain: "uitest.lock", code: 1, userInfo: [NSLocalizedDescriptionKey: "another uitest run is active (pid \(pid))"])
        }
        _ = ftruncate(descriptor, 0)
        _ = lseek(descriptor, 0, SEEK_SET)
        let pid = "\(getpid())\n"
        let written = pid.withCString { write(descriptor, $0, strlen($0)) }
        guard written == pid.utf8.count else {
            close(descriptor)
            throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno))
        }
        return RunLock(descriptor: descriptor)
    }
}

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

    func run(logURL: URL? = nil) {
        lock.lock()
        defer { lock.unlock() }  // a concurrent caller blocks until the first finishes cleaning
        guard !finished else { return }
        finished = true
        player.stop()  // runs at run end (pass or fail) and on SIGINT/SIGTERM, so afplay never outlives uitest
        recorder?.stop(logURL: logURL)
        lifecycle.quit()
    }
}

private func runCLI() -> Int32 {
    setvbuf(stdout, nil, _IOLBF, 0)  // one line per step as it happens, even when piped to a log or agent
    let args = Array(CommandLine.arguments.dropFirst())
    if args.first == "doctor", !FileManager.default.fileExists(atPath: args[0]) { return runDoctor(Array(args.dropFirst())) }
    let cli: UITestOptions
    do { cli = try UITestOptions.parse(args) }
    catch {
        print("usage: uitest <script> [--app <path to .app>] [--record <name>] [--record-dir <dir>] [--pace <duration>]")
        print("argument error: \(error.localizedDescription)")
        return 2
    }
    let runLock: RunLock
    do { runLock = try RunLock.acquire() }
    catch {
        print(error.localizedDescription)
        return 2
    }
    processRunLock = runLock
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
    func finalizeOnce(_ value: String, duration: Double) -> Bool {
        finalizeLock.lock()
        defer { finalizeLock.unlock() }
        guard !finalized else { return true }
        finalized = true
        guard let result = RecordingResult(rawValue: value) else { return false }
        guard let finalizer else {
            if result != .pass { print("recording: no recording") }
            return true
        }
        do {
            if let output = try finalizer.finish(result: result, duration: duration), result != .pass {
                print("recording: \(output.path)")
            } else if result != .pass {
                print("recording: no recording")
            }
            return true
        } catch {
            print("FAIL recording finalization: \(error.localizedDescription)")
            return false
        }
    }
    signal(SIGINT, SIG_IGN)
    signal(SIGTERM, SIG_IGN)
    let signals = [SIGINT, SIGTERM].map { value -> DispatchSourceSignal in
        let source = DispatchSource.makeSignalSource(signal: value, queue: .global())
        source.setEventHandler {
            let duration = Date().timeIntervalSince(start)
            cleanup.run(logURL: finalizer?.logURL(for: .interrupted))
            copyAppLog(to: runDir)
            print(recorder == nil ? "FAIL interrupted — run stopped by signal" : "FAIL interrupted — recording run stopped by signal")
            _ = finalizeOnce("interrupted", duration: duration)
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
    let duration = Date().timeIntervalSince(start)
    cleanup.run(logURL: finalizer?.logURL(for: passed ? .pass : .fail))
    copyAppLog(to: runDir)
    let recordingSaved = finalizeOnce(passed ? "pass" : "fail", duration: duration)
    return passed && recordingSaved ? 0 : 1
}

private func copyAppLog(to runDir: URL) {
    let source = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent("Library/Application Support/com.app.scribular.tauri/logs/scribular.log")
    guard FileManager.default.fileExists(atPath: source.path) else {
        print("app.log not copied: source log not found")
        return
    }
    let destination = runDir.appendingPathComponent("app.log")
    do {
        if FileManager.default.fileExists(atPath: destination.path) {
            try FileManager.default.removeItem(at: destination)
        }
        try FileManager.default.copyItem(at: source, to: destination)
    } catch {
        print("app.log copy failed: \(error.localizedDescription)")
    }
}

private func runDoctor(_ args: [String]) -> Int32 {
    var appPath: String?
    guard args.count <= 2 else {
        print("usage: uitest doctor [--app <path>]")
        return 2
    }
    if !args.isEmpty {
        guard args.count == 2, args[0] == "--app", !args[1].isEmpty else {
            print("usage: uitest doctor [--app <path>]")
            return 2
        }
        appPath = args[1]
    }
    let host = ProcessInfo.processInfo.environment["TERM_PROGRAM"] ?? "the terminal app running uitest"
    var checks = [
        DoctorCheck(name: "Accessibility", fix: "System Settings → Privacy & Security → Accessibility (allow \(host))", passed: AXIsProcessTrusted()),
        DoctorCheck(name: "Screen Recording", fix: "System Settings → Privacy & Security → Screen Recording (allow \(host))", passed: CGPreflightScreenCaptureAccess()),
        DoctorCheck(name: "Automation (System Events)", fix: "System Settings → Privacy & Security → Automation → allow System Events", passed: commandSucceeded("/usr/bin/osascript", ["-e", "tell application \"System Events\" to get name of first process"])),
        DoctorCheck(name: "ffmpeg", fix: "brew install ffmpeg", passed: Preflight.executable("ffmpeg") != nil),
        DoctorCheck(name: "Keychain password", fix: "bash tools/uitest/trust-build.sh --store-password", passed: commandSucceeded("/usr/bin/security", ["find-generic-password", "-s", "scribular-uitest", "-a", "login-password"])),
    ]
    var info = [String]()
    if let appPath {
        let bundleURL = URL(fileURLWithPath: appPath, isDirectory: true)
        let bundle = Bundle(url: bundleURL)
        let bundleExists = bundleURL.pathExtension == "app" && bundle?.executableURL.map { FileManager.default.isExecutableFile(atPath: $0.path) } == true
        checks.append(DoctorCheck(name: "App bundle", fix: "CARGO_TARGET_DIR=<target> APPLE_SIGNING_IDENTITY='Scribular Local Dev' TAURI_SIGNING_PRIVATE_KEY=<key> npm run tauri -- build --bundles app", passed: bundleExists))
        let quotedAppPath = "'\(appPath.replacingOccurrences(of: "'", with: "'\\''"))'"
        info.append("info after each rebuild run: bash tools/uitest/trust-build.sh \(quotedAppPath)")
    }
    let report = DoctorReport(checks: checks, info: info)
    report.lines.forEach { print($0) }
    return report.exitCode
}

private func commandSucceeded(_ path: String, _ arguments: [String]) -> Bool {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: path)
    process.arguments = arguments
    process.standardOutput = FileHandle.nullDevice
    process.standardError = FileHandle.nullDevice
    do { try process.run() } catch { return false }
    process.waitUntilExit()
    return process.terminationStatus == 0
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
