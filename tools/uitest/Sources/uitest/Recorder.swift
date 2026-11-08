import Foundation
import Darwin
import UITestCore

final class Recorder {
    private struct Marker: Codable {
        let pid: Int32
        let parentPID: Int32
        let executablePath: String?
        let outputPath: String
    }
    private let process = Process()
    private let input = Pipe()
    private let finished = DispatchSemaphore(value: 0)
    private let output: URL
    private let marker: URL

    private static func screenIndex() -> String {
        let probe = Process()
        let pipe = Pipe()
        guard let ffmpeg = Preflight.executable("ffmpeg") else { return "0" }
        probe.executableURL = ffmpeg
        probe.arguments = ["-f", "avfoundation", "-list_devices", "true", "-i", ""]
        probe.standardError = pipe
        probe.standardOutput = FileHandle.nullDevice
        guard (try? probe.run()) != nil else { return "0" }
        let text = String(decoding: pipe.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
        for line in text.split(separator: "\n") where line.contains("Capture screen") {
            let ids = line.split(separator: "[").dropFirst().compactMap { $0.split(separator: "]").first }.filter { Int($0) != nil }
            if let id = ids.last { return String(id) }
        }
        return "0"
    }

    init(output: URL) {
        self.output = output
        self.marker = output.deletingLastPathComponent().appendingPathComponent(".uitest-recorder-\(UUID().uuidString).json")
        process.executableURL = Preflight.executable("ffmpeg")
        process.arguments = ["-n", "-f", "avfoundation", "-framerate", "15", "-capture_cursor", "1", "-i", "\(Recorder.screenIndex()):none",
                             // libx264 yuv420p needs even dimensions; some displays (e.g. 1495x1297 VMs) are odd
                             "-vf", "crop=trunc(iw/2)*2:trunc(ih/2)*2", "-pix_fmt", "yuv420p",
                             // fragmented MP4 stays playable if uitest or ffmpeg dies without finalizing
                             "-movflags", "+frag_keyframe+empty_moov", output.path]
        process.standardInput = input
        process.standardOutput = FileHandle.nullDevice
        let log = output.deletingPathExtension().appendingPathExtension("ffmpeg.log")
        FileManager.default.createFile(atPath: log.path, contents: nil)
        process.standardError = (try? FileHandle(forWritingTo: log)) ?? FileHandle.nullDevice
        process.terminationHandler = { [finished] _ in finished.signal() }
    }

    func start() throws {
        guard process.executableURL != nil else { throw NSError(domain: "uitest.recorder", code: 1, userInfo: [NSLocalizedDescriptionKey: "ffmpeg is not on PATH"]) }
        try process.run()
        Thread.sleep(forTimeInterval: 0.25)
        guard process.isRunning else { throw NSError(domain: "uitest.recorder", code: Int(process.terminationStatus), userInfo: [NSLocalizedDescriptionKey: "ffmpeg could not start screen recording"]) }
        let data = try JSONEncoder().encode(Marker(pid: process.processIdentifier, parentPID: getpid(),
                                                  executablePath: process.executableURL?.resolvingSymlinksInPath().standardizedFileURL.path,
                                                  outputPath: output.standardizedFileURL.path))
        do { try data.write(to: marker, options: .atomic) }
        catch {
            process.terminate()
            _ = finished.wait(timeout: .now() + 2)
            throw error
        }
    }

    func stop(logURL: URL? = nil) {
        guard process.isRunning else {
            try? FileManager.default.removeItem(at: marker)
            let logPath = logURL?.path ?? output.deletingPathExtension().appendingPathExtension("ffmpeg.log").path
            FileHandle.standardError.write(Data("warning: ffmpeg exited early (status \(process.terminationStatus)); recording may be incomplete, see \(logPath)\n".utf8))
            return
        }
        try? input.fileHandleForWriting.write(contentsOf: Data("q\n".utf8))
        try? input.fileHandleForWriting.close()
        if finished.wait(timeout: .now() + 10) == .timedOut {
            process.terminate()
            if finished.wait(timeout: .now() + 2) == .timedOut {
                _ = kill(process.processIdentifier, SIGKILL)
                _ = finished.wait(timeout: .now() + 2)
            }
        }
        if !process.isRunning { try? FileManager.default.removeItem(at: marker) }
    }

    static func reapOrphans(in roots: [URL]) {
        let manager = FileManager.default
        let roots = roots.map { $0.standardizedFileURL }
        for root in roots where manager.fileExists(atPath: root.path) {
            let children = (try? manager.contentsOfDirectory(at: root, includingPropertiesForKeys: [.isDirectoryKey])) ?? []
            let folders = [root] + children.filter { (try? $0.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true }
            for folder in folders {
                let markers = (try? manager.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil))?.filter { $0.lastPathComponent.hasPrefix(".uitest-recorder-") && $0.pathExtension == "json" } ?? []
                for file in markers {
                    guard let data = try? Data(contentsOf: file), let item = try? JSONDecoder().decode(Marker.self, from: data),
                          roots.contains(where: { item.outputPath.hasPrefix($0.path + "/") }) else { continue }
                    let parentExited = kill(item.parentPID, 0) == -1 && errno == ESRCH
                    guard parentExited else { continue }
                    let recorderExists = kill(item.pid, 0) == 0 || errno != ESRCH
                    if shouldRemoveRecorderMarker(parentExited: parentExited, recorderExists: recorderExists) {
                        try? manager.removeItem(at: file)
                        continue
                    }
                    guard let arguments = processArguments(item.pid), let executable = processExecutable(item.pid) else { continue }
                    guard let expected = Preflight.executable("ffmpeg"),
                          recorderProcessMatches(executable: executable, arguments: arguments,
                                                 expectedExecutable: item.executablePath ?? expected.resolvingSymlinksInPath().standardizedFileURL.path,
                                                 outputPath: item.outputPath) else {
                        try? manager.removeItem(at: file)
                        continue
                    }
                    _ = kill(item.pid, SIGTERM)
                    let deadline = Date().addingTimeInterval(2)
                    while Date() < deadline && kill(item.pid, 0) == 0 { Thread.sleep(forTimeInterval: 0.05) }
                    if kill(item.pid, 0) == 0 {
                        _ = kill(item.pid, SIGKILL)
                        let killDeadline = Date().addingTimeInterval(2)
                        while Date() < killDeadline && kill(item.pid, 0) == 0 { Thread.sleep(forTimeInterval: 0.05) }
                    }
                    if kill(item.pid, 0) == -1, errno == ESRCH { try? manager.removeItem(at: file) }
                }
            }
        }
    }

    private static func processArguments(_ pid: Int32) -> [String]? {
        var mib: [Int32] = [CTL_KERN, KERN_PROCARGS2, pid]
        var size = 0
        guard mib.withUnsafeMutableBufferPointer({ sysctl($0.baseAddress, u_int($0.count), nil, &size, nil, 0) }) == 0,
              size > MemoryLayout<Int32>.size else { return nil }
        var bytes = [UInt8](repeating: 0, count: size)
        guard mib.withUnsafeMutableBufferPointer({ mib in
            bytes.withUnsafeMutableBytes { sysctl(mib.baseAddress, u_int(mib.count), $0.baseAddress, &size, nil, 0) }
        }) == 0 else { return nil }

        var argc: Int32 = 0
        withUnsafeMutableBytes(of: &argc) { target in
            bytes.withUnsafeBytes { source in target.copyMemory(from: UnsafeRawBufferPointer(rebasing: source[..<MemoryLayout<Int32>.size])) }
        }
        guard argc > 0, argc < 256 else { return nil }
        var offset = MemoryLayout<Int32>.size
        func readString() -> String? {
            guard offset < size else { return nil }
            let start = offset
            while offset < size, bytes[offset] != 0 { offset += 1 }
            guard offset < size else { return nil }
            defer { offset += 1 }
            return String(decoding: bytes[start..<offset], as: UTF8.self)
        }
        guard readString() != nil else { return nil } // executable path before argv
        while offset < size, bytes[offset] == 0 { offset += 1 }
        var arguments = [String]()
        for _ in 0..<argc {
            guard let argument = readString() else { return nil }
            arguments.append(argument)
        }
        return arguments
    }

    private static func processExecutable(_ pid: Int32) -> String? {
        var buffer = [CChar](repeating: 0, count: 4096)
        let count = proc_pidpath(pid, &buffer, UInt32(buffer.count))
        guard count > 0 else { return nil }
        let path = String(decoding: buffer.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }, as: UTF8.self)
        return URL(fileURLWithPath: path).resolvingSymlinksInPath().standardizedFileURL.path
    }
}
