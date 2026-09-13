import Foundation
import SQLCipher

/// Used on the main actor by the app. Transactions contain no suspension points;
/// receipt application, draft submission and cache promotion cannot interleave.
final class LocalStore {
    private var db: OpaquePointer?
    let identity: String, context: String, url: URL
    private let transient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)
    init(url: URL, key: Data, identity: String, context: String, schemaTarget: Int = 3) throws {
        self.url = url; self.identity = identity; self.context = context
        guard sqlite3_open_v2(url.path, &db, SQLITE_OPEN_CREATE | SQLITE_OPEN_READWRITE | SQLITE_OPEN_FULLMUTEX, nil) == SQLITE_OK else {
            sqlite3_close(db); db = nil; throw LocalError.storage
        }
        do {
            guard key.count == 32, key.withUnsafeBytes({ sqlite3_key(db, $0.baseAddress, Int32(key.count)) }) == SQLITE_OK else { throw LocalError.unavailableKey }
            guard !(try rows("PRAGMA cipher_version")).isEmpty else { throw LocalError.unavailableKey }
            _ = try rows("SELECT count(*) FROM sqlite_master") // Verify the actual key before any migration.
            try run("PRAGMA journal_mode=WAL"); try run("PRAGMA synchronous=FULL")
            try run("PRAGMA temp_store=MEMORY"); try run("PRAGMA foreign_keys=ON")
            try run("PRAGMA busy_timeout=2000")
            try migrate(to: schemaTarget)
            if let existing = try meta("identity"), existing != identity { throw LocalError.identityChanged }
            if let existing = try meta("context"), existing != context { throw LocalError.identityChanged }
            try transaction { try setMeta("identity", identity); try setMeta("context", context) }
            var values = URLResourceValues(); values.isExcludedFromBackup = true
            var file = url; try file.setResourceValues(values)
            try FileManager.default.setAttributes([.protectionKey: FileProtectionType.complete], ofItemAtPath: url.path)
        } catch { sqlite3_close(db); db = nil; throw error }
    }
    deinit { sqlite3_close(db) }
    private func migrate(to target: Int) throws {
        let version = Int(try rows("PRAGMA user_version").first?.first ?? "0") ?? 0
        guard version <= 3 else { throw LocalError.invalidProtocol }
        try transaction {
            if version < 1 {
                for sql in [
                    "CREATE TABLE metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL)",
                    "CREATE TABLE drafts(id TEXT PRIMARY KEY,body TEXT NOT NULL)",
                    "CREATE TABLE operations(seq INTEGER PRIMARY KEY AUTOINCREMENT,id TEXT NOT NULL UNIQUE,envelope TEXT NOT NULL,status TEXT NOT NULL DEFAULT 'pending',receipt TEXT,overlay INTEGER NOT NULL DEFAULT 1)",
                    "CREATE TABLE bundles(person TEXT NOT NULL,revision TEXT NOT NULL,body TEXT NOT NULL,PRIMARY KEY(person,revision))",
                    "CREATE TABLE members(generation TEXT NOT NULL,person TEXT NOT NULL,revision TEXT NOT NULL,PRIMARY KEY(generation,person))",
                    "CREATE TABLE pages(generation TEXT NOT NULL,person TEXT NOT NULL,section TEXT NOT NULL,position INTEGER NOT NULL,body TEXT NOT NULL,PRIMARY KEY(generation,person,section,position))"
                ] { try run(sql) }
                try run("PRAGMA user_version=1")
            }
            if version < 2 && target >= 2 {
                try run("ALTER TABLE operations ADD COLUMN attempts INTEGER NOT NULL DEFAULT 0")
                try run("ALTER TABLE operations ADD COLUMN retry_at REAL NOT NULL DEFAULT 0")
                try run("ALTER TABLE operations ADD COLUMN error TEXT")
                try run("PRAGMA user_version=2")
            }
            if version < 3 && target >= 3 {
                try run("CREATE INDEX IF NOT EXISTS members_person_revision ON members(person,revision)")
                try run("PRAGMA user_version=3")
            }
        }
    }
    @discardableResult func rows(_ sql: String, _ args: [String?] = []) throws -> [[String]] {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK else { throw LocalError.storage }
        defer { sqlite3_finalize(statement) }
        for (index, arg) in args.enumerated() {
            let result = arg.map { sqlite3_bind_text(statement, Int32(index + 1), $0, -1, transient) }
                ?? sqlite3_bind_null(statement, Int32(index + 1))
            guard result == SQLITE_OK else { throw LocalError.storage }
        }
        var output: [[String]] = []
        while true {
            let status = sqlite3_step(statement)
            if status == SQLITE_DONE { return output }
            guard status == SQLITE_ROW else { throw LocalError.storage }
            output.append((0..<sqlite3_column_count(statement)).map {
                sqlite3_column_text(statement, $0).map { String(cString: $0) } ?? ""
            })
        }
    }
    func run(_ sql: String, _ args: [String?] = []) throws { _ = try rows(sql, args) }
    func transaction<T>(_ work: () throws -> T) throws -> T {
        try run("BEGIN IMMEDIATE")
        do { let result = try work(); try run("COMMIT"); return result }
        catch { try? run("ROLLBACK"); throw error }
    }
    func meta(_ key: String) throws -> String? { try rows("SELECT value FROM metadata WHERE key=?", [key]).first?.first }
    func setMeta(_ key: String, _ value: String) throws {
        try run("INSERT INTO metadata VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [key, value])
    }
    private func string<T: Encodable>(_ v: T) throws -> String { String(decoding: try encode(v), as: UTF8.self) }
    func drafts() throws -> [Draft] { try rows("SELECT body FROM drafts").map { try decode(Draft.self, Data($0[0].utf8)) } }
    @discardableResult func saveDraft(_ draft: Draft) throws -> Draft {
        try transaction {
            let existing = try rows("SELECT body FROM drafts WHERE id=?", [draft.id]).first.map { try decode(Draft.self, Data($0[0].utf8)) }
            guard existing?.revision ?? 0 == draft.revision else { throw LocalError.staleDraft }
            var saved = draft; saved.revision += 1
            try run("INSERT INTO drafts VALUES(?,?) ON CONFLICT(id) DO UPDATE SET body=excluded.body", [saved.id, try string(saved)])
            return saved
        }
    }
    @discardableResult func submit(_ draft: Draft) throws -> Envelope {
        guard !draft.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { throw LocalError.invalidInput }
        var payload: [String: JSON] = ["person_id": .s(draft.person)]
        if draft.kind == "add_note" { payload["body"] = .s(draft.text) }
        else {
            payload["title"] = .s(draft.text); payload["kind"] = .s(draft.taskKind)
            payload["due_at"] = draft.dueAt.map(JSON.s) ?? .null; payload["assignee_user_id"] = .null
        }
        return try transaction {
            guard let row = try rows("SELECT body FROM drafts WHERE id=?", [draft.id]).first,
                  try decode(Draft.self, Data(row[0].utf8)).revision == draft.revision else { throw LocalError.staleDraft }
            let envelope = try insertEnvelope(kind: draft.kind, payload: .object(payload))
            try run("DELETE FROM drafts WHERE id=?", [draft.id])
            return envelope
        }
    }
    @discardableResult func complete(person: String, target: JSON) throws -> Envelope {
        try transaction { try insertEnvelope(kind: "complete_task", payload: .object(["person_id": .s(person), "target": target])) }
    }
    private func insertEnvelope(kind: String, payload: JSON) throws -> Envelope {
        let envelope = Envelope(context_id: context, operation_id: UUID().uuidString.lowercased(), kind: kind,
                                device_recorded_at: stamp(), payload: payload)
        let bytes = try string(envelope)
        guard bytes.utf8.count <= 131072 else { throw LocalError.invalidInput }
        try run("INSERT INTO operations(id,envelope) VALUES(?,?)", [envelope.operation_id, bytes])
        return envelope
    }
    func queue() throws -> [Queued] {
        try rows("SELECT envelope,status,error,receipt,overlay,attempts,retry_at FROM operations ORDER BY seq").map { row in
            let bytes = Data(row[0].utf8)
            return Queued(envelope: try decode(Envelope.self, bytes), bytes: bytes, status: row[1],
                          error: row[2].isEmpty ? nil : row[2], receipt: row[3].isEmpty ? nil : try decode(Receipt.self, Data(row[3].utf8)),
                          overlay: row[4] == "1", attempts: Int(row[5]) ?? 0, retryAt: Double(row[6]) ?? 0)
        }
    }
    func acknowledge(_ receipt: Receipt) throws {
        guard receipt.outcome == "accepted" else { throw LocalError.invalidProtocol }
        _ = try revision(receipt.person_revision)
        try transaction {
            guard let op = try queue().first(where: { $0.id == receipt.operation_id }) else { throw LocalError.invalidProtocol }
            let covers = try activeBundle(op.envelope.person).map { try revision($0.revision) >= revision(receipt.person_revision) } ?? false
            try run("UPDATE operations SET receipt=?,status='accepted',error=NULL,overlay=? WHERE id=?",
                    [try string(receipt), covers ? "0" : "1", receipt.operation_id])
        }
    }
    func failure(_ id: String, code: String, permanent: Bool, delay: Double) throws {
        try run("UPDATE operations SET status=?,error=?,attempts=attempts+1,retry_at=? WHERE id=?",
                [permanent ? "attention" : "pending", code, String(Date().timeIntervalSince1970 + delay), id])
    }
    func retryTransient() throws { try run("UPDATE operations SET retry_at=0 WHERE status='pending' AND (error IS NULL OR error<>'mobile_capacity')") }
    func activePeople() throws -> [Bundle] {
        guard let active = try meta("active") else { return [] }
        return try rows("SELECT b.body FROM members m JOIN bundles b ON b.person=m.person AND b.revision=m.revision WHERE m.generation=? ORDER BY m.person", [active])
            .map { try decode(Bundle.self, Data($0[0].utf8)) }
    }
    func activeBundle(_ person: String) throws -> Bundle? {
        guard let active = try meta("active") else { return nil }
        return try rows("SELECT b.body FROM members m JOIN bundles b ON b.person=m.person AND b.revision=m.revision WHERE m.generation=? AND m.person=?", [active, person]).first.map { try decode(Bundle.self, Data($0[0].utf8)) }
    }
    func generation() throws -> Generation? { try meta("staging").map { try decode(Generation.self, Data($0.utf8)) } }
    func begin(_ generation: Generation) throws {
        guard generation.context_id == context, generation.complete, generation.selected_count <= 25000 else { throw LocalError.invalidProtocol }
        try transaction {
            try setMeta("staging", try string(generation))
            try appendManifest(generation.generation_id, generation.manifest)
        }
    }
    func appendManifest(_ generation: String, _ manifest: Manifest) throws {
        guard manifest.items.count <= 250, manifest.complete == (manifest.next_cursor == nil) else { throw LocalError.invalidProtocol }
        for item in manifest.items {
            _ = try revision(item.revision)
            try run("INSERT INTO members VALUES(?,?,?) ON CONFLICT(generation,person) DO UPDATE SET revision=excluded.revision", [generation, item.person_id, item.revision])
        }
        try setMeta("manifest_cursor", manifest.next_cursor ?? "")
    }
    func members(_ gen: String) throws -> [(String, String)] { try rows("SELECT person,revision FROM members WHERE generation=? ORDER BY person", [gen]).map { ($0[0], $0[1]) } }
    func hasBundle(_ person: String, _ rev: String) throws -> Bool { !(try rows("SELECT 1 FROM bundles WHERE person=? AND revision=?", [person, rev])).isEmpty }
    func pages(_ gen: String, _ person: String, _ section: String) throws -> [Page] {
        try rows("SELECT body FROM pages WHERE generation=? AND person=? AND section=? ORDER BY position", [gen, person, section])
            .map { try decode(Page.self, Data($0[0].utf8)) }
    }
    func appendPage(_ page: Page, expected: String) throws {
        guard page.revision == expected, page.items.count <= 100, page.complete == (page.next_cursor == nil),
              ["summary", "notes", "tasks"].contains(page.section) else { throw LocalError.invalidProtocol }
        let body = try string(page); guard body.utf8.count <= 524288 else { throw LocalError.invalidProtocol }
        try transaction {
            let prior = try pages(page.generation_id, page.person_id, page.section)
            guard prior.last?.complete != true else { throw LocalError.invalidProtocol }
            try run("INSERT INTO pages VALUES(?,?,?,?,?)", [page.generation_id, page.person_id, page.section, String(prior.count), body])
        }
    }
    func finishBundle(_ gen: String, _ person: String, _ rev: String) throws {
        let summary = try pages(gen, person, "summary"), notes = try pages(gen, person, "notes"), tasks = try pages(gen, person, "tasks")
        guard summary.last?.complete == true, notes.last?.complete == true, tasks.last?.complete == true,
              let head = summary.first?.summary, head["id"].text == person else { throw LocalError.invalidProtocol }
        let bundle = Bundle(person: person, revision: rev, summary: head, contacts: summary.flatMap(\.items), notes: notes.flatMap(\.items), tasks: tasks.flatMap(\.items))
        try run("INSERT INTO bundles VALUES(?,?,?) ON CONFLICT(person,revision) DO NOTHING", [person, rev, try string(bundle)])
    }
    func promote(_ seal: Seal) throws {
        guard let stage = try generation(), seal.context_id == context, seal.generation_id == stage.generation_id,
              stage.selected_count == seal.selected_count else { throw LocalError.invalidProtocol }
        try transaction {
            let selected = try members(seal.generation_id)
            guard selected.count == seal.selected_count else { throw LocalError.invalidProtocol }
            for (person, rev) in selected {
                guard try hasBundle(person, rev) else { throw LocalError.invalidProtocol }
                if let old = try activeBundle(person), try revision(old.revision) > revision(rev) {
                    try run("UPDATE members SET revision=? WHERE generation=? AND person=?", [old.revision, seal.generation_id, person])
                }
            }
            try setMeta("active", seal.generation_id); try setMeta("today", try string(seal.today)); try setMeta("last_sync", seal.sealed_at)
            for op in try queue() where op.overlay && op.receipt != nil {
                if let bundle = try activeBundle(op.envelope.person), let receipt = op.receipt,
                   try revision(bundle.revision) >= revision(receipt.person_revision) {
                    try run("UPDATE operations SET overlay=0,error=NULL WHERE id=?", [op.id])
                } else if try activeBundle(op.envelope.person) == nil {
                    try run("UPDATE operations SET error='selection_removed' WHERE id=?", [op.id])
                }
            }
            try run("DELETE FROM metadata WHERE key='staging'")
            // Reclaim completed page copies only after the published pointer is durable in this transaction.
            try run("DELETE FROM pages WHERE generation=?", [seal.generation_id])
        }
    }
    func discardGeneration() throws {
        try run("DELETE FROM metadata WHERE key='staging'")
        try reclaimCache()
    }
    /// Bounded cache-only reclamation. Never touches saved drafts or operations.
    @discardableResult func reclaimCache() throws -> Bool {
        let active = try meta("active") ?? "", staging = try generation()?.generation_id ?? ""
        try transaction {
            try run("DELETE FROM pages WHERE rowid IN (SELECT rowid FROM pages WHERE generation<>? AND generation<>? LIMIT 200)", [active, staging])
            try run("DELETE FROM members WHERE rowid IN (SELECT rowid FROM members WHERE generation<>? AND generation<>? LIMIT 1000)", [active, staging])
            try run("DELETE FROM bundles WHERE rowid IN (SELECT b.rowid FROM bundles b WHERE NOT EXISTS(SELECT 1 FROM members m WHERE m.person=b.person AND m.revision=b.revision) LIMIT 50)")
        }
        return try rows("SELECT EXISTS(SELECT 1 FROM pages WHERE generation<>? AND generation<>?) OR EXISTS(SELECT 1 FROM members WHERE generation<>? AND generation<>?) OR EXISTS(SELECT 1 FROM bundles b WHERE NOT EXISTS(SELECT 1 FROM members m WHERE m.person=b.person AND m.revision=b.revision))", [active, staging, active, staging])[0][0] == "1"
    }
    func pins() throws -> [String] { try meta("pins").map { try decode([String].self, Data($0.utf8)) } ?? [] }
    func pin(_ person: String) throws {
        guard UUID(uuidString: person) != nil else { throw LocalError.invalidInput }
        var current = try pins(); if !current.contains(person) { current.append(person) }
        try setMeta("pins", try string(current))
    }
}
