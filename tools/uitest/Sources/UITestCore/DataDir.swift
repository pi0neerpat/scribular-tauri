import Foundation

public enum DataDirMove: Equatable {
    case moved(to: URL)
    case absent
}

public func moveDataDir(from source: URL, intoRunDir runDir: URL) throws -> DataDirMove {
    guard FileManager.default.fileExists(atPath: source.path) else { return .absent }
    let destination = runDir.appendingPathComponent("app-data", isDirectory: true)
    guard !FileManager.default.fileExists(atPath: destination.path) else {
        throw CocoaError(.fileWriteFileExists, userInfo: [NSFilePathErrorKey: destination.path])
    }
    try FileManager.default.moveItem(at: source, to: destination)
    return .moved(to: destination)
}

/// Points `appDataDir/models` at a persistent cache so runs never re-download models.
/// A real `models` dir seeds the cache first; files already in the cache always win and are never touched.
public func linkModelCache(appDataDir: URL, cache: URL) throws {
    let fm = FileManager.default
    let models = appDataDir.appendingPathComponent("models")
    try fm.createDirectory(at: cache, withIntermediateDirectories: true)
    // attributesOfItem does not follow links, unlike fileExists
    if let type = try? fm.attributesOfItem(atPath: models.path)[.type] as? FileAttributeType {
        if type == .typeSymbolicLink {
            if try fm.destinationOfSymbolicLink(atPath: models.path) == cache.path { return }
            try fm.removeItem(at: models)
        } else {
            for name in try fm.contentsOfDirectory(atPath: models.path)
            where !fm.fileExists(atPath: cache.appendingPathComponent(name).path) {
                try fm.moveItem(at: models.appendingPathComponent(name), to: cache.appendingPathComponent(name))
            }
            try fm.removeItem(at: models)
        }
    }
    try fm.createDirectory(at: appDataDir, withIntermediateDirectories: true)
    try fm.createSymbolicLink(at: models, withDestinationURL: cache)
}
