import Testing
@testable import UITestCore

struct DoctorTests {
    private var probes: DoctorProbes {
        DoctorProbes(
            accessibility: { true },
            screenRecording: { true },
            automation: { true },
            ffmpeg: { true },
            keychainPassword: { true },
            tmuxServer: { nil }
        )
    }

    @Test func noPromptReportsAutomationNotChecked() {
        var probes = self.probes
        probes.automation = {
            Issue.record("Automation probe ran under --no-prompt")
            return false
        }
        let report = doctorReport(probes: probes, noPrompt: true, appCheck: nil)
        #expect(report.lines.contains("not checked Automation (System Events): skipped by --no-prompt"))
        #expect(!report.lines.contains { $0.hasPrefix("ok Automation") })
        #expect(!report.checks.contains { $0.name == "Automation (System Events)" })
    }

    @Test func fullModeRunsAutomation() {
        var probes = self.probes
        var calls = 0
        probes.automation = {
            calls += 1
            return false
        }
        let report = doctorReport(probes: probes, noPrompt: false, appCheck: nil)
        #expect(calls == 1)
        #expect(report.lines.contains { $0.hasPrefix("MISSING Automation (System Events)") })
        #expect(report.exitCode == 1)
    }

    @Test func missingScreenRecordingNamesTmuxServer() {
        var probes = self.probes
        probes.screenRecording = { false }
        probes.tmuxServer = { (123, "/usr/libexec/launchd") }
        let report = doctorReport(probes: probes, noPrompt: true, appCheck: nil)
        #expect(report.info.contains("permission is decided for tmux server pid 123, started by /usr/libexec/launchd"))
        #expect(report.info.contains("fix: tmux kill-server, then in a terminal run: tmux new-session -d -s tmux-anchor (kill-server stops every running cwf session)"))
        #expect(report.exitCode == 1)
    }

    @Test(arguments: ["Terminal", "iTerm2", "Ghostty", "WezTerm", "kitty", "Alacritty", "Warp", "Visual Studio Code"])
    func appStartedServerGetsNoKillHint(app: String) {
        var probes = self.probes
        let executable = app == "WezTerm" ? "wezterm-gui" : app
        let responsible = "/Applications/\(app).app/Contents/MacOS/\(executable)"
        probes.accessibility = { false }
        probes.tmuxServer = { (123, responsible) }
        let report = doctorReport(probes: probes, noPrompt: true, appCheck: nil)
        #expect(report.info.contains("permission is decided for tmux server pid 123, started by \(responsible)"))
        #expect(!report.info.contains { $0.contains("tmux kill-server") })
    }

    @Test(arguments: [
        ("tmux", "the terminal app running uitest"),
        ("Apple_Terminal", "Terminal"),
        ("iTerm.app", "iTerm2"),
    ])
    func permissionFixesNameTheHost(termProgram: String, expectedHost: String) {
        var probes = self.probes
        probes.accessibility = { false }
        probes.screenRecording = { false }
        let report = doctorReport(probes: probes, noPrompt: true, appCheck: nil, termProgram: termProgram)
        #expect(report.lines.contains("MISSING Accessibility: System Settings → Privacy & Security → Accessibility (allow \(expectedHost))"))
        #expect(report.lines.contains("MISSING Screen Recording: System Settings → Privacy & Security → Screen Recording (allow \(expectedHost))"))
    }

    @Test func outsideTmuxAddsNoServerLine() {
        var probes = self.probes
        probes.accessibility = { false }
        let report = doctorReport(probes: probes, noPrompt: true, appCheck: nil)
        #expect(!report.info.contains { $0.contains("tmux") })
    }

    @Test func exitCodeIgnoresNotChecked() {
        var probes = self.probes
        probes.tmuxServer = {
            Issue.record("tmux server probe ran after both permission checks passed")
            return nil
        }
        let report = doctorReport(probes: probes, noPrompt: true, appCheck: nil)
        #expect(report.exitCode == 0)
    }

    @Test func foreignPromptFailsDoctor() {
        var probes = self.probes
        let text = String(repeating: "x", count: 121)
        probes.foreignPrompt = { text }
        let report = doctorReport(probes: probes, noPrompt: false, appCheck: nil)
        #expect(report.exitCode == 1)
        #expect(report.lines.contains("MISSING Foreign keychain prompt: answer or dismiss it: \(text.prefix(120))"))
    }

    @Test func multiLinePromptStaysOnOneLine() {
        var probes = self.probes
        probes.foreignPrompt = { "Agent Token Monitor\n  wants to use\tkeychain" }
        let report = doctorReport(probes: probes, noPrompt: false, appCheck: nil)
        #expect(report.lines.contains("MISSING Foreign keychain prompt: answer or dismiss it: Agent Token Monitor wants to use keychain"))
    }

    @Test func noForeignPromptPassesDoctor() {
        let report = doctorReport(probes: probes, noPrompt: false, appCheck: nil)
        #expect(report.checks.contains { $0.name == "Foreign keychain prompt" && $0.passed })
        #expect(report.lines.contains { $0.hasPrefix("ok Foreign keychain prompt") })
        #expect(report.exitCode == 0)
    }

    @Test func foreignPromptWorksInNoPromptMode() {
        var probes = self.probes
        probes.foreignPrompt = { "Some App wants to use your keychain" }
        let report = doctorReport(probes: probes, noPrompt: true, appCheck: nil)
        #expect(report.lines.contains { $0.hasPrefix("MISSING Foreign keychain prompt") })
        #expect(report.exitCode == 1)
    }

    @Test func foreignPromptNotCheckedWithoutAccessibility() {
        var probes = self.probes
        probes.accessibility = { false }
        probes.foreignPrompt = {
            Issue.record("foreign prompt probe ran without Accessibility")
            return nil
        }
        let report = doctorReport(probes: probes, noPrompt: true, appCheck: nil)
        #expect(report.info.contains("not checked Foreign keychain prompt: Accessibility missing"))
        #expect(!report.checks.contains { $0.name == "Foreign keychain prompt" })
    }

    @Test func scribularPromptIsNotForeign() {
        #expect(!DoctorProbes.isForeign(text: "Scribular wants to use com.app.scribular.tauri in your keychain"))
        #expect(!DoctorProbes.isForeign(text: ""))
        #expect(DoctorProbes.isForeign(text: "Agent Token Monitor wants to use Claude Code-credentials"))
    }

    @Test func hostLLMIsInfoNotACheck() {
        let report = doctorReport(probes: probes, noPrompt: true, appCheck: nil, hostLLM: LLMResolution.hostDown.doctorLine)
        #expect(report.lines.contains("info host llama-server: unreachable (summaries on VM CPU)"))
        #expect(report.exitCode == 0)
        #expect(!doctorReport(probes: probes, noPrompt: true, appCheck: nil).lines.contains { $0.contains("llama-server") })
    }
}
