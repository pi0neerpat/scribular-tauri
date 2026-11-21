import Darwin
import Foundation

public final class RunLock {
    private let descriptor: Int32

    private init(descriptor: Int32) { self.descriptor = descriptor }
    deinit { close(descriptor) }

    public static func acquire(timeout: Double, label: String, url: URL? = nil, out: (String) -> Void = { print($0) }) throws -> RunLock {
        let url = url ?? FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Library/Caches/scribular-uitest", isDirectory: true)
            .appendingPathComponent("uitest.lock")
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        let descriptor = open(url.path, O_CREAT | O_RDWR | O_CLOEXEC, S_IRUSR | S_IWUSR)
        guard descriptor >= 0 else { throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno)) }
        let deadline = ProcessInfo.processInfo.systemUptime + timeout
        var lastHolder: String?
        while flock(descriptor, LOCK_EX | LOCK_NB) != 0 {
            guard errno == EWOULDBLOCK else {
                let code = errno
                close(descriptor)
                throw NSError(domain: NSPOSIXErrorDomain, code: Int(code))
            }
            var buffer = [CChar](repeating: 0, count: 32)
            let count = buffer.withUnsafeMutableBytes { pread(descriptor, $0.baseAddress, $0.count - 1, 0) }
            let pid = count > 0
                ? trimmedRunLockPID(String(decoding: buffer.prefix(Int(count)).prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }, as: UTF8.self))
                : "unknown"
            if timeout == 0 || ProcessInfo.processInfo.systemUptime >= deadline {
                close(descriptor)
                let message = timeout == 0 ? "another uitest run is active (pid \(pid))" : "uitest lock timeout after \(label): held by pid \(pid)"
                throw NSError(domain: "uitest.lock", code: 1, userInfo: [NSLocalizedDescriptionKey: message])
            }
            if pid != lastHolder { out("waiting for uitest run pid \(pid)"); lastHolder = pid }
            Thread.sleep(forTimeInterval: max(0, min(2, deadline - ProcessInfo.processInfo.systemUptime)))
        }
        _ = ftruncate(descriptor, 0)
        _ = lseek(descriptor, 0, SEEK_SET)
        let pid = "\(getpid())\n"
        let written = pid.withCString { write(descriptor, $0, strlen($0)) }
        guard written == pid.utf8.count else {
            close(descriptor)
            throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno))
        }
        return RunLock(descriptor: descriptor)
    }
}

