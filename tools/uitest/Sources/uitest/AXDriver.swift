import AppKit
import ApplicationServices
import CoreGraphics
import Foundation
import UITestCore

final class AXDriver: Driver {
    private let lifecycle: AppLifecycle
    private let runDir: URL
    private let keychainPassword: String?
    private var application: NSRunningApplication?

    init(lifecycle: AppLifecycle, runDir: URL, keychainPassword: String?) {
        self.lifecycle = lifecycle
        self.runDir = runDir
        self.keychainPassword = keychainPassword
        self.application = NSRunningApplication.runningApplications(withBundleIdentifier: "com.app.scribular.tauri").first
    }

    func perform(_ command: Command) throws(DriverError) -> String? {
        switch command {
        case .launch(let keepData):
            let detail = try lifecycle.launch(keepData: keepData, runDir: runDir)
            guard let app = appElement(),
                  let windows = attribute(app, kAXWindowsAttribute) as? [AXUIElement], !windows.isEmpty else {
                throw .retryable("app window not ready")
            }
            return detail
        case .quit:
            lifecycle.quit()
            application = nil
            return nil
        case .click(let label, let index, let within):
            let element = try resolve(label, index: index, within: within)
            guard let frame = frame(of: element) else { throw .retryable("not found") }
            try click(at: CGPoint(x: frame.midX, y: frame.midY))
            return nil
        case .type(let text):
            for character in text {
                guard let down = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: true),
                      let up = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: false) else { throw .fatal("could not create keyboard event") }
                var units = Array(String(character).utf16)
                down.keyboardSetUnicodeString(stringLength: units.count, unicodeString: &units)
                up.keyboardSetUnicodeString(stringLength: units.count, unicodeString: &units)
                down.post(tap: .cghidEventTap)
                up.post(tap: .cghidEventTap)
            }
            return nil
        case .key(let combo):
            guard let code = keyCodes[combo.key] else { throw .fatal("unsupported key: \(combo.key)") }
            let flags = combo.modifiers.reduce(into: CGEventFlags()) { flags, modifier in
                switch modifier {
                case .cmd: flags.insert(.maskCommand)
                case .shift: flags.insert(.maskShift)
                case .option: flags.insert(.maskAlternate)
                case .ctrl: flags.insert(.maskControl)
                }
            }
            guard let down = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: true),
                  let up = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: false) else { throw .fatal("could not create keyboard event") }
            down.flags = flags
            up.flags = flags
            down.post(tap: .cghidEventTap)
            up.post(tap: .cghidEventTap)
            return nil
        case .expect(let label):
            guard allElements().contains(where: { matches($0, substring: label) }) else { throw .retryable("not found") }
            return nil
        case .expectNot(let label):
            guard !allElements().contains(where: { matches($0, substring: label) }) else { throw .retryable("still visible") }
            return nil
        case .wait(let seconds):
            Thread.sleep(forTimeInterval: seconds)
            return nil
        case .closeWindow:
            guard let app = appElement(), let focused = axElement(attribute(app, kAXFocusedWindowAttribute)),
                  let close = axElement(attribute(focused, kAXCloseButtonAttribute)),
                  let rect = frame(of: close) else { throw .retryable("close button not found") }
            try click(at: CGPoint(x: rect.midX, y: rect.midY))
            return nil
        case .handleKeychain:
            return try handleKeychain()
        }
    }

    func visibleLabels() -> [String] {
        var seen = Set<String>()
        return allElements().flatMap(labelValues).filter { !$0.isEmpty && seen.insert($0).inserted }
    }

    func screenshot(to url: URL) throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
        process.arguments = ["-x", url.path]
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else { throw NSError(domain: "uitest.screenshot", code: Int(process.terminationStatus)) }
    }

    private var keyCodes: [String: CGKeyCode] {
        ["a": 0, "s": 1, "d": 2, "f": 3, "h": 4, "g": 5, "z": 6, "x": 7, "c": 8, "v": 9,
         "b": 11, "q": 12, "w": 13, "e": 14, "r": 15, "y": 16, "t": 17, "1": 18, "2": 19,
         "3": 20, "4": 21, "6": 22, "5": 23, "=": 24, "9": 25, "7": 26, "-": 27, "8": 28,
         "0": 29, "]": 30, "o": 31, "u": 32, "[": 33, "i": 34, "p": 35, "enter": 36,
         "l": 37, "j": 38, "'": 39, "k": 40, ";": 41, "\\": 42, ",": 43, "/": 44,
         "n": 45, "m": 46, ".": 47, "tab": 48, "space": 49, "`": 50, "delete": 51,
         "escape": 53, "right": 124, "left": 123, "down": 125, "up": 126]
    }

    private func appElement() -> AXUIElement? {
        if let application, !application.isTerminated { return AXUIElementCreateApplication(application.processIdentifier) }
        guard let found = NSRunningApplication.runningApplications(withBundleIdentifier: "com.app.scribular.tauri").first else { return nil }
        application = found
        return AXUIElementCreateApplication(found.processIdentifier)
    }

    private func allElements(from root: AXUIElement? = nil) -> [AXUIElement] {
        guard let root = root ?? appElement() else { return [] }
        var result = [root]
        if let children = attribute(root, kAXChildrenAttribute) as? [AXUIElement] {
            for child in children { result.append(contentsOf: allElements(from: child)) }
        }
        return result
    }

    private func attribute(_ element: AXUIElement, _ name: String) -> Any? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success else { return nil }
        return value
    }

    private func axElement(_ value: Any?) -> AXUIElement? {
        guard let value else { return nil }
        let reference = value as CFTypeRef
        guard CFGetTypeID(reference) == AXUIElementGetTypeID() else { return nil }
        return unsafeDowncast(reference, to: AXUIElement.self)
    }

    private func axValue(_ value: Any?) -> AXValue? {
        guard let value else { return nil }
        let reference = value as CFTypeRef
        guard CFGetTypeID(reference) == AXValueGetTypeID() else { return nil }
        return unsafeDowncast(reference, to: AXValue.self)
    }

    private func frame(of element: AXUIElement) -> CGRect? {
        guard let position = axValue(attribute(element, kAXPositionAttribute)),
              let size = axValue(attribute(element, kAXSizeAttribute)) else { return nil }
        var origin = CGPoint.zero
        var dimensions = CGSize.zero
        guard AXValueGetValue(position, .cgPoint, &origin), AXValueGetValue(size, .cgSize, &dimensions),
              dimensions.width > 0, dimensions.height > 0 else { return nil }
        return CGRect(origin: origin, size: dimensions)
    }

    private func labelValues(_ element: AXUIElement) -> [String] {
        [kAXTitleAttribute, kAXDescriptionAttribute, kAXValueAttribute].compactMap {
            (attribute(element, $0) as? String)?.trimmingCharacters(in: .whitespacesAndNewlines)
        }
    }

    private func matches(_ element: AXUIElement, exact label: String) -> Bool {
        frame(of: element) != nil && labelValues(element).contains(label)
    }

    private func matches(_ element: AXUIElement, substring label: String) -> Bool {
        frame(of: element) != nil && labelValues(element).contains { $0.localizedCaseInsensitiveContains(label) }
    }

    private func resolve(_ label: String, index: Int?, within: String?) throws(DriverError) -> AXUIElement {
        var elements = allElements()
        if let within {
            let scopes = elements.filter { matches($0, exact: within) }
            let scope: AXUIElement
            switch pick(scopes, index: nil) {
            case .none: throw .retryable("scope not found")
            case .ambiguous(let count): throw .fatal("ambiguous: \(count) matches for \"\(within)\"")
            case .found(let value): scope = value
            }
            elements = Array(allElements(from: scope).dropFirst())
        }
        let candidates = elements.filter { matches($0, exact: label) }
        switch pick(candidates, index: index) {
        case .found(let value): return value
        case .none: throw .retryable("not found")
        case .ambiguous(let count): throw .fatal("ambiguous: \(count) matches for \"\(label)\" — add index N or in \"…\"")
        }
    }

    private func click(at point: CGPoint) throws(DriverError) {
        for type in [CGEventType.mouseMoved, .leftMouseDown, .leftMouseUp] {
            guard let event = CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: point, mouseButton: .left) else {
                throw .fatal("could not create mouse event")
            }
            event.post(tap: .cghidEventTap)
        }
    }

    private func handleKeychain() throws(DriverError) -> String? {
        let securityAgent = NSRunningApplication.runningApplications(withBundleIdentifier: "com.apple.SecurityAgent").first
        guard let securityAgent else { return "no prompt" }
        let root = AXUIElementCreateApplication(securityAgent.processIdentifier)
        let windows = (attribute(root, kAXWindowsAttribute) as? [AXUIElement]) ?? []
        guard let window = windows.first else { return "no prompt" }
        guard let keychainPassword else { throw .fatal("UITEST_KEYCHAIN_PASSWORD not set") }
        let fields = allElements(from: window).filter {
            let role = attribute($0, kAXRoleAttribute) as? String
            return role == kAXTextFieldRole || role == "AXSecureTextField"
        }
        guard let field = fields.first, let fieldRect = frame(of: field) else { throw .fatal("keychain password field not found") }
        try click(at: CGPoint(x: fieldRect.midX, y: fieldRect.midY))
        _ = try perform(.type(keychainPassword))
        let allow = allElements(from: window).filter { matches($0, exact: "Always Allow") }
        let button: AXUIElement
        switch pick(allow, index: nil) {
        case .none: throw .fatal("Always Allow button not found")
        case .ambiguous(let count): throw .fatal("ambiguous: \(count) Always Allow buttons")
        case .found(let value): button = value
        }
        guard let buttonRect = frame(of: button) else { throw .fatal("Always Allow button not found") }
        try click(at: CGPoint(x: buttonRect.midX, y: buttonRect.midY))
        return nil
    }
}
