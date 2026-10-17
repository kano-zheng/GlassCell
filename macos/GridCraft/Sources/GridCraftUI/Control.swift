import AppKit
import Foundation
import GridCraftEngine
import SwiftUI

/// The control channel's methods (`docs/control-protocol.md`), acting on the frontmost workbook
/// like a person at the window: commands through `execute`, the editor, dialogs, synthetic mouse
/// and key events delivered to the window (never to the desktop), screenshots of the window.
@MainActor
enum Control {
    struct Failure: Error {
        let message: String
        init(_ message: String) { self.message = message }
    }

    /// The reply line for one request line: `{"id", "ok": true, "result"}` or `{"id", "ok": false, "error"}`.
    static func reply(to line: Data) async -> Data {
        guard let request = (try? JSONSerialization.jsonObject(with: line)) as? [String: Any] else {
            return encode(["id": NSNull(), "ok": false, "error": "a request is one JSON object per line"])
        }
        let id = request["id"] ?? NSNull()
        guard let method = request["method"] as? String else {
            return encode(["id": id, "ok": false, "error": "missing `method`"])
        }
        let params = request["params"] as? [String: Any] ?? [:]
        do {
            let result = try await handle(method, params)
            return encode(["id": id, "ok": true, "result": result])
        } catch let failure as Failure {
            return encode(["id": id, "ok": false, "error": failure.message])
        } catch {
            return encode(["id": id, "ok": false, "error": error.localizedDescription])
        }
    }

    private static func encode(_ object: [String: Any]) -> Data {
        if JSONSerialization.isValidJSONObject(object), let data = try? JSONSerialization.data(withJSONObject: object) {
            return data
        }
        let fallback: [String: Any] = ["id": NSNull(), "ok": false, "error": "the reply isn't JSON"]
        return (try? JSONSerialization.data(withJSONObject: fallback)) ?? Data("{}".utf8)
    }

    /// The methods the Mac adds to the engine catalog in `engine.commands`.
    static let uiCommands: [(id: String, label: String, params: String)] = [
        ("ui.inspector", "Inspector", "{on?: bool} (toggles without it) → {on}"),
        ("ui.palette", "Command Palette", "{on?: bool} (toggles without it) → {on}"),
        ("ui.dialog", "Open Dialog", "{name, params?} → the dialog as in ui.inspect"),
    ]

    static func handle(_ method: String, _ p: [String: Any]) async throws -> Any {
        switch method {
        case "engine.execute", "command", "ui.menu.invoke":
            guard let id = p["command"] as? String ?? p["id"] as? String else { throw Failure("missing `command`") }
            return try execute(id, p["params"])
        case "engine.commands":
            return commands(search: p["search"] as? String)
        case "engine.journal":
            return decode(try document().engine.journal())
        case "document.inspect", "sheet.read", "cell.get", "selection.stats":
            // Reads: an edit in progress stays open.
            return decode(try engineCall { try document().perform(method, json: JSON.encode(p)) })
        case "ui.inspect":
            return inspect()
        case "ui.inspector":
            let document = try document()
            document.showInspector = p["on"] as? Bool ?? !document.showInspector
            return ["on": document.showInspector]
        case "ui.palette":
            let document = try document()
            document.showPalette = p["on"] as? Bool ?? !document.showPalette
            return ["on": document.showPalette]
        case "ui.dialog":
            let state = try openDialog(p)
            try await settleSheet()
            return state
        case "ui.dialog.set":
            return try setDialogValue(p)
        case "ui.dialog.confirm":
            return try confirmDialog()
        case "ui.dialog.cancel":
            let document = try document()
            document.form = nil
            document.dialog = nil
            return NSNull()
        case "ui.message.dismiss":
            try document().problem = nil
            return NSNull()
        case "ui.edit.begin":
            let editor = try editor()
            if let grid = editor.grid {
                grid.beginEditing(text: p["text"] as? String)
            } else {
                editor.begin(typing: p["text"] as? String)
            }
            guard editor.isEditing else { throw Failure("the active cell can't be edited") }
            return NSNull()
        case "ui.edit.text":
            let editor = try editor()
            guard editor.isEditing else { throw Failure("not editing") }
            editor.replaceText(p["text"] as? String ?? "")
            return NSNull()
        case "ui.edit.commit":
            let editor = try editor()
            guard editor.isEditing else { throw Failure("not editing") }
            let move: EditController.Move = switch p["move"] as? String {
            case "right": .right
            case "up": .up
            case "left": .left
            case "none": .none
            default: .down
            }
            guard editor.commit(move, array: p["array"] as? Bool ?? false) else {
                throw Failure(try document().problem ?? "the entry wasn't accepted")
            }
            editor.ended()
            return NSNull()
        case "ui.edit.cancel":
            let editor = try editor()
            if editor.isEditing {
                editor.cancel()
                editor.ended()
            }
            return NSNull()
        case "ui.key":
            try key(p)
            return NSNull()
        case "ui.text":
            try text(p["text"] as? String ?? "")
            return NSNull()
        case "ui.move":
            try mouse(.mouseMoved, at: windowPoint(p, "x", "y"), p)
            return NSNull()
        case "ui.click":
            return try click(at: windowPoint(p, "x", "y"), p)
        case "ui.drag":
            try drag(from: windowPoint(p, "x", "y"), to: windowPoint(p, "toX", "toY"), p)
            return NSNull()
        case "ui.cell.click":
            return try click(at: cellPoint(p["cell"]), p)
        case "ui.cell.drag":
            try drag(from: cellPoint(p["from"]), to: cellPoint(p["to"]), p)
            return NSNull()
        case "ui.screenshot":
            return try screenshot(path: p["path"] as? String)
        case "ui.resize":
            let window = try window()
            let width = (p["width"] as? Double).map { max(400, $0) } ?? window.contentLayoutRect.width
            let height = (p["height"] as? Double).map { max(300, $0) } ?? window.contentLayoutRect.height
            window.setContentSize(NSSize(width: width, height: height))
            return NSNull()
        case "ui.focus":
            let window = try window()
            NSApplication.shared.activate()
            window.makeKeyAndOrderFront(nil)
            return NSNull()
        case "app.open":
            return try await open(p)
        case "app.save":
            return try await save(path: p["path"] as? String)
        case "app.quit":
            Task { @MainActor in
                try? await Task.sleep(for: .milliseconds(200))
                NSApplication.shared.terminate(nil)
            }
            return NSNull()
        default:
            // Anything else that names a command runs it.
            if (try? document().engine.commands())?.contains(where: { $0.id == method }) == true {
                return try execute(method, p)
            }
            throw Failure("unknown method `\(method)`")
        }
    }

    // MARK: - The workbook

    private static func document() throws -> SpreadsheetDocument {
        guard let document = WorkbookRegistry.shared.frontmost else { throw Failure("no workbook is open") }
        return document
    }

    private static func editor() throws -> EditController {
        _ = try document()
        guard let editor = WorkbookRegistry.shared.frontmostEditor else { throw Failure("no workbook is open") }
        return editor
    }

    private static func window() throws -> NSWindow {
        guard let window = WorkbookRegistry.shared.window(for: try document()) else { throw Failure("the workbook has no window") }
        return window
    }

    private static func engineCall(_ body: () throws -> String) throws -> String {
        do {
            return try body()
        } catch let failure as Failure {
            throw failure
        } catch {
            throw Failure(error.localizedDescription)
        }
    }

    /// JSON text as a JSON value (`null` for none).
    private static func decode(_ text: String) -> Any {
        guard !text.isEmpty, let value = try? JSONSerialization.jsonObject(with: Data(text.utf8), options: .fragmentsAllowed) else {
            return NSNull()
        }
        return value
    }

    /// Runs a command as the menu would: an edit in progress is confirmed first.
    private static func execute(_ id: String, _ params: Any?) throws -> Any {
        let json: String
        switch params {
        case nil, is NSNull: json = ""
        case let object as [String: Any]: json = object.isEmpty ? "" : JSON.encode(object)
        default: throw Failure("`params` of `\(id)` must be an object")
        }
        return decode(try engineCall { try Automation.run(id, params: json) })
    }

    private static func commands(search: String?) -> [[String: Any]] {
        let engine = WorkbookRegistry.shared.frontmost?.engine ?? Engine()
        var list: [[String: Any]] = ((try? engine.commands()) ?? []).map { c in
            [
                "id": c.id, "label": c.label, "menu": c.menu, "shortcut": c.shortcut ?? NSNull(), "params": c.params,
                "enabled": c.enabled, "disabledReason": c.disabledReason ?? NSNull(),
            ]
        }
        list += uiCommands.map { ["id": $0.id, "label": $0.label, "params": $0.params, "enabled": true, "ui": true] }
        guard let query = search?.lowercased(), !query.isEmpty else { return list }
        return list.filter { c in
            let fields = [c["id"], c["label"]].compactMap { $0 as? String } + (c["menu"] as? [String] ?? [])
            return fields.contains { $0.lowercased().contains(query) }
        }
    }

    // MARK: - What the window shows

    private static func inspect() -> [String: Any] {
        let registry = WorkbookRegistry.shared
        var state: [String: Any] = [
            "documents": registry.documents.map { document in
                let window = registry.window(for: document)
                return ["title": window?.title ?? "", "dirty": window?.isDocumentEdited ?? false] as [String: Any]
            },
        ]
        guard let document = registry.frontmost else { return state }
        state["inspector"] = document.showInspector
        state["palette"] = document.showPalette
        state["filterMenu"] = document.filterMenu.map { $0.menu.column as Any } ?? NSNull()
        state["message"] = document.problem ?? NSNull()
        state["notice"] = document.notice ?? NSNull()
        state["dialog"] = dialogState(document) ?? NSNull()
        if let editor = registry.frontmostEditor, editor.isEditing {
            state["editor"] = ["cell": cellName(cell: editor.cell), "text": editor.text, "formulaBar": editor.fromFormulaBar, "point": editor.canPoint]
        } else {
            state["editor"] = NSNull()
        }
        if let window = registry.window(for: document), let content = window.contentView {
            state["window"] = [content.bounds.width, content.bounds.height]
            state["key"] = window.isKeyWindow
            state["firstResponder"] = window.firstResponder.map { String(describing: type(of: $0)) } ?? NSNull()
            if let grid = registry.frontmostEditor?.grid, grid.window === window {
                let r = contentRect(of: grid, in: content)
                state["grid"] = [r.minX, r.minY, r.width, r.height]
                state["scroll"] = [grid.scroll.x, grid.scroll.y]
                state["perf"] = ["frameMs": grid.lastFrameMilliseconds]
            }
        }
        return state
    }

    /// A view's frame in content coordinates: points from the content's top-left corner, the
    /// coordinates `ui.click` takes.
    private static func contentRect(of view: NSView, in content: NSView) -> CGRect {
        let r = view.convert(view.bounds, to: content)
        return content.isFlipped ? r : CGRect(x: r.minX, y: content.bounds.height - r.maxY, width: r.width, height: r.height)
    }

    private static func dialogState(_ document: SpreadsheetDocument) -> [String: Any]? {
        if let form = document.form?.session {
            var values: [String: Any] = [:]
            for field in form.fields() {
                switch field {
                case let .text(key, _), let .number(key, _), let .choice(key, _, _): values[key] = form.text(key: key)
                case let .check(key, _): values[key] = form.flag(key: key)
                case let .password(key, _): values[key] = form.text(key: key).isEmpty ? "" : "••••"
                case .note: break
                }
            }
            return ["name": form.name(), "title": form.title(), "values": values, "command": form.command()]
        }
        if let dialog = document.dialog {
            return ["name": dialog.name]
        }
        return nil
    }

    // MARK: - Dialogs

    private static func openDialog(_ p: [String: Any]) throws -> Any {
        guard let name = p["name"] as? String else { throw Failure("missing `name`") }
        let document = try document()
        let params = (p["params"] as? [String: Any]).map(JSON.encode) ?? "{}"
        document.handle(.dialog(name: name, params: params), editor: try editor())
        if let problem = document.problem, document.form == nil, document.dialog == nil {
            document.problem = nil
            throw Failure(problem)
        }
        return dialogState(document) ?? NSNull()
    }

    /// Waits for a sheet dialog to finish sliding in, so screenshots and clicks find it in place.
    private static func settleSheet() async throws {
        guard let window = WorkbookRegistry.shared.window(for: try document()) else { return }
        var last: CGRect?
        for _ in 0..<40 {
            try? await Task.sleep(for: .milliseconds(50))
            guard let sheet = window.attachedSheet else { continue }
            if sheet.frame == last, sheet.isVisible { return }
            last = sheet.frame
        }
    }

    private static func setDialogValue(_ p: [String: Any]) throws -> Any {
        let document = try document()
        guard let form = document.form?.session else { throw Failure("no form dialog is open") }
        guard let key = p["key"] as? String else { throw Failure("missing `key`") }
        let field = form.fields().first { field in
            switch field {
            case let .text(k, _), let .password(k, _), let .number(k, _), let .choice(k, _, _), let .check(k, _): k == key
            case .note: false
            }
        }
        let value = p["value"]
        let text = (value as? String) ?? (value as? NSNumber).map { "\($0)" } ?? ""
        do {
            switch field {
            case nil, .note: throw Failure("the dialog has no field `\(key)`")
            case .check: try form.setFlag(key: key, value: value as? Bool ?? (text == "true"))
            case .number: try form.setNumber(key: key, value: text)
            default: try form.setText(key: key, value: text)
            }
        } catch let failure as Failure {
            throw failure
        } catch {
            throw Failure(error.localizedDescription)
        }
        document.objectWillChange.send()
        return dialogState(document) ?? NSNull()
    }

    private static func confirmDialog() throws -> Any {
        let document = try document()
        guard let form = document.form?.session else {
            throw Failure(document.dialog == nil ? "no dialog is open" : "this dialog is confirmed in the window; ui.dialog.cancel closes it")
        }
        let result = try engineCall { try document.perform(form.command(), json: try form.params()) }
        document.form = nil
        return decode(result)
    }

    // MARK: - Mouse and keyboard, delivered to the workbook window

    /// `(x, y)` from the request (content points from the top-left) in window coordinates.
    private static func windowPoint(_ p: [String: Any], _ x: String, _ y: String) throws -> NSPoint {
        let window = try window()
        guard let content = window.contentView else { throw Failure("the workbook has no window") }
        let px = (p[x] as? Double) ?? 0
        let py = (p[y] as? Double) ?? 0
        return content.convert(NSPoint(x: px, y: content.isFlipped ? py : content.bounds.height - py), to: nil)
    }

    /// The centre of cell `name` (`B3`) in window coordinates, scrolled into view first.
    private static func cellPoint(_ name: Any?) throws -> NSPoint {
        guard let text = name as? String, let cell = parseCell(text: text) else { throw Failure("missing or invalid cell") }
        guard let grid = try editor().grid, grid.window === (try window()) else { throw Failure("the workbook has no window") }
        guard let frame = grid.cellFrame(cell), grid.gridBounds.contains(CGPoint(x: frame.midX, y: frame.midY)) else {
            throw Failure("\(cellName(cell: cell)) isn't on screen; scroll there first")
        }
        return grid.convert(CGPoint(x: frame.midX, y: frame.midY), to: nil)
    }

    private static func flags(_ p: [String: Any]) -> NSEvent.ModifierFlags {
        var flags: NSEvent.ModifierFlags = []
        if p["cmd"] as? Bool == true { flags.insert(.command) }
        if p["shift"] as? Bool == true { flags.insert(.shift) }
        if p["alt"] as? Bool == true { flags.insert(.option) }
        if p["ctrl"] as? Bool == true { flags.insert(.control) }
        return flags
    }

    /// The view a press went to: drags and the release go to it too, as AppKit does.
    private static var mouseTarget: NSView?
    /// The pressed button is a SwiftUI control, which the window delivers to.
    private static var mouseThroughWindow = false

    private static func mouseEvent(_ type: NSEvent.EventType, at point: NSPoint, _ p: [String: Any], clicks: Int = 1) throws -> NSEvent {
        let window = try window()
        let pressure: Float = type == .leftMouseUp || type == .mouseMoved ? 0 : 1
        guard let event = NSEvent.mouseEvent(
            with: type, location: point, modifierFlags: flags(p), timestamp: ProcessInfo.processInfo.systemUptime,
            windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: clicks, pressure: pressure
        ) else { throw Failure("couldn't make the mouse event") }
        return event
    }

    /// The view under `point` (window coordinates).
    private static func view(at point: NSPoint) throws -> NSView {
        guard let view = try window().contentView?.hitTest(point) else { throw Failure("nothing is at that point in the window") }
        return view
    }

    /// Delivers a mouse event straight to the grid, so it acts even when GridCraft isn't the
    /// active app (a first click on an inactive window would only bring it forward). SwiftUI's
    /// controls (the status bar's tabs and buttons) track the mouse through the window, so
    /// their events go there.
    private static func mouse(_ type: NSEvent.EventType, at point: NSPoint, _ p: [String: Any], clicks: Int = 1) throws {
        let event = try mouseEvent(type, at: point, p, clicks: clicks)
        switch type {
        case .leftMouseDown:
            let target = try view(at: point)
            guard target.firstAncestor(of: GridNSView.self) != nil || target is NSTextView else {
                mouseTarget = nil
                mouseThroughWindow = true
                try window().sendEvent(event)
                return
            }
            if let text = target as? NSTextView {
                // A text view's mouseDown tracks the mouse in a loop of its own until a real
                // release: put the insertion point under the click instead.
                mouseTarget = nil
                text.window?.makeFirstResponder(text)
                let at = text.characterIndexForInsertion(at: text.convert(point, from: nil))
                text.setSelectedRange(NSRange(location: min(at, (text.string as NSString).length), length: 0))
                return
            }
            mouseTarget = target
            target.window?.makeFirstResponder(target.acceptsFirstResponder ? target : target.window?.firstResponder)
            target.mouseDown(with: event)
        case .leftMouseDragged:
            if mouseThroughWindow { try window().sendEvent(event) }
            mouseTarget?.mouseDragged(with: event)
        case .leftMouseUp:
            if mouseThroughWindow { try window().sendEvent(event) }
            mouseTarget?.mouseUp(with: event)
            mouseTarget = nil
            mouseThroughWindow = false
        default:
            try view(at: point).mouseMoved(with: event)
        }
    }

    /// A click; a right click answers with the context menu's items instead of opening it (a
    /// menu on screen would wait for a person).
    private static func click(at point: NSPoint, _ p: [String: Any]) throws -> Any {
        if p["button"] as? String == "right" {
            let event = try mouseEvent(.rightMouseDown, at: point, p)
            let menu = try view(at: point).menu(for: event)
            return ["menu": menu?.items.map { $0.isSeparatorItem ? "-" : $0.title } ?? []]
        }
        let count = max(1, min(3, p["count"] as? Int ?? 1))
        for clicks in 1...count {
            try mouse(.leftMouseDown, at: point, p, clicks: clicks)
            try mouse(.leftMouseUp, at: point, p, clicks: clicks)
        }
        return NSNull()
    }

    private static func drag(from a: NSPoint, to b: NSPoint, _ p: [String: Any]) throws {
        let steps = max(1, min(100, p["steps"] as? Int ?? 8))
        try mouse(.leftMouseDown, at: a, p)
        for i in 1...steps {
            let t = CGFloat(i) / CGFloat(steps)
            try mouse(.leftMouseDragged, at: NSPoint(x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t), p)
        }
        try mouse(.leftMouseUp, at: b, p)
    }

    private static func functionKey(_ code: Int) -> String {
        UnicodeScalar(UInt32(code)).map { String(Character($0)) } ?? ""
    }

    /// Key names as in the egui channel: key code and characters.
    private static let keys: [String: (code: UInt16, characters: String)] = [
        "enter": (36, "\r"), "return": (36, "\r"), "tab": (48, "\t"), "space": (49, " "), "backspace": (51, "\u{7F}"),
        "escape": (53, "\u{1B}"), "esc": (53, "\u{1B}"),
        "delete": (117, functionKey(NSDeleteFunctionKey)), "del": (117, functionKey(NSDeleteFunctionKey)),
        "left": (123, functionKey(NSLeftArrowFunctionKey)), "right": (124, functionKey(NSRightArrowFunctionKey)),
        "down": (125, functionKey(NSDownArrowFunctionKey)), "up": (126, functionKey(NSUpArrowFunctionKey)),
        "home": (115, functionKey(NSHomeFunctionKey)), "end": (119, functionKey(NSEndFunctionKey)),
        "pageup": (116, functionKey(NSPageUpFunctionKey)), "pagedown": (121, functionKey(NSPageDownFunctionKey)),
        "f2": (120, functionKey(NSF2FunctionKey)), "f4": (118, functionKey(NSF4FunctionKey)), "f9": (101, functionKey(NSF9FunctionKey)),
    ]

    /// A key press, with shortcuts going to the menu bar first as they do from the keyboard.
    private static func key(_ p: [String: Any]) throws {
        guard let name = p["key"] as? String, !name.isEmpty else { throw Failure("missing `key`") }
        let named = keys[name.lowercased().replacingOccurrences(of: "arrow", with: "")]
        guard named != nil || name.count == 1 else { throw Failure("unknown key `\(name)`") }
        let characters = named?.characters ?? name
        let window = try window()
        let flags = flags(p)
        func event(_ type: NSEvent.EventType) throws -> NSEvent {
            guard let event = NSEvent.keyEvent(
                with: type, location: .zero, modifierFlags: flags, timestamp: ProcessInfo.processInfo.systemUptime,
                windowNumber: window.windowNumber, context: nil,
                characters: flags.contains(.shift) ? characters.uppercased() : characters,
                charactersIgnoringModifiers: characters.lowercased(), isARepeat: false, keyCode: named?.code ?? 0
            ) else { throw Failure("couldn't make the key event") }
            return event
        }
        let down = try event(.keyDown)
        if !flags.intersection([.command, .control, .option]).isEmpty || characters.unicodeScalars.first.map({ (0xF704...0xF726).contains($0.value) }) == true {
            if window.isKeyWindow {
                if NSApplication.shared.mainMenu?.performKeyEquivalent(with: down) == true || window.performKeyEquivalent(with: down) {
                    return
                }
            } else if let id = menuCommand(characters: characters, flags: flags) {
                // The menu bar acts on the key window only: run the command its shortcut stands for.
                _ = try execute(id, nil)
                return
            }
        }
        window.sendEvent(down)
        window.sendEvent(try event(.keyUp))
    }

    /// The command the menu bar runs for a shortcut (the Mac's shortcut table).
    private static func menuCommand(characters: String, flags: NSEvent.ModifierFlags) -> String? {
        var modifiers: EventModifiers = []
        if flags.contains(.command) { modifiers.insert(.command) }
        if flags.contains(.control) { modifiers.insert(.control) }
        if flags.contains(.option) { modifiers.insert(.option) }
        if flags.contains(.shift) { modifiers.insert(.shift) }
        let key = characters.lowercased()
        let commands = (try? document().engine.commands()) ?? []
        return commands.first { info in
            guard info.enabled, let shortcut = MacPlacement.shortcut(for: info) else { return false }
            return shortcut.modifiers == modifiers && String(shortcut.key.character).lowercased() == key
        }?.id
    }

    /// Typed text: into the editor that has the keyboard, or starting an entry in the grid.
    private static func text(_ text: String) throws {
        let window = try window()
        switch window.firstResponder {
        case let input as NSTextView:
            input.insertText(text, replacementRange: NSRange(location: NSNotFound, length: 0))
        case let grid as GridNSView:
            grid.beginEditing(text: text)
        default:
            if let grid = try editor().grid, grid.window === window {
                grid.beginEditing(text: text)
            } else {
                throw Failure("nothing in the window takes text")
            }
        }
    }

    // MARK: - Screenshots and files

    /// The window as PNG at the screen's scale, toolbar and an open sheet dialog included (only
    /// the GridCraft window, never the screen).
    private static func screenshot(path: String?) throws -> Any {
        let window = try window()
        guard let content = window.contentView else { throw Failure("the window can't be captured") }
        // The frame view holds the toolbar too; it lines up with the content (full-size content).
        let view = content.superview.flatMap { $0.bounds.size == content.bounds.size ? $0 : nil } ?? content
        guard let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { throw Failure("the window can't be captured") }
        view.cacheDisplay(in: view.bounds, to: rep)
        if let sheet = window.attachedSheet, let sheetView = sheet.contentView,
           let sheetRep = sheetView.bitmapImageRepForCachingDisplay(in: sheetView.bounds),
           let context = NSGraphicsContext(bitmapImageRep: rep) {
            sheetView.cacheDisplay(in: sheetView.bounds, to: sheetRep)
            // The context draws in points from the window's bottom-left corner.
            let frame = sheet.contentRect(forFrameRect: sheet.frame)
            NSGraphicsContext.saveGraphicsState()
            NSGraphicsContext.current = context
            sheetRep.draw(in: CGRect(x: frame.minX - window.frame.minX, y: frame.minY - window.frame.minY, width: frame.width, height: frame.height))
            NSGraphicsContext.restoreGraphicsState()
        }
        guard let png = rep.representation(using: .png, properties: [:]) else { throw Failure("the window can't be captured") }
        let url = path.map { URL(fileURLWithPath: ($0 as NSString).expandingTildeInPath) }
            ?? FileManager.default.temporaryDirectory.appendingPathComponent("gridcraft-\(Int(Date().timeIntervalSince1970 * 1000)).png")
        try png.write(to: url)
        return ["path": url.path, "width": rep.pixelsWide, "height": rep.pixelsHigh]
    }

    /// Opens a workbook in a new window (an already open one comes to the front) and waits for it.
    private static func open(_ p: [String: Any]) async throws -> Any {
        guard let path = p["path"] as? String else { throw Failure("missing `path`") }
        if p["password"] is String {
            throw Failure("password-protected workbooks open from File ▸ Open")
        }
        let url = URL(fileURLWithPath: (path as NSString).expandingTildeInPath)
        guard FileManager.default.fileExists(atPath: url.path) else { throw Failure("no such file: \(url.path)") }
        let before = WorkbookRegistry.shared.frontmost
        let opened: NSDocument? = try await withCheckedThrowingContinuation { continuation in
            NSDocumentController.shared.openDocument(withContentsOf: url, display: true) { document, _, error in
                if let error {
                    continuation.resume(throwing: Failure(error.localizedDescription))
                } else {
                    continuation.resume(returning: document)
                }
            }
        }
        // The window registers its workbook when it appears; one already open comes to the front
        // without becoming key while GridCraft isn't the active app, so register it here.
        for _ in 0..<100 {
            WorkbookRegistry.shared.activated(window: opened?.windowControllers.first?.window)
            if WorkbookRegistry.shared.frontmost !== before { break }
            try? await Task.sleep(for: .milliseconds(50))
        }
        return ["title": url.lastPathComponent]
    }

    /// Saves the frontmost workbook: to `path` (format by extension, as `file.save`), or in place.
    private static func save(path: String?) async throws -> Any {
        if let path {
            return try execute("file.save", ["path": (path as NSString).expandingTildeInPath])
        }
        guard let file = try window().windowController?.document as? NSDocument, let url = file.fileURL, let type = file.fileType else {
            throw Failure("the workbook hasn't been saved yet: pass `path`")
        }
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            file.save(to: url, ofType: type, for: .saveOperation) { error in
                if let error {
                    continuation.resume(throwing: Failure(error.localizedDescription))
                } else {
                    continuation.resume()
                }
            }
        }
        return ["path": url.path]
    }
}
