import Foundation

public enum DriverError: Error {
    case retryable(String)
    case fatal(String)
}

public protocol Driver {
    func perform(_ command: Command) throws(DriverError) -> String?
    func visibleLabels() -> [String]
    func screenshot(to url: URL) throws
}

public struct RunOptions {
    public var timeout: Double
    public var poll: Double
    public var runDir: URL
    public var recordingPath: String
    public var secrets: [String]

    public init(timeout: Double = 10, poll: Double = 0.25, runDir: URL, recordingPath: String, secrets: [String] = []) {
        self.timeout = timeout
        self.poll = poll
        self.runDir = runDir
        self.recordingPath = recordingPath
        self.secrets = secrets
    }
}

public func runSteps(_ steps: [Step], driver: Driver, options: RunOptions, out: (String) -> Void) -> Bool {
    var passed = 0

    func emit(_ line: String) {
        out(options.secrets.reduce(line) { result, secret in
            secret.isEmpty ? result : result.replacingOccurrences(of: secret, with: "***")
        })
    }

    func screenshotPath(_ url: URL) -> String {
        let absolute = url.path
        let cwd = FileManager.default.currentDirectoryPath + "/"
        return absolute.hasPrefix(cwd) ? String(absolute.dropFirst(cwd.count)) : absolute
    }

    func fail(_ step: Step, _ message: String) {
        emit("✗ \(step.line) \(step.source): \(message)")
        let screenshot = options.runDir.appendingPathComponent("fail-\(step.line).png")
        try? driver.screenshot(to: screenshot)
        emit("  screenshot: \(screenshotPath(screenshot))")
        let visible = driver.visibleLabels().prefix(40).map { "\"\($0)\"" }.joined(separator: ", ")
        emit("  visible: \(visible)")
    }

    func execute(_ step: Step) throws(DriverError) -> String? {
        guard case .screenshot(let name) = step.command else { return try driver.perform(step.command) }
        let url = options.runDir.appendingPathComponent("\(name ?? "step-\(step.line)").png")
        do { try driver.screenshot(to: url) } catch { throw .fatal("screenshot failed: \(error.localizedDescription)") }
        return screenshotPath(url)
    }

    for step in steps {
        var started: Date?  // the wait window opens at the first retryable failure, not before the first attempt
        let limit = step.timeout ?? options.timeout
        let detail: String?
        while true {
            do {
                detail = try execute(step)
                break
            } catch let error {
                switch error {
                case .fatal(let message):
                    fail(step, message)
                    emit("FAIL \(passed)/\(steps.count) steps — recording: \(options.recordingPath)")
                    return false
                case .retryable(let message):
                    if started == nil { started = Date() }
                    let remaining = limit - Date().timeIntervalSince(started ?? Date())
                    if remaining <= 0 {
                        let timeout = limit == floor(limit)
                            ? String(Int(limit))
                            : String(format: "%.2f", limit).replacingOccurrences(of: #"0+$"#, with: "", options: .regularExpression).replacingOccurrences(of: #"\.$"#, with: "", options: .regularExpression)
                        fail(step, "\(message) after \(timeout)s")
                        emit("FAIL \(passed)/\(steps.count) steps — recording: \(options.recordingPath)")
                        return false
                    }
                    Thread.sleep(forTimeInterval: min(options.poll, remaining))
                }
            }
        }
        passed += 1
        emit("✓ \(step.line) \(step.source)\(detail.map { " \($0)" } ?? "")")
    }

    emit("PASS \(passed)/\(steps.count) steps — recording: \(options.recordingPath)")
    return true
}
