import AppKit
import CoreText
import GridCraftEngine

/// Draws the active sheet's grid as the engine lays it out: headers, gridlines, the cells of each
/// pane (fills, text, data bars, icons, checkboxes, borders, comment marks, sparklines, filter
/// buttons), the frozen-pane lines and the selection. All appearance decisions are the engine's;
/// this only places and paints.
///
/// Draws into a flipped context (origin at the top left), in points.
@MainActor
public struct GridRenderer {
    /// Points per sheet unit at 100%: a default 20-unit row is 16 pt tall, desktop density.
    public static let displayScale: Float = 0.8

    public let engine: Engine
    public let measurer: CoreTextMeasurer
    /// The interface's formula syntax for Show Formulas (`en`, `es`).
    public var formulaLocale = "en"
    /// A cell is being edited: no fill handle.
    public var editing = false
    /// The selected floating object (it gets a selection frame).
    public var selectedObject: UInt32?
    /// Draw the column headers. Off under the window's frosted top, which draws them itself.
    public var columnHeaders = true
    /// The row headers' width, when it must match another layout's.
    public var headerWidth: CGFloat?

    public init(engine: Engine, measurer: CoreTextMeasurer) {
        self.engine = engine
        self.measurer = measurer
    }

    /// Where the cells start and how big a sheet unit is, for a view of `size`.
    public struct Layout {
        public var headerWidth: CGFloat
        public var headerHeight: CGFloat
        public var scale: Float
        public var geometry: GridGeometry
    }

    /// The layout for a view of `size` points scrolled to `scroll` (sheet units past the frozen panes).
    public func layout(size: CGSize, scroll: CGPoint) throws -> Layout {
        let sheet = try engine.sheet()
        let zoom = max(0.1, min(4, Float(sheet.zoom) / 100))
        let scale = zoom * Self.displayScale
        let chrome = CGFloat(max(zoom, 0.75))
        let headerHeight = sheet.showHeadings ? (21 * chrome).rounded() : 0
        var headerWidth: CGFloat = sheet.showHeadings ? ((2 * 7 + 14) * chrome).rounded() : 0
        var geometry = try engine.geometry(origin: point(headerWidth, headerHeight), size: gridSize(size, headerWidth, headerHeight), scroll: point(scroll.x, scroll.y), scale: scale)
        if sheet.showHeadings, let fixed = self.headerWidth, fixed != headerWidth {
            headerWidth = fixed
            geometry = try engine.geometry(origin: point(headerWidth, headerHeight), size: gridSize(size, headerWidth, headerHeight), scroll: point(scroll.x, scroll.y), scale: scale)
        } else if sheet.showHeadings {
            // The row headers are as wide as the bottom row's number needs.
            let lastRow = geometry.rowAt(y: Float(size.height)) + 1
            let digits = max(2, String(lastRow).count)
            let width = ((CGFloat(digits) * 7 + 14) * chrome).rounded()
            if width != headerWidth {
                headerWidth = width
                geometry = try engine.geometry(origin: point(headerWidth, headerHeight), size: gridSize(size, headerWidth, headerHeight), scroll: point(scroll.x, scroll.y), scale: scale)
            }
        }
        // In Page Layout view cells are drawn at the print scale too.
        return Layout(headerWidth: headerWidth, headerHeight: headerHeight, scale: geometry.scale(), geometry: geometry)
    }

    /// Draws the grid; returns the layout it used (for hit-testing).
    @discardableResult
    public func draw(in ctx: CGContext, size: CGSize, scroll: CGPoint) throws -> Layout {
        let layout = try self.layout(size: size, scroll: scroll)
        let sheet = try engine.sheet()
        let geometry = layout.geometry
        let selection = try engine.selection()
        ctx.setFillColor(Palette.sheet)
        ctx.fill(CGRect(origin: .zero, size: size))
        drawPages(ctx, geometry.pages(), cells: CGRect(x: layout.headerWidth, y: layout.headerHeight, width: max(0, size.width - layout.headerWidth),
                                                        height: max(0, size.height - layout.headerHeight)))
        for pane in geometry.panes() {
            ctx.saveGState()
            ctx.clip(to: cgRect(pane.rect))
            if sheet.showGridlines {
                drawGridlines(ctx, pane: pane, geometry: geometry, scale: layout.scale)
            }
            let query = ViewportQuery(sheet: nil, rows: pane.rows, cols: pane.cols, scale: layout.scale, formulaLocale: formulaLocale)
            let viewport = try engine.viewport(request: query, measurer: measurer)
            drawCells(ctx, viewport: viewport, geometry: geometry, scale: layout.scale)
            drawSelection(ctx, selection: selection, geometry: geometry, pane: pane)
            ctx.restoreGState()
        }
        let cells = CGRect(x: layout.headerWidth, y: layout.headerHeight, width: max(0, size.width - layout.headerWidth), height: max(0, size.height - layout.headerHeight))
        ObjectRenderer(engine: engine, measurer: measurer).draw(ctx, geometry: geometry, scale: layout.scale, cells: cells, selected: selectedObject)
        drawFrozenLines(ctx, geometry: geometry, size: size)
        if sheet.showHeadings {
            drawHeaders(ctx, layout: layout, size: size, selection: selection)
        }
        return layout
    }

    // MARK: - Pages

    /// Page Layout view: the desk, and each page as paper with a soft shadow; pages without
    /// printed data are faded.
    private func drawPages(_ ctx: CGContext, _ pages: [PageRect], cells: CGRect) {
        guard !pages.isEmpty else { return }
        ctx.saveGState()
        ctx.clip(to: cells)
        ctx.setFillColor(Palette.desk)
        ctx.fill(cells)
        for page in pages {
            let paper = cgRect(page.paper)
            ctx.saveGState()
            ctx.setShadow(offset: CGSize(width: 0, height: 1.5), blur: 4, color: CGColor(gray: 0, alpha: 0.25))
            ctx.setFillColor(page.data ? Palette.sheet : CGColor(gray: 0.985, alpha: 1))
            ctx.fill(paper)
            ctx.restoreGState()
        }
        ctx.restoreGState()
    }

    // MARK: - Gridlines

    private func drawGridlines(_ ctx: CGContext, pane: GridPane, geometry: GridGeometry, scale: Float) {
        let top = CGFloat(geometry.y(row: pane.rows.first))
        let bottom = CGFloat(geometry.y(row: pane.rows.last &+ 1))
        let left = CGFloat(geometry.x(col: pane.cols.first))
        let right = CGFloat(geometry.x(col: pane.cols.last &+ 1))
        ctx.setStrokeColor(Palette.gridline)
        ctx.setLineWidth(1)
        ctx.beginPath()
        var previous = -CGFloat.infinity
        for col in pane.cols.first...pane.cols.last &+ 1 {
            let x = CGFloat(geometry.x(col: col)).rounded() - 0.5
            // Hidden columns take no room: one line, not several on top of each other.
            if x == previous { continue }
            previous = x
            ctx.move(to: CGPoint(x: x, y: top))
            ctx.addLine(to: CGPoint(x: x, y: bottom))
        }
        previous = -CGFloat.infinity
        for row in pane.rows.first...pane.rows.last &+ 1 {
            let y = CGFloat(geometry.y(row: row)).rounded() - 0.5
            if y == previous { continue }
            previous = y
            ctx.move(to: CGPoint(x: left, y: y))
            ctx.addLine(to: CGPoint(x: right, y: y))
        }
        ctx.strokePath()
    }

    // MARK: - Cells

    private func drawCells(_ ctx: CGContext, viewport: Viewport, geometry: GridGeometry, scale: Float) {
        let z = CGFloat(scale)
        let rects = viewport.cells.map { cgRect(geometry.rangeRect(range: $0.span)) }
        // Fills; merged areas are blanked (no inner gridlines) and painted as one cell.
        for (cell, r) in zip(viewport.cells, rects) {
            if let fill = cell.fill {
                ctx.setFillColor(cgColor(fill))
                ctx.fill(r)
            } else if cell.merged {
                ctx.setFillColor(Palette.sheet)
                ctx.fill(r.insetBy(dx: 0.5, dy: 0.5))
            }
        }
        // Decorations and text.
        for (cell, r) in zip(viewport.cells, rects) {
            if let bar = cell.dataBar {
                drawDataBar(ctx, bar, in: r)
            }
            if let icon = cell.icon {
                drawIcon(ctx, icon, in: CGRect(x: r.minX + 2, y: r.midY - 6 * z, width: 12 * z, height: 12 * z))
            }
            if let checked = cell.checkbox {
                drawCheckbox(ctx, checked: checked, in: r, scale: z)
                continue
            }
            if let text = cell.text {
                drawText(ctx, text, in: r, geometry: geometry, scale: z)
            }
        }
        // Borders over fills and text, then comment marks.
        for (cell, r) in zip(viewport.cells, rects) {
            for segment in cell.borders {
                drawBorder(ctx, segment, cell: cell, in: r, geometry: geometry)
            }
            if let mark = cell.comment {
                let s = 6 * max(z, 0.7)
                ctx.setFillColor(mark == .threaded ? Palette.threadedComment : Palette.note)
                ctx.beginPath()
                ctx.move(to: CGPoint(x: r.maxX, y: r.minY))
                ctx.addLine(to: CGPoint(x: r.maxX, y: r.minY + s))
                ctx.addLine(to: CGPoint(x: r.maxX - s, y: r.minY))
                ctx.closePath()
                ctx.fillPath()
            }
        }
        for sparkline in viewport.sparklines {
            drawSparkline(ctx, sparkline, in: cgRect(geometry.cellRect(cell: sparkline.cell)).insetBy(dx: 3, dy: 3))
        }
        for button in viewport.filterButtons {
            drawFilterButton(ctx, active: button.active, in: cgRect(geometry.filterButton(cell: button.cell)))
        }
    }

    private func drawText(_ ctx: CGContext, _ text: CellText, in r: CGRect, geometry: GridGeometry, scale z: CGFloat) {
        let font = measurer.font(text.face, size: text.size)
        let color = cgColor(text.color)
        // The content area stops before a filter button.
        let area = CGRect(x: r.minX, y: r.minY, width: max(0, CGFloat(text.right)), height: r.height)
        ctx.saveGState()
        defer { ctx.restoreGState() }
        if case let .wrap(width, anchor) = text.layout {
            ctx.clip(to: area)
            let lines = wrappedLines(text.text, font: font, color: color, width: CGFloat(width))
            let lineHeight = lines.map(\.height).max() ?? 0
            let total = lineHeight * CGFloat(lines.count)
            var y = wrappedTop(text.valign, area: area, height: total, scale: z)
            for line in lines {
                let x: CGFloat
                switch text.halign {
                case .center, .centerAcross: x = r.minX + CGFloat(anchor) - line.width / 2
                case .right: x = r.minX + CGFloat(anchor) - line.width
                default: x = r.minX + CGFloat(anchor)
                }
                draw(line.line, ctx, at: CGPoint(x: x, y: y + line.ascent))
                y += lineHeight
            }
            return
        }
        let clip: CGRect
        if let spill = text.spill {
            let left = CGFloat(geometry.x(col: spill.first)), right = CGFloat(geometry.x(col: spill.last &+ 1))
            clip = CGRect(x: left, y: r.minY, width: right - left, height: r.height)
        } else {
            clip = area
        }
        if text.coverGridlines {
            // Text overflowing into empty neighbours covers the gridlines it crosses.
            ctx.setFillColor(Palette.sheet)
            ctx.fill(CGRect(x: clip.minX + 1, y: clip.minY + 0.5, width: max(0, clip.width - 2), height: max(0, clip.height - 1.5)))
        }
        ctx.clip(to: clip)
        let line = makeLine(text.text, font: font, color: color)
        let (ascent, descent) = bounds(line)
        if case let .rotated(wrap) = text.layout, text.rotation != 0 {
            drawRotated(ctx, text, font: font, color: color, wrap: wrap.map { CGFloat($0) }, in: area)
            return
        }
        let x = r.minX + CGFloat(text.x)
        let baseline = lineTop(text.valign, area: area, height: ascent + descent, scale: z) + ascent
        draw(line, ctx, at: CGPoint(x: x, y: baseline))
        let width = CGFloat(text.width)
        ctx.setStrokeColor(color)
        ctx.setLineWidth(1)
        switch text.underline {
        case .none: break
        case .double, .doubleAccounting:
            strokeLine(ctx, from: CGPoint(x: x, y: baseline + 1.5), to: CGPoint(x: x + width, y: baseline + 1.5))
            strokeLine(ctx, from: CGPoint(x: x, y: baseline + 3.5), to: CGPoint(x: x + width, y: baseline + 3.5))
        default:
            strokeLine(ctx, from: CGPoint(x: x, y: baseline + 1.5), to: CGPoint(x: x + width, y: baseline + 1.5))
        }
        if text.strike {
            let y = baseline - ascent * 0.32
            strokeLine(ctx, from: CGPoint(x: x, y: y), to: CGPoint(x: x + width, y: y))
        }
    }

    /// Rotated text: turned about its box's corner so the turned box sits in the cell by its
    /// alignment (upward text at the bottom, downward text at the top under Bottom).
    private func drawRotated(_ ctx: CGContext, _ text: CellText, font: CTFont, color: CGColor, wrap: CGFloat?, in area: CGRect) {
        let lines = wrap.map { wrappedLines(text.text, font: font, color: color, width: $0) } ?? [measured(makeLine(text.text, font: font, color: color))]
        let lineHeight = lines.map(\.height).max() ?? 0
        let w = lines.map(\.width).max() ?? 0, h = lineHeight * CGFloat(lines.count)
        let angle = CGFloat(text.rotation) * .pi / 180
        // The box turned counter-clockwise (y grows downward on screen).
        let (c, s) = (cos(angle), sin(angle))
        let corners = [CGPoint(x: 0, y: 0), CGPoint(x: w, y: 0), CGPoint(x: 0, y: h), CGPoint(x: w, y: h)].map { CGPoint(x: $0.x * c + $0.y * s, y: -$0.x * s + $0.y * c) }
        let minX = corners.map(\.x).min() ?? 0, maxX = corners.map(\.x).max() ?? 0
        let minY = corners.map(\.y).min() ?? 0, maxY = corners.map(\.y).max() ?? 0
        let pad = CGFloat(text.pad), indent = CGFloat(text.indent)
        let x: CGFloat
        switch text.ownHalign {
        case .center, .centerAcross, .distributed: x = area.midX - (maxX - minX) / 2
        case .right: x = area.maxX - pad - indent - (maxX - minX)
        default: x = area.minX + pad + indent
        }
        let y: CGFloat
        switch text.valign {
        case .center, .justify, .distributed: y = area.midY - (maxY - minY) / 2
        case .bottom where angle > 0: y = area.maxY - pad - (maxY - minY)
        default: y = area.minY + pad
        }
        ctx.translateBy(x: x - minX, y: y - minY)
        ctx.rotate(by: -angle)
        var lineY: CGFloat = 0
        for line in lines {
            draw(line.line, ctx, at: CGPoint(x: 0, y: lineY + line.ascent))
            lineY += lineHeight
        }
    }

    private func lineTop(_ valign: VAlign, area: CGRect, height: CGFloat, scale z: CGFloat) -> CGFloat {
        let top = area.minY + 1 * z
        let bottom = max(top, area.maxY - height - 1.5 * z)
        switch valign {
        case .top: return top
        case .center, .justify, .distributed: return (top + bottom) / 2
        case .bottom: return bottom
        }
    }

    private func wrappedTop(_ valign: VAlign, area: CGRect, height: CGFloat, scale z: CGFloat) -> CGFloat {
        let top = area.minY + 1.5 * z
        let bottom = max(top, area.maxY - height - 1.5 * z)
        switch valign {
        case .top: return top
        case .center, .justify, .distributed: return (top + bottom) / 2
        case .bottom: return bottom
        }
    }

    // MARK: - Decorations

    private func drawDataBar(_ ctx: CGContext, _ bar: DataBar, in r: CGRect) {
        let fraction = CGFloat(max(0, min(1, bar.fraction)))
        let box = CGRect(x: r.minX + 2, y: r.minY + 2, width: max(0, (r.width - 4) * fraction), height: max(0, r.height - 4))
        let color = cgColor(bar.color)
        guard bar.gradient else {
            ctx.setFillColor(color)
            ctx.fill(box)
            return
        }
        let light = cgColor(Rgb(r: UInt8((UInt16(bar.color.r) + 765) / 4), g: UInt8((UInt16(bar.color.g) + 765) / 4), b: UInt8((UInt16(bar.color.b) + 765) / 4)))
        if let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: [color, light] as CFArray, locations: [0, 1]) {
            ctx.saveGState()
            ctx.clip(to: box)
            ctx.drawLinearGradient(gradient, start: CGPoint(x: box.minX, y: box.midY), end: CGPoint(x: box.maxX, y: box.midY), options: [])
            ctx.restoreGState()
        }
        ctx.setStrokeColor(color)
        ctx.setLineWidth(1)
        ctx.stroke(box.insetBy(dx: 0.5, dy: 0.5))
    }

    private func drawIcon(_ ctx: CGContext, _ icon: CfIcon, in r: CGRect) {
        let n = max(Int(icon.count), 1), i = Int(icon.index)
        let t = CGFloat(i) / CGFloat(max(n - 1, 1))
        let palette3 = [Rgb(r: 0xD1, g: 0x3B, b: 0x2F), Rgb(r: 0xF2, g: 0xB8, b: 0x2E), Rgb(r: 0x2E, g: 0x9E, b: 0x4F)]
        let rgb: Rgb
        if n <= 3 {
            rgb = i < palette3.count ? palette3[i] : Rgb(r: 128, g: 128, b: 128)
        } else {
            rgb = Rgb(r: UInt8(0xD1 * (1 - t) + 0x2E * t), g: UInt8(0x3B * (1 - t) + 0x9E * t), b: UInt8(0x2F * (1 - t) + 0x4F * t))
        }
        let set = icon.set.lowercased()
        let c = CGPoint(x: r.midX, y: r.midY), s = r.width / 2
        if set.contains("arrow") {
            let points: [CGPoint]
            if i + 1 == n {
                points = [CGPoint(x: c.x, y: c.y - s), CGPoint(x: c.x + s, y: c.y + 0.1 * s), CGPoint(x: c.x - s, y: c.y + 0.1 * s)]
            } else if i == 0 {
                points = [CGPoint(x: c.x, y: c.y + s), CGPoint(x: c.x - s, y: c.y - 0.1 * s), CGPoint(x: c.x + s, y: c.y - 0.1 * s)]
            } else {
                points = [CGPoint(x: c.x + s, y: c.y), CGPoint(x: c.x - 0.1 * s, y: c.y - s), CGPoint(x: c.x - 0.1 * s, y: c.y + s)]
            }
            ctx.setFillColor(cgColor(rgb))
            ctx.beginPath()
            ctx.addLines(between: points)
            ctx.closePath()
            ctx.fillPath()
        } else if set.contains("rating") || set.contains("quarter") {
            ctx.setStrokeColor(Palette.secondary)
            ctx.strokeEllipse(in: r.insetBy(dx: 0.5, dy: 0.5))
            let h = (r.height - 4) * t
            ctx.setFillColor(cgColor(Rgb(r: 0x2B, g: 0x7C, b: 0xD3)))
            ctx.fill(CGRect(x: r.minX + 2, y: r.maxY - 2 - h, width: r.width - 4, height: h))
        } else {
            ctx.setFillColor(cgColor(rgb))
            ctx.fillEllipse(in: r.insetBy(dx: 0.5, dy: 0.5))
        }
    }

    private func drawCheckbox(_ ctx: CGContext, checked: Bool, in r: CGRect, scale z: CGFloat) {
        let s = max(8, min(24, 11 * z * 1.2))
        let box = CGRect(x: r.midX - s / 2, y: r.midY - s / 2, width: s, height: s)
        let path = CGPath(roundedRect: box, cornerWidth: 2, cornerHeight: 2, transform: nil)
        ctx.addPath(path)
        ctx.setFillColor(checked ? Palette.checkbox : Palette.sheet)
        ctx.fillPath()
        ctx.addPath(path)
        ctx.setStrokeColor(checked ? Palette.checkbox : Palette.secondary)
        ctx.setLineWidth(1)
        ctx.strokePath()
        if checked {
            ctx.setStrokeColor(CGColor(gray: 1, alpha: 1))
            ctx.setLineWidth(max(1.5, s / 8))
            ctx.beginPath()
            ctx.move(to: CGPoint(x: box.minX + s * 0.25, y: box.midY))
            ctx.addLine(to: CGPoint(x: box.minX + s * 0.43, y: box.maxY - s * 0.28))
            ctx.addLine(to: CGPoint(x: box.maxX - s * 0.22, y: box.minY + s * 0.28))
            ctx.strokePath()
        }
    }

    private func drawBorder(_ ctx: CGContext, _ b: BorderSegment, cell: CellView, in r: CGRect, geometry: GridGeometry) {
        // A merged area's edges come in segments, one per visible cell along them.
        let x0 = cell.merged ? CGFloat(geometry.x(col: b.from)) : r.minX
        let x1 = cell.merged ? CGFloat(geometry.x(col: b.to &+ 1)) : r.maxX
        let y0 = cell.merged ? CGFloat(geometry.y(row: b.from)) : r.minY
        let y1 = cell.merged ? CGFloat(geometry.y(row: b.to &+ 1)) : r.maxY
        let (a, e): (CGPoint, CGPoint)
        switch b.edge {
        case .top: (a, e) = (CGPoint(x: x0, y: r.minY), CGPoint(x: x1, y: r.minY))
        case .bottom: (a, e) = (CGPoint(x: x0, y: r.maxY), CGPoint(x: x1, y: r.maxY))
        case .left: (a, e) = (CGPoint(x: r.minX, y: y0), CGPoint(x: r.minX, y: y1))
        case .right: (a, e) = (CGPoint(x: r.maxX, y: y0), CGPoint(x: r.maxX, y: y1))
        case .diagDown: (a, e) = (CGPoint(x: r.minX, y: r.minY), CGPoint(x: r.maxX, y: r.maxY))
        case .diagUp: (a, e) = (CGPoint(x: r.minX, y: r.maxY), CGPoint(x: r.maxX, y: r.minY))
        }
        ctx.saveGState()
        defer { ctx.restoreGState() }
        ctx.setStrokeColor(cgColor(b.color))
        ctx.setLineWidth(CGFloat(b.width))
        switch b.style {
        case .double:
            let horizontal = abs(a.y - e.y) < 0.5
            let off = horizontal ? CGPoint(x: 0, y: 1.5) : CGPoint(x: 1.5, y: 0)
            ctx.setLineWidth(1)
            strokeLine(ctx, from: CGPoint(x: a.x - off.x, y: a.y - off.y), to: CGPoint(x: e.x - off.x, y: e.y - off.y))
            strokeLine(ctx, from: CGPoint(x: a.x + off.x, y: a.y + off.y), to: CGPoint(x: e.x + off.x, y: e.y + off.y))
        case .dashed, .mediumDashed:
            ctx.setLineDash(phase: 0, lengths: [4, 2])
            strokeLine(ctx, from: a, to: e)
        case .dashDot, .mediumDashDot, .slantDashDot:
            ctx.setLineDash(phase: 0, lengths: [4, 1.5, 1, 1.5])
            strokeLine(ctx, from: a, to: e)
        case .dashDotDot, .mediumDashDotDot:
            ctx.setLineDash(phase: 0, lengths: [4, 1.5, 1, 1.5, 1, 1.5])
            strokeLine(ctx, from: a, to: e)
        case .dotted, .hair:
            ctx.setLineDash(phase: 0, lengths: [1, 1])
            strokeLine(ctx, from: a, to: e)
        case .none:
            break
        default:
            strokeLine(ctx, from: a, to: e)
        }
    }

    private func drawSparkline(_ ctx: CGContext, _ s: SparklineView, in r: CGRect) {
        guard r.width > 0, r.height > 0 else { return }
        let prims = sparklinePrims(kind: s.kind, values: s.values, color: s.color, markers: s.markers, width: Float(r.width), height: Float(r.height))
        ObjectRenderer(engine: engine, measurer: measurer).drawPrims(ctx, prims, origin: r.origin, scale: 1)
    }

    private func drawFilterButton(_ ctx: CGContext, active: Bool, in r: CGRect) {
        guard r.width > 0 else { return }
        let path = CGPath(roundedRect: r, cornerWidth: 2, cornerHeight: 2, transform: nil)
        ctx.addPath(path)
        ctx.setFillColor(CGColor(gray: 0.98, alpha: 1))
        ctx.fillPath()
        ctx.addPath(path)
        ctx.setStrokeColor(CGColor(gray: 0.67, alpha: 1))
        ctx.setLineWidth(1)
        ctx.strokePath()
        ctx.setStrokeColor(CGColor(gray: 0.27, alpha: 1))
        ctx.setLineWidth(1.2)
        ctx.beginPath()
        let c = CGPoint(x: r.midX, y: r.midY), s = r.width * 0.22
        if active {
            // A funnel.
            ctx.addLines(between: [CGPoint(x: c.x - 1.6 * s, y: c.y - s), CGPoint(x: c.x + 1.6 * s, y: c.y - s), CGPoint(x: c.x + 0.3 * s, y: c.y + 0.2 * s), CGPoint(x: c.x + 0.3 * s, y: c.y + 1.3 * s), CGPoint(x: c.x - 0.3 * s, y: c.y + 1.3 * s), CGPoint(x: c.x - 0.3 * s, y: c.y + 0.2 * s)])
            ctx.closePath()
        } else {
            ctx.addLines(between: [CGPoint(x: c.x - 1.4 * s, y: c.y - 0.6 * s), CGPoint(x: c.x, y: c.y + 0.7 * s), CGPoint(x: c.x + 1.4 * s, y: c.y - 0.6 * s)])
        }
        ctx.strokePath()
    }

    // MARK: - Selection, frozen panes, headers

    private func drawSelection(_ ctx: CGContext, selection: SelectionState, geometry: GridGeometry, pane: GridPane) {
        ctx.setFillColor(Palette.selectionFill)
        for range in selection.ranges where !(range.first == range.last) {
            ctx.fill(cgRect(geometry.rangeRect(range: range)))
        }
        guard let current = selection.ranges.last else { return }
        let outline = cgRect(geometry.rangeRect(range: current))
        ctx.setStrokeColor(Palette.accent)
        ctx.setLineWidth(2)
        ctx.stroke(outline)
        // The fill handle, in the pane that shows the selection's corner.
        let handle = cgRect(geometry.fillHandle())
        if !editing, geometry.rangePanes(range: CellRange(first: current.last, last: current.last)).contains(where: { cgRect($0) == cgRect(pane.rect) }) {
            ctx.setFillColor(Palette.accent)
            ctx.fill(handle.insetBy(dx: 1.5, dy: 1.5))
        }
    }

    private func drawFrozenLines(_ ctx: CGContext, geometry: GridGeometry, size: CGSize) {
        let frozen = geometry.frozen(), extent = geometry.frozenSize()
        let cells = geometry.panes().first.map { cgRect($0.rect) } ?? .zero
        ctx.setStrokeColor(Palette.frozenLine)
        ctx.setLineWidth(1)
        if frozen.row > 0 {
            let y = cells.minY - 0.5
            strokeLine(ctx, from: CGPoint(x: cells.minX - CGFloat(extent.width), y: y), to: CGPoint(x: size.width, y: y))
        }
        if frozen.col > 0 {
            let x = cells.minX - 0.5
            strokeLine(ctx, from: CGPoint(x: x, y: cells.minY - CGFloat(extent.height)), to: CGPoint(x: x, y: size.height))
        }
    }

    private func drawHeaders(_ ctx: CGContext, layout: Layout, size: CGSize, selection: SelectionState) {
        let geometry = layout.geometry
        let hw = layout.headerWidth, hh = layout.headerHeight
        ctx.setFillColor(Palette.header)
        ctx.fill(CGRect(x: 0, y: 0, width: hw, height: size.height))
        let current = selection.ranges.last
        var rows: [UInt32] = []
        for pane in geometry.panes() {
            rows.append(contentsOf: Array(pane.rows.first...pane.rows.last))
        }
        if columnHeaders {
            drawColumnHeaders(in: ctx, layout: layout, width: size.width, selection: selection, background: Palette.header)
        }
        let font = headerFont(layout)
        ctx.setStrokeColor(Palette.headerLine)
        ctx.setLineWidth(1)
        for row in Set(rows).sorted() {
            let rowRect = cgRect(geometry.cellRect(cell: CellAddress(row: row, col: 0)))
            let y0 = rowRect.minY, y1 = rowRect.maxY
            guard y1 > y0, y1 > hh, y0 < size.height else { continue }
            let cell = CGRect(x: 0, y: max(y0, hh), width: hw, height: y1 - max(y0, hh))
            if let current, row >= current.first.row, row <= current.last.row {
                ctx.setFillColor(Palette.headerSelected)
                ctx.fill(cell)
            }
            strokeLine(ctx, from: CGPoint(x: 0, y: y1 - 0.5), to: CGPoint(x: hw, y: y1 - 0.5))
            let line = makeLine(String(row + 1), font: font, color: Palette.headerText)
            let (a, d) = bounds(line)
            let w = CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil))
            if (y1 - y0) > a {
                ctx.saveGState()
                ctx.clip(to: cell)
                draw(line, ctx, at: CGPoint(x: (hw - w) / 2, y: (y0 + y1 + a - d) / 2))
                ctx.restoreGState()
            }
        }
        strokeLine(ctx, from: CGPoint(x: hw - 0.5, y: columnHeaders ? 0 : hh), to: CGPoint(x: hw - 0.5, y: size.height))
        if columnHeaders {
            ctx.setFillColor(Palette.header)
            ctx.fill(CGRect(x: 0, y: 0, width: hw - 1, height: hh - 1))
        }
    }

    private func headerFont(_ layout: Layout) -> CTFont {
        CTFontCreateUIFontForLanguage(.system, 11 * CGFloat(max(0.75, layout.scale / Self.displayScale)), nil) ?? measurer.font(FontFace(name: "Helvetica", bold: false, italic: false), size: 11)
    }

    /// The column headers (and the corner) on `background`: the grid's own, or a translucent one
    /// over the window's frosted top.
    public func drawColumnHeaders(in ctx: CGContext, layout: Layout, width: CGFloat, selection: SelectionState, background: CGColor) {
        let geometry = layout.geometry
        let hw = layout.headerWidth, hh = layout.headerHeight
        ctx.setFillColor(background)
        ctx.fill(CGRect(x: 0, y: 0, width: width, height: hh))
        let font = headerFont(layout)
        let current = selection.ranges.last
        var cols: [UInt32] = []
        for pane in geometry.panes() {
            cols.append(contentsOf: Array(pane.cols.first...pane.cols.last))
        }
        ctx.setStrokeColor(Palette.headerLine)
        ctx.setLineWidth(1)
        for col in Set(cols).sorted() {
            // The column's own width: in Page Layout view the next column may be on the next page.
            let cell = cgRect(geometry.cellRect(cell: CellAddress(row: 0, col: col)))
            let x0 = cell.minX, x1 = cell.maxX
            guard x1 > x0, x1 > hw, x0 < width else { continue }
            let header = CGRect(x: max(x0, hw), y: 0, width: x1 - max(x0, hw), height: hh)
            if let current, col >= current.first.col, col <= current.last.col {
                ctx.setFillColor(Palette.headerSelected)
                ctx.fill(header)
            }
            strokeLine(ctx, from: CGPoint(x: x1 - 0.5, y: 0), to: CGPoint(x: x1 - 0.5, y: hh))
            let line = makeLine(columnLabel(col: col), font: font, color: Palette.headerText)
            let (a, d) = bounds(line)
            let w = CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil))
            if (x1 - x0) > w {
                ctx.saveGState()
                ctx.clip(to: header)
                draw(line, ctx, at: CGPoint(x: (x0 + x1 - w) / 2, y: (hh + a - d) / 2))
                ctx.restoreGState()
            }
        }
        strokeLine(ctx, from: CGPoint(x: 0, y: hh - 0.5), to: CGPoint(x: width, y: hh - 0.5))
        strokeLine(ctx, from: CGPoint(x: hw - 0.5, y: 0), to: CGPoint(x: hw - 0.5, y: hh))
    }

    // MARK: - Text helpers

    private struct MeasuredLine {
        let line: CTLine
        let width: CGFloat
        let ascent: CGFloat
        let height: CGFloat
    }

    private func makeLine(_ text: String, font: CTFont, color: CGColor) -> CTLine {
        let attributes: [NSAttributedString.Key: Any] = [.init(kCTFontAttributeName as String): font, .init(kCTForegroundColorAttributeName as String): color]
        return CTLineCreateWithAttributedString(NSAttributedString(string: text, attributes: attributes))
    }

    private func bounds(_ line: CTLine) -> (CGFloat, CGFloat) {
        var ascent: CGFloat = 0, descent: CGFloat = 0, leading: CGFloat = 0
        CTLineGetTypographicBounds(line, &ascent, &descent, &leading)
        return (ascent, descent)
    }

    private func measured(_ line: CTLine) -> MeasuredLine {
        var ascent: CGFloat = 0, descent: CGFloat = 0, leading: CGFloat = 0
        let width = CTLineGetTypographicBounds(line, &ascent, &descent, &leading)
        return MeasuredLine(line: line, width: CGFloat(width), ascent: ascent, height: ascent + descent + leading)
    }

    /// `text` broken into lines `width` wide (and at its line breaks).
    private func wrappedLines(_ text: String, font: CTFont, color: CGColor, width: CGFloat) -> [MeasuredLine] {
        var out: [MeasuredLine] = []
        for paragraph in text.split(separator: "\n", omittingEmptySubsequences: false) {
            let attributes: [NSAttributedString.Key: Any] = [.init(kCTFontAttributeName as String): font, .init(kCTForegroundColorAttributeName as String): color]
            let string = NSAttributedString(string: String(paragraph), attributes: attributes)
            let typesetter = CTTypesetterCreateWithAttributedString(string)
            var start = 0
            let length = string.length
            if length == 0 {
                out.append(measured(makeLine(" ", font: font, color: color)))
            }
            while start < length, out.count < 2000 {
                let count = max(1, CTTypesetterSuggestLineBreak(typesetter, start, Double(max(width, 1))))
                out.append(measured(CTTypesetterCreateLine(typesetter, CFRange(location: start, length: count))))
                start += count
            }
        }
        return out
    }

    private func draw(_ line: CTLine, _ ctx: CGContext, at p: CGPoint) {
        ctx.saveGState()
        // The context is flipped; glyphs are drawn upright.
        ctx.textMatrix = CGAffineTransform(scaleX: 1, y: -1)
        ctx.textPosition = p
        CTLineDraw(line, ctx)
        ctx.restoreGState()
    }

    private func strokeLine(_ ctx: CGContext, from a: CGPoint, to b: CGPoint) {
        ctx.beginPath()
        ctx.move(to: a)
        ctx.addLine(to: b)
        ctx.strokePath()
    }
}

// MARK: - Conversions

func cgRect(_ r: GridRect) -> CGRect {
    CGRect(x: CGFloat(r.x), y: CGFloat(r.y), width: CGFloat(r.width), height: CGFloat(r.height))
}

func point(_ x: CGFloat, _ y: CGFloat) -> GridPoint {
    GridPoint(x: Float(x), y: Float(y))
}

private func gridSize(_ size: CGSize, _ headerWidth: CGFloat, _ headerHeight: CGFloat) -> GridSize {
    GridSize(width: Float(max(0, size.width - headerWidth)), height: Float(max(0, size.height - headerHeight)))
}

func cgColor(_ c: Rgb) -> CGColor {
    CGColor(srgbRed: CGFloat(c.r) / 255, green: CGFloat(c.g) / 255, blue: CGFloat(c.b) / 255, alpha: 1)
}

/// The grid's colours. Cells sit on white paper (as the file defines them); the chrome follows
/// the system's accent colour.
@MainActor
enum Palette {
    static let sheet = CGColor(gray: 1, alpha: 1)
    static let gridline = CGColor(gray: 0.86, alpha: 1)
    static let header = CGColor(gray: 0.96, alpha: 1)
    static let headerLine = CGColor(gray: 0.82, alpha: 1)
    static let headerText = CGColor(gray: 0.33, alpha: 1)
    static let frozenLine = CGColor(gray: 0.59, alpha: 1)
    static let desk = CGColor(gray: 0.88, alpha: 1)
    static let secondary = CGColor(gray: 0.43, alpha: 1)
    static let note = CGColor(srgbRed: 0.82, green: 0.18, blue: 0.18, alpha: 1)
    static let threadedComment = CGColor(srgbRed: 0.53, green: 0.33, blue: 0.78, alpha: 1)
    static let checkbox = CGColor(srgbRed: 0.06, green: 0.49, blue: 0.25, alpha: 1)
    static var accent: CGColor { NSColor.controlAccentColor.cgColor }
    static var selectionFill: CGColor { NSColor.controlAccentColor.withAlphaComponent(0.12).cgColor }
    static var headerSelected: CGColor { NSColor.controlAccentColor.withAlphaComponent(0.18).cgColor }
}
