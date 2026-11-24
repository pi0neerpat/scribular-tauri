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
