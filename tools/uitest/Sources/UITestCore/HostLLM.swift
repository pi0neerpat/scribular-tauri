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

    /// Doctor info line; nil outside a VM.
    public var doctorLine: String? {
        switch self {
        case .host(let url): "info host llama-server: ok \(url)"
        case .hostDown: "info host llama-server: unreachable (summaries on VM CPU)"
        case .override(let url): "info host llama-server: not probed, SCRIBULAR_LLM_URL=\(url.isEmpty ? "(empty, on-device)" : url)"
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

    public static func resolve(
        environment: [String: String] = ProcessInfo.processInfo.environment,
        isVM: () -> Bool = HostLLM.isVM,
        healthOK: (URL) -> Bool = HostLLM.healthOK
    ) -> LLMResolution {
        if let value = environment["SCRIBULAR_LLM_URL"] { return .override(value) }
        guard isVM() else { return .onDevice }
        return healthOK(URL(string: "\(url)/health")!) ? .host(url) : .hostDown
    }

    /// True when the app log shows the app used the URL; a failing check means the build lacks `remote-llm`.
    public static func logConfirms(_ log: String?) -> Bool { log?.contains(logMarker) == true }

    /// `kern.hv_vmm_present` is 1 inside a virtual machine, 0 on the host.
    public static func isVM() -> Bool {
        var value: Int32 = 0
        var size = MemoryLayout<Int32>.size
        return sysctlbyname("kern.hv_vmm_present", &value, &size, nil, 0) == 0 && value == 1
    }

    /// GET with a 2 s timeout; healthy is HTTP 200 with "ok" in the body (llama-server: `{"status":"ok"}`).
    public static func healthOK(_ url: URL) -> Bool {
        var request = URLRequest(url: url)
        request.timeoutInterval = 2
        let done = DispatchSemaphore(value: 0)
        nonisolated(unsafe) var ok = false
        URLSession.shared.dataTask(with: request) { data, response, _ in
            ok = (response as? HTTPURLResponse)?.statusCode == 200 && data.map { String(decoding: $0, as: UTF8.self).contains("ok") } == true
            done.signal()
        }.resume()
        // read ok only after the signal, so a timed-out request never races the callback
        return done.wait(timeout: .now() + 3) == .success && ok
    }
}
