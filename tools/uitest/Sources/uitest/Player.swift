import Foundation

/// Plays a file through /usr/bin/afplay so the app can capture it in App-audio mode.
final class Player: @unchecked Sendable {
    private let lock = NSLock()
    private var process: Process?
    private var generation = 0  // bumped by start/stop so a stale loop never restarts

    func start(_ file: URL, loop: Bool) throws {
        stop()
        lock.lock()
        defer { lock.unlock() }
        try spawn(file, loop: loop, generation: generation)
    }

    func stop() {
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
    private func spawn(_ file: URL, loop: Bool, generation current: Int) throws {
        let child = Process()
        child.executableURL = URL(fileURLWithPath: "/usr/bin/afplay")
        child.arguments = [file.path]
        if loop {
            child.terminationHandler = { [weak self] _ in
                guard let self else { return }
                self.lock.lock()
                defer { self.lock.unlock() }
                guard self.generation == current else { return }
                try? self.spawn(file, loop: true, generation: current)
            }
        }
        try child.run()
        process = child
    }
}
