public enum ClickGuard {
    public enum Failure: Equatable, Sendable {
        case keychainPrompt
        case covered(String)
        case appNotFrontmost(String)

        public var message: String {
            switch self {
            case .keychainPrompt: "keychain prompt is covering the app"
            case .covered(let app): "click point is covered by \(app)"
            case .appNotFrontmost(let app): "app is not frontmost (frontmost: \(app))"
            }
        }
    }

    public static func shouldPressAccessibility(for failure: Failure) -> Bool {
        switch failure {
        case .covered, .appNotFrontmost: true
        case .keychainPrompt: false
        }
    }

    public static func failure(targetPID: Int32, hitPID: Int32?,
                               hitName: String?, keychainPromptOpen: Bool,
                               appIsFrontmost: Bool = true, frontmostName: String? = nil,
                               hitTestComplete: Bool = true, hitBelongsToTarget: Bool = true) -> Failure? {
        if keychainPromptOpen { return .keychainPrompt }
        if !appIsFrontmost { return .appNotFrontmost(frontmostName ?? "unknown") }
        guard hitTestComplete else { return nil }
        guard let hitPID else { return .covered("unknown app") }
        guard hitPID == targetPID else { return .covered(hitName ?? "unknown app") }
        guard hitBelongsToTarget else { return .covered("another Scribular element") }
        return nil
    }
}
