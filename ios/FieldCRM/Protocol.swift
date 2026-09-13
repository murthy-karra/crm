import Foundation

// Wire values retain decimal revisions as strings; no floating point conversion.
enum JSON: Codable, Equatable, Sendable {
    case object([String: JSON]), array([JSON]), string(String), number(Double), bool(Bool), null
    init(from decoder: Decoder) throws {
        let c = try decoder.singleValueContainer()
        if c.decodeNil() { self = .null }
        else if let v = try? c.decode(Bool.self) { self = .bool(v) }
        else if let v = try? c.decode(String.self) { self = .string(v) }
        else if let v = try? c.decode([String: JSON].self) { self = .object(v) }
        else if let v = try? c.decode([JSON].self) { self = .array(v) }
        else { self = .number(try c.decode(Double.self)) }
    }
    func encode(to encoder: Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .object(let v): try c.encode(v)
        case .array(let v): try c.encode(v)
        case .string(let v): try c.encode(v)
        case .number(let v): try c.encode(v)
        case .bool(let v): try c.encode(v)
        case .null: try c.encodeNil()
        }
    }
    subscript(_ key: String) -> JSON { if case .object(let o) = self { return o[key] ?? .null }; return .null }
    var text: String { if case .string(let s) = self { return s }; return "" }
    var list: [JSON] { if case .array(let a) = self { return a }; return [] }
    var flag: Bool { self == .bool(true) }
    static func s(_ value: String) -> JSON { .string(value) }
}
func encode<T: Encodable>(_ value: T) throws -> Data {
    let encoder = JSONEncoder(); encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
    return try encoder.encode(value)
}
func decode<T: Decodable>(_ type: T.Type, _ data: Data) throws -> T { try JSONDecoder().decode(type, from: data) }
func stamp(_ date: Date = Date()) -> String { ISO8601DateFormatter().string(from: date) }
func date(_ value: String) throws -> Date {
    let f = ISO8601DateFormatter(); f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    if let d = f.date(from: value) ?? ISO8601DateFormatter().date(from: value) { return d }
    throw LocalError.invalidProtocol
}
func revision(_ value: String) throws -> Int64 {
    guard let n = Int64(value), n > 0, String(n) == value else { throw LocalError.invalidProtocol }; return n
}
struct Bootstrap: Codable, Equatable, Sendable {
    let context_id: String, installation_id: String, actor_user_id: String, organization_id: String
    let workspace_revision: String, authorized_at: String, offline_access_expires_at: String, server_time: String
    let capabilities: [String]
    let `protocol`: String
    var identity: String { actor_user_id + "_" + organization_id }
}
struct Envelope: Codable, Equatable, Sendable {
    let context_id: String, operation_id: String, kind: String, device_recorded_at: String
    let payload: JSON
    var person: String { payload["person_id"].text }
}
struct Receipt: Codable, Equatable, Sendable {
    let operation_id: String, outcome: String, resource_type: String, resource_id: String
    let committed_revision: String?, person_revision: String, accepted_at: String
    let changed: Bool, replayed: Bool
}
struct ManifestItem: Codable, Sendable { let person_id: String, revision: String; let reasons: [String] }
struct Manifest: Codable, Sendable { let items: [ManifestItem]; let next_cursor: String?; let complete: Bool }
struct Generation: Codable, Sendable {
    let generation_id: String, context_id: String, evaluated_at: String, expires_at: String
    let complete: Bool, selected_count: Int, manifest: Manifest
}
struct Page: Codable, Sendable {
    let generation_id: String, person_id: String, revision: String, section: String
    let summary: JSON?, items: [JSON], next_cursor: String?, complete: Bool
}
struct Seal: Codable, Sendable {
    let generation_id: String, context_id: String, sealed_at: String, evaluated_at: String
    let selected_count: Int, today: JSON
}
struct Bundle: Codable, Sendable {
    let person: String, revision: String, summary: JSON, contacts: [JSON], notes: [JSON], tasks: [JSON]
}
struct Draft: Codable, Identifiable, Sendable {
    var id: String, person: String, kind: String, text: String, revision: Int
    var taskKind = "follow_up", dueAt: String? = nil
}
struct Queued: Identifiable, Sendable {
    let envelope: Envelope, bytes: Data, status: String, error: String?, receipt: Receipt?, overlay: Bool
    let attempts: Int, retryAt: Double
    var id: String { envelope.operation_id }
    var title: String { envelope.kind == "add_note" ? envelope.payload["body"].text : envelope.payload["title"].text }
}
enum LocalError: Error, LocalizedError {
    case locked, storage, invalidProtocol, staleDraft, invalidInput, unavailableKey, identityChanged, lostResponse
    var errorDescription: String? {
        switch self {
        case .locked: "Online sign-in is required. Saved work remains protected on this device."
        case .storage: "Could not save to device storage. Your previous saved work is retained. Free space, then retry."
        case .invalidProtocol: "The server response could not be verified. Update or retry; saved work is retained."
        case .staleDraft: "This draft changed. Reopen its latest saved revision before submitting."
        case .invalidInput: "Enter valid text before saving this action."
        case .unavailableKey: "Protected storage is unavailable. Set a device passcode and unlock the device, then retry."
        case .identityChanged: "Account changed. Work from the previous account remains protected."
        case .lostResponse: "Response interrupted after upload. Retry will use the same saved action."
        }
    }
}
