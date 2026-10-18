import AppKit
import SwiftUI

/// Where the grid is scrolled, for the scrollers (sheet units past the frozen panes).
@MainActor @Observable
public final class GridScrollState {
    /// The scroll offset across and down.
    public var x: CGFloat = 0
    public var y: CGFloat = 0
    /// How far it can scroll now: through the used range and some more, or to where it is.
    public var extentX: CGFloat = 0
    public var extentY: CGFloat = 0
    /// How much of the sheet the grid shows.
    public var visibleX: CGFloat = 0
    public var visibleY: CGFloat = 0
    /// Where the scrolled cells start below the grid's top (the column headers and frozen rows).
    public var cellsTop: CGFloat = 0

    public init() {}

    func update(x: CGFloat, y: CGFloat, extentX: CGFloat, extentY: CGFloat, visibleX: CGFloat, visibleY: CGFloat, cellsTop: CGFloat) {
        if self.x != x { self.x = x }
        if self.y != y { self.y = y }
        if self.extentX != extentX { self.extentX = extentX }
        if self.extentY != extentY { self.extentY = extentY }
        if self.visibleX != visibleX { self.visibleX = visibleX }
        if self.visibleY != visibleY { self.visibleY = visibleY }
        if self.cellsTop != cellsTop { self.cellsTop = cellsTop }
    }

    /// The offset, extent and visible length along `axis`.
    func along(_ axis: Axis) -> (offset: CGFloat, extent: CGFloat, visible: CGFloat) {
        axis == .horizontal ? (x, extentX, visibleX) : (y, extentY, visibleY)
    }
}

/// One of the grid's scrollers, a capsule of glass: across in the status bar (where Excel has it,
/// by the sheet tabs), down along the grid's right edge. Drag the knob, or click beside it to
/// scroll a screenful; the scroll wheel over it scrolls the grid.
struct GlassScroller: View {
    let axis: Axis
    let state: GridScrollState
    /// Scrolls the grid to an offset along the axis.
    let scroll: (CGFloat) -> Void
    /// A scroll-wheel event over the scroller.
    let wheel: (NSEvent) -> Void
    /// The offset and extent when a drag began (the extent stays put while dragging).
    @State private var dragStart: (offset: CGFloat, extent: CGFloat)?
    @State private var hovering = false
    /// The vertical knob shows for a moment while the sheet scrolls under the wheel.
    @State private var scrolling = false
    @State private var hideTask: Task<Void, Never>?

    /// The capsule's thickness.
    static func thickness(_ axis: Axis) -> CGFloat { axis == .horizontal ? 28 : 18 }
    private static let inset: CGFloat = 4
    private static let minimumKnob: CGFloat = 28

    var body: some View {
        GeometryReader { proxy in
            let length = axis == .horizontal ? proxy.size.width : proxy.size.height
            let now = state.along(axis)
            let track = max(0, length - Self.inset * 2)
            let extent = dragStart?.extent ?? now.extent
            let knob = min(track, max(Self.minimumKnob, track * now.visible / max(extent + now.visible, 1)))
            let travel = max(0, track - knob)
            let offset = extent > 0 ? travel * min(max(now.offset / extent, 0), 1) : 0
            let active = hovering || scrolling || dragStart != nil
            let thin: CGFloat = active ? 9 : 7
            ZStack(alignment: axis == .horizontal ? .leading : .top) {
                Color.clear
                // Only the knob, and only once the pointer is near (or it is being dragged).
                Capsule()
                    .fill(.secondary.opacity(0.7))
                    .frame(width: axis == .horizontal ? knob : thin, height: axis == .horizontal ? thin : knob)
                    .offset(x: axis == .horizontal ? Self.inset + offset : 0, y: axis == .horizontal ? 0 : Self.inset + offset)
                    .opacity(active ? 1 : 0)
            }
            // The mouse in AppKit: once pressed, the scroller gets every drag until the release,
            // wherever the pointer goes (a SwiftUI gesture inside the glass lost the drag when
            // the pointer left the capsule). A press on the knob drags it; a click beside it
            // scrolls a screenful that way.
            .overlay {
                ScrollerMouse(
                    axis: axis,
                    down: { at in
                        if (Self.inset + offset - 6...Self.inset + offset + knob + 6).contains(at) {
                            dragStart = (now.offset, now.extent)
                        }
                    },
                    drag: { moved in
                        guard let start = dragStart, travel > 0 else { return }
                        scroll(max(0, start.offset + moved / travel * start.extent))
                    },
                    up: { at, moved in
                        defer { dragStart = nil }
                        guard dragStart == nil, abs(moved) < 4 else { return }
                        if at < Self.inset + offset {
                            scroll(max(0, now.offset - now.visible))
                        } else if at > Self.inset + offset + knob {
                            scroll(now.offset + now.visible)
                        }
                    },
                    wheel: wheel
                )
            }
        }
        .frame(width: axis == .vertical ? Self.thickness(axis) : nil, height: axis == .horizontal ? Self.thickness(axis) : nil)
        .contentShape(.rect)
        .onHover { hovering = $0 }
        .animation(.smooth(duration: 0.2), value: hovering)
        .animation(.smooth(duration: 0.2), value: scrolling)
        .onChange(of: state.y) {
            guard axis == .vertical else { return }
            scrolling = true
            hideTask?.cancel()
            hideTask = Task {
                try? await Task.sleep(for: .seconds(1))
                if !Task.isCancelled { scrolling = false }
            }
        }
        .accessibilityElement()
        .accessibilityLabel(axis == .horizontal ? "Horizontal scroll" : "Vertical scroll")
        .accessibilityValue(state.along(axis).extent > 0 ? "\(Int((state.along(axis).offset / state.along(axis).extent * 100).rounded()))%" : "0%")
        .accessibilityAdjustableAction { direction in
            let now = state.along(axis)
            switch direction {
            case .increment: scroll(now.offset + now.visible)
            case .decrement: scroll(max(0, now.offset - now.visible))
            @unknown default: break
            }
        }
    }
}

/// A scroller's mouse handling: AppKit sends a view that took the mouse down every drag and
/// the release, even with the pointer outside it.
struct ScrollerMouse: NSViewRepresentable {
    let axis: Axis
    /// Pressed at a point along the axis (from the scroller's start).
    let down: (CGFloat) -> Void
    /// Dragged this far along the axis since the press.
    let drag: (CGFloat) -> Void
    /// Released at a point, this far from the press.
    let up: (CGFloat, CGFloat) -> Void
    /// The scroll wheel over it.
    let wheel: (NSEvent) -> Void

    func makeNSView(context: Context) -> ScrollerMouseView {
        ScrollerMouseView(frame: .zero)
    }

    func updateNSView(_ view: ScrollerMouseView, context: Context) {
        view.axis = axis
        view.down = down
        view.drag = drag
        view.up = up
        view.wheel = wheel
    }
}

final class ScrollerMouseView: NSView {
    var axis: Axis = .horizontal
    var down: ((CGFloat) -> Void)?
    var drag: ((CGFloat) -> Void)?
    var up: ((CGFloat, CGFloat) -> Void)?
    var wheel: ((NSEvent) -> Void)?
    private var pressedAt: CGFloat?

    override var isFlipped: Bool { true }

    /// A click on the scroller of a window in the background scrolls it too.
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    private func along(_ event: NSEvent) -> CGFloat {
        let p = convert(event.locationInWindow, from: nil)
        return axis == .horizontal ? p.x : p.y
    }

    override func mouseDown(with event: NSEvent) {
        let at = along(event)
        pressedAt = at
        down?(at)
    }

    override func mouseDragged(with event: NSEvent) {
        guard let pressedAt else { return }
        drag?(along(event) - pressedAt)
    }

    override func mouseUp(with event: NSEvent) {
        guard let pressedAt else { return }
        let at = along(event)
        self.pressedAt = nil
        up?(at, at - pressedAt)
    }

    override func scrollWheel(with event: NSEvent) {
        wheel?(event)
    }
}
