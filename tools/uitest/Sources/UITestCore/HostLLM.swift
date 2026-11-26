import Foundation

/// Where summaries run for this uitest launch (README "Host llama-server (VM)").
public enum LLMResolution: Equatable {
    /// `SCRIBULAR_LLM_URL` was set by the caller; empty means on-device.
    case override(String)
    /// In a VM and the host llama-server answered its health check.
    case host(String)
    /// In a VM but the host llama-server did not answer: on-device on the VM CPU.
    case hostDown
    /// Not a VM: on-device, as before.
    case onDevice

    /// The value to pass to the app as `SCRIBULAR_LLM_URL`, or nil for on-device.
    public var url: String? {
        switch self {
        case .override(let url): url.isEmpty ? nil : url
        case .host(let url): url
        case .hostDown, .onDevice: nil
        }
    }

    /// Line uitest prints before launching, or nil when there is nothing to say.
    public var runLine: String? {
        switch self {
        case .host(let url): "info host llama-server: \(url)"
        case .hostDown: "WARN host llama-server unreachable at \(HostLLM.address); summaries run on the VM CPU (see README \"Host llama-server (VM)\")"
        case .override, .onDevice: nil
        }
    }

    /// Doctor info line; nil outside a VM unless SCRIBULAR_LLM_URL is set.
    public var doctorLine: String? {
        switch self {
        case .host(let url): "info host llama-server: ok \(url)"
        case .hostDown: "info host llama-server: unreachable (summaries on VM CPU)"
        case .override(let url): "info host llama-server: not probed, SCRIBULAR_LLM_URL=\(url.isEmpty ? "(empty, on-device)" : HostLLM.redacted(url))"
        case .onDevice: nil
        }
    }
}

public enum HostLLM {
    // shortcut: fixed Softnet bridge address; read it from the env if a second VM network is ever used.
    public static let address = "192.168.64.1:5801"
    public static let url = "http://\(address)"
    /// The app writes this at setup when it honours `SCRIBULAR_LLM_URL` (src-tauri/src/lib.rs).
    public static let logMarker = "SCRIBULAR_LLM_URL set: summaries use"
    public static let ignoredMessage = "app ignored SCRIBULAR_LLM_URL: build lacks --features remote-llm (tools/uitest/tauri-debug-overlay.json)"
    public static let missingLogMessage = "app.log missing: cannot confirm SCRIBULAR_LLM_URL was honoured"

    public static func resolve(
        environment: [String: String] = ProcessInfo.processInfo.environment,
        isVM: () -> Bool = HostLLM.isVM,
        healthOK: (URL) -> Bool = HostLLM.healthOK
    ) -> LLMResolution {
        if let value = environment["SCRIBULAR_LLM_URL"] { return .override(value) }
        guard isVM() else { return .onDevice }
        return healthOK(URL(string: "\(url)/health")!) ? .host(url) : .hostDown
    }

    /// True when a log line ends with the marker and exactly `url`; a failing check means the build lacks `remote-llm`.
    public static func logConfirms(_ log: String?, url: String) -> Bool {
        let line = "\(logMarker) \(url)"
        return log?.split(whereSeparator: \.isNewline).contains { $0.hasSuffix(line) } == true
    }

    /// Identity and size of the app log file, taken before the run.
    public struct LogMark: Equatable {
        public let fileID: Int
        public let size: Int
        public init(fileID: Int, size: Int) { self.fileID = fileID; self.size = size }

        /// nil when the log does not exist yet.
        public init?(at url: URL) {
            guard let attributes = try? FileManager.default.attributesOfItem(atPath: url.path),
                  let fileID = (attributes[.systemFileNumber] as? NSNumber)?.intValue,
                  let size = (attributes[.size] as? NSNumber)?.intValue else { return nil }
            self.init(fileID: fileID, size: size)
        }
    }

    /// The bytes this run wrote: past `before` when the log is the same file and has not shrunk,
    /// otherwise the whole log (it was created, replaced or truncated during the run).
    public static func currentRun(_ log: Data, fileID: Int?, since before: LogMark?) -> String {
        guard let before, before.fileID == fileID, log.count >= before.size else { return String(decoding: log, as: UTF8.self) }
        return String(decoding: log.dropFirst(before.size), as: UTF8.self)
    }

    /// `url` with userinfo and query values replaced by `***`, for printing.
    public static func redacted(_ url: String) -> String {
        guard var components = URLComponents(string: url) else { return "(unparseable, hidden)" }
        if components.user != nil || components.password != nil {
            components.percentEncodedUser = "***"
            components.percentEncodedPassword = nil
        }
        components.queryItems = components.queryItems?.map { URLQueryItem(name: $0.name, value: $0.value == nil ? nil : "***") }
        components.fragment = nil
        return components.string ?? "(unparseable, hidden)"
    }

    /// `kern.hv_vmm_present` is 1 inside a virtual machine, 0 on the host.
    public static func isVM() -> Bool {
        var value: Int32 = 0
        var size = MemoryLayout<Int32>.size
        return sysctlbyname("kern.hv_vmm_present", &value, &size, nil, 0) == 0 && value == 1
    }

    /// True for a JSON body whose `status` is exactly `"ok"` (llama-server: `{"status":"ok"}`).
    public static func healthBodyOK(_ data: Data?) -> Bool {
        guard let data, let body = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return false }
        return body["status"] as? String == "ok"
    }

    /// GET with a 2 s timeout; healthy is HTTP 200 and `healthBodyOK`.
    public static func healthOK(_ url: URL) -> Bool {
        var request = URLRequest(url: url)
        request.timeoutInterval = 2
        let done = DispatchSemaphore(value: 0)
        nonisolated(unsafe) var ok = false
        URLSession.shared.dataTask(with: request) { data, response, _ in
            ok = (response as? HTTPURLResponse)?.statusCode == 200 && healthBodyOK(data)
            done.signal()
        }.resume()
        // read ok only after the signal, so a timed-out request never races the callback
        return done.wait(timeout: .now() + 3) == .success && ok
    }
}
