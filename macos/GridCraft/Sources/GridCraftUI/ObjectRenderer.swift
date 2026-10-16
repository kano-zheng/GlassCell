import AppKit
import CoreText
import GridCraftEngine

/// Draws a sheet's floating objects (shapes, pictures, charts, slicers) and chart-style
/// primitives (`ChartPrim`, also sparklines) into a flipped context.
@MainActor
struct ObjectRenderer {
    let engine: Engine
    let measurer: CoreTextMeasurer

    /// Decoded pictures, by object id and byte count.
    private static var images: [String: NSImage] = [:]

    /// The objects over the cells area `cells`; `selected` gets a selection frame.
    func draw(_ ctx: CGContext, geometry: GridGeometry, scale: Float, cells: CGRect, selected: UInt32?) {
        guard let objects = try? engine.objects(), !objects.isEmpty else { return }
        ctx.saveGState()
        ctx.clip(to: cells)
        let z = CGFloat(scale)
        for object in objects {
            let r = cgRect(geometry.objectRect(anchor: object.anchor))
            guard r.intersects(cells) else { continue }
            switch object.content {
            case let .shape(style): drawShape(ctx, style, in: r, scale: z)
            case .picture: drawPicture(ctx, id: object.id, in: r)
            case .chart: drawChart(ctx, id: object.id, in: r, scale: z)
            case let .slicer(caption): drawSlicer(ctx, caption: caption, in: r, scale: z)
            }
            if object.id == selected {
                drawSelectionFrame(ctx, r)
            }
        }
        ctx.restoreGState()
    }

    // MARK: - Shapes

    private func drawShape(_ ctx: CGContext, _ s: ShapeStyle, in r: CGRect, scale z: CGFloat) {
        let fill = s.fill.map(cgColor)
        let line = s.line.map(cgColor)
        let path = CGMutablePath()
        switch s.shape {
        case .rectangle, .textBox: path.addRect(r)
        case .roundedRectangle: path.addRoundedRect(in: r, cornerWidth: min(10 * z, r.width / 2), cornerHeight: min(10 * z, r.height / 2))
        case .ellipse: path.addEllipse(in: r)
        case .triangle: path.addLines(between: [CGPoint(x: r.midX, y: r.minY), CGPoint(x: r.maxX, y: r.maxY), CGPoint(x: r.minX, y: r.maxY)]); path.closeSubpath()
        case .line, .arrow:
            ctx.setStrokeColor(line ?? CGColor(gray: 0.25, alpha: 1))
            ctx.setLineWidth(2)
            ctx.beginPath()
            ctx.move(to: CGPoint(x: r.minX, y: r.minY))
            ctx.addLine(to: CGPoint(x: r.maxX, y: r.maxY))
            ctx.strokePath()
            if s.shape == .arrow {
                let angle = atan2(r.height, r.width), tip = CGPoint(x: r.maxX, y: r.maxY)
                let back = CGPoint(x: tip.x - 10 * cos(angle), y: tip.y - 10 * sin(angle))
                let n = CGPoint(x: -sin(angle) * 5, y: cos(angle) * 5)
                ctx.setFillColor(line ?? CGColor(gray: 0.25, alpha: 1))
                ctx.beginPath()
                ctx.addLines(between: [tip, CGPoint(x: back.x + n.x, y: back.y + n.y), CGPoint(x: back.x - n.x, y: back.y - n.y)])
                ctx.closePath()
                ctx.fillPath()
            }
            return
        case .ink:
            guard let ink = s.ink, ink.points.count >= 4 else { return }
            ctx.setStrokeColor(line ?? CGColor(gray: 0, alpha: 1))
            ctx.setLineWidth(CGFloat(ink.width) * z)
            ctx.setLineCap(.round)
            ctx.setLineJoin(.round)
            ctx.beginPath()
            for i in stride(from: 0, to: ink.points.count - 1, by: 2) {
                let p = CGPoint(x: r.minX + CGFloat(ink.points[i]) * z, y: r.minY + CGFloat(ink.points[i + 1]) * z)
                if i == 0 { ctx.move(to: p) } else { ctx.addLine(to: p) }
            }
            ctx.strokePath()
            return
        case .icon:
            // GridCraft's own icon set is drawn in code in egui (icons.rs); until it is ported,
            // a framed placeholder with the icon's name.
            ctx.setStrokeColor(CGColor(gray: 0.5, alpha: 1))
            ctx.stroke(r.insetBy(dx: 0.5, dy: 0.5))
            drawText(ctx, s.icon ?? "", in: r.insetBy(dx: 2, dy: 2), size: 10, color: CGColor(gray: 0.4, alpha: 1), bold: false, italic: false, font: nil, align: .center, anchor: .middle)
            return
        }
        if let fill {
            ctx.addPath(path)
            ctx.setFillColor(fill)
            ctx.fillPath()
        }
        if let line {
            ctx.addPath(path)
            ctx.setStrokeColor(line)
            ctx.setLineWidth(1)
            ctx.strokePath()
        }
        if !s.text.isEmpty {
            drawText(ctx, s.text, in: r.insetBy(dx: 4, dy: 4), size: CGFloat(s.textSize) * z, color: cgColor(s.textColor), bold: s.bold, italic: s.italic,
                     font: s.font, align: s.align, anchor: s.anchor)
        }
    }

    /// Wrapped text in `r`, aligned and anchored.
    private func drawText(_ ctx: CGContext, _ text: String, in r: CGRect, size: CGFloat, color: CGColor, bold: Bool, italic: Bool, font: String?,
                          align: TextAlign, anchor: TextAnchor) {
        let ctFont = measurer.font(FontFace(name: font ?? "Helvetica Neue", bold: bold, italic: italic), size: Float(max(1, size)))
        let paragraph = NSMutableParagraphStyle()
        paragraph.alignment = align == .left ? .left : align == .center ? .center : .right
        let string = NSAttributedString(string: text, attributes: [.font: ctFont as NSFont, .foregroundColor: NSColor(cgColor: color) ?? .black, .paragraphStyle: paragraph])
        let bounds = string.boundingRect(with: CGSize(width: max(1, r.width), height: .greatestFiniteMagnitude), options: [.usesLineFragmentOrigin])
        let height = min(bounds.height, r.height)
        let y: CGFloat = anchor == .top ? r.minY : anchor == .middle ? r.midY - height / 2 : r.maxY - height
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(cgContext: ctx, flipped: true)
        string.draw(with: CGRect(x: r.minX, y: y, width: r.width, height: height), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
        NSGraphicsContext.restoreGraphicsState()
    }

    // MARK: - Pictures, slicers, selection

    private func drawPicture(_ ctx: CGContext, id: UInt32, in r: CGRect) {
        guard let data = try? engine.picture(id: id) else { return }
        let key = "\(id)-\(data.count)"
        let image = Self.images[key] ?? NSImage(data: data)
        if let image {
            if Self.images.count > 64 { Self.images.removeAll() }
            Self.images[key] = image
            NSGraphicsContext.saveGraphicsState()
            NSGraphicsContext.current = NSGraphicsContext(cgContext: ctx, flipped: true)
            image.draw(in: r, from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: nil)
            NSGraphicsContext.restoreGraphicsState()
        } else {
            ctx.setFillColor(CGColor(gray: 0.9, alpha: 1))
            ctx.fill(r)
        }
    }

    private func drawSlicer(_ ctx: CGContext, caption: String, in r: CGRect, scale z: CGFloat) {
        ctx.setFillColor(CGColor(gray: 1, alpha: 1))
        ctx.fill(r)
        ctx.setStrokeColor(CGColor(gray: 0.7, alpha: 1))
        ctx.stroke(r.insetBy(dx: 0.5, dy: 0.5))
        drawText(ctx, caption, in: CGRect(x: r.minX + 6 * z, y: r.minY + 4 * z, width: r.width - 12 * z, height: 20 * z), size: 13 * z,
                 color: CGColor(gray: 0.15, alpha: 1), bold: true, italic: false, font: nil, align: .left, anchor: .middle)
    }

    private func drawSelectionFrame(_ ctx: CGContext, _ r: CGRect) {
        ctx.setStrokeColor(NSColor.controlAccentColor.cgColor)
        ctx.setLineWidth(1.5)
        ctx.stroke(r.insetBy(dx: -1, dy: -1))
        ctx.setFillColor(CGColor(gray: 1, alpha: 1))
        for p in [CGPoint(x: r.minX, y: r.minY), CGPoint(x: r.maxX, y: r.minY), CGPoint(x: r.minX, y: r.maxY), CGPoint(x: r.maxX, y: r.maxY)] {
            let handle = CGRect(x: p.x - 4, y: p.y - 4, width: 8, height: 8)
            ctx.fillEllipse(in: handle)
            ctx.strokeEllipse(in: handle)
        }
    }

    // MARK: - Charts

    private func drawChart(_ ctx: CGContext, id: UInt32, in r: CGRect, scale z: CGFloat) {
        // A white card with a soft shadow.
        ctx.saveGState()
        ctx.setShadow(offset: CGSize(width: 0, height: 1.5), blur: 3, color: CGColor(gray: 0, alpha: 0.18))
        ctx.setFillColor(CGColor(gray: 1, alpha: 1))
        ctx.fill(r)
        ctx.restoreGState()
        guard let prims = try? engine.chartPrims(id: id, measurer: measurer) else { return }
        ctx.saveGState()
        ctx.clip(to: r)
        drawPrims(ctx, prims, origin: r.origin, scale: z)
        ctx.restoreGState()
    }

    /// Draws primitives laid out from `origin`, `scale` screen units per primitive unit.
    func drawPrims(_ ctx: CGContext, _ prims: [ChartPrim], origin o: CGPoint, scale k: CGFloat) {
        func pt(_ x: Float, _ y: Float) -> CGPoint { CGPoint(x: o.x + CGFloat(x) * k, y: o.y + CGFloat(y) * k) }
        func points(_ flat: [Float]) -> [CGPoint] { stride(from: 0, to: flat.count - 1, by: 2).map { pt(flat[$0], flat[$0 + 1]) } }
        func color(_ c: Rgba) -> CGColor { CGColor(srgbRed: CGFloat(c.r) / 255, green: CGFloat(c.g) / 255, blue: CGFloat(c.b) / 255, alpha: CGFloat(c.a) / 255) }
        func stroke(_ s: Stroke?) {
            guard let s else { return }
            ctx.setStrokeColor(color(s.color))
            ctx.setLineWidth(max(0.5, CGFloat(s.width) * k))
            ctx.strokePath()
        }
        for prim in prims {
            switch prim {
            case let .rect(x, y, w, h, fill, s, radius):
                let rect = CGRect(origin: pt(x, y), size: CGSize(width: CGFloat(w) * k, height: CGFloat(h) * k))
                let path = CGPath(roundedRect: rect, cornerWidth: min(CGFloat(radius) * k, rect.width / 2), cornerHeight: min(CGFloat(radius) * k, rect.height / 2), transform: nil)
                if let fill { ctx.addPath(path); ctx.setFillColor(color(fill)); ctx.fillPath() }
                if s != nil { ctx.addPath(path); stroke(s) }
            case let .line(flat, c, width, dash):
                ctx.saveGState()
                if dash { ctx.setLineDash(phase: 0, lengths: [3 * k, 2 * k]) }
                ctx.beginPath()
                ctx.addLines(between: points(flat))
                ctx.setStrokeColor(color(c))
                ctx.setLineWidth(max(0.5, CGFloat(width) * k))
                ctx.strokePath()
                ctx.restoreGState()
            case let .polygon(flat, fill, s):
                ctx.beginPath(); ctx.addLines(between: points(flat)); ctx.closePath()
                ctx.setFillColor(color(fill)); ctx.fillPath()
                if s != nil { ctx.beginPath(); ctx.addLines(between: points(flat)); ctx.closePath(); stroke(s) }
            case let .wedge(cx, cy, ro, ri, a0, a1, fill, s):
                // 0 rad at 12 o'clock, clockwise (on screen: y down).
                let c = pt(cx, cy)
                let path = CGMutablePath()
                let start = CGFloat(a0) - .pi / 2, end = CGFloat(a1) - .pi / 2
                path.addArc(center: c, radius: CGFloat(ro) * k, startAngle: start, endAngle: end, clockwise: false)
                if ri > 0 {
                    path.addArc(center: c, radius: CGFloat(ri) * k, startAngle: end, endAngle: start, clockwise: true)
                } else {
                    path.addLine(to: c)
                }
                path.closeSubpath()
                ctx.addPath(path); ctx.setFillColor(color(fill)); ctx.fillPath()
                if s != nil { ctx.addPath(path); stroke(s) }
            case let .circle(cx, cy, r, fill, s):
                let rect = CGRect(x: o.x + CGFloat(cx - r) * k, y: o.y + CGFloat(cy - r) * k, width: CGFloat(2 * r) * k, height: CGFloat(2 * r) * k)
                if let fill { ctx.setFillColor(color(fill)); ctx.fillEllipse(in: rect) }
                if s != nil { ctx.addEllipse(in: rect); stroke(s) }
            case let .text(x, y, text, size, c, bold, align, valign, rotation):
                let font = measurer.font(FontFace(name: "", bold: bold, italic: false), size: max(1, size * Float(k)))
                let line = CTLineCreateWithAttributedString(NSAttributedString(string: text, attributes: [
                    .init(kCTFontAttributeName as String): font, .init(kCTForegroundColorAttributeName as String): color(c),
                ]))
                var ascent: CGFloat = 0, descent: CGFloat = 0, leading: CGFloat = 0
                let w = CGFloat(CTLineGetTypographicBounds(line, &ascent, &descent, &leading))
                let dx: CGFloat = align == 0 ? 0 : align == 1 ? -w / 2 : -w
                let dy: CGFloat = valign == 0 ? ascent : valign == 1 ? (ascent - descent) / 2 : -descent
                ctx.saveGState()
                let anchor = pt(x, y)
                ctx.translateBy(x: anchor.x, y: anchor.y)
                ctx.rotate(by: CGFloat(rotation.isFinite ? rotation : 0))
                ctx.textMatrix = CGAffineTransform(scaleX: 1, y: -1)
                ctx.textPosition = CGPoint(x: dx, y: dy)
                CTLineDraw(line, ctx)
                ctx.restoreGState()
            }
        }
    }
}
