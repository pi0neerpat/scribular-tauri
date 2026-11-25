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
}
