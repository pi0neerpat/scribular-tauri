import Foundation

public enum Modifier: String, Hashable {
    case cmd, shift, option, ctrl
}

public struct KeyCombo: Equatable {
    public let modifiers: Set<Modifier>
    public let key: String

    public init(modifiers: Set<Modifier>, key: String) {
        self.modifiers = modifiers
        self.key = key.lowercased()
    }
}

public enum Command: Equatable {
    case launch(keepData: Bool)
    case click(label: String, index: Int?, within: String?)
    case type(String)
    case key(KeyCombo)
    case expect(String)
    case expectNot(String)
    case wait(seconds: Double)
    case pause(seconds: Double)
    case pace(seconds: Double)
    case closeWindow
    case handleKeychain
    case labels
    case screenshot(name: String?)
    case play(path: String, loop: Bool)
    case stopAudio
    case quit
}

public struct Step: Equatable {
    public let line: Int
    public let source: String
    public let command: Command
    /// Overrides the run's retry window for this step (`... timeout 300s`).
    public let timeout: Double?

    public init(line: Int, source: String, command: Command, timeout: Double? = nil) {
        self.line = line
        self.source = source
        self.command = command
        self.timeout = timeout
    }
}

public struct ScriptError: Error, Equatable {
    public let line: Int
    public let message: String

    public init(line: Int, message: String) {
        self.line = line
        self.message = message
    }
}

private struct Token {
    let text: String
    let quoted: Bool
}

public func parseScript(_ text: String) throws(ScriptError) -> [Step] {
    var steps = [Step]()
    for (offset, rawLine) in text.components(separatedBy: .newlines).enumerated() {
        let lineNumber = offset + 1
        let source = try uncomment(rawLine, line: lineNumber).trimmingCharacters(in: .whitespaces)
        guard !source.isEmpty else { continue }
        let tokens = try tokenize(source, line: lineNumber)
        guard let first = tokens.first else { continue }
        var args = Array(tokens.dropFirst())

        func fail(_ message: String) -> ScriptError { ScriptError(line: lineNumber, message: message) }
        func quoted(_ token: Token?) -> String? { token?.quoted == true ? token?.text : nil }
        func seconds(_ token: Token) throws(ScriptError) -> Double {
            let duration = token.text
            if !token.quoted, duration.hasSuffix("ms"), let value = Double(duration.dropLast(2)), value.isFinite, value >= 0 { return value / 1000 }
            if !token.quoted, duration.hasSuffix("s"), let value = Double(duration.dropLast()), value.isFinite, value >= 0 { return value }
            throw fail("invalid duration: \(duration)")
        }

        var timeout: Double?
        if args.count >= 2, args[args.count - 2].text == "timeout", !args[args.count - 2].quoted {
            timeout = try seconds(args[args.count - 1])
            args.removeLast(2)
        }

        let command: Command
        switch first.text {
        case "launch":
            guard args.isEmpty || args.map(\.text) == ["--keep-data"] else { throw fail("usage: launch [--keep-data]") }
            command = .launch(keepData: !args.isEmpty)
        case "click":
            guard let label = quoted(args.first), !label.isEmpty else { throw fail("usage: click \"label\" [index N] [in \"Scope\"]") }
            var index: Int?
            var within: String?
            var cursor = 1
            while cursor < args.count {
                switch args[cursor].text {
                case "index":
                    guard index == nil, cursor + 1 < args.count, !args[cursor + 1].quoted,
                          let value = Int(args[cursor + 1].text), value > 0 else { throw fail("invalid click index") }
                    index = value
                    cursor += 2
                case "in":
                    guard within == nil, cursor + 1 < args.count, let scope = quoted(args[cursor + 1]) else { throw fail("invalid click scope") }
                    within = scope
                    cursor += 2
                default:
                    throw fail("unexpected click argument: \(args[cursor].text)")
                }
            }
            command = .click(label: label, index: index, within: within)
        case "type":
            guard args.count == 1, let value = quoted(args.first) else { throw fail("usage: type \"text\"") }
            command = .type(value)
        case "key":
            guard args.count == 1, !args[0].quoted else { throw fail("usage: key cmd+shift+s") }
            let parts = args[0].text.lowercased().split(separator: "+", omittingEmptySubsequences: false).map(String.init)
            guard let key = parts.last, !key.isEmpty else { throw fail("invalid key combo") }
            var modifiers = Set<Modifier>()
            for part in parts.dropLast() {
                guard let modifier = Modifier(rawValue: part), modifiers.insert(modifier).inserted else { throw fail("invalid key modifier: \(part)") }
            }
            command = .key(KeyCombo(modifiers: modifiers, key: key))
        case "expect", "expect-not":
            guard args.count == 1, let value = quoted(args.first) else { throw fail("usage: \(first.text) \"label\"") }
            command = first.text == "expect" ? .expect(value) : .expectNot(value)
        case "wait", "pause", "pace":
            guard args.count == 1, !args[0].quoted else { throw fail("usage: \(first.text) 3s or \(first.text) 500ms") }
            let duration = try seconds(args[0])
            command = first.text == "wait" ? .wait(seconds: duration) : first.text == "pause" ? .pause(seconds: duration) : .pace(seconds: duration)
        case "labels":
            guard args.isEmpty else { throw fail("unexpected argument: \(args[0].text)") }
            command = .labels
        case "screenshot":
            guard args.count <= 1 else { throw fail("usage: screenshot [\"name\"]") }
            let name = quoted(args.first)
            // the name becomes a file in the run folder; keep it from escaping or hiding
            guard args.isEmpty || name.map({ !$0.isEmpty && !$0.contains("/") && !$0.hasPrefix(".") }) == true else {
                throw fail("screenshot name must be a quoted file name without / or a leading .")
            }
            if let name, steps.contains(where: { $0.command == .screenshot(name: name) }) {
                throw fail("duplicate screenshot name: \(name)")
            }
            command = .screenshot(name: name)
        case "play":
            guard let path = quoted(args.first), !path.isEmpty,
                  args.count == 1 || (args.count == 2 && !args[1].quoted && args[1].text == "loop") else {
                throw fail("usage: play \"file\" [loop]")
            }
            command = .play(path: path, loop: args.count == 2)
        case "stop-audio":
            guard args.isEmpty else { throw fail("unexpected argument: \(args[0].text)") }
            command = .stopAudio
        case "close-window", "handle-keychain", "quit":
            guard args.isEmpty else { throw fail("unexpected argument: \(args[0].text)") }
            command = first.text == "close-window" ? .closeWindow : first.text == "handle-keychain" ? .handleKeychain : .quit
        default:
            throw fail("unknown command: \(first.text)")
        }
        // only these commands retry; a timeout anywhere else would be silently ignored
        if timeout != nil, !["launch", "click", "expect", "expect-not", "close-window"].contains(first.text) {
            throw fail("timeout only applies to launch, click, expect, expect-not and close-window")
        }
        steps.append(Step(line: lineNumber, source: source, command: command, timeout: timeout))
    }
    guard !steps.isEmpty else { throw ScriptError(line: 0, message: "script has no steps") }
    return steps
}

private func uncomment(_ line: String, line number: Int) throws(ScriptError) -> String {
    var quoted = false
    var escaped = false
    var result = ""
    for character in line {
        if escaped { result.append(character); escaped = false; continue }
        if character == "\\", quoted { result.append(character); escaped = true; continue }
        if character == "\"" { quoted.toggle(); result.append(character); continue }
        if character == "#", !quoted { break }
        result.append(character)
    }
    if quoted { throw ScriptError(line: number, message: "unterminated quote") }
    return result
}

private func tokenize(_ line: String, line number: Int) throws(ScriptError) -> [Token] {
    let chars = Array(line)
    var tokens = [Token]()
    var index = 0
    while index < chars.count {
        while index < chars.count && chars[index].isWhitespace { index += 1 }
        guard index < chars.count else { break }
        if chars[index] == "\"" {
            index += 1
            var value = ""
            while index < chars.count && chars[index] != "\"" {
                if chars[index] == "\\", index + 1 < chars.count, chars[index + 1] == "\"" || chars[index + 1] == "\\" {
                    index += 1
                }
                value.append(chars[index])
                index += 1
            }
            guard index < chars.count else { throw ScriptError(line: number, message: "unterminated quote") }
            index += 1
            tokens.append(Token(text: value, quoted: true))
        } else {
            let start = index
            while index < chars.count && !chars[index].isWhitespace { index += 1 }
            tokens.append(Token(text: String(chars[start..<index]), quoted: false))
        }
    }
    return tokens
}
