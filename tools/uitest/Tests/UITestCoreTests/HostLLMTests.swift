import Foundation
import Testing
@testable import UITestCore

struct HostLLMTests {
    private func noProbe(_ name: String) -> () -> Bool { { Issue.record("\(name) probe ran"); return false } }

    @Test func envOverrideWinsWithoutProbing() {
        let result = HostLLM.resolve(environment: ["SCRIBULAR_LLM_URL": "http://example:1"], isVM: noProbe("isVM"),
                                     healthOK: { _ in Issue.record("health probe ran"); return false })
        #expect(result == .override("http://example:1"))
        #expect(result.url == "http://example:1")
        #expect(result.runLine == nil)
    }

    @Test func emptyEnvForcesOnDevice() {
        let result = HostLLM.resolve(environment: ["SCRIBULAR_LLM_URL": ""], isVM: noProbe("isVM"),
                                     healthOK: { _ in Issue.record("health probe ran"); return false })
        #expect(result == .override(""))
        #expect(result.url == nil)
    }

    @Test func vmWithHealthyHostUsesHost() {
        var probed: URL?
        let result = HostLLM.resolve(environment: [:], isVM: { true }, healthOK: { probed = $0; return true })
        #expect(probed?.absoluteString == "http://192.168.64.1:5801/health")
        #expect(result.url == "http://192.168.64.1:5801")
        #expect(result.runLine == "info host llama-server: http://192.168.64.1:5801")
        #expect(result.doctorLine == "info host llama-server: ok http://192.168.64.1:5801")
    }

    @Test func vmWithHostDownWarnsAndRunsOnDevice() {
        let result = HostLLM.resolve(environment: [:], isVM: { true }, healthOK: { _ in false })
        #expect(result == .hostDown)
        #expect(result.url == nil)
        #expect(result.runLine?.hasPrefix("WARN host llama-server unreachable at 192.168.64.1:5801") == true)
        #expect(result.doctorLine == "info host llama-server: unreachable (summaries on VM CPU)")
    }

    @Test func hostMachineStaysOnDeviceWithoutNetwork() {
        let result = HostLLM.resolve(environment: [:], isVM: { false },
                                     healthOK: { _ in Issue.record("health probe ran"); return true })
        #expect(result == .onDevice)
        #expect(result.url == nil && result.runLine == nil && result.doctorLine == nil)
    }

    @Test func logCheckNeedsTheAppMarkerWithTheResolvedURL() {
        let url = "http://192.168.64.1:5801"
        #expect(HostLLM.logConfirms("x\nINFO SCRIBULAR_LLM_URL set: summaries use http://192.168.64.1:5801\n", url: url))
        #expect(!HostLLM.logConfirms("INFO SCRIBULAR_LLM_URL set: summaries use http://other:5801\n", url: url))
        #expect(!HostLLM.logConfirms("INFO SCRIBULAR_LLM_URL set: summaries use http://192.168.64.1:58011\n", url: url))
        #expect(!HostLLM.logConfirms("INFO summarizer loaded on device", url: url))
        #expect(!HostLLM.logConfirms(nil, url: url))
    }

    @Test func logCheckIgnoresLinesFromEarlierRuns() {
        let url = "http://192.168.64.1:5801"
        let stale = "INFO SCRIBULAR_LLM_URL set: summaries use \(url)\n"
        let log = Data((stale + "INFO summarizer loaded on device\n").utf8)
        let before = HostLLM.LogMark(fileID: 7, size: stale.utf8.count)
        #expect(!HostLLM.logConfirms(HostLLM.currentRun(log, fileID: 7, since: before), url: url))
        // replaced or truncated log: the whole file is this run's
        #expect(HostLLM.logConfirms(HostLLM.currentRun(log, fileID: 8, since: before), url: url))
        #expect(HostLLM.logConfirms(HostLLM.currentRun(log, fileID: 7, since: HostLLM.LogMark(fileID: 7, size: 10_000)), url: url))
        #expect(HostLLM.logConfirms(HostLLM.currentRun(log, fileID: 7, since: nil), url: url))
    }

    @Test func doctorOverrideLineRedactsSecrets() {
        let result = HostLLM.resolve(environment: ["SCRIBULAR_LLM_URL": "http://user:pw@h:1/v1?key=abc&x"], isVM: noProbe("isVM"),
                                     healthOK: { _ in false })
        #expect(result.doctorLine == "info host llama-server: not probed, SCRIBULAR_LLM_URL=http://***@h:1/v1?key=***&x")
        #expect(HostLLM.resolve(environment: ["SCRIBULAR_LLM_URL": "http://h:1"], isVM: { false }, healthOK: { _ in false }).doctorLine
            == "info host llama-server: not probed, SCRIBULAR_LLM_URL=http://h:1")
    }

    @Test func healthNeedsStatusExactlyOK() {
        #expect(HostLLM.healthBodyOK(Data(#"{"status":"ok"}"#.utf8)))
        #expect(!HostLLM.healthBodyOK(Data(#"{"status":"not ok"}"#.utf8)))
        #expect(!HostLLM.healthBodyOK(Data("ok".utf8)))
        #expect(!HostLLM.healthBodyOK(nil))
    }
}
