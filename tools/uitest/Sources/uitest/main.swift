import AppKit
import ApplicationServices
import CoreGraphics
import Darwin
import Dispatch
import Foundation
import UITestCore

nonisolated(unsafe) private var processRunLock: RunLock?

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

private func keychainPassword() -> String? {
    resolveKeychainPassword(environment: ProcessInfo.processInfo.environment) {
        let process = Process()
        let pipe = Pipe()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/security")
        process.arguments = ["find-generic-password", "-s", "scribular-uitest", "-a", "login-password", "-w"]
        process.standardOutput = pipe
        process.standardError = Pipe()
        guard (try? process.run()) != nil else { return nil }
        let output = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else { return nil }
        let password = String(decoding: output, as: UTF8.self).trimmingCharacters(in: .newlines)
        return password.isEmpty ? nil : password
    }
}

private func runCLI() -> Int32 {
    setvbuf(stdout, nil, _IOLBF, 0)  // one line per step as it happens, even when piped to a log or agent
    let args = Array(CommandLine.arguments.dropFirst())
    if args.first == "doctor", !FileManager.default.fileExists(atPath: args[0]) { return runDoctor(Array(args.dropFirst())) }
    let cli: UITestOptions
    do { cli = try UITestOptions.parse(args) }
    catch {
        print("usage: uitest <script> [--app <path to .app>] [--record <name>] [--record-dir <dir>] [--record-window] [--pace <duration>] [--lock-timeout <duration>] [--no-trust]")
        print("argument error: \(error.localizedDescription)")
        return 2
    }
    let runLock: RunLock
    do { runLock = try RunLock.acquire(timeout: cli.lockTimeout, label: cli.lockTimeoutLabel) }
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
        try Data().write(to: runDir.appendingPathComponent("trust-build.log"))
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

    let recorder = finalizer.map { Recorder(output: $0.temporaryURL, windowOnly: cli.recordWindow) }
    if let recorder {
        do { try recorder.start() }
        catch {
            print("preflight error: \(error.localizedDescription)")
            return 2
        }
    }

    let llm = HostLLM.resolve()
    if let line = llm.runLine { print(line) }
    let lifecycle = AppLifecycle(appPath: appPath, noTrust: cli.noTrust, llmURL: llm.url)
    let player = Player()
    let cleanup = Cleanup(lifecycle: lifecycle, recorder: recorder, player: player)
    let appLogBefore = HostLLM.LogMark(at: appLogSource)
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

    let password = keychainPassword()
    let driver = AXDriver(lifecycle: lifecycle, runDir: runDir, keychainPassword: password, player: player)
    let passed = runSteps(steps, driver: driver,
                          options: RunOptions(runDir: runDir, recordingPath: recordingURL?.path ?? "no recording",
                                              secrets: [password].compactMap { $0 },
                                              pace: cli.pace,
                                              onStep: { step, date in finalizer?.record(step, at: date) }),
                          out: { print($0) })
    if recorder != nil { Thread.sleep(forTimeInterval: 3) }
    let duration = Date().timeIntervalSince(start)
    cleanup.run(logURL: finalizer?.logURL(for: passed ? .pass : .fail))
    copyAppLog(to: runDir)
    var llmHonoured = true
    if let url = llm.url {
        if let log = try? Data(contentsOf: runDir.appendingPathComponent("app.log")) {
            let run = HostLLM.currentRun(log, fileID: HostLLM.LogMark(at: appLogSource)?.fileID, since: appLogBefore)
            if !HostLLM.logConfirms(run, url: url) {
                print("FAIL \(HostLLM.ignoredMessage)")
                llmHonoured = false
            }
        } else {
            print("FAIL \(HostLLM.missingLogMessage)")
            llmHonoured = false
        }
    }
    let recordingSaved = finalizeOnce(passed && llmHonoured ? "pass" : "fail", duration: duration)
    return passed && llmHonoured && recordingSaved ? 0 : 1
}

private let appLogSource = FileManager.default.homeDirectoryForCurrentUser
    .appendingPathComponent("Library/Application Support/com.app.scribular.tauri/logs/scribular.log")

private func copyAppLog(to runDir: URL) {
    let source = appLogSource
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
    var args = args[...]
    var noPrompt = false
    var appPath: String?
    while let option = args.popFirst() {
        switch option {
        case "--no-prompt" where !noPrompt:
            noPrompt = true
        case "--app" where appPath == nil && args.first.map({ !$0.isEmpty && !$0.hasPrefix("--") }) == true:
            appPath = args.popFirst()
        default:
            print("usage: uitest doctor [--no-prompt] [--app <path>]")
            return 2
        }
    }
    let probes = DoctorProbes(
        accessibility: { AXIsProcessTrusted() },
        screenRecording: { CGPreflightScreenCaptureAccess() },
        automation: { commandSucceeded("/usr/bin/osascript", ["-e", "tell application \"System Events\" to get name of first process"]) },
        ffmpeg: { Preflight.executable("ffmpeg") != nil },
        keychainPassword: { commandSucceeded("/usr/bin/security", ["find-generic-password", "-s", "scribular-uitest", "-a", "login-password"]) },
        tmuxServer: tmuxServer,
        foreignPrompt: foreignKeychainPrompt
    )
    let appCheck = appPath.map { appPath in
        let bundleURL = URL(fileURLWithPath: appPath, isDirectory: true)
        let bundle = Bundle(url: bundleURL)
        let bundleExists = bundleURL.pathExtension == "app" && bundle?.executableURL.map { FileManager.default.isExecutableFile(atPath: $0.path) } == true
        return DoctorCheck(name: "App bundle", fix: "CARGO_TARGET_DIR=<target> APPLE_SIGNING_IDENTITY='Scribular Local Dev' npm run tauri -- build --debug --bundles app --config tools/uitest/tauri-debug-overlay.json", passed: bundleExists)
    }
    let report = doctorReport(probes: probes, noPrompt: noPrompt, appCheck: appCheck, hostLLM: HostLLM.resolve().doctorLine)
    report.lines.forEach { print($0) }
    if let appPath {
        let quotedAppPath = "'\(appPath.replacingOccurrences(of: "'", with: "'\\''"))'"
        print("info after each rebuild run: bash tools/uitest/trust-build.sh \(quotedAppPath)")
    }
    return report.exitCode
}

/// Label text of an open SecurityAgent window that is not Scribular's; reads AX only, so it raises no prompt.
private func foreignKeychainPrompt() -> String? {
    guard let agent = NSRunningApplication.runningApplications(withBundleIdentifier: "com.apple.SecurityAgent").first else { return nil }
    func value(_ element: AXUIElement, _ name: String) -> CFTypeRef? {
        var result: CFTypeRef?
        return AXUIElementCopyAttributeValue(element, name as CFString, &result) == .success ? result : nil
    }
    func labels(_ element: AXUIElement) -> [String] {
        let own = [kAXTitleAttribute, kAXDescriptionAttribute, kAXValueAttribute].compactMap {
            (value(element, $0) as? String)?.trimmingCharacters(in: .whitespacesAndNewlines)
        }.filter { !$0.isEmpty }
        return own + ((value(element, kAXChildrenAttribute) as? [AXUIElement]) ?? []).flatMap(labels)
    }
    let windows = (value(AXUIElementCreateApplication(agent.processIdentifier), kAXWindowsAttribute) as? [AXUIElement]) ?? []
    return windows.map { labels($0).joined(separator: " ") }.first { DoctorProbes.isForeign(text: $0) }
}

@_silgen_name("responsibility_get_pid_responsible_for_pid")
private func responsiblePID(_ pid: Int32) -> Int32

private func tmuxServer() -> (pid: Int32, responsible: String)? {
    guard ProcessInfo.processInfo.environment["TMUX"] != nil,
          let tmux = Preflight.executable("tmux") else { return nil }
    let process = Process()
    let output = Pipe()
    process.executableURL = tmux
    process.arguments = ["display-message", "-p", "#{pid}"]
    process.standardOutput = output
    process.standardError = FileHandle.nullDevice
    do { try process.run() } catch { return nil }
    let data = output.fileHandleForReading.readDataToEndOfFile()
    process.waitUntilExit()
    guard process.terminationStatus == 0,
          let pid = Int32(String(decoding: data, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)),
          pid > 0 else { return nil }

    let responsible = responsiblePID(pid)
    var buffer = [UInt8](repeating: 0, count: 4096)
    guard responsible > 0, proc_pidpath(responsible, &buffer, UInt32(buffer.count)) > 0 else { return nil }
    return (pid, String(decoding: buffer.prefix { $0 != 0 }, as: UTF8.self))
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
