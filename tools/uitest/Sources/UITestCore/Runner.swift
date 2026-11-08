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
    public var pace: Double
    public var sleep: (Double) -> Void
    public var onStep: ((Step, Date) -> Void)?

    public init(timeout: Double = 10, poll: Double = 0.25, runDir: URL, recordingPath: String, secrets: [String] = [], pace: Double = 0, sleep: @escaping (Double) -> Void = { Thread.sleep(forTimeInterval: $0) }, onStep: ((Step, Date) -> Void)? = nil) {
        self.timeout = timeout
        self.poll = poll
        self.runDir = runDir
        self.recordingPath = recordingPath
        self.secrets = secrets
        self.pace = pace
        self.sleep = sleep
        self.onStep = onStep
    }
}

public func runSteps(_ steps: [Step], driver: Driver, options: RunOptions, out: (String) -> Void) -> Bool {
    var passed = 0
    var pace = options.pace
    var visibleChange = false

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
        switch step.command {
        case .screenshot(let name):
            let url = options.runDir.appendingPathComponent("\(name ?? "step-\(step.line)").png")
            do { try driver.screenshot(to: url) } catch { throw .fatal("screenshot failed: \(error.localizedDescription)") }
            return screenshotPath(url)
        case .screenshotMenubar(let name):
            let path = try driver.perform(.screenshotMenubar(name: name ?? "menubar-\(step.line)"))
            return path.map { screenshotPath(URL(fileURLWithPath: $0)) }
        case .screenshotScreen(let name):
            let path = try driver.perform(.screenshotScreen(name: name ?? "screen-\(step.line)"))
            return path.map { screenshotPath(URL(fileURLWithPath: $0)) }
        default:
            return try driver.perform(step.command)
        }
    }

    for step in steps {
        if case .pace(let seconds) = step.command {
            pace = seconds
            options.onStep?(step, Date())
            passed += 1
            emit("✓ \(step.line) \(step.source)")
            continue
        }
        if case .pause(let seconds) = step.command {
            if seconds > 0 { options.sleep(seconds) }
            options.onStep?(step, Date())
            passed += 1
            emit("✓ \(step.line) \(step.source)")
            continue
        }
        var started: Date? = if case .pickApp = step.command { Date() } else { nil }
        var attempts = 0
        let limit = step.timeout ?? options.timeout
        let detail: String?
        while true {
            attempts += 1
            do {
                detail = try execute(step)
                break
            } catch let error {
                switch error {
                case .fatal(let message):
                    fail(step, message)
                    emit("FAIL \(passed)/\(steps.count) steps\(options.recordingPath == "no recording" ? " — recording: no recording" : "")")
                    return false
                case .retryable(let message):
                    if started == nil { started = Date() }
                    let remaining = limit - Date().timeIntervalSince(started ?? Date())
                    if remaining <= 0 {
                        let timeout = limit == floor(limit)
                            ? String(Int(limit))
                            : String(format: "%.2f", limit).replacingOccurrences(of: #"0+$"#, with: "", options: .regularExpression).replacingOccurrences(of: #"\.$"#, with: "", options: .regularExpression)
                        let failure = if case .pickApp(let app) = step.command {
                            pickAppTimeoutMessage(app: app, timeout: timeout, attempts: attempts, lastFailure: message)
                        } else {
                            "\(message) after \(timeout)s"
                        }
                        fail(step, failure)
                        emit("FAIL \(passed)/\(steps.count) steps\(options.recordingPath == "no recording" ? " — recording: no recording" : "")")
                        return false
                    }
                    let poll = switch step.command {
                    case .expectStatusItem, .expectNotification: min(options.poll, 0.2)
                    default: options.poll
                    }
                    Thread.sleep(forTimeInterval: min(poll, remaining))
                }
            }
        }
        passed += 1
        let completedAt = Date()
        options.onStep?(step, completedAt)
        let changed = switch step.command {
        case .click, .pickApp, .type, .key, .closeWindow, .handleKeychain, .statusItemClick, .dockClick: true
        default: false
        }
        if changed {
            if pace > 0 { options.sleep(pace) }
            visibleChange = true
        } else if case .expect = step.command, visibleChange {
            options.sleep(1)
            visibleChange = false
        } else if case .expectNot = step.command, visibleChange {
            options.sleep(1)
            visibleChange = false
        }
        let resultDetail = if case .pickApp(let app) = step.command {
            pickAppSuccessDetail(app: app, attempts: attempts)
        } else {
            detail
        }
        emit("✓ \(step.line) \(step.source)\(resultDetail.map { " \($0)" } ?? "")")
    }

    emit("PASS \(passed)/\(steps.count) steps — recording: \(options.recordingPath)")
    return true
}
