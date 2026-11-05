import Foundation
import Testing
@testable import UITestCore

@Test func moveDataDirMovesContents() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    let source = root.appendingPathComponent("source")
    let run = root.appendingPathComponent("run")
    try FileManager.default.createDirectory(at: source, withIntermediateDirectories: true)
    try FileManager.default.createDirectory(at: run, withIntermediateDirectories: true)
    try Data("db".utf8).write(to: source.appendingPathComponent("a.db"))

    let result = try moveDataDir(from: source, intoRunDir: run)

    #expect(result == .moved(to: run.appendingPathComponent("app-data")))
    #expect(FileManager.default.fileExists(atPath: run.appendingPathComponent("app-data/a.db").path))
    #expect(!FileManager.default.fileExists(atPath: source.path))
}

@Test func moveDataDirAbsentIsNoop() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    let run = root.appendingPathComponent("run")
    try FileManager.default.createDirectory(at: run, withIntermediateDirectories: true)

    #expect(try moveDataDir(from: root.appendingPathComponent("missing"), intoRunDir: run) == .absent)
}

@Test func moveDataDirRefusesOverwrite() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    let source = root.appendingPathComponent("source")
    let run = root.appendingPathComponent("run")
    try FileManager.default.createDirectory(at: source, withIntermediateDirectories: true)
    try FileManager.default.createDirectory(at: run.appendingPathComponent("app-data"), withIntermediateDirectories: true)
    try Data("keep".utf8).write(to: source.appendingPathComponent("a.db"))

    #expect(throws: (any Error).self) { try moveDataDir(from: source, intoRunDir: run) }
    #expect(FileManager.default.fileExists(atPath: source.appendingPathComponent("a.db").path))
}
