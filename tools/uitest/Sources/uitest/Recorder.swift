import Foundation

final class Recorder {
    private let process = Process()
    private let input = Pipe()
    private let finished = DispatchSemaphore(value: 0)

    init(output: URL) {
        process.executableURL = Preflight.executable("ffmpeg")
        process.arguments = ["-f", "avfoundation", "-framerate", "15", "-capture_cursor", "1", "-i", "0:none", "-pix_fmt", "yuv420p", output.path]
        process.standardInput = input
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        process.terminationHandler = { [finished] _ in finished.signal() }
    }

    func start() throws {
        guard process.executableURL != nil else { throw NSError(domain: "uitest.recorder", code: 1, userInfo: [NSLocalizedDescriptionKey: "ffmpeg is not on PATH"]) }
        try process.run()
        Thread.sleep(forTimeInterval: 0.25)
        guard process.isRunning else { throw NSError(domain: "uitest.recorder", code: Int(process.terminationStatus), userInfo: [NSLocalizedDescriptionKey: "ffmpeg could not start screen recording"]) }
    }

    func stop() {
        guard process.isRunning else { return }
        try? input.fileHandleForWriting.write(contentsOf: Data("q\n".utf8))
        try? input.fileHandleForWriting.close()
        if finished.wait(timeout: .now() + 10) == .timedOut {
            process.terminate()
            _ = finished.wait(timeout: .now() + 2)
        }
    }
}
