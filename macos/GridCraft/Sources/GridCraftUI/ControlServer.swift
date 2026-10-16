import Foundation
import Network

/// The control channel for agents and scripts (`docs/control-protocol.md`): newline-delimited
/// JSON requests on 127.0.0.1, answered in order on the same connection by `Control`. Off unless
/// the app is launched with `--control <port>` or `GRIDCRAFT_CONTROL_PORT`; it never listens
/// beyond the loopback interface.
public final class ControlServer: Sendable {
    /// A request line longer than this closes the connection.
    static let maxLine = 64 << 20

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
                FileHandle.standardError.write(Data("GridCraft control channel: \(error)\n".utf8))
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
            FileHandle.standardError.write(Data("GridCraft control channel on 127.0.0.1:\(port)\n".utf8))
        } catch {
            FileHandle.standardError.write(Data("GridCraft control channel: \(error.localizedDescription)\n".utf8))
        }
    }
}

/// One client: lines in, replies out, one request at a time on the main actor.
private final class ControlConnection: @unchecked Sendable {
    private let connection: NWConnection
    private let queue: DispatchQueue
    /// Touched only on `queue`.
    private var buffer = Data()
    private let lines: AsyncStream<Data>
    private let continuation: AsyncStream<Data>.Continuation

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
                let reply = await Control.reply(to: line)
                connection.send(content: reply + Data("\n".utf8), completion: .contentProcessed { _ in })
            }
            connection.send(content: nil, contentContext: .finalMessage, isComplete: true, completion: .contentProcessed { [connection] _ in connection.cancel() })
        }
    }

    private func receive() {
        connection.receive(minimumIncompleteLength: 1, maximumLength: 1 << 16) { [self] data, _, isComplete, error in
            if let data {
                buffer.append(data)
                while let newline = buffer.firstIndex(of: UInt8(ascii: "\n")) {
                    let line = buffer[buffer.startIndex..<newline]
                    buffer.removeSubrange(buffer.startIndex...newline)
                    if !line.allSatisfy({ $0 == UInt8(ascii: " ") || $0 == UInt8(ascii: "\r") }) {
                        continuation.yield(Data(line))
                    }
                }
                if buffer.count > ControlServer.maxLine {
                    connection.cancel()
                    return
                }
            }
            if error != nil {
                continuation.finish()
                connection.cancel()
            } else if isComplete {
                // The client is done sending: answer what came, then close.
                continuation.finish()
            } else {
                receive()
            }
        }
    }
}
