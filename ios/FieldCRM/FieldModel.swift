import Foundation
import SwiftUI
import Network

@MainActor final class FieldModel: ObservableObject {
    @Published var people: [Bundle] = []
    @Published var queue: [Queued] = []
    @Published var drafts: [Draft] = []
    @Published var today: JSON = .null
    @Published var todayIsStale = false
    @Published var message = "Sign in to download your field workspace."
    @Published var lastSync = "Never"
    @Published var unlocked = false
    @Published var syncing = false
    @Published var updateRequired = false
    @Published var signingIn = false
    @Published var paused = appDefaults.bool(forKey: "pauseSync") { didSet { appDefaults.set(paused, forKey: "pauseSync") } }
    @Published var connected = true
    @Published var account = ""
    @Published var selectedPerson: String?
    private(set) var secure: SecureStorage
    private(set) var store: LocalStore?, credential: Credential?, api: API?
    private var epoch = UUID(), monitor = NWPathMonitor()
    private var reconciliationFailures = 0
    let installation: String
    let synthetic: Bool
    var pendingCount: Int { queue.filter { $0.status != "accepted" }.count }
    var pendingContactCount: Int { queue.filter { $0.isContact && $0.status != "accepted" }.count }
    var canLogContact: Bool { unlocked && (credential?.bootstrap.capabilities.contains("log_contact_attempt") ?? false) }
    #if MOBILE002_QA
    @Published var qaFixtureStage = "not requested"
    @Published var qaMigrationStage = "not inspected"
    @Published var qaConflictStage = "no pending edit"
    private var qaConflictNoteID = ""
    #endif
    #if MOBILE003_QA
    @Published var qaMobile003MigrationStage = "not inspected"
    /// Structural-only QA evidence for the isolated Mobile003 bundle. It never
    /// renders cached customer text or immutable payload values.
    func inspectMobile003Migration() {
        guard let store else { qaMobile003MigrationStage = "protected store unavailable"; return }
        do {
            let operations = try store.queue()
            let fingerprints = operations.map { $0.id.prefix(8) + ":" + String(mobile003ByteDigest($0.bytes), radix: 16) }.joined(separator: ",")
            qaMobile003MigrationStage = "schema=" + (try store.rows("PRAGMA user_version")[0][0]) + " ops=" + String(operations.count) + " drafts=" + String(drafts.count) + " ids:digest=" + fingerprints
        } catch { qaMobile003MigrationStage = "probe error: " + error.localizedDescription }
    }
    private func mobile003ByteDigest(_ bytes: Data) -> UInt64 {
        bytes.reduce(1469598103934665603) { ($0 ^ UInt64($1)) &* 1099511628211 }
    }
    #endif
    init(synthetic: Bool = false, startMonitor: Bool = true, restoreOnInit: Bool = true) {
        secure = SecureStorage(synthetic: synthetic); self.synthetic = secure.synthetic
        #if MOBILE002_QA
        // UI acceptance needs to form a real immutable envelope before the
        // second actor advances its server revision.  This is a launch-only,
        // synthetic-QA pause; customer configurations do not compile it.
        if ProcessInfo.processInfo.arguments.contains("--mobile002-qa-start-offline") { paused = true }
        #endif
        if let existing = appDefaults.string(forKey: "installation") { installation = existing }
        else { installation = UUID().uuidString.lowercased(); appDefaults.set(installation, forKey: "installation") }
        if restoreOnInit { restore() }
        if startMonitor {
            monitor.pathUpdateHandler = { [weak self] path in
                Task { @MainActor in self?.connected = path.status == .satisfied; if path.status == .satisfied { await self?.sync() } }
            }
            monitor.start(queue: DispatchQueue(label: "field.connectivity"))
        }
    }
    #if DEBUG
    private var testingAPI: API?
    private var testingDirectory: URL?
    func configureForTesting(secure: SecureStorage, api: API, directory: URL) {
        self.secure = secure; testingAPI = api; testingDirectory = directory
    }
    #endif
    private func client(base: String) throws -> API {
        #if DEBUG
        if let testingAPI { return testingAPI }
        #endif
        return try API(base: base)
    }
    private func directory() throws -> URL {
        #if DEBUG
        if let testingDirectory { return testingDirectory }
        #endif
        return try SecureStorage.directory(synthetic: synthetic)
    }
    private var qaBaseURL: String {
        #if MOBILE002_QA || MOBILE003_QA
        return "http://127.0.0.1:3102"
        #else
        return "http://127.0.0.1:3101"
        #endif
    }
    deinit { monitor.cancel() }
    func restore() {
        guard !unlocked else { return }
        do {
            guard try !secure.isLocked() else { throw LocalError.locked }
            guard let data = try secure.read("credential") else { return }
            var saved = try decode(Credential.self, data)
            guard !saved.signedOut else { message = "Sign in to reopen this account’s protected saved work."; return }
            try saved.lease.validate(.current()); try secure.write("credential", encode(saved))
            try open(saved)
        } catch { lock(error.localizedDescription, persist: false) }
    }
    private func open(_ saved: Credential) throws {
        let dir = try directory()
        let path = dir.appendingPathComponent(saved.bootstrap.identity + ".sqlite")
        let key = try secure.key(for: saved.bootstrap.identity, existingFile: FileManager.default.fileExists(atPath: path.path))
        let opened = try LocalStore(url: path, key: key, identity: saved.bootstrap.identity, context: saved.bootstrap.context_id)
        let client = try client(base: saved.baseURL); client.cookie = saved.cookie
        epoch = UUID(); credential = saved; store = opened; api = client
        account = saved.bootstrap.actor_user_id + " / " + saved.bootstrap.organization_id
        updateRequired = try opened.meta("update_required") == "1"
        unlocked = true; message = "Saved workspace is available on this device."; try reload()
    }
    func signIn(email: String, password: String) async {
        guard !signingIn else { return }; signingIn = true; let loginEpoch = epoch; defer { signingIn = false }
        do {
            #if DEBUG && targetEnvironment(simulator)
            let base = qaBaseURL
            #else
            // Independent distribution requires an approved HTTPS environment/signing configuration.
            throw LocalError.invalidProtocol
            #endif
            #if DEBUG && targetEnvironment(simulator)
            let client = try client(base: base); try await client.login(email: email, password: password)
            guard epoch == loginEpoch else { throw LocalError.identityChanged }
            let boot: Bootstrap = try await client.call("/bootstrap", method: "POST", body: .object(["protocol": .s("mobile-v1"), "installation_id": .s(installation)]))
            guard epoch == loginEpoch else { throw LocalError.identityChanged }
            guard boot.protocol == "mobile-v1", boot.installation_id == installation,
                  UUID(uuidString: boot.actor_user_id) != nil, UUID(uuidString: boot.organization_id) != nil else { throw LocalError.invalidProtocol }
            let saved = try Credential(cookie: client.cookie, baseURL: base, bootstrap: boot, lease: Lease(bootstrap: boot, clock: .current()), signedOut: false)
            try secure.write("credential", encode(saved)); try open(saved); try secure.clearLockAfterAuthorization(); reconciliationFailures = 0
            await sync(manual: true)
            #endif
        } catch { message = error.localizedDescription }
    }
    #if MOBILE002_QA
    func loadQANoteFixture() async {
        guard !syncing, unlocked, let store else { qaFixtureStage = "complete a sync before loading"; return }
        do { guard try store.activeBundle(qaFixturePersonID()) != nil else { qaFixtureStage = "no complete Person bundle"; return } }
        catch { qaFixtureStage = "bundle inspection failed"; return }
        await installQANoteFixtureIfRequested()
    }
    /// QA-only proof surface: hashes operation bytes and reports only structural
    /// migration state, never cached CRM text.
    func inspectQAMigration() {
        guard let store else { qaMigrationStage = "protected store unavailable"; return }
        do {
            let operations = try store.queue()
            let fingerprints = operations.map { $0.id.prefix(8) + ":" + String(byteDigest($0.bytes), radix: 16) }.joined(separator: ",")
            let legacyNotes = people.flatMap(\.notes).filter { $0["revision"].text.isEmpty }.count
            let qualified = people.flatMap(\.notes).filter { !$0["revision"].text.isEmpty }.count
            qaMigrationStage = "ops=" + String(operations.count) + " drafts=" + String(drafts.count) + " legacyNotes=" + String(legacyNotes) + " versionedNotes=" + String(qualified) + " ids:digest=" + fingerprints
        } catch { qaMigrationStage = "probe error: " + error.localizedDescription }
    }
    private func byteDigest(_ bytes: Data) -> UInt64 {
        bytes.reduce(1469598103934665603) { ($0 ^ UInt64($1)) &* 1099511628211 }
    }
    func advanceQAPendingEditAsSecondActor() async {
        let targetNote = qaConflictNoteID.isEmpty ? qaFixtureNoteID() : qaConflictNoteID
        guard let pending = queue.first(where: { $0.envelope.kind == "edit_note" && $0.targetID == targetNote && !["accepted", "superseded", "discarded", "unavailable"].contains($0.status) }),
              let noteID = pending.targetID else {
            qaConflictStage = "no pending note edit"; return
        }
        do {
            let other = try client(base: qaBaseURL); try await other.login(email: "second@mobile.test", password: "Mobile-demo-only-123!")
            let key = "mobile002.qa.second.installation"
            let installation = appDefaults.string(forKey: key) ?? { let value = UUID().uuidString.lowercased(); appDefaults.set(value, forKey: key); return value }()
            let boot: Bootstrap = try await other.call("/bootstrap", method: "POST", body: .object(["protocol": .s("mobile-v1"), "installation_id": .s(installation)]))
            let expected = pending.envelope.payload["expected_revision"].text
            guard !expected.isEmpty else { qaConflictStage = "pending edit has no baseline"; return }
            let replacement = Envelope(context_id: boot.context_id, operation_id: UUID().uuidString.lowercased(), kind: "edit_note", device_recorded_at: stamp(), payload: .object(["person_id": .s(pending.envelope.person), "note_id": .s(noteID), "expected_revision": .s(expected), "body": .s("second actor current version " + UUID().uuidString.lowercased())]))
            let receipt = try await other.operation(try encode(replacement), context: boot.context_id)
            qaConflictStage = "second actor accepted revision " + (receipt.committed_revision ?? "?")
        } catch { qaConflictStage = "second actor error: " + error.localizedDescription }
    }
    func createQAFreshConflictNote() async {
        guard let api, let credential, let store else { qaConflictStage = "fresh note unavailable"; return }
        let person = qaFixturePersonID(), body = "iOS QA conflict seed " + UUID().uuidString.lowercased()
        guard !person.isEmpty else { qaConflictStage = "fresh note missing Person"; return }
        do {
            let envelope = Envelope(context_id: credential.bootstrap.context_id, operation_id: UUID().uuidString.lowercased(), kind: "add_note", device_recorded_at: stamp(), payload: .object(["person_id": .s(person), "body": .s(body)]))
            let receipt = try await api.operation(try encode(envelope), context: credential.bootstrap.context_id)
            let current = try await api.currentNote(person: person, note: receipt.resource_id, context: credential.bootstrap.context_id)
            guard current.context_id == credential.bootstrap.context_id, current.person_id == person, let note = current.note else { throw LocalError.invalidProtocol }
            try store.installQANoteFixture(person: person, note: note)
            qaConflictNoteID = receipt.resource_id; try reload()
            qaConflictStage = "fresh note " + receipt.resource_id + " revision " + note["revision"].text
        } catch { qaConflictStage = "fresh note error: " + error.localizedDescription }
    }
    func prepareQAPendingConflictEdit() {
        guard let store, let person = people.first(where: { $0.person == qaFixturePersonID() }),
              let note = person.notes.first(where: { $0["id"].text == (qaConflictNoteID.isEmpty ? qaFixtureNoteID() : qaConflictNoteID) }) else {
            qaConflictStage = "fixture note unavailable"; return
        }
        if let pending = queue.first(where: { $0.envelope.kind == "edit_note" && $0.targetID == (qaConflictNoteID.isEmpty ? qaFixtureNoteID() : qaConflictNoteID) && !["accepted", "superseded", "discarded", "unavailable"].contains($0.status) }) {
            qaConflictStage = "primary edit already queued " + pending.id; return
        }
        do {
            var draft = try startEdit(person: person.person, type: "note", record: note)
            draft.text += " primary conflict proposal " + UUID().uuidString.lowercased()
            // Avoid submit()'s normal background wakeup: this QA-only setup
            // must leave actor one's immutable envelope unsent until actor two
            // has committed the deliberately intervening revision.
            draft = try save(draft); try store.submit(draft); try reload()
            guard let operation = try store.queue().first(where: { $0.envelope.kind == "edit_note" && $0.status == "pending" }) else { throw LocalError.invalidProtocol }
            qaConflictStage = "primary edit queued " + operation.id
        } catch { qaConflictStage = "primary edit error: " + error.localizedDescription }
    }
    func resumeQASync() { paused = false }
    func drainQAConflict() async {
        await sync(manual: true)
        let target = qaConflictNoteID.isEmpty ? qaFixtureNoteID() : qaConflictNoteID
        if let operation = queue.last(where: { $0.targetID == target }) {
            qaConflictStage = "target " + operation.status + " receipt " + operation.id
        } else { qaConflictStage = "target operation unavailable" }
    }
    private func qaFixturePersonID() -> String {
        let args = ProcessInfo.processInfo.arguments
        guard let index = args.firstIndex(of: "--mobile002-qa-person-id"), args.indices.contains(index + 1) else { return "" }
        return args[index + 1]
    }
    private func qaFixtureNoteID() -> String {
        let args = ProcessInfo.processInfo.arguments
        guard let index = args.firstIndex(of: "--mobile002-qa-note-id"), args.indices.contains(index + 1) else { return "" }
        return args[index + 1]
    }
    private func installQANoteFixtureIfRequested() async {
        let args = ProcessInfo.processInfo.arguments
        guard let noteIndex = args.firstIndex(of: "--mobile002-qa-note-id"), args.indices.contains(noteIndex + 1),
              let personIndex = args.firstIndex(of: "--mobile002-qa-person-id"), args.indices.contains(personIndex + 1),
              let api, let credential, let store else { qaFixtureStage = "missing launch flag or protected context"; return }
        let noteID = args[noteIndex + 1], personID = args[personIndex + 1], run = epoch
        do {
            qaFixtureStage = "fixture requested"
            guard try store.activeBundle(personID) != nil else { qaFixtureStage = "no active Person bundle"; return }
            qaFixtureStage = "active Person bundle"
            let current = try await api.currentNote(person: personID, note: noteID, context: credential.bootstrap.context_id)
            try self.current(run)
            guard current.context_id == credential.bootstrap.context_id, current.person_id == personID, let note = current.note else { qaFixtureStage = "current-note context mismatch"; return }
            qaFixtureStage = "current note " + note["id"].text
            try store.installQANoteFixture(person: personID, note: note); try reload(); qaFixtureStage = "injection committed"
            message = "QA fixture loaded from the authorized current-note response."
        } catch { qaFixtureStage = "fixture error: " + error.localizedDescription; message = error.localizedDescription }
    }
    #endif
    @discardableResult func validateAccess() -> Bool {
        do {
            guard var saved = credential, !saved.signedOut, unlocked else { throw LocalError.locked }
            try saved.lease.validate(.current()); try secure.write("credential", encode(saved)); credential = saved
            return true
        } catch { lock(error.localizedDescription); return false }
    }
    func lock(_ reason: String, persist: Bool = true) {
        epoch = UUID(); unlocked = false; store = nil; api = nil
        people = []; queue = []; drafts = []; today = .null; todayIsStale = false; selectedPerson = nil; account = ""
        if persist {
            do { try secure.setLocked() }
            catch {
                // Independent fallback: delete only the credential, never the encryption keys or CRM database.
                do { try secure.removeCredential() }
                catch { message = "Access is locked in this process, but the device could not persist the lock. Keep the device locked and retry sign-out before reopening."; return }
            }
            if var saved = credential {
                saved.signedOut = true; saved.cookie = ""; credential = saved
                // A durable marker already prevents stale credential reopening if this replacement fails.
                try? secure.write("credential", encode(saved))
            }
        }
        message = reason
    }
    func signOut() async {
        let client = api
        // Local lock is immediate and does not wait for the network. Existing server session revocation is best effort.
        lock("Signed out on this device. Pending work is protected; sign in as the same account to reopen it.")
        await client?.logout()
    }
    func protectedDataUnavailable() { lock("Unlock the device to reopen protected storage.", persist: false) }
    func reload() throws {
        guard let store else { return }
        people = try store.activePeople(); queue = try store.queue(); drafts = try store.drafts()
        today = try store.meta("today").map { try decode(JSON.self, Data($0.utf8)) } ?? .null
        todayIsStale = try store.meta("today_refresh_pending") == "1"
        lastSync = try store.meta("last_sync") ?? "Never"
    }
    func save(_ draft: Draft) throws -> Draft {
        guard validateAccess(), let store else { throw LocalError.locked }
        do { let result = try store.saveDraft(draft); try reload(); return result }
        catch { message = error.localizedDescription; throw error }
    }
    func submit(_ draft: Draft) throws {
        guard validateAccess(), let store else { throw LocalError.locked }
        do { try store.submit(draft); try reload(); message = "Saved on device. Queued for sync."; Task { await sync() } }
        catch { message = error.localizedDescription; throw error }
    }
    func newContactDraft(person: String) throws -> Draft {
        guard validateAccess(), canLogContact else { throw LocalError.invalidInput }
        let now = stamp()
        return try save(Draft(id: UUID().uuidString.lowercased(), person: person, kind: "log_contact_attempt", revision: 0,
                              contactChannel: "call", contactOutcome: "reached", occurredAt: now, deviceRecordedAt: now))
    }
    func revisedContactDraft(from operation: Queued) throws -> Draft {
        guard operation.isContact else { throw LocalError.invalidInput }
        let now = stamp()
        return try save(Draft(id: UUID().uuidString.lowercased(), person: operation.envelope.person, kind: "log_contact_attempt", revision: 0,
                              contactChannel: operation.envelope.payload["channel"].text, contactOutcome: operation.envelope.payload["outcome"].text,
                              occurredAt: operation.envelope.payload["occurred_at"].text, deviceRecordedAt: now))
    }
    func startEdit(person: String, type: String, record: JSON) throws -> Draft {
        guard validateAccess(), let store, let id = Optional(record["id"].text), !id.isEmpty,
              let expected = Optional(record["revision"].text), !expected.isEmpty, record["can_manage"].flag else { throw LocalError.invalidInput }
        guard try store.editableRecord(person: person, type: type, id: id) != nil else { throw LocalError.invalidInput }
        _ = try revision(expected)
        return try makeEditDraft(person: person, type: type, record: record)
    }
    private func makeEditDraft(person: String, type: String, record: JSON) throws -> Draft {
        let id = record["id"].text, expected = record["revision"].text
        guard !id.isEmpty, !expected.isEmpty, record["can_manage"].flag else { throw LocalError.invalidInput }
        _ = try revision(expected)
        let kind = type == "note" ? "edit_note" : "update_task"
        let text = type == "note" ? record["body"].text : record["title"].text
        let baseline: JSON = type == "note" ? .object(["body": .s(text)]) : .object(["title": .s(text), "kind": record["kind"], "due_at": record["due_at"]])
        let draft = Draft(id: UUID().uuidString.lowercased(), person: person, kind: kind, text: text, revision: 0,
                          taskKind: record["kind"].text.isEmpty ? "follow_up" : record["kind"].text,
                          dueAt: record["due_at"] == .null ? nil : record["due_at"].text,
                          targetID: id, expectedRevision: expected, baseline: baseline, proposal: baseline)
        return try save(draft)
    }
    func startEditFromCurrent(person: String, type: String, id: String) async throws -> Draft {
        guard validateAccess(), let api, let credential else { throw LocalError.locked }
        let run = epoch
        let response = type == "note" ? try await api.currentNote(person: person, note: id, context: credential.bootstrap.context_id) : try await api.currentTask(person: person, task: id, context: credential.bootstrap.context_id)
        try current(run)
        guard response.context_id == credential.bootstrap.context_id, response.person_id == person, let record = response.record, record["id"].text == id else { throw LocalError.invalidProtocol }
        return try makeEditDraft(person: person, type: type, record: record)
    }
    func requalifyEdit(_ draft: Draft) async throws -> Draft {
        guard validateAccess(), let api, let credential, let id = draft.targetID else { throw LocalError.locked }
        let run = epoch, response: CurrentRecordResponse
        if draft.kind == "edit_note" { response = try await api.currentNote(person: draft.person, note: id, context: credential.bootstrap.context_id) }
        else { response = try await api.currentTask(person: draft.person, task: id, context: credential.bootstrap.context_id) }
        try current(run)
        guard response.context_id == credential.bootstrap.context_id, response.person_id == draft.person,
              let record = response.record, record["id"].text == id, record["can_manage"].flag,
              !record["revision"].text.isEmpty else { throw LocalError.invalidProtocol }
        guard let store else { throw LocalError.locked }
        let saved = try store.saveCurrent(draft.id, current: record, contextID: response.context_id, person: draft.person, editorEpoch: draft.editorEpoch)
        try reload(); return saved
    }
    func resolveUsingCurrent(_ draft: Draft) throws {
        guard let store else { throw LocalError.locked }
        if let predecessor = draft.predecessor { try store.markSuperseded(predecessor) }
        try store.discardDraft(draft.id); try reload(); message = "Your saved proposal was discarded. The current version remains available."
    }
    func revisedDraft(_ draft: Draft) throws -> Draft {
        guard let current = draft.current, let id = draft.targetID, let store else { throw LocalError.invalidProtocol }
        guard !current["revision"].text.isEmpty else { throw LocalError.invalidProtocol }
        if let predecessor = draft.predecessor { try store.markSuperseded(predecessor) }
        var next = Draft(id: UUID().uuidString.lowercased(), person: draft.person, kind: draft.kind, text: draft.text, revision: 0,
                         taskKind: draft.kind == "update_task" ? draft.taskKind : "follow_up", dueAt: draft.kind == "update_task" ? draft.dueAt : nil,
                         targetID: id, expectedRevision: current["revision"].text, baseline: draft.kind == "edit_note" ? .object(["body": current["body"]]) : .object(["title": current["title"], "kind": current["kind"], "due_at": current["due_at"]]), proposal: draft.proposal, mode: "editing", predecessor: draft.predecessor)
        next = try store.saveDraft(next); try reload(); return next
    }
    func complete(person: String, target: JSON) {
        guard validateAccess(), let store else { return }
        do { try store.complete(person: person, target: target); try reload(); message = "Completion saved on device."; Task { await sync() } }
        catch { message = error.localizedDescription }
    }
    func pin(_ person: String) {
        guard validateAccess(), let store else { return }
        do { try store.pin(person); message = "Saved for the next download."; Task { await sync(manual: true) } }
        catch { message = error.localizedDescription }
    }
    func sync(manual: Bool = false) async {
        guard !syncing, !paused, !updateRequired, connected, unlocked, validateAccess(), let store, let api, let credential else { return }
        syncing = true; let run = epoch; defer { syncing = false }
        do {
            if manual { try store.retryTransient(); reconciliationFailures = 0 }
            for op in try store.queue() where op.status == "pending" && op.retryAt <= Date().timeIntervalSince1970 {
                try current(run)
                guard !paused else { message = "Sync paused. Saved work remains on device."; return }
                if op.isContact && !credential.bootstrap.capabilities.contains("log_contact_attempt") {
                    // Capability loss must not delete or rewrite protected work.
                    // Keep the exact envelope pending until an authorized app can submit it.
                    message = "Contact logging is unavailable for this account. Saved contact work remains protected."
                    continue
                }
                do {
                    let receipt = try await api.operation(op.bytes, context: credential.bootstrap.context_id)
                    try current(run); try store.acknowledge(receipt); try reload()
                } catch let error as APIError {
                    try current(run)
                    if error.status == 401 || error.code == "workspace_in_migration_review" { lock(error.localizedDescription); return }
                    if error.code == "protocol_unsupported" { try requireUpdate(store); throw error }
                    if error.status == 403 {
                        do { try await api.verifyAuthority(credential.bootstrap); try current(run) }
                        catch { lock("Online authorization is required before reopening saved work."); return }
                    }
                    if error.code == "revision_conflict" {
                        var currentRecord: JSON? = nil
                        if let draft = try store.draftForOperation(op.id), let target = draft.targetID {
                            do {
                                let currentResponse = draft.kind == "edit_note" ? try await api.currentNote(person: draft.person, note: target, context: credential.bootstrap.context_id) : try await api.currentTask(person: draft.person, task: target, context: credential.bootstrap.context_id)
                                try current(run)
                                if currentResponse.context_id == credential.bootstrap.context_id, currentResponse.person_id == draft.person { currentRecord = currentResponse.record }
                            } catch { /* Preserve original proposal and baseline while offline/unavailable. */ }
                        }
                        try store.recordConflict(op.id, current: currentRecord, contextID: credential.bootstrap.context_id, person: op.envelope.person)
                        try reload(); continue
                    }
                    let transient = error.status == 429 || error.status >= 500 || error.code == "dependency_pending"
                    try store.failure(op.id, code: error.code, permanent: !transient, delay: error.status == 429 ? 30 : backoff(op.attempts))
                    if transient { throw error }
                } catch {
                    try current(run); try store.failure(op.id, code: "connection_interrupted", permanent: false, delay: backoff(op.attempts)); throw error
                }
            }
            try current(run)
            if let until = try store.meta("download_retry_after"), let deadline = Double(until), deadline > Date().timeIntervalSince1970 {
                message = "Server is busy. Download will retry after the required pause."; try reload(); return
            }
            guard reconciliationFailures < 3 else { message = "Download changed repeatedly. Your saved workspace is retained. Tap Sync to try again."; try reload(); return }
            try await reconcile(store, api, credential.bootstrap, run)
            try current(run); try await drainCache(store, run); try reload(); message = pendingCount == 0 ? "Synced. Complete downloaded workspace is ready offline." : "Download complete. Some saved actions need attention."
            reconciliationFailures = 0
        } catch let error as APIError {
            if epoch != run { return }
            if error.status == 401 || error.status == 403 || error.code == "workspace_in_migration_review" { lock(error.localizedDescription); return }
            if error.status == 429 { try? store.setMeta("download_retry_after", String(Date().timeIntervalSince1970 + 30)) }
            if error.status == 404 || ["generation_changed", "generation_expired"].contains(error.code) { reconciliationFailures += 1; try? store.discardGeneration() }
            if error.code == "protocol_unsupported" { try? requireUpdate(store) }
            message = error.localizedDescription; try? reload()
        } catch { if epoch == run { message = error.localizedDescription; try? reload() } }
    }
    private func requireUpdate(_ store: LocalStore) throws {
        updateRequired = true
        try store.setMeta("update_required", "1")
    }
    private func backoff(_ attempts: Int) -> Double { min(300, pow(2, Double(min(attempts + 1, 8)))) + Double.random(in: 0...2) }
    private func current(_ run: UUID) throws {
        guard run == epoch, unlocked else { throw LocalError.identityChanged }
        guard validateAccess() else { throw LocalError.locked }
    }
    private func drainCache(_ store: LocalStore, _ run: UUID) async throws {
        while try store.reclaimCache() { try current(run); await Task.yield() }
    }
    private func reconcile(_ store: LocalStore, _ api: API, _ boot: Bootstrap, _ run: UUID) async throws {
        // Finish the bounded cleanup backlog before admitting another generation.
        // Every batch yields; active/staged bundles and saved work remain intact.
        try await drainCache(store, run)
        // A contact receipt affects Today independently of Person revision. A pre-receipt
        // staging generation cannot be sealed as evidence of that fact.
        if try store.meta("today_refresh_pending") == "1" { try store.discardGeneration() }
        var generation = try store.generation()
        if let old = generation, try date(old.expires_at) <= date(boot.server_time).addingTimeInterval(continuousSeconds() - (credential?.lease.anchor.uptime ?? 0)) {
            try store.discardGeneration(); generation = nil
        }
        if generation == nil {
            let fresh: Generation = try await api.call("/reconciliations", method: "POST", body: .object(["protocol": .s("mobile-v1"), "installation_id": .s(installation), "pinned_person_ids": .array(try store.pins().map(JSON.s))]), context: boot.context_id)
            try current(run); try store.begin(fresh); generation = fresh
        }
        guard let gen = generation else { throw LocalError.invalidProtocol }
        while let cursor = try store.meta("manifest_cursor"), !cursor.isEmpty {
            let manifest: Manifest = try await api.call("/reconciliations/\(gen.generation_id)/manifest?cursor=\(API.cursor(cursor))", context: boot.context_id)
            try current(run); try store.transaction { try store.appendManifest(gen.generation_id, manifest) }
        }
        let members = try store.members(gen.generation_id)
        guard members.count == gen.selected_count else { throw LocalError.invalidProtocol }
        for (index, (person, rev)) in members.enumerated() {
            try current(run)
            message = "Downloading \(index + 1) of \(members.count) people. Previous workspace remains available."
            if try store.hasBundle(person, rev) { continue }
            for section in ["summary", "notes", "tasks"] {
                while true {
                    let prior = try store.pages(gen.generation_id, person, section)
                    if prior.last?.complete == true { break }
                    let suffix = prior.last?.next_cursor.map { "?cursor=" + API.cursor($0) } ?? ""
                    let page: Page = try await api.call("/reconciliations/\(gen.generation_id)/people/\(person)/\(section)\(suffix)", context: boot.context_id)
                    try current(run)
                    guard page.generation_id == gen.generation_id, page.person_id == person, page.section == section else { throw LocalError.invalidProtocol }
                    try store.appendPage(page, expected: rev)
                }
            }
            try store.finishBundle(gen.generation_id, person, rev)
        }
        let seal: Seal = try await api.call("/reconciliations/\(gen.generation_id)/seal", method: "POST", body: .object([:]), context: boot.context_id)
        try current(run); try store.promote(seal)
    }
}
