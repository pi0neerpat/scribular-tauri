import Foundation
import Testing
@testable import UITestCore

@Test func parsesPlayheadCommands() throws {
    #expect(try parseScript("click-child 2 in \"Timed transcript\"").first?.command == .clickChild(index: 2, within: "Timed transcript"))
    #expect(try parseScript("expect-playhead \"Playback position\" in \"Timed transcript\" timeout 5s").first
        == Step(line: 1, source: "expect-playhead \"Playback position\" in \"Timed transcript\" timeout 5s",
                command: .expectPlayhead(slider: "Playback position", within: "Timed transcript"), timeout: 5))
    #expect(throws: ScriptError.self) { try parseScript("click-child 0 in \"Timed transcript\"") }
    #expect(throws: ScriptError.self) { try parseScript("click-child 1 \"Timed transcript\"") }
    #expect(throws: ScriptError.self) { try parseScript("expect-playhead \"Playback position\"") }
}

@Test func parsesLaunchWithEnv() throws {
    #expect(try parseScript("launch-with-env \"SCRIBULAR_UITEST_SUMMARY=unsupported\" \"SCRIBULAR_UITEST_LICENSE=lapsed\"").first?.command
        == .launchWithEnv(["SCRIBULAR_UITEST_SUMMARY": "unsupported", "SCRIBULAR_UITEST_LICENSE": "lapsed"]))
    #expect(try parseScript("launch\nquit\nlaunch-with-env \"SCRIBULAR_UITEST_SUMMARY=loadFailed\"").last?.command
        == .launchWithEnv(["SCRIBULAR_UITEST_SUMMARY": "loadFailed"]))
    for bad in ["launch-with-env", "launch-with-env \"SCRIBULAR_SKIP_LICENSE=1\"", "launch-with-env \"PATH=/tmp\"",
                "launch-with-env \"SCRIBULAR_UITEST_SUMMARY=\"", "launch-with-env SCRIBULAR_UITEST_SUMMARY=x",
                "launch-with-env \"SCRIBULAR_UITEST_A=1\" \"SCRIBULAR_UITEST_A=2\"",
                "launch\nlaunch-with-env \"SCRIBULAR_UITEST_SUMMARY=unsupported\""] {
        #expect(throws: ScriptError.self) { try parseScript(bad) }
    }
}

@Test func readsLineStarts() {
    #expect(lineStartSeconds("0:05 Client: hello") == 5)
    #expect(lineStartSeconds("12:34 text") == 754)
    #expect(lineStartSeconds("1:02:03 text") == 3723)
    #expect(lineStartSeconds("Client: hello") == nil)
    #expect(lineStartSeconds("0:5 text") == nil)
    #expect(lineStartSeconds("0:75 text") == nil)
}

@Test func checksPlayheadAgreement() {
    let lines = { (current: Int?) in [0, 4, 9].enumerated().map { PlayheadLine(start: $0.element, current: $0.offset == current) } }
    #expect(playheadDisagreement(position: 5.2, lines: lines(1)) == nil)
    #expect(playheadDisagreement(position: 9.0, lines: lines(2)) == nil)
    #expect(playheadDisagreement(position: 30, lines: lines(2)) == nil)
    // floored labels: a line shown at 0:09 may start at 9.6s, so 9.3s with line 2 highlighted still agrees
    #expect(playheadDisagreement(position: 9.3, lines: lines(1)) == nil)
    #expect(playheadDisagreement(position: 5.2, lines: lines(0)) != nil)
    #expect(playheadDisagreement(position: 3.0, lines: lines(1)) != nil)
    #expect(playheadDisagreement(position: 3.0, lines: lines(nil)) != nil)
    #expect(playheadDisagreement(position: 3.0, lines: [PlayheadLine(start: 4, current: false)]) == nil)
    #expect(playheadDisagreement(position: 3.0, lines: [PlayheadLine(start: 0, current: true), PlayheadLine(start: 0, current: true)]) != nil)
    #expect(playheadDisagreement(position: .nan, lines: lines(0)) != nil)
    #expect(playheadDisagreement(position: 1, lines: []) != nil)
}

@Test func parsesExpectDisabled() throws {
    #expect(try parseScript("expect-disabled \"Therapy (DORA)\"").first?.command == .expectDisabled(label: "Therapy (DORA)", index: nil))
    #expect(try parseScript("expect-disabled \"Undo\" index 2 timeout 5s").first?.command == .expectDisabled(label: "Undo", index: 2))
    for bad in ["expect-disabled", "expect-disabled Undo", "expect-disabled \"Undo\" index 0", "expect-disabled \"Undo\" index"] {
        #expect(throws: ScriptError.self) { try parseScript(bad) }
    }
}

@Test func parsesRecordingPR3AS11AndAS15Scenarios() throws {
    let scenarios = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        .appendingPathComponent("scenarios")
    let variants = try parseScript(String(contentsOf: scenarios.appendingPathComponent("recording-pr3-as11-variants.uitest"), encoding: .utf8)).map(\.command)
    #expect(variants.filter { if case .launchWithEnv = $0 { true } else { false } }.count == 3)
    #expect(variants.contains(.expectNot("Try Again")) && variants.contains(.expect("Try Again")))
    let playhead = try parseScript(String(contentsOf: scenarios.appendingPathComponent("recording-pr3-as15-playhead.uitest"), encoding: .utf8)).map(\.command)
    #expect(playhead.filter { $0 == .expectPlayhead(slider: "Playback position", within: "Timed transcript") }.count == 7)
    #expect(playhead.contains(.clickChild(index: 2, within: "Timed transcript")))
}
