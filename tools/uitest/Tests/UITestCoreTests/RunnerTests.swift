import Foundation
import Testing
@testable import UITestCore

private final class FakeDriver: Driver {
    var results = [Result<String?, DriverError>]()
    var calls = [Command]()
    var labels = [String]()
    var screenshots = [URL]()

    func perform(_ command: Command) throws(DriverError) -> String? {
        calls.append(command)
        guard !results.isEmpty else { return nil }
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
                          options: RunOptions(timeout: 0.05, poll: 0.01, runDir: runDir, recordingPath: "r.mp4"),
                          out: { output.append($0) })
    #expect(!passed)
    #expect(output[0] == "✗ 1 expect \"X\": not found after 0.05s")
    #expect(output[1].contains("screenshot: "))
    #expect(output[2] == "  visible: \"Settings\", \"Note styles\"")
    #expect(output[3] == "FAIL 0/1 steps — recording: r.mp4")
    #expect(driver.screenshots == [runDir.appendingPathComponent("fail-1.png")])
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
    #expect(output.allSatisfy { !$0.contains("hunter2") })
}
