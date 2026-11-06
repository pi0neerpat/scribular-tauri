import Foundation

/// Resolves a `play` path against the directory uitest runs from; a missing file fails the step now,
/// not later as an empty transcript that looks like a capture bug.
public func resolvePlayPath(_ path: String, cwd: URL) throws(DriverError) -> URL {
    // not URL(relativeTo:): a cwd without a trailing slash would lose its last component
    let url = (path.hasPrefix("/") ? URL(fileURLWithPath: path) : cwd.appendingPathComponent(path)).standardizedFileURL
    guard FileManager.default.fileExists(atPath: url.path) else { throw .fatal("play: file not found: \(url.path)") }
    return url
}
