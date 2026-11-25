import AVFoundation
import Foundation
import Testing
@testable import UITestCore

@Test func shortcutScenariosUseLongPlayableSpeech() throws {
    let uitest = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
    let fixture = uitest.appendingPathComponent("fixtures/keyboard-pr3-synthetic-speech.m4a")
    let audio = try AVAudioFile(forReading: fixture)
    #expect(Double(audio.length) / audio.fileFormat.sampleRate >= 50)

    for name in ["style-at-stop", "paused-speakers-long-audio", "background-mic-long-audio"] {
        let scenario = uitest.appendingPathComponent("scenarios/keyboard-pr3-\(name).uitest")
        let steps = try parseScript(String(contentsOf: scenario, encoding: .utf8))
        #expect(steps.contains {
            $0.command == .play(path: "tools/uitest/fixtures/\(fixture.lastPathComponent)", loop: false)
        })
    }
}

@Test func backgroundCallScenarioSelectsAfplayAndStopsWithFinderActive() throws {
    let uitest = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
    let scenario = uitest.appendingPathComponent("scenarios/keyboard-pr3-background-call-long-audio.uitest")
    let commands = try parseScript(String(contentsOf: scenario, encoding: .utf8)).map(\.command)
    let onCall = try #require(commands.firstIndex(of: .click(label: "On a call", index: 1, within: nil)))
    let callApp = try #require(commands.firstIndex(of: .pickApp("afplay")))
    let finder = Command.expectActiveApp("com.apple.finder")
    let finderFocus = try #require(commands.firstIndex(of: finder))
    let play = try #require(commands.firstIndex(of: .play(
        path: "tools/uitest/fixtures/keyboard-pr3-synthetic-speech.m4a", loop: false
    )))
    let key = Command.key(KeyCombo(modifiers: [.ctrl, .option, .cmd], key: "r"))
    let start = try #require(commands.firstIndex(of: key))
    let stop = try #require(commands.lastIndex(of: key))
    #expect(onCall < callApp && callApp < finderFocus && finderFocus < play && play < start && start < stop)
    #expect(start < stop && commands[(start + 1)..<stop].contains(finder))
    #expect(commands.dropFirst(stop + 1).contains(finder))
    #expect(commands.dropFirst(stop + 1).contains(.expectNot("How many people spoke?")))
}
