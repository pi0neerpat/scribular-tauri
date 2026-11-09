import Foundation
import Testing
@testable import UITestCore

@Test func playResolvesRelativeToCwd() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: root.appendingPathComponent("audio"), withIntermediateDirectories: true)
    try Data("RIFF".utf8).write(to: root.appendingPathComponent("audio/a.wav"))

    let url = try resolvePlayPath("audio/a.wav", cwd: root)

    #expect(url.path == root.appendingPathComponent("audio/a.wav").path)
}

@Test func playMissingFileFails() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    do {
        _ = try resolvePlayPath("missing.wav", cwd: root)
        Issue.record("expected a throw")
    } catch {
        guard case .fatal(let message) = error else { Issue.record("expected .fatal"); return }
        #expect(message == "play: file not found: \(root.appendingPathComponent("missing.wav").path)")
    }
}

@Test func loopingPlayFailsWhenPlayerExitsNonzero() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    let audio = root.appendingPathComponent("audio.wav")
    try Data("RIFF\0\0\0\0WAVE".utf8).write(to: audio)
    let player = Player(executableURL: URL(fileURLWithPath: "/usr/bin/false"))
    defer { player.stop() }

    #expect(throws: DriverError.self) { try player.start(audio, loop: true) }
}

@Test func oneShotPlayFailsWhenPlayerExitsNonzero() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    let audio = root.appendingPathComponent("audio.wav")
    try Data("RIFF\0\0\0\0WAVE".utf8).write(to: audio)
    let player = Player(executableURL: URL(fileURLWithPath: "/usr/bin/false"))
    defer { player.stop() }

    #expect(throws: DriverError.self) { try player.start(audio, loop: false) }
}

@Test func playRejectsTruncatedWaveBeforeStartingPlayer() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    let bad = root.appendingPathComponent("bad.wav")
    try Data("RIFF".utf8).write(to: bad)
    let player = Player()
    defer { player.stop() }

    #expect(throws: DriverError.self) { try player.start(bad, loop: true) }
}
