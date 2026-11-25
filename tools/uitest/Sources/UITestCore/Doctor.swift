import Foundation

public struct DoctorCheck: Equatable {
    public let name: String
    public let fix: String
    public let passed: Bool

    public init(name: String, fix: String, passed: Bool) {
        self.name = name
        self.fix = fix
        self.passed = passed
    }
}

public struct DoctorReport: Equatable {
    public let checks: [DoctorCheck]
    public let info: [String]

    public init(checks: [DoctorCheck], info: [String] = []) {
        self.checks = checks
        self.info = info
    }

    public var exitCode: Int32 { checks.allSatisfy(\.passed) ? 0 : 1 }

    public var lines: [String] {
        checks.map { "\($0.passed ? "ok" : "MISSING") \($0.name): \($0.fix)" } + info
    }
}

public struct DoctorProbes {
    public var accessibility: () -> Bool
    public var screenRecording: () -> Bool
    public var automation: () -> Bool
    public var ffmpeg: () -> Bool
    public var keychainPassword: () -> Bool
    public var tmuxServer: () -> (pid: Int32, responsible: String)?

    public init(
        accessibility: @escaping () -> Bool,
        screenRecording: @escaping () -> Bool,
        automation: @escaping () -> Bool,
        ffmpeg: @escaping () -> Bool,
        keychainPassword: @escaping () -> Bool,
        tmuxServer: @escaping () -> (pid: Int32, responsible: String)?
    ) {
        self.accessibility = accessibility
        self.screenRecording = screenRecording
        self.automation = automation
        self.ffmpeg = ffmpeg
        self.keychainPassword = keychainPassword
        self.tmuxServer = tmuxServer
    }
}

public func doctorReport(
    probes: DoctorProbes,
    noPrompt: Bool,
    appCheck: DoctorCheck?,
    termProgram: String? = ProcessInfo.processInfo.environment["TERM_PROGRAM"]
) -> DoctorReport {
    let host = switch termProgram {
    case "Apple_Terminal": "Terminal"
    case "iTerm.app": "iTerm2"
    default: "the terminal app running uitest"
    }
    let accessibility = probes.accessibility()
    let screenRecording = probes.screenRecording()
    var checks = [
        DoctorCheck(name: "Accessibility", fix: "System Settings → Privacy & Security → Accessibility (allow \(host))", passed: accessibility),
        DoctorCheck(name: "Screen Recording", fix: "System Settings → Privacy & Security → Screen Recording (allow \(host))", passed: screenRecording),
    ]
    var info = [String]()
    if noPrompt {
        info.append("not checked Automation (System Events): skipped by --no-prompt")
    } else {
        checks.append(DoctorCheck(name: "Automation (System Events)", fix: "System Settings → Privacy & Security → Automation → allow System Events", passed: probes.automation()))
    }
    checks.append(DoctorCheck(name: "ffmpeg", fix: "brew install ffmpeg", passed: probes.ffmpeg()))
    checks.append(DoctorCheck(name: "Keychain password", fix: "bash tools/uitest/trust-build.sh --store-password", passed: probes.keychainPassword()))
    if let appCheck { checks.append(appCheck) }

    if (!accessibility || !screenRecording), let (pid, responsible) = probes.tmuxServer() {
        info.append("permission is decided for tmux server pid \(pid), started by \(responsible)")
        if !responsible.split(separator: "/").contains(where: { $0.lowercased().hasSuffix(".app") }) {
            info.append("fix: tmux kill-server, then in a terminal run: tmux new-session -d -s tmux-anchor (kill-server stops every running cwf session)")
        }
    }
    return DoctorReport(checks: checks, info: info)
}
