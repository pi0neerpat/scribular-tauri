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
        probes.automation = { fatalError("Automation may prompt") }
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

    @Test(arguments: ["Terminal", "iTerm2", "Ghostty", "WezTerm", "kitty", "Alacritty"])
    func terminalStartedServerGetsNoKillHint(terminal: String) {
        var probes = self.probes
        let executable = terminal == "WezTerm" ? "wezterm-gui" : terminal
        let responsible = "/Applications/\(terminal).app/Contents/MacOS/\(executable)"
        probes.accessibility = { false }
        probes.tmuxServer = { (123, responsible) }
        let report = doctorReport(probes: probes, noPrompt: true, appCheck: nil)
        #expect(report.info.contains("permission is decided for tmux server pid 123, started by \(responsible)"))
        #expect(!report.info.contains { $0.contains("tmux kill-server") })
    }

    @Test func outsideTmuxAddsNoServerLine() {
        var probes = self.probes
        probes.accessibility = { false }
        let report = doctorReport(probes: probes, noPrompt: true, appCheck: nil)
        #expect(!report.info.contains { $0.contains("tmux") })
    }

    @Test func exitCodeIgnoresNotChecked() {
        var probes = self.probes
        probes.tmuxServer = { fatalError("Permissions already passed") }
        let report = doctorReport(probes: probes, noPrompt: true, appCheck: nil)
        #expect(report.exitCode == 0)
    }
}
