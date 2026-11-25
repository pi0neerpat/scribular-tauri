import Foundation
import Testing
@testable import UITestCore

@Test func processExitProbeDistinguishesRunningAndExitedChild() throws {
    let child = Process()
    child.executableURL = URL(fileURLWithPath: "/bin/cat")
    let input = Pipe()
    child.standardInput = input
    try child.run()
    let pid = child.processIdentifier
    #expect(!processHasExited(pid))
    try input.fileHandleForWriting.close()
    child.waitUntilExit()
    #expect(processHasExited(pid))
    #expect(!processHasExited(-1))
}
