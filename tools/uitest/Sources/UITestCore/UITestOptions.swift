import Foundation

public struct UITestOptions {
    public let scriptPath: String
    public let appPath: String?
    public let recordingName: String?
    public let recordingDirectory: String?
    public let pace: Double
    public let lockTimeout: Double
    public let lockTimeoutLabel: String
    public let noTrust: Bool
    /// Crop the recording to the app's windows instead of the whole display (requires `--record`).
    public let recordWindow: Bool

    public func makeRecorder<T>(using factory: (String) -> T) -> T? {
        recordingName.map(factory)
    }

    public static func parse(_ arguments: [String]) throws -> UITestOptions {
        guard let script = arguments.first else { throw UITestOptionsError("missing scenario") }
        var app: String?
        var name: String?
        var directory: String?
        var pace: Double?
        var lockTimeout: Double?
        var lockTimeoutLabel = "30m"
        var noTrust = false
        var recordWindow = false
        var index = 1
        while index < arguments.count {
            let flag = arguments[index]
            if flag == "--no-trust" {
                guard !noTrust else { throw UITestOptionsError("unexpected or duplicate option: \(flag)") }
                noTrust = true; index += 1; continue
            }
            if flag == "--record-window" {
                guard !recordWindow else { throw UITestOptionsError("unexpected or duplicate option: \(flag)") }
                recordWindow = true; index += 1; continue
            }
            guard index + 1 < arguments.count else { throw UITestOptionsError("missing value for \(flag)") }
            let value = arguments[index + 1]
            switch flag {
            case "--app" where app == nil: app = value
            case "--record" where name == nil:
                guard !value.isEmpty, value.range(of: #"^[A-Za-z0-9][A-Za-z0-9._-]*\z"#, options: .regularExpression) != nil else {
                    throw UITestOptionsError("record name must contain only letters, numbers, dots, underscores or hyphens")
                }
                name = value
            case "--record-dir" where directory == nil: directory = value
            case "--pace" where pace == nil: pace = try Self.duration(value)
            case "--lock-timeout" where lockTimeout == nil: lockTimeout = try Self.duration(value); lockTimeoutLabel = value
            default: throw UITestOptionsError("unexpected or duplicate option: \(flag)")
            }
            index += 2
        }
        if recordWindow && name == nil { throw UITestOptionsError("--record-window requires --record <name>") }
        return UITestOptions(scriptPath: script, appPath: app, recordingName: name, recordingDirectory: directory, pace: pace ?? (name == nil ? 0 : 2.5), lockTimeout: lockTimeout ?? 1800, lockTimeoutLabel: lockTimeoutLabel, noTrust: noTrust, recordWindow: recordWindow)
    }

    private init(scriptPath: String, appPath: String?, recordingName: String?, recordingDirectory: String?, pace: Double, lockTimeout: Double, lockTimeoutLabel: String, noTrust: Bool, recordWindow: Bool) {
        self.scriptPath = scriptPath
        self.appPath = appPath
        self.recordingName = recordingName
        self.recordingDirectory = recordingDirectory
        self.pace = pace
        self.lockTimeout = lockTimeout
        self.lockTimeoutLabel = lockTimeoutLabel
        self.noTrust = noTrust
        self.recordWindow = recordWindow
    }

    private static func duration(_ value: String) throws -> Double {
        if value == "0" { return 0 }
        let number: Double?
        if value.hasSuffix("ms"), let ms = Double(value.dropLast(2)) { number = ms / 1000 }
        else if value.hasSuffix("s"), let seconds = Double(value.dropLast()) { number = seconds }
        else if value.hasSuffix("m"), let minutes = Double(value.dropLast()) { number = minutes * 60 }
        else { number = nil }
        guard let number, number.isFinite, number >= 0 else { throw UITestOptionsError("invalid duration: \(value)") }
        return number
    }
}

public struct UITestOptionsError: LocalizedError {
    public let message: String
    public var errorDescription: String? { message }
    public init(_ message: String) { self.message = message }
}
