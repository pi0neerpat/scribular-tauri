import Testing
@testable import UITestCore

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
