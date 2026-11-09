import AppKit
import ApplicationServices
import CoreGraphics
import Foundation
import Darwin
import UITestCore

final class AXDriver: Driver {
    private let lifecycle: AppLifecycle
    private let runDir: URL
    private let keychainPassword: String?
    private let player: Player
    private var application: NSRunningApplication?
    private var keychainHandled = false  // the runner retries .launch; handle each prompt once

    init(lifecycle: AppLifecycle, runDir: URL, keychainPassword: String?, player: Player) {
        self.lifecycle = lifecycle
        self.player = player
        self.runDir = runDir
        self.keychainPassword = keychainPassword
        self.application = NSRunningApplication.runningApplications(withBundleIdentifier: "com.app.scribular.tauri").first
    }

    func perform(_ command: Command) throws(DriverError) -> String? {
        switch command {
        case .launch(let keepData):
            let detail = try lifecycle.launch(keepData: keepData, runDir: runDir)
            // a rebuilt (re-signed) app blocks on a keychain prompt before its window opens
            // re-arm once a prompt closes: setup reads two keychain items, so a second prompt can follow
            if keychainWindow() == nil { keychainHandled = false }
            else if !keychainHandled {
                keychainHandled = true
                if keychainPassword != nil { _ = try handleKeychain() }
                else {
                    // no password: give a person 2 minutes to approve the prompt by hand
                    let deadline = Date().addingTimeInterval(120)
                    while keychainWindow() != nil && Date() < deadline { Thread.sleep(forTimeInterval: 0.5) }
                }
            }
            guard let app = appElement(),
                  let windows = attribute(app, kAXWindowsAttribute) as? [AXUIElement], !windows.isEmpty else {
                throw .retryable("app window not ready")
            }
            return detail
        case .play(let path, let loop):
            let file = try resolvePlayPath(path, cwd: URL(fileURLWithPath: FileManager.default.currentDirectoryPath))
            do { try player.start(file, loop: loop) }
            catch let error as DriverError { throw error }
            catch { throw .fatal("play: could not start afplay: \(error.localizedDescription)") }
            return file.path
        case .stopAudio:
            player.stop()
            return nil
        case .quit:
            player.stop()
            lifecycle.quit()
            application = nil
            keychainHandled = false
            return nil
        case .click(let label, let index, let within):
            let element = try resolve(label, index: index, within: within)
            // a click on a disabled button is silently dropped; wait for it like any other UI state
            if attribute(element, kAXEnabledAttribute) as? Bool == false { throw .retryable("disabled") }
            // WebKit reports frames for off-screen content; scroll first so the click lands on the element
            if AXUIElementPerformAction(element, "AXScrollToVisible" as CFString) == .success { Thread.sleep(forTimeInterval: 0.2) }
            guard let frame = frame(of: element) else { throw .retryable("not found") }
            return try click(element, at: CGPoint(x: frame.midX, y: frame.midY))
        case .pickApp(let app):
            if popupMenuOpen() {
                _ = try perform(.key(KeyCombo(modifiers: [], key: "escape")))
                let closed = waitUntil(timeout: 2) { !popupMenuOpen() }
                Thread.sleep(forTimeInterval: 1)
                guard closed && !popupMenuOpen() else { throw .retryable("a menu is still open") }
            }
            if let picker = try appPicker(), (attribute(picker, kAXValueAttribute) as? String) == app { return nil }
            _ = try perform(.click(label: "Refresh", index: nil, within: nil))
            Thread.sleep(forTimeInterval: 2.5)
            guard let picker = try appPicker() else { throw .retryable("picker not found") }
            guard let frame = frame(of: picker) else { throw .retryable("picker not visible") }
            try click(at: CGPoint(x: frame.midX, y: frame.midY))
            guard waitUntil(timeout: 3, { popupMenuOpen() }) else { throw .retryable("picker menu did not open") }
            Thread.sleep(forTimeInterval: 0.3)
            _ = try perform(.type(app))
            Thread.sleep(forTimeInterval: 0.3)
            _ = try perform(.key(KeyCombo(modifiers: [], key: "enter")))
            var closed = waitUntil(timeout: 2) { !popupMenuOpen() }
            if !closed {
                _ = try perform(.key(KeyCombo(modifiers: [], key: "escape")))
                closed = waitUntil(timeout: 2) { !popupMenuOpen() }
            }
            Thread.sleep(forTimeInterval: 1)
            guard closed && !popupMenuOpen() else { throw .retryable("a menu is still open") }
            let value = attribute(picker, kAXValueAttribute) as? String ?? "(unavailable)"
            guard value == app else { throw .retryable("picker shows \"\(value)\"") }
            return nil
        case .type(let text):
            for character in text {
                guard let down = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: true),
                      let up = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: false) else { throw .fatal("could not create keyboard event") }
                // nil-source events inherit session modifier state; a stuck flag (seen: numeric-pad) makes WebKit drop the text
                down.flags = []
                up.flags = []
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
        case .pause, .pace:
            throw .fatal("pacing commands are handled by the runner")
        case .closeWindow:
            guard let app = appElement(), let focused = axElement(attribute(app, kAXFocusedWindowAttribute)),
                  let close = axElement(attribute(focused, kAXCloseButtonAttribute)),
                  let rect = frame(of: close) else { throw .retryable("close button not found") }
            try click(at: CGPoint(x: rect.midX, y: rect.midY))
            return nil
        case .handleKeychain:
            return try handleKeychain()
        case .labels:
            return labelListing()
        case .screenshot:
            throw .fatal("screenshot is handled by the runner")
        case .expectStatusItem(let help):
            guard statusItem(help) != nil else { throw .retryable("status item \"\(help)\" not found; AXHelp present: \(statusItemHelpValues())") }
            return nil
        case .statusItemClick(let help):
            guard let item = statusItem(help) else { throw .retryable("status item \"\(help)\" not found; AXHelp present: \(statusItemHelpValues())") }
            guard AXUIElementPerformAction(item, "AXPress" as CFString) == .success else { throw .fatal("could not press status item \"\(help)\"") }
            return nil
        case .expectNotification(let text):
            guard notificationTexts().contains(where: { $0.localizedCaseInsensitiveContains(text) }) else { throw .retryable("notification text not found: \"\(text)\"") }
            return nil
        case .dockClick(let title):
            let items = processElements(bundleID: "com.apple.dock").filter {
                attribute($0, kAXRoleAttribute) as? String == "AXDockItem"
            }
            let matches = items.filter { attribute($0, kAXTitleAttribute) as? String == title }
            guard matches.count == 1 else {
                let titles = (matches.isEmpty ? items : matches).compactMap { attribute($0, kAXTitleAttribute) as? String }
                throw .fatal("Dock item \"\(title)\" matched \(matches.count) items; matching titles: \(titles)")
            }
            guard AXUIElementPerformAction(matches[0], "AXPress" as CFString) == .success else { throw .fatal("could not press Dock item \"\(title)\"") }
            return nil
        case .screenshotMenubar(let name):
            return try surfaceScreenshot(name: name ?? "menubar", menubar: true)
        case .screenshotScreen(let name):
            return try surfaceScreenshot(name: name ?? "screen", menubar: false)
        }
    }

    private func statusItem(_ help: String) -> AXUIElement? {
        guard let app = appElement(), let extras = axElement(attribute(app, "AXExtrasMenuBar")),
              let children = attribute(extras, kAXChildrenAttribute) as? [AXUIElement] else { return nil }
        return children.first { attribute($0, "AXHelp") as? String == help }
    }

    private func statusItemHelpValues() -> [String] {
        guard let app = appElement(), let extras = axElement(attribute(app, "AXExtrasMenuBar")),
              let children = attribute(extras, kAXChildrenAttribute) as? [AXUIElement] else { return [] }
        return children.compactMap { attribute($0, "AXHelp") as? String }
    }

    private func processElements(bundleID: String) -> [AXUIElement] {
        NSRunningApplication.runningApplications(withBundleIdentifier: bundleID).flatMap { app in
            allElements(from: AXUIElementCreateApplication(app.processIdentifier))
        }
    }

    private func notificationTexts() -> [String] {
        let elements = processElements(bundleID: "com.apple.notificationcenterui")
        let banners = elements.filter {
            ["AXNotificationCenterBanner", "AXNotificationCenterAlert", "AXNotificationCenterAlertStack"].contains(attribute($0, kAXSubroleAttribute) as? String ?? "")
        }
        return banners.flatMap { allElements(from: $0) }.compactMap { element in
            guard attribute(element, kAXRoleAttribute) as? String == "AXStaticText" else { return nil }
            return attribute(element, kAXValueAttribute) as? String
        }
    }

    private func surfaceScreenshot(name: String, menubar: Bool) throws(DriverError) -> String {
        let url = runDir.appendingPathComponent("\(name).png")
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
        if menubar {
            guard let screen = NSScreen.screens.first, screen.frame.width > 0 else { throw .fatal("menu bar display width is unavailable") }
            process.arguments = ["-x", "-R0,0,\(Int(screen.frame.width)),40", url.path]
        } else {
            process.arguments = ["-x", url.path]
        }
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        do { try process.run() } catch { throw .fatal("screencapture failed: \(error.localizedDescription)") }
        process.waitUntilExit()
        guard process.terminationStatus == 0 else { throw .fatal("screencapture exited \(process.terminationStatus)") }
        return url.path
    }

    func visibleLabels() -> [String] {
        var seen = Set<String>()
        return allElements().flatMap(labelValues).filter { !$0.isEmpty && seen.insert($0).inserted }
    }

    func screenshot(to url: URL) throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
        var arguments = ["-x"]
        // crop to the app window's screen region (overlapping windows can still appear); a keychain prompt or a missing window needs the full screen
        let keychainPrompt = !NSRunningApplication.runningApplications(withBundleIdentifier: "com.apple.SecurityAgent").isEmpty
        if !keychainPrompt, let app = appElement(),
           let window = axElement(attribute(app, kAXFocusedWindowAttribute)) ?? (attribute(app, kAXWindowsAttribute) as? [AXUIElement])?.first,
           let rect = frame(of: window) {
            arguments += ["-R", "\(Int(rect.minX)),\(Int(rect.minY)),\(Int(rect.width)),\(Int(rect.height))"]
        }
        process.arguments = arguments + [url.path]
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

    private func popupMenuOpen() -> Bool {
        guard let pid = application?.processIdentifier else { return false }
        let layer = Int(CGWindowLevelForKey(.popUpMenuWindow))
        return (CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]])?.contains {
            ($0[kCGWindowOwnerPID as String] as? Int) == Int(pid) && ($0[kCGWindowLayer as String] as? Int) == layer
        } ?? false
    }

    private func waitUntil(timeout: TimeInterval, interval: TimeInterval = 0.1, _ condition: () -> Bool) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while !condition() {
            let remaining = deadline.timeIntervalSinceNow
            guard remaining > 0 else { return false }
            Thread.sleep(forTimeInterval: min(interval, remaining))
        }
        return true
    }

    private func appPicker() throws(DriverError) -> AXUIElement? {
        let popups = allElements().filter { attribute($0, kAXRoleAttribute) as? String == "AXPopUpButton" }
        let titled = popups.filter { attribute($0, kAXTitleAttribute) as? String == "App" }
        let candidates = titled.isEmpty
            ? popups.filter { attribute($0, kAXValueAttribute) as? String == "Choose an app…" }
            : titled
        switch candidates.count {
        case 0: return nil
        case 1: return candidates[0]
        case let count: throw .fatal("ambiguous: \(count) App pickers")
        }
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

    /// Every accessible label with its role, one entry per element in tree order: what `click` and `expect` can see.
    private func labelListing() -> String {
        let lines = allElements().filter { frame(of: $0) != nil }.flatMap { element -> [String] in
            let role = (attribute(element, kAXRoleAttribute) as? String ?? "AXUnknown").replacingOccurrences(of: "AX", with: "")
            var seen = Set<String>()  // title/description/value often repeat within one element; distinct elements stay separate for `click ... index N`
            // `click` needs exact labels, so only actionable roles keep full title/description; values and other text are bounded to avoid leaking content into logs
            let actionable = ["Button", "Link", "MenuItem", "MenuBarItem", "CheckBox", "RadioButton", "PopUpButton", "MenuButton", "Tab", "TabGroup", "DisclosureTriangle"].contains(role)
            let entries = [(kAXTitleAttribute, actionable), (kAXDescriptionAttribute, actionable), (kAXValueAttribute, false)].compactMap { name, full -> String? in
                guard let text = (attribute(element, name) as? String)?.trimmingCharacters(in: .whitespacesAndNewlines) else { return nil }
                return full || text.count <= 80 ? text : String(text.prefix(80)) + "…"
            }
            return entries.filter { !$0.isEmpty && seen.insert($0).inserted }.map { label in
                "\(role) \"\(label.replacingOccurrences(of: "\n", with: " "))\""
            }
        }
        return "\(lines.count) labels" + lines.map { "\n  " + $0 }.joined()
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
            let labelled = elements.filter { matches($0, exact: within) }
            // A container and its own heading often share a label (aria-label + <p>); keep the outermost
            let scopes = labelled.filter { scope in
                !labelled.contains { other in !CFEqual(other, scope) && allElements(from: other).contains { CFEqual($0, scope) } }
            }
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
        try guardClick(at: point)
        try postClick(at: point)
    }

    private func click(_ element: AXUIElement, at point: CGPoint) throws(DriverError) -> String? {
        do {
            try guardClick(at: point)
            try postClick(at: point)
            return nil
        } catch let error {
            guard case .retryable(let reason) = error,
                  ClickGuard.shouldPressAccessibility(reason: reason, keychainOpen: keychainWindow() != nil) else {
                throw error
            }
            guard AXUIElementPerformAction(element, kAXPressAction as CFString) == .success else { throw error }
            return "pressed via Accessibility (\(reason))"
        }
    }

    private func guardClick(at point: CGPoint) throws(DriverError) {
        if keychainWindow() != nil { throw .retryable("keychain prompt is covering the app") }
        guard let app = application.flatMap({ $0.isTerminated ? nil : $0 })
                ?? NSRunningApplication.runningApplications(withBundleIdentifier: "com.app.scribular.tauri").first else {
            throw .retryable("app is not frontmost (frontmost: \(NSWorkspace.shared.frontmostApplication?.localizedName ?? "unknown"))")
        }
        if NSWorkspace.shared.frontmostApplication?.processIdentifier != app.processIdentifier {
            let appElement = AXUIElementCreateApplication(app.processIdentifier)
            _ = AXUIElementSetAttributeValue(appElement, kAXFrontmostAttribute as CFString, kCFBooleanTrue)
            if let window = axElement(attribute(appElement, kAXMainWindowAttribute))
                    ?? (attribute(appElement, kAXWindowsAttribute) as? [AXUIElement])?.first {
                _ = AXUIElementPerformAction(window, kAXRaiseAction as CFString)
            }
            _ = app.activate(options: [.activateAllWindows])
            _ = waitUntil(timeout: 2) { NSWorkspace.shared.frontmostApplication?.processIdentifier == app.processIdentifier }
        }
        let frontmost = NSWorkspace.shared.frontmostApplication
        if let reason = ClickGuard.failure(targetPID: app.processIdentifier, hitPID: nil,
                                           hitName: nil,
                                           keychainPromptOpen: false,
                                           appIsFrontmost: frontmost?.processIdentifier == app.processIdentifier,
                                           frontmostName: frontmost?.localizedName,
                                           hitTestComplete: false) {
            throw .retryable(reason)
        }
        let system = AXUIElementCreateSystemWide()
        var hit: AXUIElement?
        guard AXUIElementCopyElementAtPosition(system, Float(point.x), Float(point.y), &hit) == .success,
              let hit else {
            throw .retryable("click point is covered by unknown app")
        }
        var hitPID: pid_t = 0
        guard AXUIElementGetPid(hit, &hitPID) == .success else {
            throw .retryable("click point is covered by unknown app")
        }
        let hitName = NSRunningApplication(processIdentifier: hitPID)?.localizedName
        if let reason = ClickGuard.failure(targetPID: app.processIdentifier, hitPID: hitPID,
                                           hitName: hitName,
                                           keychainPromptOpen: false, hitTestComplete: true) {
            throw .retryable(reason)
        }
    }

    private func postClick(at point: CGPoint) throws(DriverError) {
        for type in [CGEventType.mouseMoved, .leftMouseDown, .leftMouseUp] {
            guard let event = CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: point, mouseButton: .left) else {
                throw .fatal("could not create mouse event")
            }
            event.post(tap: .cghidEventTap)
        }
    }

    func frontmostApplicationName() -> String? { NSWorkspace.shared.frontmostApplication?.localizedName }

    /// SecurityAgent can linger after a prompt closes, so only an open window counts as a prompt.
    private func keychainWindow() -> AXUIElement? {
        guard let agent = NSRunningApplication.runningApplications(withBundleIdentifier: "com.apple.SecurityAgent").first else { return nil }
        return (attribute(AXUIElementCreateApplication(agent.processIdentifier), kAXWindowsAttribute) as? [AXUIElement])?.first
    }

    private func handleKeychain() throws(DriverError) -> String? {
        // the prompt can appear a moment after the app touches the keychain; wait up to 5s before concluding there is none
        var found: (AXUIElement)?
        for attempt in 0..<20 {
            if let first = keychainWindow() {
                found = first
                break
            }
            if attempt < 19 { Thread.sleep(forTimeInterval: 0.25) }
        }
        guard let window = found else { return "no prompt" }
        guard let keychainPassword else { throw .fatal("UITEST_KEYCHAIN_PASSWORD not set") }
        let fields = allElements(from: window).filter {
            let role = attribute($0, kAXRoleAttribute) as? String
            return role == kAXTextFieldRole || role == "AXSecureTextField"
        }
        let field: AXUIElement
        switch pick(fields, index: nil) {
        case .none: throw .fatal("keychain password field not found")
        case .ambiguous(let count): throw .fatal("ambiguous: \(count) keychain password fields")
        case .found(let value): field = value
        }
        guard let fieldRect = frame(of: field) else { throw .fatal("keychain password field not found") }
        try postClick(at: CGPoint(x: fieldRect.midX, y: fieldRect.midY))
        _ = try perform(.type(keychainPassword))
        let allow = allElements(from: window).filter { matches($0, exact: "Always Allow") }
        let button: AXUIElement
        switch pick(allow, index: nil) {
        case .none: throw .fatal("Always Allow button not found")
        case .ambiguous(let count): throw .fatal("ambiguous: \(count) Always Allow buttons")
        case .found(let value): button = value
        }
        guard let buttonRect = frame(of: button) else { throw .fatal("Always Allow button not found") }
        try postClick(at: CGPoint(x: buttonRect.midX, y: buttonRect.midY))
        return nil
    }
}
