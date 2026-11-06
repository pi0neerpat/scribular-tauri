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

private func cacheFixture() throws -> (app: URL, cache: URL) {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    return (root.appendingPathComponent("app"), root.appendingPathComponent("cache/models"))
}

@Test func linksFreshDataDir() throws {
    let (app, cache) = try cacheFixture()

    try linkModelCache(appDataDir: app, cache: cache)

    #expect(try FileManager.default.destinationOfSymbolicLink(atPath: app.appendingPathComponent("models").path) == cache.path)
}

@Test func seedsCacheFromExistingModels() throws {
    let (app, cache) = try cacheFixture()
    let models = app.appendingPathComponent("models")
    try FileManager.default.createDirectory(at: models, withIntermediateDirectories: true)
    try Data("weights".utf8).write(to: models.appendingPathComponent("w.bin"))

    try linkModelCache(appDataDir: app, cache: cache)

    #expect(try Data(contentsOf: cache.appendingPathComponent("w.bin")) == Data("weights".utf8))
    #expect(try FileManager.default.destinationOfSymbolicLink(atPath: models.path) == cache.path)
}

@Test func linkIsIdempotent() throws {
    let (app, cache) = try cacheFixture()

    try linkModelCache(appDataDir: app, cache: cache)
    try linkModelCache(appDataDir: app, cache: cache)

    #expect(try FileManager.default.destinationOfSymbolicLink(atPath: app.appendingPathComponent("models").path) == cache.path)
}

@Test func linkKeepsCacheContents() throws {
    let (app, cache) = try cacheFixture()
    let models = app.appendingPathComponent("models")
    try FileManager.default.createDirectory(at: models, withIntermediateDirectories: true)
    try FileManager.default.createDirectory(at: cache, withIntermediateDirectories: true)
    try Data("old".utf8).write(to: cache.appendingPathComponent("w.bin"))
    try Data("new".utf8).write(to: models.appendingPathComponent("w.bin"))

    try linkModelCache(appDataDir: app, cache: cache)

    #expect(try Data(contentsOf: cache.appendingPathComponent("w.bin")) == Data("old".utf8))
}
