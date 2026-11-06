import Testing
@testable import UITestCore

@Test func pickSingle() {
    #expect(pick([7], index: nil) == .found(7))
}

@Test func pickNone() {
    #expect(pick([Int](), index: nil) == .none)
}

@Test func pickAmbiguousFails() {
    #expect(pick([1, 2], index: nil) == .ambiguous(count: 2))
}

@Test func pickByIndex() {
    #expect(pick([1, 2], index: 2) == .found(2))
    #expect(pick([1, 2], index: 3) == .none)
}
