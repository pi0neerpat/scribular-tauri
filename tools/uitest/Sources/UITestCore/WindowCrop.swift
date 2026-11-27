import CoreGraphics

/// Accumulates the app's window frames seen during a `--record-window` run and turns their union
/// into an ffmpeg crop for the full-display recording.
public struct WindowCrop: Sendable {
    public private(set) var union: CGRect?

    public init() {}

    /// Adds window frames in display points (top-left origin, as Accessibility reports them).
    public mutating func add(_ frames: [CGRect]) {
        for frame in frames where frame.width > 0 && frame.height > 0 {
            union = union.map { $0.union(frame) } ?? frame
        }
    }

    /// The ffmpeg `crop=w:h:x:y` filter for the union, clamped to the display and scaled from points
    /// to video pixels, with even dimensions for yuv420p. `nil` when no window was seen.
    public func filter(display: CGSize, scale: CGFloat) -> String? {
        guard let union, scale > 0 else { return nil }
        let clamped = union.intersection(CGRect(origin: .zero, size: display))
        guard !clamped.isNull, clamped.width >= 2, clamped.height >= 2 else { return nil }
        func even(_ value: CGFloat) -> Int { Int(value) / 2 * 2 }
        let x = even((clamped.minX * scale).rounded(.down))
        let y = even((clamped.minY * scale).rounded(.down))
        let width = even((clamped.width * scale).rounded(.down))
        let height = even((clamped.height * scale).rounded(.down))
        guard width >= 2, height >= 2 else { return nil }
        return "crop=\(width):\(height):\(x):\(y)"
    }
}
