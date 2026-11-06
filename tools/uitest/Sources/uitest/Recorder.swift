import Foundation

final class Recorder {
    private let process = Process()
    private let input = Pipe()
    private let finished = DispatchSemaphore(value: 0)

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
        process.executableURL = Preflight.executable("ffmpeg")
        process.arguments = ["-f", "avfoundation", "-framerate", "15", "-capture_cursor", "1", "-i", "\(Recorder.screenIndex()):none",
                             // libx264 yuv420p needs even dimensions; some displays (e.g. 1495x1297 VMs) are odd
                             "-vf", "crop=trunc(iw/2)*2:trunc(ih/2)*2", "-pix_fmt", "yuv420p",
                             // fragmented MP4 stays playable if uitest or ffmpeg dies without finalizing
                             "-movflags", "+frag_keyframe+empty_moov", output.path]
        process.standardInput = input
        process.standardOutput = FileHandle.nullDevice
        let log = output.deletingLastPathComponent().appendingPathComponent("ffmpeg.log")
        FileManager.default.createFile(atPath: log.path, contents: nil)
        process.standardError = (try? FileHandle(forWritingTo: log)) ?? FileHandle.nullDevice
        process.terminationHandler = { [finished] _ in finished.signal() }
    }

    func start() throws {
        guard process.executableURL != nil else { throw NSError(domain: "uitest.recorder", code: 1, userInfo: [NSLocalizedDescriptionKey: "ffmpeg is not on PATH"]) }
        try process.run()
        Thread.sleep(forTimeInterval: 0.25)
        guard process.isRunning else { throw NSError(domain: "uitest.recorder", code: Int(process.terminationStatus), userInfo: [NSLocalizedDescriptionKey: "ffmpeg could not start screen recording"]) }
    }

    func stop() {
        guard process.isRunning else {
            FileHandle.standardError.write(Data("warning: ffmpeg exited early (status \(process.terminationStatus)); recording may be incomplete, see ffmpeg.log in the run folder\n".utf8))
            return
        }
        try? input.fileHandleForWriting.write(contentsOf: Data("q\n".utf8))
        try? input.fileHandleForWriting.close()
        if finished.wait(timeout: .now() + 10) == .timedOut {
            process.terminate()
            _ = finished.wait(timeout: .now() + 2)
        }
    }
}
