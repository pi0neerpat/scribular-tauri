import CoreGraphics
import Testing
@testable import UITestCore

@Test func recordWindowFlagRequiresRecord() throws {
    #expect(try UITestOptions.parse(["scenario.uitest"]).recordWindow == false)
    let options = try UITestOptions.parse(["scenario.uitest", "--record", "02-theme-happy-path", "--record-window"])
    #expect(options.recordWindow)
    #expect(options.recordingName == "02-theme-happy-path")
    #expect(throws: UITestOptionsError.self) { try UITestOptions.parse(["scenario.uitest", "--record-window"]) }
    #expect(throws: UITestOptionsError.self) {
        try UITestOptions.parse(["scenario.uitest", "--record", "x", "--record-window", "--record-window"])
    }
}

@Test func windowCropUsesUnionOfWindowsScaledToPixels() {
    var crop = WindowCrop()
    #expect(crop.filter(display: CGSize(width: 1440, height: 900), scale: 2) == nil)
    crop.add([CGRect(x: 100, y: 50, width: 800, height: 600)])
    crop.add([CGRect(x: 300, y: 200, width: 900, height: 500), CGRect(x: 0, y: 0, width: 0, height: 0)])
    // union is x 100...1200, y 50...700 in points; Retina doubles it
    #expect(crop.filter(display: CGSize(width: 1440, height: 900), scale: 2) == "crop=2200:1300:200:100")
    #expect(crop.filter(display: CGSize(width: 1440, height: 900), scale: 1) == "crop=1100:650:100:50")
}

@Test func windowCropClampsToDisplayAndKeepsEvenDimensions() {
    var crop = WindowCrop()
    crop.add([CGRect(x: -20, y: 11, width: 1001, height: 2000)])
    // clamped to x 0...981, y 11...900 on a 1495x900 display at 1x; odd values round down to even
    #expect(crop.filter(display: CGSize(width: 1495, height: 900), scale: 1) == "crop=980:888:0:10")
    var offscreen = WindowCrop()
    offscreen.add([CGRect(x: 2000, y: 0, width: 100, height: 100)])
    #expect(offscreen.filter(display: CGSize(width: 1440, height: 900), scale: 1) == nil)
}
