import Foundation

public enum RecordingResult: String, Codable {
    case pass, fail, interrupted
}

public struct RecordingMetadata: Codable {
    public struct Step: Codable {
        public let line: Int
        public let source: String
        public let timestamp: String
    }

    public let scenario: String
    public let appBuildTime: String
    public let stepCount: Int
    public let duration: Double
    public let result: RecordingResult
    public let steps: [Step]
}

public enum RecordingFinalizerError: Error {
    case outputExists
    case recordingMissing
}

public final class RecordingFinalizer {
    public let recordingURL: URL
    public let failedURL: URL
    public let temporaryURL: URL
    public let metadataURL: URL

    private let scenario: String
    private let appBuildTime: String
    private let stepCount: Int
    private let lock = NSLock()
    private var steps = [RecordingMetadata.Step]()
    private var result: RecordingResult?
    private var finalURL: URL?

    public init(name: String, directory: URL, scenario: String, appBuildTime: String, stepCount: Int) {
        recordingURL = directory.appendingPathComponent("\(name).mp4")
        failedURL = directory.appendingPathComponent("\(name)-FAILED.mp4")
        metadataURL = directory.appendingPathComponent("\(name).json")
        temporaryURL = directory.appendingPathComponent(".uitest-\(UUID().uuidString).mp4")
        self.scenario = scenario
        self.appBuildTime = appBuildTime
        self.stepCount = stepCount
    }

    public func validateAvailable() throws {
        guard ![recordingURL, failedURL, metadataURL].contains(where: { FileManager.default.fileExists(atPath: $0.path) }) else {
            throw RecordingFinalizerError.outputExists
        }
    }

    public func record(_ step: UITestCore.Step, at date: Date) {
        lock.lock()
        defer { lock.unlock() }
        guard result == nil else { return }
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        steps.append(.init(line: step.line, source: step.source, timestamp: formatter.string(from: date)))
    }

    @discardableResult
    public func finish(result: RecordingResult, duration: Double) throws -> URL? {
        lock.lock()
        defer { lock.unlock() }
        if self.result != nil { return finalURL }

        let destination = result == .pass ? recordingURL : failedURL
        if FileManager.default.fileExists(atPath: temporaryURL.path) {
            try FileManager.default.moveItem(at: temporaryURL, to: destination)
            finalURL = destination
        } else if result == .pass {
            throw RecordingFinalizerError.recordingMissing
        }
        self.result = result

        let metadata = RecordingMetadata(scenario: scenario, appBuildTime: appBuildTime, stepCount: stepCount,
                                         duration: duration, result: result, steps: steps)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let temporaryMetadataURL = metadataURL.deletingLastPathComponent().appendingPathComponent(".uitest-\(UUID().uuidString).json")
        try encoder.encode(metadata).write(to: temporaryMetadataURL, options: .atomic)
        do {
            try FileManager.default.linkItem(at: temporaryMetadataURL, to: metadataURL)
            try? FileManager.default.removeItem(at: temporaryMetadataURL)
        } catch {
            try? FileManager.default.removeItem(at: temporaryMetadataURL)
            throw error
        }
        return finalURL
    }
}

public func recorderProcessMatches(executable: String, command: String, expectedExecutable: String, outputPath: String) -> Bool {
    executable == expectedExecutable && command.hasSuffix(" \(outputPath)")
}

public func shouldRemoveRecorderMarker(parentExited: Bool, recorderExists: Bool) -> Bool {
    parentExited && !recorderExists
}
