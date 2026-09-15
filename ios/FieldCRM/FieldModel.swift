import Foundation
import SwiftUI
import Network

struct StageProposal: Identifiable {
    let id = UUID()
    let person: String, baseline: Stage, expected: String, superseding: String?
    /// A saved follow-up remains mutable input until the agent explicitly
    /// submits it after the earlier operation has resolved.
    let draftID: String?, draftRevision: Int?
    var selectedID: String
}

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
    private var stageCapabilitiesReady: Bool {
        guard let capabilities = credential?.bootstrap.capabilities else { return false }
        return ["change_person_stage", "stage_revisions", "stage_catalog"].allSatisfy(capabilities.contains)
    }
    private var detailsCapabilitiesReady: Bool {
        guard let capabilities = credential?.bootstrap.capabilities else { return false }
        return ["update_person_details", "details_revisions"].allSatisfy(capabilities.contains)
    }
    private var metadataCapabilitiesReady: Bool {
        guard let capabilities = credential?.bootstrap.capabilities else { return false }
        return ["update_person_metadata", "metadata_revisions", "metadata_catalog"].allSatisfy(capabilities.contains)
    }
    func canEditMetadata(person: String) -> Bool {
        guard unlocked, metadataCapabilitiesReady, let store else { return false }
        return (try? store.editableMetadata(person: person)) != nil
    }
    func displayedMetadata(person: String) -> JSON? {
        guard let store else { return nil }
        return try? store.metadataBaseline(person: person)
    }
    func canEditDetails(person: String) -> Bool {
        guard unlocked, detailsCapabilitiesReady, let store else { return false }
        return (try? store.editableDetails(person: person)) != nil
    }
    var canChangeStage: Bool {
        guard unlocked, stageCapabilitiesReady, let store else { return false }
        return (try? store.activeStages().isEmpty == false) ?? false
    }
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
            let draftFingerprints = try store.drafts().map { $0.id.prefix(8) + ":" + String(mobile003ByteDigest(try encode($0)), radix: 16) }.joined(separator: ",")
            let accepted = operations.filter { $0.receipt != nil }.count
            let active = try store.meta("active") ?? "none"
            qaMobile003MigrationStage = "schema=" + (try store.rows("PRAGMA user_version")[0][0]) + " active=" + active.prefix(8) + " ops=" + String(operations.count) + " receipts=" + String(accepted) + " drafts=" + String(drafts.count) + " ids:digest=" + fingerprints + " drafts:digest=" + draftFingerprints
        } catch { qaMobile003MigrationStage = "probe error: " + error.localizedDescription }
    }
    private func mobile003ByteDigest(_ bytes: Data) -> UInt64 {
        bytes.reduce(1469598103934665603) { ($0 ^ UInt64($1)) &* 1099511628211 }
    }
    func mobile003QAInventoryURL() throws -> URL { try directory().appendingPathComponent("historical-upgrade-inventory.txt") }
    #endif
    #if MOBILE005_QA
    @Published var qaProfileConflictStage = "no pending profile change"
    #endif
    #if MOBILE006_QA
    @Published var qaMetadataConflictStage = "no pending metadata conflict"
    /// Creates an entirely local conflict review from the sealed synthetic
    /// workspace. It is intentionally compiled only into the Mobile006 QA app:
    /// no second server mutation is needed to verify that the UI renders the
    /// three protected values and saves a revised proposal.
    func prepareQAMetadataConflictReview() {
        guard let store,
              let person = try? store.activePeople().first?.person,
              let baseline = try? store.editableMetadata(person: person) else { qaMetadataConflictStage = "sealed metadata unavailable"; return }
        let selected = Set(baseline["tags"].list.map { $0["id"].text })
        guard let candidate = baseline["catalog_tags"].list.first(where: { !selected.contains($0["id"].text) }) ?? baseline["catalog_tags"].list.first else {
            qaMetadataConflictStage = "catalog has no tag"; return
        }
        let adding = !selected.contains(candidate["id"].text)
        let action: JSON = adding ? .object(["kind": .s("add_tag"), "tag_id": .s(candidate["id"].text)]) : .object(["kind": .s("remove_tag"), "tag_id": .s(candidate["id"].text)])
        do {
            let draft = try store.saveDraft(Draft(id: UUID().uuidString.lowercased(), person: person, kind: "update_person_metadata", revision: 0,
                                                  expectedRevision: baseline["metadata_revision"].text, expectedCatalogRevision: baseline["catalog_revision"].text,
                                                  baseline: baseline, proposal: .object(["actions": .array([action])])) )
            let envelope = try store.submit(draft)
            let current = CurrentMetadataResponse(context_id: store.context, person_id: person, person_revision: baseline["person_revision"].text,
                                                  metadata_revision: baseline["metadata_revision"].text, catalog_revision: baseline["catalog_revision"].text,
                                                  tags: baseline["tags"].list, values: baseline["values"].list, complete: true)
            try store.recordMetadataConflict(envelope.operation_id, current: current, code: "revision_conflict", contextID: store.context, person: person)
            try reload()
            qaMetadataConflictStage = "local metadata conflict ready"
        } catch { qaMetadataConflictStage = "metadata conflict error: " + error.localizedDescription }
    }
    #endif
    #if MOBILE006_UPGRADE_QA
    @Published var qaMobile006UpgradeStage = "not inspected"
    /// Structural-only evidence for the in-place Mobile005→006 install.  It
    /// deliberately exposes counts and schema rather than protected content.
    func inspectMobile006Upgrade() {
        guard let store else { qaMobile006UpgradeStage = "protected store unavailable"; return }
        do {
            let queue = try store.queue(), drafts = try store.drafts(), people = try store.activePeople()
            let receipts = queue.filter { $0.receipt != nil }.count
            qaMobile006UpgradeStage = "schema=" + (try store.rows("PRAGMA user_version")[0][0]) + " people=" + String(people.count) + " ops=" + String(queue.count) + " receipts=" + String(receipts) + " drafts=" + String(drafts.count)
        } catch { qaMobile006UpgradeStage = "probe error: " + error.localizedDescription }
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
        #if MOBILE006_QA || MOBILE006_UPGRADE_QA
        return "http://127.0.0.1:3106"
        #elseif MOBILE005_QA || MOBILE005_UPGRADE_QA
        return "http://127.0.0.1:3103"
        #elseif MOBILE002_QA || MOBILE003_QA || MOBILE004_QA || MOBILE004_UPGRADE_QA
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
    #if MOBILE005_QA
    /// Synthetic-simulator-only harness for the real profile conflict path.
    /// It creates the same immutable outbox envelope as the editor, then a
    /// distinct authorized actor advances the server revision before sync.
    func prepareQAProfileConflict() {
        guard let store, let person = people.first(where: { $0.person == "07cb08d0-56c3-43c1-a538-68573e5a24f0" }) else { qaProfileConflictStage = "profile unavailable"; return }
        if let pending = queue.first(where: { $0.isDetails && !["accepted", "superseded", "discarded", "unavailable"].contains($0.status) }) { qaProfileConflictStage = "primary profile already queued " + pending.id; return }
        do {
            // A failed/restarted synthetic QA run may leave only its old
            // conflict record.  Supersede that completed test fixture before
            // creating a fresh, independently reviewable conflict.
            for old in try store.queue() where old.isDetails && old.status == "conflict" { try store.markSuperseded(old.id) }
            var draft = try newDetailsDraft(person: person.person)
            draft.proposal = .object(["last_name": .s("iOS primary profile conflict " + UUID().uuidString.lowercased())])
            draft = try save(draft); try store.submit(draft); try reload()
            guard let operation = try store.queue().first(where: { $0.isDetails && $0.status == "pending" }) else { throw LocalError.invalidProtocol }
            qaProfileConflictStage = "primary profile queued " + operation.id
        } catch { qaProfileConflictStage = "primary profile error: " + error.localizedDescription }
    }
    func advanceQAProfileConflictAsSecondActor() async {
        guard let pending = queue.first(where: { $0.isDetails && $0.status == "pending" }) else { qaProfileConflictStage = "no pending profile change"; return }
        do {
            let other = try client(base: qaBaseURL); try await other.login(email: "second@mobile.test", password: "Mobile-demo-only-123!")
            let key = "mobile005.qa.second.profile.installation"
            let installation = appDefaults.string(forKey: key) ?? { let value = UUID().uuidString.lowercased(); appDefaults.set(value, forKey: key); return value }()
            let boot: Bootstrap = try await other.call("/bootstrap", method: "POST", body: .object(["protocol": .s("mobile-v1"), "installation_id": .s(installation)]))
            let current = try await other.currentDetails(person: pending.envelope.person, context: boot.context_id)
            let replacement = Envelope(context_id: boot.context_id, operation_id: UUID().uuidString.lowercased(), kind: "update_person_details", device_recorded_at: stamp(), payload: .object(["person_id": .s(pending.envelope.person), "expected_details_revision": .s(current.details_revision), "last_name": .s("iOS second actor profile " + UUID().uuidString.lowercased()), "contact_operations": .array([])]))
            let receipt = try await other.operation(try encode(replacement), context: boot.context_id)
            qaProfileConflictStage = "second actor accepted profile " + (receipt.committed_revision ?? "?")
        } catch { qaProfileConflictStage = "second actor profile error: " + error.localizedDescription }
    }
    func drainQAProfileConflict() async {
        await sync(manual: true)
        if let operation = queue.last(where: { $0.isDetails }) { qaProfileConflictStage = "profile " + operation.status + " " + operation.id }
        else { qaProfileConflictStage = "profile operation unavailable" }
    }
    func inspectQAProfileOutbox() {
        guard let operation = queue.last(where: { $0.isDetails }) else { qaProfileConflictStage = "profile outbox unavailable"; return }
        let digest = operation.bytes.reduce(1469598103934665603) { ($0 ^ UInt64($1)) &* 1099511628211 }
        qaProfileConflictStage = "profile outbox " + operation.id + " " + String(digest, radix: 16)
    }
    func inspectQAProfileCache() {
        guard let store else { qaProfileConflictStage = "profile cache unavailable"; return }
        do {
            let person = "07cb08d0-56c3-43c1-a538-68573e5a24f0"
            let bundle = try store.activeBundle(person)
            let qualified = try bundle.map { try store.hasQualifiedDetailsBundle(person, $0.revision) } ?? false
            qaProfileConflictStage = "profile cache rev=" + (bundle?.revision ?? "none") + " qualified=" + (qualified ? "yes" : "no") + " editable=" + ((try store.editableDetails(person: person)) == nil ? "no" : "yes") + " caps=" + (detailsCapabilitiesReady ? "yes" : "no")
        } catch { qaProfileConflictStage = "profile cache error: " + error.localizedDescription }
    }
    func verifyQAProfileReceipt() async {
        guard let api, let credential, let operation = queue.last(where: { $0.isDetails }), operation.status == "accepted",
              let receipt = operation.receipt, receipt.resource_type == "person_details", receipt.outcome == "accepted" else { qaProfileConflictStage = "profile receipt unavailable"; return }
        do {
            let operations = operation.envelope.payload["contact_operations"].list
            let additions = operations.enumerated().filter { $0.element["op"].text == "add" }
            guard let mapping = receipt.added_contact_ids, mapping.count == additions.count,
                  Set(mapping.map(\.ordinal)).count == mapping.count else { throw LocalError.invalidProtocol }
            var cursor: String? = nil, contacts: [JSON] = [], first: CurrentDetailsResponse?
            repeat {
                let page = try await api.currentDetails(person: operation.envelope.person, cursor: cursor, context: credential.bootstrap.context_id)
                guard page.context_id == credential.bootstrap.context_id, page.person_id == operation.envelope.person,
                      page.complete == (page.next_cursor == nil), (try? revision(page.details_revision)) != nil else { throw LocalError.invalidProtocol }
                if let first { guard first.details_revision == page.details_revision && first.person_revision == page.person_revision else { throw LocalError.invalidProtocol } }
                else { first = page }
                contacts += page.items; cursor = page.next_cursor
            } while cursor != nil
            for entry in operations where entry["op"].text == "edit" {
                guard contacts.contains(where: { $0["id"].text == entry["id"].text && $0["value"].text == entry["value"].text }) else { throw LocalError.invalidProtocol }
            }
            for entry in mapping {
                guard let source = additions.first(where: { $0.offset == entry.ordinal })?.element,
                      contacts.contains(where: { $0["id"].text == entry.id && $0["value"].text == source["value"].text }) else { throw LocalError.invalidProtocol }
            }
            qaProfileConflictStage = "profile receipt verified " + operation.id
        } catch { qaProfileConflictStage = "profile receipt error: " + error.localizedDescription }
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
    func newDetailsDraft(person: String) throws -> Draft {
        guard validateAccess(), detailsCapabilitiesReady, let store, let baseline = try store.editableDetails(person: person) else { throw LocalError.invalidInput }
        let expected = baseline["details_revision"].text
        guard (try? revision(expected)) != nil else { throw LocalError.invalidProtocol }
        return try save(Draft(id: UUID().uuidString.lowercased(), person: person, kind: "update_person_details", revision: 0,
                              expectedRevision: expected, baseline: baseline,
                              proposal: .object([:])))
    }
    func newMetadataDraft(person: String) throws -> Draft {
        guard validateAccess(), metadataCapabilitiesReady, let store, let baseline = try store.editableMetadata(person: person) else { throw LocalError.invalidInput }
        let metadata = baseline["metadata_revision"].text, catalog = baseline["catalog_revision"].text
        guard (try? revision(metadata)) != nil, (try? revision(catalog)) != nil else { throw LocalError.invalidProtocol }
        return try save(Draft(id: UUID().uuidString.lowercased(), person: person, kind: "update_person_metadata", revision: 0,
                              expectedRevision: metadata, expectedCatalogRevision: catalog, baseline: baseline, proposal: .object(["actions": .array([])])))
    }
    func revisedMetadataDraft(_ draft: Draft) throws -> Draft {
        guard draft.isMetadata, let current = draft.current, let store else { throw LocalError.invalidProtocol }
        let metadata = current["metadata_revision"].text, catalog = current["catalog_revision"].text
        guard (try? revision(metadata)) != nil, (try? revision(catalog)) != nil else { throw LocalError.invalidProtocol }
        // `/current` deliberately contains values and tokens only. It can be
        // used for a revision only with a *fresh sealed* workspace catalog that
        // proves it describes the same catalog revision. Never carry catalog
        // rows forward from the conflicted proposal.
        guard let qualified = try store.editableMetadata(person: draft.person),
              qualified["catalog_revision"].text == catalog,
              case .object(let currentValues) = current else { throw LocalError.invalidProtocol }
        var merged = currentValues
        for key in ["catalog_tags", "fields", "options"] { merged[key] = qualified[key] }
        var next = Draft(id: UUID().uuidString.lowercased(), person: draft.person, kind: "update_person_metadata", revision: 0,
                         expectedRevision: metadata, expectedCatalogRevision: catalog, baseline: .object(merged), proposal: draft.proposal, mode: "editing", predecessor: draft.predecessor)
        // All validation happens before the transaction that saves the
        // replacement and supersedes the old conflict.
        try store.validateMetadataProposal(next)
        next = try store.saveMetadataReplacement(next, superseding: draft.predecessor); try reload(); return next
    }
    func requalifyMetadata(_ draft: Draft) async throws -> Draft {
        guard draft.isMetadata, validateAccess(), let api, let credential, let store else { throw LocalError.locked }
        let run = epoch, response = try await api.currentMetadata(person: draft.person, context: credential.bootstrap.context_id)
        try current(run)
        guard response.context_id == credential.bootstrap.context_id, response.person_id == draft.person else { throw LocalError.invalidProtocol }
        guard let qualified = try store.editableMetadata(person: draft.person), qualified["catalog_revision"].text == response.catalog_revision else { throw LocalError.invalidProtocol }
        let current: JSON = .object(["person_revision": .s(response.person_revision), "metadata_revision": .s(response.metadata_revision), "catalog_revision": .s(response.catalog_revision), "tags": .array(response.tags), "values": .array(response.values), "catalog_tags": qualified["catalog_tags"], "fields": qualified["fields"], "options": qualified["options"]])
        let saved = try store.saveCurrent(draft.id, current: current, contextID: response.context_id, person: draft.person, editorEpoch: draft.editorEpoch)
        try reload(); return saved
    }
    func startDetails(person: String) throws -> Draft { try newDetailsDraft(person: person) }
    private func completeCurrentDetails(person: String) async throws -> CurrentDetailsResponse {
        guard validateAccess(), let api, let credential else { throw LocalError.locked }
        let run = epoch
        var cursor: String? = nil, all: [JSON] = [], first: CurrentDetailsResponse?
        var visited = Set<String>(), contacts = Set<UUID>()
        repeat {
            let page = try await api.currentDetails(person: person, cursor: cursor, context: credential.bootstrap.context_id)
            try current(run)
            guard page.context_id == credential.bootstrap.context_id, page.person_id == person,
                  (try? revision(page.details_revision)) != nil,
                  (try? revision(page.person_revision)) != nil else { throw LocalError.invalidProtocol }
            if let first {
                guard first.details_revision == page.details_revision,
                      first.first_name == page.first_name, first.last_name == page.last_name else { throw LocalError.invalidProtocol }
            }
            else { first = page }
            for item in page.items {
                guard let id = UUID(uuidString: item["id"].text), contacts.insert(id).inserted else { throw LocalError.invalidProtocol }
            }
            all += page.items
            if let next = page.next_cursor {
                guard !next.isEmpty, visited.insert(next).inserted else { throw LocalError.invalidProtocol }
            }
            cursor = page.next_cursor
            if page.complete {
                return CurrentDetailsResponse(context_id: page.context_id, person_id: page.person_id,
                    person_revision: page.person_revision, details_revision: page.details_revision,
                    first_name: page.first_name, last_name: page.last_name, items: all, next_cursor: nil, complete: true)
            }
        } while true
    }
    func requalifyDetails(_ draft: Draft) async throws -> Draft {
        guard draft.isDetails else { throw LocalError.invalidProtocol }
        let response = try await completeCurrentDetails(person: draft.person)
        guard let store else { throw LocalError.locked }
        let currentJSON: JSON = .object(["first_name": response.first_name.map(JSON.s) ?? .null, "last_name": response.last_name.map(JSON.s) ?? .null,
                                         "details_revision": .s(response.details_revision), "contacts": .array(response.items)])
        let saved = try store.saveCurrent(draft.id, current: currentJSON, contextID: response.context_id, person: draft.person, editorEpoch: draft.editorEpoch)
        try reload(); return saved
    }
    func revisedDetailsDraft(_ draft: Draft) throws -> Draft {
        guard draft.isDetails, let current = draft.current, let store else { throw LocalError.invalidProtocol }
        let expected = current["details_revision"].text
        guard (try? revision(expected)) != nil else { throw LocalError.invalidProtocol }
        if let predecessor = draft.predecessor { try store.markSuperseded(predecessor) }
        var next = Draft(id: UUID().uuidString.lowercased(), person: draft.person, kind: "update_person_details", revision: 0,
                         expectedRevision: expected, baseline: current, proposal: draft.proposal, mode: "editing", predecessor: draft.predecessor)
        next = try store.saveDraft(next); try reload(); return next
    }
    func revisedContactDraft(from operation: Queued) throws -> Draft {
        guard operation.isContact else { throw LocalError.invalidInput }
        let now = stamp()
        return try save(Draft(id: UUID().uuidString.lowercased(), person: operation.envelope.person, kind: "log_contact_attempt", revision: 0,
                              contactChannel: operation.envelope.payload["channel"].text, contactOutcome: operation.envelope.payload["outcome"].text,
                              occurredAt: operation.envelope.payload["occurred_at"].text, deviceRecordedAt: now))
    }
    func stageBaseline(person: String) throws -> (Stage, String) {
        guard validateAccess(), stageCapabilitiesReady, let store, let baseline = try store.editableStage(person: person) else { throw LocalError.invalidInput }
        return baseline
    }
    func availableStages() -> [Stage] { (try? store?.activeStages()) ?? [] }
    func displayedStageName(_ bundle: Bundle) -> String {
        let id = bundle.summary["stage"]["id"].text
        return (try? store?.activeStages().first(where: { $0.id == id })?.name) ?? bundle.summary["stage"]["name"].text
    }
    func queueStage(person: String, baseline: Stage, expected: String, proposal: Stage, superseding: String? = nil, draftID: String? = nil, draftRevision: Int? = nil) throws {
        guard validateAccess(), stageCapabilitiesReady, let store else { throw LocalError.locked }
        guard try store.activeStages().contains(where: { $0.id == proposal.id }) else { throw LocalError.invalidInput }
        let saved = try store.queueStage(person: person, baseline: baseline, stage: proposal, expected: expected, superseding: superseding, draftID: draftID, draftRevision: draftRevision)
        try reload()
        if saved.mode == "follow_up" {
            message = "Stage follow-up saved on device. It is waiting for the previous stage change and will not submit automatically."
        } else {
            message = "Stage proposal saved on device. The server stage and Today remain unchanged until it is accepted."
            Task { await sync() }
        }
    }
    func newStageProposal(person: String) throws -> StageProposal {
        let baseline = try stageBaseline(person: person)
        return StageProposal(person: person, baseline: baseline.0, expected: baseline.1, superseding: nil, draftID: nil, draftRevision: nil, selectedID: baseline.0.id)
    }
    func revisedStageProposal(_ draft: Draft) throws -> StageProposal {
        guard draft.kind == "change_person_stage" else { throw LocalError.invalidProtocol }
        if draft.mode == "conflict" {
            guard let current = draft.current,
                  let expected = Optional(current["stage_revision"].text), (try? revision(expected)) != nil,
                  let id = Optional(current["id"].text), !id.isEmpty, let name = Optional(current["name"].text), !name.isEmpty else { throw LocalError.invalidProtocol }
            return StageProposal(person: draft.person, baseline: Stage(id: id, name: name, position: 0), expected: expected,
                                 superseding: draft.predecessor, draftID: nil, draftRevision: nil, selectedID: draft.targetID ?? id)
        }
        guard draft.mode == "follow_up", let baseline = draft.baseline,
              let expected = Optional(baseline["stage_revision"].text), (try? revision(expected)) != nil,
              let id = Optional(baseline["id"].text), !id.isEmpty, let name = Optional(baseline["name"].text), !name.isEmpty else { throw LocalError.invalidProtocol }
        return StageProposal(person: draft.person, baseline: Stage(id: id, name: name, position: 0), expected: expected,
                             superseding: nil, draftID: draft.id, draftRevision: draft.revision, selectedID: draft.targetID ?? id)
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
                if op.isStage && !stageCapabilitiesReady {
                    message = "Stage changes are unavailable for this account. Saved stage work remains protected."
                    continue
                }
                if op.isDetails && !detailsCapabilitiesReady {
                    message = "Profile editing is unavailable for this account. Saved profile work remains protected."
                    continue
                }
                if op.isMetadata && !metadataCapabilitiesReady {
                    message = "Tag and custom-field editing is unavailable for this account. Saved metadata work remains protected."
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
                    if error.code == "revision_conflict" || (op.isMetadata && error.code == "catalog_revision_conflict") {
                        var currentRecord: JSON? = nil
                        if op.isMetadata {
                            var currentMetadata: CurrentMetadataResponse? = nil
                            if let draft = try store.draftForOperation(op.id) {
                                do { currentMetadata = try await api.currentMetadata(person: draft.person, context: credential.bootstrap.context_id); try current(run) }
                                catch { /* A proposal remains usable for review when a current read is unavailable. */ }
                            }
                            try store.recordMetadataConflict(op.id, current: currentMetadata, code: error.code, contextID: credential.bootstrap.context_id, person: op.envelope.person)
                            try reload(); continue
                        }
                        if op.isStage {
                            var currentStage: CurrentStageResponse? = nil
                            if let draft = try store.draftForOperation(op.id) {
                                do { currentStage = try await api.currentStage(person: draft.person, context: credential.bootstrap.context_id); try current(run) }
                                catch { /* Preserve the stage proposal if a conflict baseline is unavailable. */ }
                            }
                            try store.recordStageConflict(op.id, current: currentStage, contextID: credential.bootstrap.context_id, person: op.envelope.person)
                            try reload(); continue
                        }
                        if op.isDetails {
                            var details: CurrentDetailsResponse? = nil
                            if let draft = try store.draftForOperation(op.id) {
                                do {
                                    details = try await completeCurrentDetails(person: draft.person)
                                } catch { /* Keep the protected proposal when the bounded current read is unavailable. */ }
                            }
                            try current(run)
                            let currentJSON: JSON? = details.map { JSON.object(["first_name": $0.first_name.map(JSON.s) ?? JSON.null, "last_name": $0.last_name.map(JSON.s) ?? JSON.null, "details_revision": JSON.s($0.details_revision), "contacts": JSON.array($0.items)]) }
                            try store.recordConflict(op.id, current: currentJSON, contextID: credential.bootstrap.context_id, person: op.envelope.person)
                            try reload(); continue
                        }
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
            var request: [String: JSON] = ["protocol": .s("mobile-v1"), "installation_id": .s(installation), "pinned_person_ids": .array(try store.pins().map(JSON.s))]
            if stageCapabilitiesReady { request["include_stage_catalog"] = .bool(true) }
            if metadataCapabilitiesReady { request["include_metadata"] = .bool(true) }
            let fresh: Generation = try await api.call("/reconciliations", method: "POST", body: .object(request), context: boot.context_id)
            if stageCapabilitiesReady && fresh.stage_catalog == nil { throw LocalError.invalidProtocol }
            if metadataCapabilitiesReady && fresh.metadata == nil { throw LocalError.invalidProtocol }
            try current(run); try store.begin(fresh); generation = fresh
        }
        guard let gen = generation else { throw LocalError.invalidProtocol }
        while let cursor = try store.meta("manifest_cursor"), !cursor.isEmpty {
            let manifest: Manifest = try await api.call("/reconciliations/\(gen.generation_id)/manifest?cursor=\(API.cursor(cursor))", context: boot.context_id)
            try current(run); try store.transaction { try store.appendManifest(gen.generation_id, manifest) }
        }
        let members = try store.members(gen.generation_id)
        guard members.count == gen.selected_count else { throw LocalError.invalidProtocol }
        if let catalog = gen.stage_catalog {
            while true {
                let cursor = try store.stageCatalogCursor() ?? ""
                let suffix = cursor.isEmpty ? "" : "?cursor=" + API.cursor(cursor)
                let page: StagePage = try await api.call("/reconciliations/\(gen.generation_id)/stages\(suffix)", context: boot.context_id)
                try current(run)
                guard page.generation_id == gen.generation_id else { throw LocalError.invalidProtocol }
                try store.appendStagePage(page, expected: catalog.revision)
                if page.complete { break }
            }
        }
        if let catalog = gen.metadata {
            for section in ["tags", "fields", "options"] {
                guard let initialCursor = try store.metadataCatalogCursor(section) else { continue }
                var cursor = initialCursor
                while true {
                    let suffix = cursor.isEmpty ? "" : "?cursor=" + API.cursor(cursor)
                    let page: MetadataCatalogPage = try await api.call("/reconciliations/\(gen.generation_id)/metadata/catalog/\(section)\(suffix)", context: boot.context_id)
                    try current(run)
                    guard page.generation_id == gen.generation_id, page.section == section else { throw LocalError.invalidProtocol }
                    try store.appendMetadataCatalogPage(page, expected: catalog.catalog_revision)
                    if page.complete { break }
                    guard let next = try store.metadataCatalogCursor(section) else { throw LocalError.invalidProtocol }
                    cursor = next
                }
            }
        }
        for (index, (person, rev)) in members.enumerated() {
            try current(run)
            message = "Downloading \(index + 1) of \(members.count) people. Previous workspace remains available."
            let stageQualified = gen.stage_catalog == nil ? true : (try store.hasQualifiedStageBundle(person, rev))
            let detailsQualified = detailsCapabilitiesReady ? try store.hasQualifiedDetailsBundle(person, rev) : true
            let metadataQualified = metadataCapabilitiesReady ? try store.hasQualifiedMetadataBundle(person, rev) : true
            let hasQualifiedRepresentation = stageQualified && detailsQualified && metadataQualified
            if try store.hasBundle(person, rev), hasQualifiedRepresentation { continue }
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
            if metadataCapabilitiesReady {
                let component: MetadataComponent = try await api.call("/reconciliations/\(gen.generation_id)/people/\(person)/metadata", context: boot.context_id)
                try current(run)
                guard component.generation_id == gen.generation_id, component.person_id == person else { throw LocalError.invalidProtocol }
                try store.appendMetadataComponent(component, expected: rev)
            }
        }
        let seal: Seal = try await api.call("/reconciliations/\(gen.generation_id)/seal", method: "POST", body: .object([:]), context: boot.context_id)
        try current(run); try store.promote(seal)
    }
}
