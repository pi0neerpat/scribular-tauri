public func pickAppSuccessDetail(app: String, attempts: Int) -> String {
    "selected \"\(app)\" after \(attempts) attempt(s)"
}

public func pickAppTimeoutMessage(app: String, timeout: String, attempts: Int, lastFailure: String) -> String {
    let count = attempts == 1 ? "1 attempt" : "\(attempts) attempts"
    let hint = app == "afplay" ? "; is `play ... loop` running?" : ""
    return "pick-app \"\(app)\": \(lastFailure) after \(timeout)s (\(count))\(hint)"
}
