import Foundation
import Testing
@testable import UITestCore

private final class FakeDriver: Driver {
    var results = [Result<String?, DriverError>]()
    var calls = [Command]()
    var labels = [String]()
    var screenshots = [URL]()
    var firstCallDelay = 0.0

    func perform(_ command: Command) throws(DriverError) -> String? {
        if case .screenshotScreen(let name) = command { return "/tmp/run/\(name ?? "screen").png" }
        calls.append(command)
        if calls.count == 1 { Thread.sleep(forTimeInterval: firstCallDelay) }
        guard !results.isEmpty else {
            Issue.record("FakeDriver: no scripted result for \(command)")
            return nil
        }
        return try results.removeFirst().get()
    }

    func visibleLabels() -> [String] { labels }
    func screenshot(to url: URL) throws { screenshots.append(url) }
}

private func step(_ line: Int, _ command: Command, source: String) -> Step {
    Step(line: line, source: source, command: command)
}

@Test func allPassPrintsChecksAndSummary() {
    let driver = FakeDriver()
    driver.results = [.success(nil), .success(nil)]
    var output = [String]()
    let passed = runSteps(
        [step(1, .launch(keepData: false), source: "launch"), step(2, .quit, source: "quit")],
        driver: driver,
        options: RunOptions(runDir: URL(fileURLWithPath: "/tmp/run"), recordingPath: "r.mp4"),
        out: { output.append($0) }
    )
    #expect(passed)
    #expect(output == ["✓ 1 launch", "✓ 2 quit", "PASS 2/2 steps — recording: r.mp4"])
}

@Test func retryableThenSucceeds() {
    let driver = FakeDriver()
    driver.results = [.failure(.retryable("not there yet")), .failure(.retryable("not there yet")), .success(nil)]
    var output = [String]()
    #expect(runSteps([step(1, .expect("X"), source: "expect \"X\"")], driver: driver,
                     options: RunOptions(timeout: 0.1, poll: 0.01, runDir: URL(fileURLWithPath: "/tmp/run"), recordingPath: "r.mp4"),
                     out: { output.append($0) }))
    #expect(output == ["✓ 1 expect \"X\"", "PASS 1/1 steps — recording: r.mp4"])
}

@Test func retryableTimesOut() {
    let driver = FakeDriver()
    driver.results = Array(repeating: .failure(.retryable("not found")), count: 20)
    driver.labels = ["Settings", "Note styles"]
    var output = [String]()
    let runDir = URL(fileURLWithPath: "/tmp/run")
    let passed = runSteps([step(1, .expect("X"), source: "expect \"X\"")], driver: driver,
                          options: RunOptions(timeout: 0.05, poll: 0.01, runDir: runDir, recordingPath: "no recording"),
                          out: { output.append($0) })
    #expect(!passed)
    #expect(output[0] == "✗ 1 expect \"X\": not found after 0.05s")
    #expect(output[1].contains("screenshot: "))
    #expect(output[2] == "  screen: /tmp/run/fail-1-screen.png")
    #expect(output[3] == "  frontmost: (unknown)")
    #expect(output[4] == "  visible: \"Settings\", \"Note styles\"")
    #expect(output[5] == "FAIL 0/1 steps — recording: no recording")
    #expect(driver.screenshots == [runDir.appendingPathComponent("fail-1.png")])
}

@Test func stepTimestampIsCapturedBeforeActionAndExpectPacingSleeps() {
    final class EventLog {
        var events = [String]()
    }
    final class EventDriver: Driver {
        let log: EventLog
        init(log: EventLog) { self.log = log }
        func perform(_ command: Command) throws(DriverError) -> String? {
            log.events.append("perform")
            return nil
        }
        func visibleLabels() -> [String] { [] }
        func screenshot(to url: URL) throws {}
    }

    let log = EventLog()
    let driver = EventDriver(log: log)
    #expect(runSteps([step(1, .click(label: "Save", index: nil, within: nil), source: "click \"Save\""),
                      step(2, .expect("Saved"), source: "expect \"Saved\"")],
                     driver: driver,
                     options: RunOptions(runDir: URL(fileURLWithPath: "/tmp/run"), recordingPath: "r.mp4", pace: 2.5,
                                         sleep: { _ in log.events.append("sleep") },
                                         onStep: { _, _ in log.events.append("timestamp") }),
                     out: { _ in }))
    #expect(log.events == ["perform", "timestamp", "sleep", "perform", "timestamp", "sleep"])
}

@Test func fatalStopsImmediately() {
    let driver = FakeDriver()
    driver.results = [.failure(.fatal("ambiguous: 2 matches for \"Cancel\""))]
    var output = [String]()
    let passed = runSteps([step(1, .click(label: "Cancel", index: nil, within: nil), source: "click \"Cancel\""),
                           step(2, .quit, source: "quit")], driver: driver,
                          options: RunOptions(runDir: URL(fileURLWithPath: "/tmp/run"), recordingPath: "r.mp4"),
                          out: { output.append($0) })
    #expect(!passed)
    #expect(driver.calls.count == 1)
    #expect(output[0].contains("ambiguous: 2 matches"))
}

@Test func passwordNeverPrinted() {
    let driver = FakeDriver()
    driver.results = [.failure(.fatal("bad password hunter2"))]
    driver.labels = ["hunter2"]
    var output = [String]()
    _ = runSteps([step(1, .handleKeychain, source: "handle-keychain")], driver: driver,
                 options: RunOptions(runDir: URL(fileURLWithPath: "/tmp/run"), recordingPath: "r.mp4", secrets: ["hunter2"]),
                 out: { output.append($0) })
    #expect(!output.isEmpty)
    #expect(output.allSatisfy { !$0.contains("hunter2") })
    #expect(output[0].contains("bad password ***"))
    #expect(output.contains("  visible: \"***\""))
}

@Test func screenshotStepSavesNamedFileAndPrintsPath() {
    let driver = FakeDriver()
    var output = [String]()
    let runDir = URL(fileURLWithPath: "/tmp/run")
    #expect(runSteps([step(1, .screenshot(name: "layout"), source: "screenshot \"layout\""),
                      step(2, .screenshot(name: nil), source: "screenshot")], driver: driver,
                     options: RunOptions(runDir: runDir, recordingPath: "r.mp4"), out: { output.append($0) }))
    #expect(driver.screenshots == [runDir.appendingPathComponent("layout.png"), runDir.appendingPathComponent("step-2.png")])
    #expect(driver.calls.isEmpty)
    #expect(output[0] == "✓ 1 screenshot \"layout\" /tmp/run/layout.png")
}

@Test func slowFirstAttemptDoesNotConsumeTimeout() {
    let driver = FakeDriver()
    driver.firstCallDelay = 0.2
    driver.results = [.failure(.retryable("not yet")), .success(nil)]
    var output = [String]()
    #expect(runSteps([step(1, .launch(keepData: false), source: "launch")], driver: driver,
                     options: RunOptions(timeout: 0.1, poll: 0.01, runDir: URL(fileURLWithPath: "/tmp/run"), recordingPath: "r.mp4"),
                     out: { output.append($0) }))
}

@Test func stepTimeoutOverridesRunTimeout() {
    let driver = FakeDriver()
    driver.results = Array(repeating: .failure(.retryable("not found")), count: 40)
    var output = [String]()
    let passed = runSteps([Step(line: 1, source: "expect \"X\" timeout 0.1s", command: .expect("X"), timeout: 0.1)], driver: driver,
                          options: RunOptions(timeout: 0.02, poll: 0.01, runDir: URL(fileURLWithPath: "/tmp/run"), recordingPath: "r.mp4"),
                          out: { output.append($0) })
    #expect(!passed)
    #expect(output[0] == "✗ 1 expect \"X\" timeout 0.1s: not found after 0.1s")
}

@Test func pickAppTimesOutWithAttemptCount() {
    let driver = FakeDriver()
    driver.results = Array(repeating: .failure(.retryable("picker still shows \"assistantd\"")), count: 4)
    var output = [String]()
    let passed = runSteps([Step(line: 1, source: "pick-app \"afplay\" timeout 0s", command: .pickApp("afplay"), timeout: 0)], driver: driver,
                          options: RunOptions(runDir: URL(fileURLWithPath: "/tmp/run"), recordingPath: "r.mp4"),
                          out: { output.append($0) })
    #expect(!passed)
    #expect(driver.calls == [.pickApp("afplay")])
    #expect(output[0] == "✗ 1 pick-app \"afplay\" timeout 0s: pick-app \"afplay\": picker still shows \"assistantd\" after 0s (1 attempt); is `play ... loop` running?")
    #expect(driver.screenshots == [URL(fileURLWithPath: "/tmp/run/fail-1.png")])
}

@Test func pickAppAttemptDetails() {
    #expect(pickAppSuccessDetail(app: "afplay", attempts: 3) == "selected \"afplay\" after 3 attempt(s)")
    #expect(pickAppTimeoutMessage(app: "afplay", timeout: "60", attempts: 13, lastFailure: "picker still shows \"assistantd\"") ==
            "pick-app \"afplay\": picker still shows \"assistantd\" after 60s (13 attempts); is `play ... loop` running?")
    #expect(pickAppTimeoutMessage(app: "Music", timeout: "10", attempts: 1, lastFailure: "picker not found") ==
            "pick-app \"Music\": picker not found after 10s (1 attempt)")
}

@Test func pickAppRetriesAndPacesAsOneAction() {
    let driver = FakeDriver()
    driver.results = [.failure(.retryable("app not listed")), .success(nil)]
    var sleeps = [Double]()
    #expect(runSteps([Step(line: 1, source: "pick-app \"afplay\"", command: .pickApp("afplay"))], driver: driver,
                      options: RunOptions(timeout: 1, poll: 0, runDir: URL(fileURLWithPath: "/tmp/run"), recordingPath: "r.mp4", pace: 2.5,
                                          sleep: { sleeps.append($0) }), out: { _ in }))
    #expect(driver.calls == [.pickApp("afplay"), .pickApp("afplay")])
    #expect(sleeps == [2.5])
}
