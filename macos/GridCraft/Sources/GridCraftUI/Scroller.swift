import SwiftUI

/// Where the grid is scrolled across, for the scroller (sheet units past the frozen columns).
@MainActor @Observable
public final class GridScrollState {
    /// The scroll offset.
    public var x: CGFloat = 0
    /// How far it can scroll now: through the used range and some more, or to where it is.
    public var extent: CGFloat = 0
    /// How much of the sheet the grid shows across.
    public var visible: CGFloat = 0

    public init() {}

    func update(x: CGFloat, extent: CGFloat, visible: CGFloat) {
        if self.x != x { self.x = x }
        if self.extent != extent { self.extent = extent }
        if self.visible != visible { self.visible = visible }
    }
}

/// The grid's horizontal scroller, a capsule of glass in the status bar (where Excel has it, by
/// the sheet tabs): drag the knob, or click beside it to scroll a screenful.
struct HorizontalScroller: View {
    let state: GridScrollState
    /// Scrolls the grid to an offset.
    let scroll: (CGFloat) -> Void
    /// The offset and extent when a drag began (the extent stays put while dragging).
    @State private var dragStart: (x: CGFloat, extent: CGFloat)?
    @State private var hovering = false

    private static let inset: CGFloat = 10
    private static let minimumKnob: CGFloat = 28

    var body: some View {
        GeometryReader { proxy in
            let track = max(0, proxy.size.width - Self.inset * 2)
            let extent = dragStart?.extent ?? state.extent
            let knob = min(track, max(Self.minimumKnob, track * state.visible / max(extent + state.visible, 1)))
            let travel = max(0, track - knob)
            let offset = extent > 0 ? travel * min(max(state.x / extent, 0), 1) : 0
            ZStack(alignment: .leading) {
                // Not quite clear: inside the glass a clear view takes no clicks.
                Rectangle().fill(.white.opacity(0.001))
                Capsule()
                    .fill(.secondary.opacity(hovering || dragStart != nil ? 0.7 : 0.45))
                    .frame(width: knob, height: hovering || dragStart != nil ? 9 : 7)
                    .offset(x: Self.inset + offset)
            }
            .contentShape(.rect)
            // One gesture for the whole track: a press on the knob drags it; a click beside it
            // scrolls a screenful that way.
            .gesture(
                DragGesture(minimumDistance: 0, coordinateSpace: .local)
                    .onChanged { drag in
                        let onKnob = (Self.inset + offset - 6...Self.inset + offset + knob + 6).contains(drag.startLocation.x)
                        guard onKnob else { return }
                        let start = dragStart ?? (state.x, state.extent)
                        dragStart = start
                        guard travel > 0 else { return }
                        scroll(max(0, start.x + drag.translation.width / travel * start.extent))
                    }
                    .onEnded { drag in
                        defer { dragStart = nil }
                        guard dragStart == nil, abs(drag.translation.width) < 4 else { return }
                        if drag.location.x < Self.inset + offset {
                            scroll(max(0, state.x - state.visible))
                        } else if drag.location.x > Self.inset + offset + knob {
                            scroll(state.x + state.visible)
                        }
                    }
            )
            .frame(maxHeight: .infinity)
        }
        .frame(height: 28)
        .glassEffect(.regular, in: .capsule)
        .onHover { hovering = $0 }
        .animation(.smooth(duration: 0.15), value: hovering)
        .accessibilityElement()
        .accessibilityLabel("Horizontal scroll")
        .accessibilityValue(state.extent > 0 ? "\(Int((state.x / state.extent * 100).rounded()))%" : "0%")
        .accessibilityAdjustableAction { direction in
            switch direction {
            case .increment: scroll(state.x + state.visible)
            case .decrement: scroll(max(0, state.x - state.visible))
            @unknown default: break
            }
        }
    }
}
