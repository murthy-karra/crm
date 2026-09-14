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
    /// The server maps additions to positions in the original operation array.
    /// This is deliberately optional so pre-Mobile005 receipt bytes still decode.
    let added_contact_ids: [AddedContactID]?
    init(operation_id: String, outcome: String, resource_type: String, resource_id: String, committed_revision: String?, person_revision: String, accepted_at: String, changed: Bool, replayed: Bool, added_contact_ids: [AddedContactID]? = nil) {
        self.operation_id = operation_id; self.outcome = outcome; self.resource_type = resource_type; self.resource_id = resource_id
        self.committed_revision = committed_revision; self.person_revision = person_revision; self.accepted_at = accepted_at
        self.changed = changed; self.replayed = replayed; self.added_contact_ids = added_contact_ids
    }
}
struct AddedContactID: Codable, Equatable, Sendable { let ordinal: Int, id: String }
struct ManifestItem: Codable, Sendable { let person_id: String, revision: String; let reasons: [String] }
struct Manifest: Codable, Sendable { let items: [ManifestItem]; let next_cursor: String?; let complete: Bool }
struct Generation: Codable, Sendable {
    let generation_id: String, context_id: String, evaluated_at: String, expires_at: String
    let complete: Bool, selected_count: Int, manifest: Manifest
    let stage_catalog: StageCatalog?
    init(generation_id: String, context_id: String, evaluated_at: String, expires_at: String, complete: Bool, selected_count: Int, manifest: Manifest, stage_catalog: StageCatalog? = nil) {
        self.generation_id = generation_id; self.context_id = context_id; self.evaluated_at = evaluated_at; self.expires_at = expires_at
        self.complete = complete; self.selected_count = selected_count; self.manifest = manifest; self.stage_catalog = stage_catalog
    }
}
struct StageCatalog: Codable, Equatable, Sendable { let revision: String, stages_url: String }
struct Stage: Codable, Equatable, Identifiable, Sendable {
    let id: String, name: String, position: Int
    init(id: String, name: String, position: Int) { self.id = id; self.name = name; self.position = position }
    enum CodingKeys: String, CodingKey { case id, name, position }
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id); name = try c.decode(String.self, forKey: .name)
        position = try c.decodeIfPresent(Int.self, forKey: .position) ?? 0
    }
}
struct StagePage: Codable, Sendable {
    let generation_id: String, revision: String, items: [Stage], next_cursor: String?, complete: Bool
}
struct Page: Codable, Sendable {
    let generation_id: String, person_id: String, revision: String, section: String
    let summary: JSON?, items: [JSON], next_cursor: String?, complete: Bool
}
struct Seal: Codable, Sendable {
    let generation_id: String, context_id: String, sealed_at: String, evaluated_at: String
    let selected_count: Int, today: JSON
}
struct CurrentRecordResponse: Codable, Sendable {
    let context_id: String, person_id: String, person_revision: String
    let note: JSON?
    let task: JSON?
    var record: JSON? { note ?? task }
}
struct CurrentStageResponse: Codable, Sendable {
    let context_id: String, person_id: String, person_revision: String, stage_revision: String, stage: Stage
}
struct CurrentDetailsResponse: Codable, Sendable {
    let context_id: String, person_id: String, person_revision: String, details_revision: String
    let first_name: String?, last_name: String?, items: [JSON], next_cursor: String?, complete: Bool
}
struct Bundle: Codable, Sendable {
    let person: String, revision: String, summary: JSON, contacts: [JSON], tasks: [JSON]
    var notes: [JSON]
    var orderedContacts: [JSON] { ContactDisplayOrder.sorted(contacts) }
}

/// Summary pages retain their UUID pagination order. Only the complete display
/// projection uses the server's imported-first contact ordering. Old downloaded
/// representations without ordering metadata retain their readable legacy order.
enum ContactDisplayOrder {
    static func sorted(_ contacts: [JSON]) -> [JSON] {
        var keyed: [(value: JSON, order: Int?, created: Date, id: String)] = []
        for contact in contacts {
            guard let created = try? date(contact["created_at"].text),
                  let id = UUID(uuidString: contact["id"].text) else { return contacts }
            let order: Int?
            switch contact["import_order"] {
            case .null: order = nil
            case .number(let value):
                guard value.isFinite, value.rounded() == value, value >= Double(Int32.min), value <= Double(Int32.max) else { return contacts }
                order = Int(value)
            default: return contacts
            }
            keyed.append((contact, order, created, id.uuidString.lowercased()))
        }
        return keyed.sorted {
            if $0.order != $1.order { return ($0.order ?? Int.max) < ($1.order ?? Int.max) }
            if $0.created != $1.created { return $0.created < $1.created }
            return $0.id < $1.id
        }.map(\.value)
    }
}
/// A draft is protected input, not a projection of the replaceable downloaded
/// bundle.  `baseline` and `proposal` deliberately live in the encrypted store.
struct Draft: Codable, Identifiable, Sendable {
    var id: String, person: String, kind: String, text: String, revision: Int
    var taskKind: String = "follow_up", dueAt: String? = nil
    /// Contact input is kept separate from note/task text.  It is protected
    /// draft input until the immutable envelope has been committed locally.
    var contactChannel: String? = nil
    var contactOutcome: String? = nil
    var occurredAt: String? = nil
    var deviceRecordedAt: String? = nil
    var targetID: String? = nil
    var expectedRevision: String? = nil
    var baseline: JSON? = nil
    var proposal: JSON? = nil
    var mode: String = "editing" // editing, follow_up, conflict, superseded, unavailable
    var predecessor: String? = nil
    var current: JSON? = nil
    var editorEpoch: String = UUID().uuidString.lowercased()
    enum CodingKeys: String, CodingKey { case id, person, kind, text, revision, taskKind, dueAt, contactChannel, contactOutcome, occurredAt, deviceRecordedAt, targetID, expectedRevision, baseline, proposal, mode, predecessor, current, editorEpoch }
    init(id: String, person: String, kind: String, text: String = "", revision: Int, taskKind: String = "follow_up", dueAt: String? = nil, contactChannel: String? = nil, contactOutcome: String? = nil, occurredAt: String? = nil, deviceRecordedAt: String? = nil, targetID: String? = nil, expectedRevision: String? = nil, baseline: JSON? = nil, proposal: JSON? = nil, mode: String = "editing", predecessor: String? = nil, current: JSON? = nil, editorEpoch: String = UUID().uuidString.lowercased()) {
        self.id = id; self.person = person; self.kind = kind; self.text = text; self.revision = revision
        self.taskKind = taskKind; self.dueAt = dueAt; self.targetID = targetID; self.expectedRevision = expectedRevision
        self.contactChannel = contactChannel; self.contactOutcome = contactOutcome; self.occurredAt = occurredAt; self.deviceRecordedAt = deviceRecordedAt
        self.baseline = baseline; self.proposal = proposal; self.mode = mode; self.predecessor = predecessor; self.current = current; self.editorEpoch = editorEpoch
    }
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id); person = try c.decode(String.self, forKey: .person)
        kind = try c.decode(String.self, forKey: .kind); text = try c.decode(String.self, forKey: .text)
        revision = try c.decode(Int.self, forKey: .revision)
        taskKind = try c.decodeIfPresent(String.self, forKey: .taskKind) ?? "follow_up"; dueAt = try c.decodeIfPresent(String.self, forKey: .dueAt)
        contactChannel = try c.decodeIfPresent(String.self, forKey: .contactChannel); contactOutcome = try c.decodeIfPresent(String.self, forKey: .contactOutcome)
        occurredAt = try c.decodeIfPresent(String.self, forKey: .occurredAt); deviceRecordedAt = try c.decodeIfPresent(String.self, forKey: .deviceRecordedAt)
        targetID = try c.decodeIfPresent(String.self, forKey: .targetID); expectedRevision = try c.decodeIfPresent(String.self, forKey: .expectedRevision)
        baseline = try c.decodeIfPresent(JSON.self, forKey: .baseline); proposal = try c.decodeIfPresent(JSON.self, forKey: .proposal)
        mode = try c.decodeIfPresent(String.self, forKey: .mode) ?? "editing"; predecessor = try c.decodeIfPresent(String.self, forKey: .predecessor)
        current = try c.decodeIfPresent(JSON.self, forKey: .current); editorEpoch = try c.decodeIfPresent(String.self, forKey: .editorEpoch) ?? UUID().uuidString.lowercased()
    }
    var isEdit: Bool { kind == "edit_note" || kind == "update_task" }
    var isDetails: Bool { kind == "update_person_details" }
    var resourceType: String? { kind == "edit_note" ? "note" : kind == "update_task" ? "task" : nil }
}
struct Queued: Identifiable, Sendable {
    let envelope: Envelope, bytes: Data, status: String, error: String?, receipt: Receipt?, overlay: Bool
    let attempts: Int, retryAt: Double
    var id: String { envelope.operation_id }
    var isContact: Bool { envelope.kind == "log_contact_attempt" }
    var isStage: Bool { envelope.kind == "change_person_stage" }
    var isDetails: Bool { envelope.kind == "update_person_details" }
    var title: String {
        if envelope.kind == "add_note" || envelope.kind == "edit_note" { return envelope.payload["body"].text }
        if isContact { return [envelope.payload["channel"].text, envelope.payload["outcome"].text, envelope.payload["occurred_at"].text].filter { !$0.isEmpty }.joined(separator: " · ") }
        if isStage { return envelope.payload["stage_id"].text }
        if isDetails { return "Profile details" }
        if ["create_task", "update_task"].contains(envelope.kind) { return envelope.payload["title"].text }
        return ""
    }
    var targetID: String? {
        if envelope.kind == "edit_note" { return envelope.payload["note_id"].text }
        if envelope.kind == "update_task" { return envelope.payload["task_id"].text }
        if envelope.kind == "complete_task" { return envelope.payload["target"]["task_id"].text.isEmpty ? nil : envelope.payload["target"]["task_id"].text }
        if envelope.kind == "change_person_stage" { return envelope.payload["person_id"].text }
        if envelope.kind == "update_person_details" { return envelope.payload["person_id"].text }
        return nil
    }
}
enum LocalError: Error, LocalizedError {
    case locked, storage, invalidProtocol, staleDraft, invalidInput, unavailableKey, identityChanged, lostResponse, waitingPredecessor
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
        case .waitingPredecessor: "Saved draft — waiting for the previous change."
        }
    }
}
