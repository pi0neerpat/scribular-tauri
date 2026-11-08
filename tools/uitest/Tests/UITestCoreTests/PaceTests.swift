import Foundation
import Testing
@testable import UITestCore

private struct PaceDriver: Driver {
    func perform(_ command: Command) throws(DriverError) -> String? { nil }
    func visibleLabels() -> [String] { [] }
    func screenshot(to url: URL) throws {}
}

@Test func parsesPacingCommandsAndRecordingDefaults() throws {
    let steps = try parseScript("pause 3s\npace 2s\npause 500ms")
    #expect(steps.map(\.command) == [.pause(seconds: 3), .pace(seconds: 2), .pause(seconds: 0.5)])
    let plain = try UITestOptions.parse(["scenario.uitest"])
    #expect(plain.recordingName == nil)
    #expect(plain.pace == 0)
    let recorded = try UITestOptions.parse(["scenario.uitest", "--record", "04-happy-path", "--pace", "2s", "--record-dir", "/tmp/records"])
    #expect(recorded.recordingName == "04-happy-path")
    #expect(recorded.pace == 2)
    #expect(recorded.recordingDirectory == "/tmp/records")
    #expect(try UITestOptions.parse(["scenario.uitest", "--record", "smoke"]).pace == 2.5)
    #expect(throws: UITestOptionsError.self) { try UITestOptions.parse(["scenario.uitest", "--record", "../bad"]) }
}

@Test func recorderFactoryIsNotCalledWithoutRecordFlag() throws {
    var constructed = false
    let options = try UITestOptions.parse(["scenario.uitest"])
    let recorder = options.makeRecorder { _ in constructed = true; return "fake" }
    #expect(recorder == nil)
    #expect(!constructed)
}

@Test func pacesActionsAndExpectsButZeroPaceSkipsActionDelay() throws {
    var sleeps = [Double]()
    let steps = try parseScript("click \"Save\"\nexpect \"Saved\"\npace 0s\ntype \"x\"\nexpect-not \"Dialog\"\npause 3s")
    #expect(runSteps(steps, driver: PaceDriver(),
                     options: RunOptions(runDir: URL(fileURLWithPath: "/tmp/run"), recordingPath: "none", pace: 2.5, sleep: { sleeps.append($0) }),
                     out: { _ in }))
    #expect(sleeps == [2.5, 1, 1, 3])
}
