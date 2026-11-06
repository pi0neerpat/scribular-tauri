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
