import Foundation

/// Plays a file through /usr/bin/afplay so the app can capture it in App-audio mode.
public final class Player: @unchecked Sendable {
    private let lock = NSLock()
    private var process: Process?
    private var generation = 0  // bumped by start/stop so a stale loop never restarts

    public init() {}

    public func start(_ file: URL, loop: Bool) throws {
        stop()
        lock.lock()
        let child: Process
        do { child = try spawn(file, loop: loop, generation: generation) } catch { lock.unlock(); throw error }
        lock.unlock()
        // afplay rejects an unplayable file within milliseconds; fail the step instead of recording silence
        Thread.sleep(forTimeInterval: 0.3)
        if !child.isRunning && child.terminationStatus != 0 {
            stop()
            throw DriverError.fatal("play: afplay exited with status \(child.terminationStatus): \(file.path)")
        }
    }

    public func stop() {
        lock.lock()
        generation += 1
        let running = process
        process = nil
        lock.unlock()
        guard let running, running.isRunning else { return }
        running.terminate()
        running.waitUntilExit()
    }

    // caller holds `lock`
    @discardableResult
    private func spawn(_ file: URL, loop: Bool, generation current: Int) throws -> Process {
        let child = Process()
        child.executableURL = URL(fileURLWithPath: "/usr/bin/afplay")
        child.arguments = [file.path]
        if loop {
            child.terminationHandler = { [weak self] ended in
                // restart only after a clean finish, so a failing afplay never respawns in a tight loop
                guard let self, ended.terminationReason == .exit, ended.terminationStatus == 0 else { return }
                self.lock.lock()
                defer { self.lock.unlock() }
                guard self.generation == current else { return }
                try? self.spawn(file, loop: true, generation: current)
            }
        }
        try child.run()
        process = child
        return child
    }
}
