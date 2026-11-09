import Testing
@testable import UITestCore

@Test func clickGuardAllowsTarget() {
    #expect(ClickGuard.failure(targetPID: 10, hitPID: 10, hitName: "Scribular", keychainPromptOpen: false, hitBelongsToTarget: true) == nil)
}

@Test func clickGuardReportsCoveringApp() {
    #expect(ClickGuard.failure(targetPID: 10, hitPID: 20, hitName: "Brave Browser", keychainPromptOpen: false) == .covered("Brave Browser"))
}

@Test func clickGuardPrioritizesKeychainPrompt() {
    #expect(ClickGuard.failure(targetPID: 10, hitPID: 20, hitName: "SecurityAgent", keychainPromptOpen: true) == .keychainPrompt)
}

@Test func clickGuardReportsFrontmostApplication() {
    #expect(ClickGuard.failure(targetPID: 10, hitPID: nil, hitName: nil,
                               keychainPromptOpen: false, appIsFrontmost: false,
                               frontmostName: "Brave Browser", hitTestComplete: false) == .appNotFrontmost("Brave Browser"))
}

@Test func clickGuardAllowsAccessibilityPressForCoveredClick() {
    #expect(ClickGuard.shouldPressAccessibility(for: .covered("Brave Browser")))
    #expect(ClickGuard.shouldPressAccessibility(for: .covered("unknown app")))
    #expect(ClickGuard.shouldPressAccessibility(for: .appNotFrontmost("Brave Browser")))
}

@Test func clickGuardNeverAccessibilityPressesThroughKeychainPrompt() {
    #expect(!ClickGuard.shouldPressAccessibility(for: .keychainPrompt))
}

@Test func clickGuardRejectsAnotherWindowInSameProcess() {
    #expect(ClickGuard.failure(targetPID: 10, hitPID: 10, hitName: "Scribular", keychainPromptOpen: false,
                               hitBelongsToTarget: false) == .covered("another Scribular element"))
}
