import Foundation

public func trimmedRunLockPID(_ pid: String) -> String {
    let pid = pid.trimmingCharacters(in: .whitespacesAndNewlines)
    return pid.isEmpty ? "unknown" : pid
}
