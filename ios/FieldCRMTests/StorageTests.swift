import XCTest
@testable import FieldCRM

final class StorageTests: XCTestCase {
    let identity = "11111111-1111-4111-8111-111111111111_22222222-2222-4222-8222-222222222222"
    let context = "33333333-3333-4333-8333-333333333333"
    let person = "44444444-4444-4444-8444-444444444444"
    let key = Data(repeating: 37, count: 32)
    var directory: URL!
    override func setUpWithError() throws {
        directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    }
    override func tearDownWithError() throws { try FileManager.default.removeItem(at: directory) }
    func open(_ name: String = "store", version: Int = 3) throws -> LocalStore {
        try LocalStore(url: directory.appendingPathComponent(name + ".sqlite"), key: key, identity: identity, context: context, schemaTarget: version)
    }
    func draft(_ text: String = "Synthetic iOS private note") -> Draft { Draft(id: UUID().uuidString, person: person, kind: "add_note", text: text, revision: 0) }
    func generation(_ rev: String = "1") -> Generation {
        Generation(generation_id: UUID().uuidString, context_id: context, evaluated_at: "2026-09-13T00:00:00Z", expires_at: "2026-09-13T00:30:00Z", complete: true, selected_count: 1, manifest: Manifest(items: [ManifestItem(person_id: person, revision: rev, reasons: ["assigned"])], next_cursor: nil, complete: true))
    }
    func stage(_ store: LocalStore, _ gen: Generation, notes: [JSON] = []) throws {
        try store.begin(gen)
        for section in ["summary", "notes", "tasks"] {
            let page = Page(generation_id: gen.generation_id, person_id: person, revision: gen.manifest.items[0].revision, section: section,
                            summary: section == "summary" ? .object(["id": .s(person), "display_name": .s("Synthetic Person")]) : nil,
                            items: section == "notes" ? notes : [], next_cursor: nil, complete: true)
            try store.appendPage(page, expected: gen.manifest.items[0].revision)
        }
        try store.finishBundle(gen.generation_id, person, gen.manifest.items[0].revision)
    }
    func seal(_ gen: Generation) -> Seal { Seal(generation_id: gen.generation_id, context_id: context, sealed_at: "2026-09-13T00:01:00Z", evaluated_at: gen.evaluated_at, selected_count: 1, today: .object(["items": .array([])])) }
    func testCipherWALFullWrongKeyAndReopen() throws {
        var store: LocalStore? = try open()
        XCTAssertEqual(try store!.rows("PRAGMA journal_mode")[0][0], "wal")
        XCTAssertEqual(try store!.rows("PRAGMA synchronous")[0][0], "2")
        XCTAssertEqual(try store!.rows("PRAGMA temp_store")[0][0], "2")
        XCTAssertTrue(try store!.rows("PRAGMA cipher_version")[0][0].hasPrefix("4.19.0"))
        let saved = try store!.saveDraft(draft())
        let op = try store!.submit(saved)
        let bytes = try store!.queue()[0].bytes
        let file = store!.url
        XCTAssertNotEqual(try Data(contentsOf: file).prefix(16), Data("SQLite format 3\0".utf8))
        let wal = try Data(contentsOf: URL(fileURLWithPath: file.path + "-wal"))
        XCTAssertNil(wal.range(of: Data(saved.text.utf8)))
        XCTAssertEqual(try file.resourceValues(forKeys: [.isExcludedFromBackupKey]).isExcludedFromBackup, true)
        XCTAssertTrue(try store!.rows("PRAGMA cipher_integrity_check").isEmpty)
        store = nil
        XCTAssertThrowsError(try LocalStore(url: file, key: Data(repeating: 9, count: 32), identity: identity, context: context))
        store = try open()
        XCTAssertEqual(try store!.queue()[0].id, op.operation_id)
        XCTAssertEqual(try store!.queue()[0].bytes, bytes)
        XCTAssertTrue(try store!.drafts().isEmpty)
    }
    func testVersionOneMigrationPreservesSavedWorkAndFutureSchemaFailsClosed() throws {
        var old: LocalStore? = try open(version: 1)
        let saved = try old!.saveDraft(draft())
        try old!.submit(saved)
        old = nil
        var upgraded: LocalStore? = try open()
        XCTAssertEqual(try upgraded!.rows("PRAGMA user_version")[0][0], "3")
        XCTAssertEqual(try upgraded!.queue().count, 1)
        XCTAssertEqual(try upgraded!.queue()[0].attempts, 0)
        try upgraded!.run("PRAGMA user_version=999"); upgraded = nil
        XCTAssertThrowsError(try open())
        XCTAssertTrue(FileManager.default.fileExists(atPath: directory.appendingPathComponent("store.sqlite").path))
    }
    func testActualSQLiteFullRollsBackDraftConsumptionAndNoFalseSave() throws {
        let store = try open()
        let saved = try store.saveDraft(draft(String(repeating: "Synthetic", count: 5000)))
        let count = try store.rows("PRAGMA page_count")[0][0]
        try store.run("PRAGMA max_page_count=" + count)
        XCTAssertThrowsError(try store.submit(saved))
        XCTAssertEqual(try store.drafts().first?.revision, saved.revision)
        XCTAssertTrue(try store.queue().isEmpty)
        var changed = saved; changed.text += String(repeating: "more", count: 10000)
        XCTAssertThrowsError(try store.saveDraft(changed))
        XCTAssertEqual(try store.drafts().first?.text, saved.text)
        try store.run("PRAGMA max_page_count=100000")
        XCTAssertNoThrow(try store.submit(saved))
        XCTAssertEqual(try store.queue().count, 1)
    }
    func testImmutableEnvelopeStaleDraftAndLocalTaskDependency() throws {
        let store = try open()
        let first = try store.saveDraft(draft())
        var changed = first; changed.text = "Revised local draft"
        let second = try store.saveDraft(changed)
        XCTAssertThrowsError(try store.submit(first))
        let op = try store.submit(second)
        let exact = try store.queue()[0].bytes
        try store.failure(op.operation_id, code: "connection_interrupted", permanent: false, delay: 30)
        XCTAssertEqual(try store.queue()[0].bytes, exact)
        var task = draft("Synthetic task"); task.kind = "create_task"
        let create = try store.submit(store.saveDraft(task))
        let completion = try store.complete(person: person, target: .object(["created_by_operation_id": .s(create.operation_id)]))
        XCTAssertEqual(completion.payload["target"]["created_by_operation_id"].text, create.operation_id)
        XCTAssertEqual(completion.payload["target"]["task_id"], .null)
        XCTAssertThrowsError(try store.submit(second))
    }
    func testReceiptBeforeOlderGenerationPreservesOverlayAndMonotonicCache() throws {
        let store = try open()
        let gen1 = generation(); try stage(store, gen1)
        let op = try store.submit(store.saveDraft(draft()))
        let receipt = Receipt(operation_id: op.operation_id, outcome: "accepted", resource_type: "note", resource_id: UUID().uuidString, committed_revision: nil, person_revision: "2", accepted_at: stamp(), changed: true, replayed: false)
        try store.acknowledge(receipt); try store.promote(seal(gen1))
        XCTAssertTrue(try store.queue()[0].overlay)
        let gen2 = generation("2"); try stage(store, gen2, notes: [.object(["id": .s(receipt.resource_id), "body": .s("Synthetic iOS private note")])]); try store.promote(seal(gen2))
        XCTAssertFalse(try store.queue()[0].overlay)
        let older = generation(); try store.begin(older); try store.promote(seal(older))
        XCTAssertEqual(try store.activeBundle(person)?.revision, "2")
        // Reverse ordering: promotion before acknowledgement also retires only with causal proof.
        try store.acknowledge(receipt); XCTAssertFalse(try store.queue()[0].overlay)
    }
    func testInterruptedStagingReopensWithoutPublishingPartialOrDeletingQueue() throws {
        var store: LocalStore? = try open()
        let active = generation(); try stage(store!, active); try store!.promote(seal(active))
        try store!.submit(store!.saveDraft(draft()))
        let next = generation("2"); try store!.begin(next)
        let partial = Page(generation_id: next.generation_id, person_id: person, revision: "2", section: "notes", summary: nil, items: [.object(["id": .s(UUID().uuidString)])], next_cursor: "opaque-checkpoint", complete: false)
        try store!.appendPage(partial, expected: "2")
        XCTAssertThrowsError(try store!.promote(seal(next)))
        store = nil; store = try open()
        XCTAssertEqual(try store!.generation()?.generation_id, next.generation_id)
        XCTAssertEqual(try store!.pages(next.generation_id, person, "notes").last?.next_cursor, "opaque-checkpoint")
        XCTAssertEqual(try store!.activeBundle(person)?.revision, "1")
        XCTAssertEqual(try store!.queue().count, 1)
    }
    func testWrongIdentityAndContextCannotAdoptQueue() throws {
        var store: LocalStore? = try open(); try store!.submit(store!.saveDraft(draft())); let file = store!.url; store = nil
        XCTAssertThrowsError(try LocalStore(url: file, key: key, identity: "other", context: context))
        XCTAssertThrowsError(try LocalStore(url: file, key: key, identity: identity, context: "other"))
        let other = try LocalStore(url: directory.appendingPathComponent("other.sqlite"), key: Data(repeating: 8, count: 32), identity: "other", context: "other")
        XCTAssertTrue(try other.queue().isEmpty)
        store = try open(); XCTAssertEqual(try store!.queue().count, 1)
    }
    func testSevenDayLeaseRestartRebootRollbackAndExpiry() throws {
        let boot = try fixture(Bootstrap.self, "bootstrap")
        var lease = try Lease(bootstrap: boot, clock: ClockSample(boot: "same-boot", uptime: 100, wall: 10000))
        try lease.validate(ClockSample(boot: "same-boot", uptime: 200, wall: 10100))
        var reopened = try decode(Lease.self, encode(lease))
        XCTAssertNoThrow(try reopened.validate(ClockSample(boot: "same-boot", uptime: 300, wall: 10200)))
        XCTAssertThrowsError(try reopened.validate(ClockSample(boot: "new-boot", uptime: 301, wall: 10201)))
        XCTAssertThrowsError(try reopened.validate(ClockSample(boot: "same-boot", uptime: 400, wall: 10000)))
        XCTAssertThrowsError(try reopened.validate(ClockSample(boot: "same-boot", uptime: 100 + 7 * 86400, wall: 700000)))
        XCTAssertThrowsError(try reopened.validate(ClockSample(boot: "same-boot", uptime: 200, wall: 10300)))
        XCTAssertFalse(try ClockSample.current().boot.isEmpty)
    }
    func fixture<T: Decodable>(_ type: T.Type, _ name: String) throws -> T {
        let url = try XCTUnwrap(Foundation.Bundle(for: StorageTests.self).url(forResource: name, withExtension: "json", subdirectory: "contracts"))
        return try decode(T.self, Data(contentsOf: url))
    }
    func testDurableLockMarkerSurvivesCredentialWriteFailureAndReopen() throws {
        let secure = SecureStorage(synthetic: true); secure.testDirectory = directory
        try secure.setLocked()
        secure.failCredentialWritesForTesting = true
        XCTAssertThrowsError(try secure.write("credential", Data("stale-authorized-credential".utf8)))
        let reopened = SecureStorage(synthetic: true); reopened.testDirectory = directory
        XCTAssertTrue(try reopened.isLocked())
        try reopened.clearLockAfterAuthorization()
        XCTAssertFalse(try reopened.isLocked())
    }
    func testCacheReclamationKeepsActiveStagedDraftsAndQueue() throws {
        let store = try open(); let old = generation(); try stage(store, old); try store.promote(seal(old))
        try store.submit(store.saveDraft(draft())); _ = try store.saveDraft(draft("Unsubmitted draft"))
        let current = generation("2"); try stage(store, current); try store.promote(seal(current))
        let next = generation("3"); try store.begin(next)
        try store.reclaimCache()
        XCTAssertTrue(try store.members(old.generation_id).isEmpty)
        XCTAssertEqual(try store.activeBundle(person)?.revision, "2")
        XCTAssertEqual(try store.members(next.generation_id).count, 1)
        XCTAssertEqual(try store.queue().count, 1); XCTAssertEqual(try store.drafts().count, 1)
        XCTAssertFalse(try store.hasBundle(person, "1"))
    }
    func testCleanupBacklogLargerThanEveryBatchDrainsBeforeNextGeneration() throws {
        let store = try open(); let current = generation(); try stage(store, current); try store.promote(seal(current))
        try store.transaction {
            for index in 0..<2501 { try store.run("INSERT INTO members VALUES('obsolete',?, '1')", [String(index)]) }
            for index in 0..<151 { try store.run("INSERT INTO bundles VALUES(?, '1','{}')", ["unreferenced-" + String(index)]) }
            for index in 0..<401 { try store.run("INSERT INTO pages VALUES('obsolete','person','notes',?,'{}')", [String(index)]) }
        }
        var batches = 1
        while try store.reclaimCache() { batches += 1 }
        XCTAssertGreaterThan(batches, 2)
        XCTAssertTrue(try store.rows("SELECT 1 FROM members WHERE generation='obsolete'").isEmpty)
        XCTAssertTrue(try store.rows("SELECT 1 FROM pages WHERE generation='obsolete'").isEmpty)
        XCTAssertEqual(try store.rows("SELECT COUNT(*) FROM bundles")[0][0], "1")
        XCTAssertEqual(try store.activeBundle(person)?.revision, "1")
    }
    func testSyntheticSimulatorKeychainActuallyPersistsThisDeviceOnlyItems() throws {
        let secure = SecureStorage(synthetic: true)
        let account = "test-key-" + UUID().uuidString
        let key = try secure.key(for: account, existingFile: false)
        XCTAssertEqual(try SecureStorage(synthetic: true).key(for: account, existingFile: true), key)
    }
    func testNormalSimulatorModeFailsClosedInsteadOfWeakeningPasscodePolicy() throws {
        #if targetEnvironment(simulator)
        XCTAssertThrowsError(try SecureStorage(synthetic: false).write("credential", Data("synthetic-test-only".utf8)))
        #endif
    }
    func testActualContractFixturesDecodeAndRevisionNeverRounds() throws {
        _ = try fixture(Bootstrap.self, "bootstrap")
        _ = try fixture(Generation.self, "reconciliation")
        _ = try fixture(Page.self, "summary_page"); _ = try fixture(Page.self, "notes_page"); _ = try fixture(Page.self, "tasks_page")
        _ = try fixture(Seal.self, "seal")
        _ = try fixture(Receipt.self, "create_task_receipt")
        _ = try fixture(Envelope.self, "add_note_request")
        XCTAssertEqual(try revision("9007199254740993"), 9007199254740993)
        XCTAssertThrowsError(try revision("01")); XCTAssertThrowsError(try revision("0"))
        XCTAssertEqual(try date("2026-09-13T01:27:15.217133+00:00"), try date("2026-09-13T01:27:15.217133Z"))
    }
}
