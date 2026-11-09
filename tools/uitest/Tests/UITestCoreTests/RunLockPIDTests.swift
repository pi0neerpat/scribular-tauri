import Testing
@testable import UITestCore

@Test func runLockPIDTrimsTrailingNewline() {
    #expect(trimmedRunLockPID("12345\n") == "12345")
    #expect(trimmedRunLockPID("\n") == "unknown")
}
