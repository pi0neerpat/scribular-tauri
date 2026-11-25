import Foundation
import Testing

@Test func parseClipboardMatch() throws {
    #expect(try parseScript(#"expect-clipboard-matches "Data:""#).first?.command == .expectClipboardMatches("Data:"))
    #expect(throws: ScriptError.self) { try parseScript(#"expect-clipboard-matches """#) }
}

@Test func parsesClipboardMatchTimeout() throws {
    let step = try #require(parseScript(#"expect-clipboard-matches "Data:" timeout 300s"#).first)
    #expect(step.command == .expectClipboardMatches("Data:"))
    #expect(step.timeout == 300)
}
@testable import UITestCore

@Test func parsesReopenApp() throws {
    #expect(try parseScript("reopen-app").first?.command == .reopenApp)
    #expect(throws: ScriptError.self) { try parseScript("reopen-app Scribular") }
}

@Test func parsesFreshLaunchWithUnavailableHotkey() throws {
    #expect(try parseScript("launch-with-unavailable-hotkey").first?.command == .launchWithUnavailableHotkey)
    #expect(throws: ScriptError.self) { try parseScript("launch-with-unavailable-hotkey --keep-data") }
}

@Test func parsesBackgroundStatusItemClick() throws {
    let steps = try parseScript(#"background-status-item-click "Scribular — Recording""#)
    #expect(steps.map(\.command) == [.backgroundStatusItemClick("Scribular — Recording")])
    #expect(throws: ScriptError.self) { try parseScript(#"background-status-item-click """#) }
}

@Test func parsesAllCommands() throws {
    let script = """
    launch
    click "Settings"
    click "Note styles"
    click "New style"
    type "Bullet recap"
    key cmd+s
    expect "Bullet recap"
    expect-not "Discard changes?"
    wait 30s
    close-window
    handle-keychain
    quit
    """
    let steps = try parseScript(script)
    #expect(steps.count == 12)
    #expect(steps[1].command == .click(label: "Settings", index: nil, within: nil))
    #expect(steps[5].command == .key(KeyCombo(modifiers: [.cmd], key: "s")))
    #expect(steps[8].command == .wait(seconds: 30))
}

@Test func parsesLaunchKeepData() throws {
    #expect(try parseScript("launch --keep-data").first?.command == .launch(keepData: true))
}

@Test func parsesClickScopeAndIndex() throws {
    let steps = try parseScript("click \"Cancel\" in \"Discard changes?\"\nclick \"Save\" index 2")
    #expect(steps[0].command == .click(label: "Cancel", index: nil, within: "Discard changes?"))
    #expect(steps[1].command == .click(label: "Save", index: 2, within: nil))
}

@Test func ignoresCommentsAndBlanks() throws {
    let steps = try parseScript("# x\n\nquit # bye")
    #expect(steps.count == 1)
    #expect(steps[0].line == 3)
    #expect(steps[0].source == "quit")
}

@Test func hashInsideQuotesIsText() throws {
    #expect(try parseScript("type \"a # b\"").first?.command == .type("a # b"))
}

@Test func unknownCommandFails() {
    #expect(throws: ScriptError(line: 1, message: "unknown command: frobnicate")) {
        try parseScript("frobnicate")
    }
}

@Test func unterminatedQuoteFails() {
    #expect(throws: ScriptError.self) { try parseScript("click \"Save") }
}

@Test func badDurationFails() {
    #expect(throws: ScriptError.self) { try parseScript("wait soon") }
}

@Test func parsesLabelsAndScreenshot() throws {
    let steps = try parseScript("labels\nscreenshot\nscreenshot \"settings-layout\"")
    #expect(steps.map(\.command) == [.labels, .screenshot(name: nil), .screenshot(name: "settings-layout")])
}

@Test func screenshotNameCannotEscapeRunFolder() {
    for bad in ["screenshot \"../x\"", "screenshot \".hidden\"", "screenshot \"\"", "screenshot x", "screenshot \"a\" \"b\""] {
        #expect(throws: ScriptError.self) { try parseScript(bad) }
    }
}

@Test func duplicateScreenshotNamesRejected() {
    #expect(throws: ScriptError.self) { try parseScript("screenshot \"a\"\nscreenshot \"a\"") }
    #expect(throws: ScriptError.self) { try parseScript("screenshot \"menubar-2\"\nscreenshot-menubar") }
    #expect(throws: ScriptError.self) { try parseScript("screenshot-screen \"step-2\"\nscreenshot") }
    #expect(throws: ScriptError.self) { try parseScript("screenshot \"screen-2\"\nscreenshot-screen") }
}

@Test func parsesExpectNotWaitMsAndEscapedQuote() throws {
    let steps = try parseScript("expect \"A\"\nexpect-not \"B\"\nwait 500ms\ntype \"say \\\"hi\\\"\"")
    #expect(steps.map(\.command) == [.expect("A"), .expectNot("B"), .wait(seconds: 0.5), .type("say \"hi\"")])
}

@Test func rejectsEmptyClickLabel() {
    #expect(throws: ScriptError.self) { try parseScript("click \"\"") }
}

@Test func rejectsKeyComboWithEmptyComponent() {
    #expect(throws: ScriptError.self) { try parseScript("key cmd++s") }
}

@Test func rejectsScriptWithNoSteps() {
    #expect(throws: ScriptError.self) { try parseScript("") }
    #expect(throws: ScriptError.self) { try parseScript("# only a comment\n\n") }
}

@Test func parsesStepTimeout() throws {
    let steps = try parseScript("expect-not \"Getting ready\" timeout 300s\nclick \"timeout\" index 2\nexpect \"X\"")
    #expect(steps[0].command == .expectNot("Getting ready"))
    #expect(steps[0].timeout == 300)
    #expect(steps[1].command == .click(label: "timeout", index: 2, within: nil))
    #expect(steps[2].timeout == nil)
    #expect(throws: ScriptError.self) { try parseScript("expect \"X\" timeout soon") }
    #expect(throws: ScriptError.self) { try parseScript("expect \"X\" timeout infs") }
    #expect(throws: ScriptError.self) { try parseScript("wait 5s timeout 300s") }
}

@Test func parsesPlayAndStopAudio() throws {
    let steps = try parseScript("play \"a.wav\"\nplay \"a.wav\" loop\nstop-audio")
    #expect(steps[0].command == .play(path: "a.wav", loop: false))
    #expect(steps[1].command == .play(path: "a.wav", loop: true))
    #expect(steps[2].command == .stopAudio)
}

@Test func parsesPickAppAndTimeout() throws {
    let step = try #require(parseScript("pick-app \"afplay\" timeout 60s").first)
    #expect(step.command == .pickApp("afplay"))
    #expect(step.timeout == 60)
    #expect(throws: ScriptError.self) { try parseScript("pick-app afplay") }
    #expect(throws: ScriptError.self) { try parseScript("pick-app \"afplay\" timeout soon") }
}

@Test func rejectsBadPlay() throws {
    for line in ["play", "play a.wav", "play \"a.wav\" forever", "play \"a.wav\" loop loop"] {
        #expect(throws: ScriptError(line: 1, message: "usage: play \"file\" [loop]")) { try parseScript(line) }
    }
    #expect(throws: ScriptError.self) { try parseScript("play \"a.wav\" timeout 5s") }
    #expect(throws: ScriptError.self) { try parseScript("stop-audio now") }
}

@Test func parsesNativeSurfaceCommands() throws {
    let script = """
    expect-status-item "Scribular" timeout 30s
    status-item click "Scribular"
    screenshot-menubar
    screenshot-menubar "tray-icon"
    screenshot-screen
    screenshot-screen "tray-menu"
    expect-notification "ready" timeout 30s
    dock click "Scribular"
    """
    let steps = try parseScript(script)
    #expect(steps.map(\.command) == [
        .expectStatusItem("Scribular"), .statusItemClick("Scribular"),
        .screenshotMenubar(name: nil), .screenshotMenubar(name: "tray-icon"),
        .screenshotScreen(name: nil), .screenshotScreen(name: "tray-menu"),
        .expectNotification("ready"), .dockClick("Scribular")
    ])
    #expect(steps[0].timeout == 30)
    #expect(steps[6].timeout == 30)
}

@Test func rejectsMalformedNativeSurfaceCommandsWithLineNumbers() {
    #expect(throws: ScriptError(line: 1, message: #"usage: expect-status-item "text""#)) { try parseScript("expect-status-item Scribular") }
    #expect(throws: ScriptError(line: 2, message: #"usage: status-item click "help""#)) { try parseScript("quit\nstatus-item hover \"Scribular\"") }
    #expect(throws: ScriptError(line: 3, message: #"usage: dock click "title""#)) { try parseScript("quit\nquit\ndock press \"Scribular\"") }
}

@Test func parsesStatusItemMouseClick() throws {
    #expect(try parseScript("status-item-mouse-click \"Scribular\"").map(\.command) == [.statusItemMouseClick("Scribular")])
    #expect(throws: ScriptError.self) { try parseScript("status-item-mouse-click Scribular") }
}

@Test func parsesCallAppPickerWithTimeout() throws {
    let step = try #require(parseScript("pick-call-app \"afplay\" timeout 60s").first)
    #expect(step.command == .pickCallApp("afplay"))
    #expect(step.timeout == 60)
    #expect(throws: ScriptError.self) { try parseScript("pick-call-app afplay") }
}

@Test func doctorReportFormatsResultsWithoutSecrets() {
    let passed = DoctorReport(checks: [
        DoctorCheck(name: "Accessibility", fix: "System Settings", passed: true),
        DoctorCheck(name: "Keychain password", fix: "bash tools/uitest/trust-build.sh --store-password", passed: true),
    ])
    #expect(passed.exitCode == 0)
    #expect(passed.lines == ["ok Accessibility: System Settings", "ok Keychain password: bash tools/uitest/trust-build.sh --store-password"])
    let missing = DoctorReport(checks: [DoctorCheck(name: "ffmpeg", fix: "brew install ffmpeg", passed: false)])
    #expect(missing.exitCode == 1)
    #expect(missing.lines == ["MISSING ffmpeg: brew install ffmpeg"])
    #expect(!missing.lines.joined().contains("secret-password"))
}

@Test func parsesShortcutObservationCommands() throws {
    let steps = try parseScript("""
    expect-frontmost "com.apple.finder" timeout 5s
    observe-no-notification "Recording started" 3s
    """)
    #expect(steps.map(\.command) == [
        .expectFrontmost("com.apple.finder"),
        .observeNoNotification("Recording started", seconds: 3)
    ])
    #expect(steps[0].timeout == 5)
    #expect(throws: ScriptError.self) { try parseScript("expect-frontmost Finder") }
    #expect(throws: ScriptError.self) { try parseScript(#"observe-no-notification "Recording stopped" 0s"#) }
}

@Test func parsesClaimHotkey() throws {
    let steps = try parseScript("claim-hotkey ctrl+option+cmd+r")
    #expect(steps.map(\.command) == [.claimHotkey(KeyCombo(modifiers: [.ctrl, .option, .cmd], key: "r"))])
    #expect(throws: ScriptError.self) { try parseScript("claim-hotkey r") }
}

@Test func parsesTimeoutsForBothFocusAssertions() throws {
    let steps = try parseScript("""
    expect-frontmost "com.apple.finder" timeout 5s
    expect-active-app "com.apple.finder" timeout 3s
    """)
    #expect(steps.map(\.timeout) == [5, 3])
}

@Test func unavailableHotkeyLaunchRequiresFreshApp() throws {
    #expect(throws: ScriptError.self) { try parseScript("launch\nlaunch-with-unavailable-hotkey") }
    #expect(throws: ScriptError.self) {
        try parseScript("launch-with-unavailable-hotkey\nlaunch-with-unavailable-hotkey")
    }
    #expect(try parseScript("launch\nquit\nlaunch-with-unavailable-hotkey").last?.command == .launchWithUnavailableHotkey)
}

@Test func parsesFreshFixtureFailureCommands() throws {
    let steps = try parseScript("""
    expect-active-app "com.apple.finder"
    hold-fixture-database-writer
    release-fixture-database-writer
    """)
    #expect(steps.map(\.command) == [
        .expectActiveApp("com.apple.finder"), .holdFixtureDatabaseWriter, .releaseFixtureDatabaseWriter
    ])
    #expect(throws: ScriptError.self) { try parseScript("hold-fixture-database-writer arbitrary-path") }
}

@Test func parsesKeyboardPR3RecoveryScenarios() throws {
    let scenarios = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        .appendingPathComponent("scenarios")
    let paused = try parseScript(String(contentsOf: scenarios.appendingPathComponent(
        "keyboard-pr3-paused-speakers-frontmost.uitest"
    ), encoding: .utf8)).map(\.command)
    let foreground = Command.expectActiveApp("com.app.scribular.tauri")
    let reopen = try #require(paused.firstIndex(of: .reopenApp))
    #expect(paused.dropFirst(reopen + 1).first == foreground)
    let resume = try #require(paused.firstIndex(of: .expect("Resume")))
    #expect(Array(paused.dropFirst(resume + 1).prefix(2)) == [foreground, .closeWindow])

    let fallback = try parseScript(String(contentsOf: scenarios.appendingPathComponent(
        "keyboard-pr3-unavailable-hotkey-tray-start.uitest"
    ), encoding: .utf8)).map(\.command)
    #expect(fallback.first == .launchWithUnavailableHotkey)
    let openTray = try #require(fallback.firstIndex(of: .statusItemMouseClick("Scribular")))
    #expect(fallback.prefix(openTray).contains(.closeWindow))
    #expect(Array(fallback.dropFirst(openTray + 1).prefix(2)) == [
        .expect("Show Scribular"),
        .click(label: "Start Recording", index: nil, within: nil)
    ])
}
