import Foundation

/// One timed transcript line as `expect-playhead` reads it from the accessibility tree.
public struct PlayheadLine: Equatable {
    /// Start in whole seconds, from the line's leading `m:ss` or `h:mm:ss` label.
    public let start: Int
    /// The line carries `aria-current` (WebKit `AXARIACurrent`).
    public let current: Bool

    public init(start: Int, current: Bool) {
        self.start = start
        self.current = current
    }
}

/// Seconds from a label's leading `m:ss` or `h:mm:ss` time, or nil when it has none.
public func lineStartSeconds(_ label: String) -> Int? {
    guard let token = label.trimmingCharacters(in: .whitespaces).split(separator: " ").first else { return nil }
    let parts = token.split(separator: ":", omittingEmptySubsequences: false)
    guard (2...3).contains(parts.count), parts.allSatisfy({ !$0.isEmpty && $0.allSatisfy(\.isASCII) && $0.allSatisfy(\.isNumber) }),
          parts.dropFirst().allSatisfy({ $0.count == 2 }) else { return nil }
    let values = parts.compactMap { Int($0) }
    guard values.count == parts.count, values.dropFirst().allSatisfy({ $0 < 60 }) else { return nil }
    return values.reduce(0) { $0 * 60 + $1 }
}

/// Nil when the highlighted line is the last line started at `position` seconds, else why not.
/// Labels show floored seconds, so a line counts as started when its floored start is at or before
/// the playhead, and the next line as not started when its floored start is not before the floored playhead.
public func playheadDisagreement(position: Double, lines: [PlayheadLine]) -> String? {
    guard position.isFinite, position >= 0 else { return "playhead value is not a time: \(position)" }
    guard !lines.isEmpty else { return "no timed lines" }
    let current = lines.indices.filter { lines[$0].current }
    guard current.count <= 1 else { return "\(current.count) lines are highlighted" }
    let floored = Int(position.rounded(.down))
    if let index = current.first {
        guard Double(lines[index].start) <= position else {
            return "highlighted line starts at \(lines[index].start)s, after the playhead at \(position)s"
        }
        if index + 1 < lines.count, lines[index + 1].start < floored {
            return "line \(index + 2) starts at \(lines[index + 1].start)s, before the playhead at \(position)s, but line \(index + 1) is highlighted"
        }
        return nil
    }
    guard lines[0].start >= floored else {
        return "no line is highlighted, but line 1 starts at \(lines[0].start)s, before the playhead at \(position)s"
    }
    return nil
}
