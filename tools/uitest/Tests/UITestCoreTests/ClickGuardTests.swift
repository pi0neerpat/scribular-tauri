import Testing
@testable import UITestCore

@Test func clickGuardAllowsTarget() {
    #expect(ClickGuard.failure(targetPID: 10, hitPID: 10, hitName: "Scribular", keychainPromptOpen: false) == nil)
}

@Test func clickGuardReportsCoveringApp() {
    #expect(ClickGuard.failure(targetPID: 10, hitPID: 20, hitName: "Brave Browser", keychainPromptOpen: false) == "click point is covered by Brave Browser")
}

@Test func clickGuardPrioritizesKeychainPrompt() {
    #expect(ClickGuard.failure(targetPID: 10, hitPID: 20, hitName: "SecurityAgent", keychainPromptOpen: true) == "keychain prompt is covering the app")
}

@Test func clickGuardReportsFrontmostApplication() {
    #expect(ClickGuard.failure(targetPID: 10, hitPID: nil, hitName: nil,
                               keychainPromptOpen: false, appIsFrontmost: false,
                               frontmostName: "Brave Browser", hitTestComplete: false) == "app is not frontmost (frontmost: Brave Browser)")
}
