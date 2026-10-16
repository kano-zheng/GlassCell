import Foundation

/// Command parameters and results are JSON text.
public enum JSON {
    public static func encode(_ object: [String: Any]) -> String {
        guard let data = try? JSONSerialization.data(withJSONObject: object), let text = String(data: data, encoding: .utf8) else {
            return "{}"
        }
        return text
    }

    public static func decode(_ text: String) -> [String: Any] {
        guard let data = text.data(using: .utf8), let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return [:]
        }
        return object
    }
}
