public enum ClickGuard {
    public static func shouldPressAccessibility(reason: String, keychainOpen: Bool) -> Bool {
        !keychainOpen && (reason.hasPrefix("click point is covered by") || reason.hasPrefix("app is not frontmost"))
    }

    public static func failure(targetPID: Int32, hitPID: Int32?,
                               hitName: String?, keychainPromptOpen: Bool,
                               appIsFrontmost: Bool = true, frontmostName: String? = nil,
                               hitTestComplete: Bool = true) -> String? {
        if keychainPromptOpen { return "keychain prompt is covering the app" }
        if !appIsFrontmost { return "app is not frontmost (frontmost: \(frontmostName ?? "unknown"))" }
        guard hitTestComplete else { return nil }
        guard let hitPID else { return "click point is covered by unknown app" }
        guard hitPID == targetPID else { return "click point is covered by \(hitName ?? "unknown app")" }
        return nil
    }
}
