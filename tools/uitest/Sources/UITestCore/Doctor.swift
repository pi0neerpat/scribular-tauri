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
