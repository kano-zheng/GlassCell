import Foundation
import Network

/// The control channel for agents and scripts (`docs/control-protocol.md`): newline-delimited
/// JSON requests on 127.0.0.1, answered in order on the same connection by `Control`. Off unless
/// the app is launched with `--control <port>` or `GRIDCRAFT_CONTROL_PORT`; it never listens
/// beyond the loopback interface.
///
/// The server only reads requests: a line that is not a JSON object with a string `method` (an
/// HTTP request line, a stray `nc`, binary data, a line over `maxLine`) gets one error reply and
/// the connection is closed, so nothing sent after it is read or run.
public final class ControlServer: Sendable {
    /// A request line longer than this closes the connection.
    static let maxLine = 64 << 20

    /// One line from a client, sorted before anything runs.
    enum Line: Equatable, Sendable {
        /// A JSON object with a string `method`, for `Control.reply`.
        case request(Data)
        case blank
        /// Not a request: reply with this error and close the connection.
        case reject(String)
    }

    /// Sorts one line (newline stripped) of at most `max` bytes.
    static func classify(_ line: Data, max: Int = maxLine) -> Line {
        if line.count > max {
            return .reject("request too large (over \(max) bytes); closing the connection")
        }
        guard let text = String(validating: line, as: UTF8.self) else {
            return .reject("not a control request (invalid UTF-8); closing the connection")
        }
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty { return .blank }
        if isHTTPRequestLine(trimmed) {
            return .reject("HTTP is not served here: this port takes JSON lines; closing the connection")
        }
        let json: Any
        do {
            json = try JSONSerialization.jsonObject(with: Data(trimmed.utf8))
        } catch {
            return .reject("invalid JSON; expected one JSON request object per line; closing the connection")
        }
        guard let object = json as? [String: Any] else {
            return .reject("not a control request (expected a JSON object); closing the connection")
        }
        guard object["method"] is String else {
            return .reject("not a control request (missing string `method`); closing the connection")
        }
        return .request(Data(trimmed.utf8))
    }

    /// An HTTP/1 request line (`POST / HTTP/1.1`) or the HTTP/2 preface (`PRI * HTTP/2.0`).
    static func isHTTPRequestLine(_ text: String) -> Bool {
        let methods: Set<Substring> = ["GET", "HEAD", "POST", "PUT", "DELETE", "CONNECT", "OPTIONS", "TRACE", "PATCH", "PRI"]
        guard let firstSpace = text.firstIndex(of: " "), let lastSpace = text.lastIndex(of: " ") else { return false }
        return methods.contains(text[..<firstSpace]) || text[text.index(after: lastSpace)...].hasPrefix("HTTP/")
    }

    /// The error reply that ends a connection.
    static func rejection(_ message: String) -> Data {
        let object: [String: Any] = ["id": NSNull(), "ok": false, "error": message]
        return (try? JSONSerialization.data(withJSONObject: object)) ?? Data(#"{"id":null,"ok":false}"#.utf8)
    }

    private let listener: NWListener
    private let queue = DispatchQueue(label: "gridcraft.control")

    /// Listens on `port` (0: any free port; `port()` tells which).
    public init(port: UInt16) throws {
        let parameters = NWParameters.tcp
        parameters.requiredLocalEndpoint = .hostPort(host: .ipv4(.loopback), port: NWEndpoint.Port(rawValue: port) ?? .any)
        parameters.allowLocalEndpointReuse = true
        listener = try NWListener(using: parameters)
        listener.newConnectionHandler = { [queue] connection in
            ControlConnection(connection: connection, queue: queue).start()
        }
        listener.stateUpdateHandler = { state in
            if case let .failed(error) = state {
                FileHandle.standardError.write(Data("GlassCell control channel: \(error)\n".utf8))
            }
        }
        listener.start(queue: queue)
    }

    /// The port once listening (`nil` if it couldn't within a few seconds).
    public func port() async -> UInt16? {
        for _ in 0..<200 {
            if listener.state == .ready, let port = listener.port { return port.rawValue }
            if case .failed = listener.state { return nil }
            try? await Task.sleep(for: .milliseconds(20))
        }
        return nil
    }

    public func stop() {
        listener.cancel()
    }

    /// The port asked for at launch: `--control <port>` or `GRIDCRAFT_CONTROL_PORT`.
    public static func requestedPort(arguments: [String] = CommandLine.arguments, environment: [String: String] = ProcessInfo.processInfo.environment) -> UInt16? {
        if let flag = arguments.firstIndex(of: "--control"), let value = arguments.dropFirst(flag + 1).first, let port = UInt16(value) {
            return port
        }
        return environment["GRIDCRAFT_CONTROL_PORT"].flatMap { UInt16($0) }
    }

    @MainActor private static var running: ControlServer?

    /// Starts the channel when launch asked for it.
    @MainActor public static func startIfRequested() {
        guard running == nil, let port = requestedPort() else { return }
        do {
            running = try ControlServer(port: port)
            FileHandle.standardError.write(Data("GlassCell control channel on 127.0.0.1:\(port)\n".utf8))
        } catch {
            FileHandle.standardError.write(Data("GlassCell control channel: \(error.localizedDescription)\n".utf8))
        }
    }
}

/// One client: lines in, replies out, one request at a time on the main actor.
private final class ControlConnection: @unchecked Sendable {
    private let connection: NWConnection
    private let queue: DispatchQueue
    /// Touched only on `queue`.
    private var buffer = Data()
    private let lines: AsyncStream<ControlServer.Line>
    private let continuation: AsyncStream<ControlServer.Line>.Continuation

    init(connection: NWConnection, queue: DispatchQueue) {
        self.connection = connection
        self.queue = queue
        (lines, continuation) = AsyncStream.makeStream()
    }

    func start() {
        connection.stateUpdateHandler = { [self] state in
            switch state {
            case .failed, .cancelled: continuation.finish()
            default: break
            }
        }
        connection.start(queue: queue)
        receive()
        Task { @MainActor [self] in
            for await line in lines {
                switch line {
                case let .request(data):
                    let reply = await Control.reply(to: data)
                    connection.send(content: reply + Data("\n".utf8), completion: .contentProcessed { _ in })
                case let .reject(message):
                    connection.send(content: ControlServer.rejection(message) + Data("\n".utf8), completion: .contentProcessed { _ in })
                case .blank:
                    break
                }
            }
            connection.send(content: nil, contentContext: .finalMessage, isComplete: true, completion: .contentProcessed { [connection] _ in connection.cancel() })
        }
    }

    /// Queues `line`; `false` when it isn't a request and reading must stop.
    private func accept(_ line: ControlServer.Line) -> Bool {
        switch line {
        case .blank:
            return true
        case .request:
            continuation.yield(line)
            return true
        case .reject:
            // The rejection is answered after the requests before it, then the connection closes.
            continuation.yield(line)
            continuation.finish()
            return false
        }
    }

    private func receive() {
        connection.receive(minimumIncompleteLength: 1, maximumLength: 1 << 16) { [self] data, _, isComplete, error in
            if let data {
                buffer.append(data)
                while let newline = buffer.firstIndex(of: UInt8(ascii: "\n")) {
                    let line = Data(buffer[buffer.startIndex..<newline])
                    buffer.removeSubrange(buffer.startIndex...newline)
                    guard accept(ControlServer.classify(line)) else { return }
                }
                if buffer.count > ControlServer.maxLine {
                    _ = accept(ControlServer.classify(buffer))
                    return
                }
            }
            if error != nil {
                continuation.finish()
                connection.cancel()
            } else if isComplete {
                // The client is done sending: the last line may lack its newline.
                if !buffer.isEmpty, !accept(ControlServer.classify(buffer)) { return }
                // Answer what came, then close.
                continuation.finish()
            } else {
                receive()
            }
        }
    }
}
