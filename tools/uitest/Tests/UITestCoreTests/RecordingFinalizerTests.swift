import Foundation
import Testing
@testable import UITestCore

@Test func interruptedRecordingIsRenamedAndMetadataIsWrittenOnlyOnce() throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }

    let finalizer = RecordingFinalizer(name: "04-smoke", directory: directory,
                                       scenario: "smoke.uitest", appBuildTime: "build", stepCount: 1)
    try finalizer.validateAvailable()
    try Data("partial video".utf8).write(to: finalizer.temporaryURL)
    let step = Step(line: 3, source: "click \"Start\"", command: .click(label: "Start", index: nil, within: nil))
    finalizer.record(step, at: Date(timeIntervalSince1970: 10))

    #expect(try finalizer.finish(result: .interrupted, duration: 3) == finalizer.failedURL)
    #expect(try finalizer.finish(result: .pass, duration: 4) == finalizer.failedURL)
    #expect(FileManager.default.fileExists(atPath: finalizer.failedURL.path))
    #expect(!FileManager.default.fileExists(atPath: finalizer.recordingURL.path))
    let metadata = try JSONDecoder().decode(RecordingMetadata.self, from: Data(contentsOf: finalizer.metadataURL))
    #expect(metadata.result == .interrupted)
    #expect(metadata.steps.map(\.line) == [3])
}

@Test func recordingFinalizerRejectsExistingArtifactsWithoutChangingThem() throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }

    let finalizer = RecordingFinalizer(name: "04-smoke", directory: directory,
                                       scenario: "smoke.uitest", appBuildTime: "build", stepCount: 0)
    let original = Data("keep me".utf8)
    try original.write(to: finalizer.recordingURL)
    #expect(throws: RecordingFinalizerError.outputExists) { try finalizer.validateAvailable() }
    #expect(try Data(contentsOf: finalizer.recordingURL) == original)
}

@Test func recordingFinalizerRejectsAnOutputCreatedAfterPreflight() throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }

    let finalizer = RecordingFinalizer(name: "04-smoke", directory: directory,
                                       scenario: "smoke.uitest", appBuildTime: "build", stepCount: 0)
    try finalizer.validateAvailable()
    let original = Data("keep me".utf8)
    try original.write(to: finalizer.recordingURL)
    try Data("new video".utf8).write(to: finalizer.temporaryURL)
    #expect(throws: RecordingFinalizerError.outputExists) { try finalizer.finish(result: .pass, duration: 1) }
    #expect(try Data(contentsOf: finalizer.recordingURL) == original)
}

@Test func successfulRecordingCannotFinalizeWithoutVideo() throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }

    let finalizer = RecordingFinalizer(name: "04-smoke", directory: directory,
                                       scenario: "smoke.uitest", appBuildTime: "build", stepCount: 0)
    #expect(throws: RecordingFinalizerError.self) { try finalizer.finish(result: .pass, duration: 1) }
}

@Test func recorderProcessMustMatchExecutableAndExactOutputArgument() {
    let executable = "/opt/homebrew/bin/ffmpeg"
    let output = "/tmp/test recording.mp4"
    #expect(recorderProcessMatches(executable: executable, arguments: ["ffmpeg", "-i", "input", output],
                                   expectedExecutable: executable, outputPath: output))
    #expect(!recorderProcessMatches(executable: executable, arguments: ["ffmpeg", "-i", "input", "prefix \(output)"],
                                    expectedExecutable: executable, outputPath: output))
    #expect(!recorderProcessMatches(executable: "/usr/bin/ffmpeg-wrapper", arguments: ["ffmpeg-wrapper", "-i", "input", output],
                                    expectedExecutable: executable, outputPath: output))
    #expect(!recorderProcessMatches(executable: executable, arguments: ["ffmpeg", "-i", "input", "prefix\(output)"],
                                    expectedExecutable: executable, outputPath: output))
}

@Test func staleRecorderMarkerCanBeRemovedOnlyWhenBothProcessesAreGone() {
    #expect(shouldRemoveRecorderMarker(parentExited: true, recorderExists: false))
    #expect(!shouldRemoveRecorderMarker(parentExited: false, recorderExists: false))
    #expect(!shouldRemoveRecorderMarker(parentExited: true, recorderExists: true))
}

@Test func finalizeRenamesFfmpegLogBesideRecording() throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }

    let finalizer = RecordingFinalizer(name: "04-smoke", directory: directory,
                                       scenario: "smoke.uitest", appBuildTime: "build", stepCount: 0)
    #expect(finalizer.logURL(for: .pass).lastPathComponent == "04-smoke.ffmpeg.log")
    #expect(finalizer.logURL(for: .fail).lastPathComponent == "04-smoke-FAILED.ffmpeg.log")
    try Data("v".utf8).write(to: finalizer.temporaryURL)
    let log = finalizer.temporaryURL.deletingPathExtension().appendingPathExtension("ffmpeg.log")
    try Data("l".utf8).write(to: log)
    try finalizer.finish(result: .fail, duration: 1)
    #expect(FileManager.default.fileExists(atPath: directory.appendingPathComponent("04-smoke-FAILED.ffmpeg.log").path))
    #expect(!FileManager.default.fileExists(atPath: log.path))
}
