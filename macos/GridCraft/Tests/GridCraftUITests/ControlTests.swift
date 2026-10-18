import AppKit
import Foundation
import GridCraftEngine
@testable import GridCraftUI
import Network
import Testing

@MainActor
@Suite(.serialized) struct ControlTests {
    /// One request through the dispatcher, the reply decoded.
    private func call(_ method: String, _ params: [String: Any] = [:], id: Int = 1) async -> [String: Any] {
        let line = (try? JSONSerialization.data(withJSONObject: ["id": id, "method": method, "params": params])) ?? Data()
        return (try? JSONSerialization.jsonObject(with: await Control.reply(to: line))) as? [String: Any] ?? [:]
    }

    private func open() -> (SpreadsheetDocument, EditController) {
        let document = SpreadsheetDocument()
        let editor = EditController(document: document)
        WorkbookRegistry.shared.activated(document, editor: editor)
        return (document, editor)
    }

    @Test func theLaunchArgumentOrEnvironmentTurnsItOn() {
        #expect(ControlServer.requestedPort(arguments: ["GridCraft", "--control", "7979"], environment: [:]) == 7979)
        #expect(ControlServer.requestedPort(arguments: ["GridCraft"], environment: ["GRIDCRAFT_CONTROL_PORT": "8001"]) == 8001)
        #expect(ControlServer.requestedPort(arguments: ["GridCraft", "--control"], environment: [:]) == nil)
        #expect(ControlServer.requestedPort(arguments: ["GridCraft", "--control", "no"], environment: [:]) == nil)
    }

    @Test func commandsRunAndReadsAnswer() async throws {
        let (document, editor) = open()
        defer { withExtendedLifetime(editor) {} }
        let set = await call("engine.execute", ["command": "cell.set", "params": ["cell": "B2", "input": "=6*7"]], id: 7)
        #expect(set["ok"] as? Bool == true && set["id"] as? Int == 7)
        let get = await call("cell.get", ["cell": "B2"])
        #expect((get["result"] as? [String: Any])?["text"] as? String == "42")
        // A command id is a method too.
        #expect(await call("home.bold", ["range": "B2", "on": true])["ok"] as? Bool == true)
        let journal = await call("engine.journal")["result"] as? [[String: Any]] ?? []
        #expect(journal.contains { $0["command"] as? String == "home.bold" })
        let found = await call("engine.commands", ["search": "inspector"])["result"] as? [[String: Any]] ?? []
        #expect(found.contains { $0["id"] as? String == "ui.inspector" })
        #expect(await call("ui.inspector", ["on": true])["ok"] as? Bool == true && document.showInspector)
    }

    @Test func badRequestsGetErrorsNotCrashes() async {
        let opened = open()
        defer { withExtendedLifetime(opened) {} }
        let garbage = (try? JSONSerialization.jsonObject(with: await Control.reply(to: Data("{not json".utf8)))) as? [String: Any]
        #expect(garbage?["ok"] as? Bool == false)
        #expect(await call("no.such.method")["ok"] as? Bool == false)
        #expect(await call("engine.execute", ["command": "cell.set", "params": [1, 2]])["ok"] as? Bool == false)
        #expect(await call("engine.execute", ["command": "no.such"])["ok"] as? Bool == false)
        #expect(await call("ui.edit.text", ["text": "x"])["ok"] as? Bool == false)
        #expect(await call("ui.dialog.confirm")["ok"] as? Bool == false)
        #expect(await call("ui.dialog", ["name": "noSuchDialog"])["ok"] as? Bool == false)
        #expect(await call("ui.key", ["key": "nonsense"])["ok"] as? Bool == false)
        #expect(await call("app.open", ["path": "/no/such/book.xlsx"])["ok"] as? Bool == false)
        #expect(await call("app.save")["ok"] as? Bool == false)
        // Every method survives empty params.
        for method in ["engine.execute", "engine.commands", "document.inspect", "sheet.read", "cell.get", "ui.inspect", "ui.dialog",
                       "ui.dialog.set", "ui.dialog.cancel", "ui.message.dismiss", "ui.edit.begin", "ui.edit.text", "ui.edit.commit",
                       "ui.edit.cancel", "ui.key", "ui.text", "ui.move", "ui.click", "ui.drag", "ui.cell.click", "ui.cell.drag",
                       "ui.resize", "ui.focus", "ui.palette", "ui.palette", "app.open"] {
            #expect(!(await call(method)).isEmpty, "\(method)")
        }
    }

    @Test func theEditorIsDrivenLikeTyping() async {
        let (document, editor) = open()
        #expect(await call("ui.edit.begin", ["text": "="])["ok"] as? Bool == true)
        #expect(await call("ui.edit.text", ["text": "=2+3"])["ok"] as? Bool == true)
        let state = await call("ui.inspect")["result"] as? [String: Any]
        #expect((state?["editor"] as? [String: Any])?["text"] as? String == "=2+3")
        #expect(await call("ui.edit.commit", ["move": "none"])["ok"] as? Bool == true)
        #expect(!editor.isEditing)
        let get = await call("cell.get", ["cell": "A1"])
        #expect((get["result"] as? [String: Any])?["text"] as? String == "5")
    }

    @Test func formDialogsAreFilledAndConfirmed() async {
        let (document, editor) = open()
        defer { withExtendedLifetime(editor) {} }
        let opened = await call("ui.dialog", ["name": "rowHeight"])
        #expect((opened["result"] as? [String: Any])?["name"] as? String == "rowHeight")
        #expect(document.form != nil)
        #expect(await call("ui.dialog.set", ["key": "nope", "value": 1])["ok"] as? Bool == false)
        #expect(await call("ui.dialog.set", ["key": "height", "value": 40])["ok"] as? Bool == true)
        #expect(await call("ui.dialog.confirm")["ok"] as? Bool == true)
        #expect(document.form == nil)
        let again = try? document.engine.openForm(name: "rowHeight", params: "{}")
        #expect(again?.text(key: "height") == "40" || again?.text(key: "height") == "40.0")
        #expect(await call("ui.dialog", ["name": "find"])["ok"] as? Bool == true && document.dialog?.name == "find")
        #expect(await call("ui.dialog.cancel")["ok"] as? Bool == true && document.dialog == nil)
    }

    @Test func aClientTalksOverLoopback() async throws {
        let opened = open()
        defer { withExtendedLifetime(opened) {} }
        let server = try ControlServer(port: 0)
        defer { server.stop() }
        let port = try #require(await server.port())
        let replies = try await Self.exchange(port: port, lines: [
            #"{"id": 1, "method": "cell.set", "params": {"cell": "C3", "input": "hello"}}"#,
            #"{"id": 2, "method": "cell.get", "params": {"cell": "C3"}}"#,
            "not json",
        ])
        #expect(replies.count == 3)
        #expect(replies.first?.contains(#""id":1"#) == true)
        #expect(replies.dropFirst().first?.contains("hello") == true)
        #expect(replies.last?.contains(#""ok":false"#) == true)
    }

    @Test func onlyRequestLinesAreRequests() {
        let request = #"{"id": 1, "method": "cell.get"}"#
        #expect(ControlServer.classify(Data(request.utf8)) == .request(Data(request.utf8)))
        #expect(ControlServer.classify(Data(" \r".utf8)) == .blank)
        #expect(ControlServer.classify(Data()) == .blank)
        let rejected = ["POST / HTTP/1.1", "GET /favicon.ico HTTP/1.1", "PRI * HTTP/2.0", "Host: 127.0.0.1:7979", "not json",
                        "[1, 2]", "42", #"{"id": 1}"#, #"{"method": 7}"#]
        for line in rejected {
            guard case .reject = ControlServer.classify(Data(line.utf8)) else {
                Issue.record("accepted \(line)")
                continue
            }
        }
        guard case .reject = ControlServer.classify(Data([0x7B, 0xFF, 0xFE, 0x7D])) else {
            Issue.record("accepted bytes that aren't UTF-8")
            return
        }
        guard case .reject = ControlServer.classify(Data(request.utf8), max: 8) else {
            Issue.record("accepted a line over the limit")
            return
        }
    }

    @Test func nothingAfterALineThatIsNotARequestRuns() async throws {
        let (document, editor) = open()
        defer { withExtendedLifetime(editor) {} }
        let server = try ControlServer(port: 0)
        defer { server.stop() }
        let port = try #require(await server.port())
        // A browser POST: request line, headers, blank line, then a JSON body that must not run.
        let replies = try await Self.exchange(port: port, lines: [
            #"{"id": 1, "method": "cell.set", "params": {"cell": "A1", "input": "kept"}}"#,
            "POST / HTTP/1.1",
            "Host: 127.0.0.1:\(port)",
            "Content-Type: text/plain",
            "",
            #"{"id": 2, "method": "cell.set", "params": {"cell": "A2", "input": "ran"}}"#,
        ])
        #expect(replies.count == 2)
        #expect(replies.first?.contains(#""ok":true"#) == true)
        #expect(replies.last?.contains("HTTP is not served here") == true)
        // Text that isn't JSON closes it too.
        let afterGarbage = try await Self.exchange(port: port, lines: [
            "",
            "hello",
            #"{"id": 3, "method": "cell.set", "params": {"cell": "A3", "input": "ran"}}"#,
        ])
        #expect(afterGarbage.count == 1 && afterGarbage.first?.contains(#""ok":false"#) == true)
        let a1 = await call("cell.get", ["cell": "A1"])["result"] as? [String: Any]
        #expect(a1?["text"] as? String == "kept")
        for cell in ["A2", "A3"] {
            let text = (await call("cell.get", ["cell": cell])["result"] as? [String: Any])?["text"] as? String
            #expect(text != "ran", "\(cell)")
        }
        withExtendedLifetime(document) {}
    }

    /// Sends `lines`, half-closes, and reads every reply line.
    nonisolated private static func exchange(port: UInt16, lines: [String]) async throws -> [String] {
        let connection = NWConnection(host: .ipv4(.loopback), port: NWEndpoint.Port(rawValue: port) ?? .any, using: .tcp)
        let queue = DispatchQueue(label: "control-test")
        connection.start(queue: queue)
        defer { connection.cancel() }
        let payload = Data(lines.map { $0 + "\n" }.joined().utf8)
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            connection.send(content: payload, contentContext: .finalMessage, isComplete: true, completion: .contentProcessed { error in
                if let error { continuation.resume(throwing: error) } else { continuation.resume() }
            })
        }
        var received = Data()
        while true {
            let (data, done): (Data?, Bool) = try await withCheckedThrowingContinuation { continuation in
                connection.receive(minimumIncompleteLength: 1, maximumLength: 1 << 16) { data, _, isComplete, error in
                    if let error { continuation.resume(throwing: error) } else { continuation.resume(returning: (data, isComplete)) }
                }
            }
            if let data { received.append(data) }
            if done { break }
        }
        return String(decoding: received, as: UTF8.self).split(separator: "\n").map(String.init)
    }
}

@MainActor
@Suite struct ControlShortcutTests {
    @Test func aShortcutRunsItsMenuCommandWithoutAKeyWindow() async throws {
        let document = SpreadsheetDocument()
        let editor = EditController(document: document)
        WorkbookRegistry.shared.activated(document, editor: editor)
        defer { withExtendedLifetime(editor) {} }
        // No window: the key can't be delivered, but nothing crashes either.
        let line = try JSONSerialization.data(withJSONObject: ["id": 1, "method": "ui.key", "params": ["key": "b", "cmd": true]])
        let reply = try #require(try JSONSerialization.jsonObject(with: await Control.reply(to: line)) as? [String: Any])
        #expect(reply["ok"] as? Bool == false)
    }
}
