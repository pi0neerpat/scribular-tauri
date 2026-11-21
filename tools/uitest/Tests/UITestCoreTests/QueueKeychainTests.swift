import Darwin
import Foundation
import Testing
@testable import UITestCore

@Test func lockWaitsThenAcquiresAndTimesOut() throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }
    let url = directory.appendingPathComponent("uitest.lock")
    let holder = Process()
    let ready = Pipe()
    let release = Pipe()
    holder.executableURL = URL(fileURLWithPath: "/usr/bin/swift")
    holder.arguments = ["-e", """
        import Darwin
        import Foundation
        let fd = open(ProcessInfo.processInfo.environment["UITEST_LOCK_PATH"]!, O_CREAT | O_RDWR, 0o600)
        guard fd >= 0, flock(fd, LOCK_EX) == 0 else { exit(1) }
        let pid = "\\(getpid())\\n"
        _ = pid.withCString { write(fd, $0, strlen($0)) }
        print("ready")
        fflush(stdout)
        _ = FileHandle.standardInput.readDataToEndOfFile()
        """]
    holder.environment = ProcessInfo.processInfo.environment.merging(["UITEST_LOCK_PATH": url.path]) { _, path in path }
    holder.standardInput = release
    holder.standardOutput = ready
    try holder.run()
    defer { if holder.isRunning { holder.terminate() }; holder.waitUntilExit() }
    #expect(String(decoding: ready.fileHandleForReading.availableData, as: UTF8.self).contains("ready"))
    var lines = [String]()
    #expect(throws: NSError.self) { _ = try RunLock.acquire(timeout: 0, label: "0", url: url, out: { lines.append($0) }) }
    #expect(lines.isEmpty)
    do {
        _ = try RunLock.acquire(timeout: 0.1, label: "0.1s", url: url, out: { lines.append($0) })
        Issue.record("expected lock timeout")
    } catch {
        #expect(error.localizedDescription.contains("uitest lock timeout after 0.1s: held by pid \(holder.processIdentifier)"))
    }
    #expect(lines == ["waiting for uitest run pid \(holder.processIdentifier)"])
    try release.fileHandleForWriting.close()
    holder.waitUntilExit()
    _ = try RunLock.acquire(timeout: 3, label: "3s", url: url)
}

@Test func passwordResolutionUsesOverrideThenStoredValue() {
    #expect(resolveKeychainPassword(environment: ["UITEST_KEYCHAIN_PASSWORD": "override"], stored: { Issue.record("should not query keychain"); return nil }) == "override")
    #expect(resolveKeychainPassword(environment: [:], stored: { "stored" }) == "stored")
    #expect(resolveKeychainPassword(environment: ["UITEST_KEYCHAIN_PASSWORD": ""], stored: { "stored" }) == "stored")
    #expect(resolveKeychainPassword(environment: ["UITEST_NO_STORED_PASSWORD": "1"], stored: { Issue.record("should not query keychain"); return nil }) == nil)
    #expect(resolveKeychainPassword(environment: [:], stored: { nil }) == nil)
}

@Test func parsesQueueAndPromptTestOptions() throws {
    let defaults = try UITestOptions.parse(["scenario.uitest"])
    #expect(defaults.lockTimeout == 1800)
    #expect(defaults.lockTimeoutLabel == "30m")
    let explicit = try UITestOptions.parse(["scenario.uitest", "--lock-timeout", "10s", "--no-trust"])
    #expect(explicit.lockTimeout == 10)
    #expect(explicit.noTrust)
    #expect(try UITestOptions.parse(["scenario.uitest", "--lock-timeout", "0"]).lockTimeout == 0)
    #expect(throws: UITestOptionsError.self) { try UITestOptions.parse(["scenario.uitest", "--lock-timeout", "oops"]) }
    do {
        _ = try UITestOptions.parse(["scenario.uitest", "--no-trust", "--no-trust"])
        Issue.record("expected duplicate option")
    } catch let error as UITestOptionsError {
        #expect(error.message == "unexpected or duplicate option: --no-trust")
    }
}
