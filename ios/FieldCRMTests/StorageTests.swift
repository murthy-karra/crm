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
    func open(_ name: String = "store", version: Int = 8) throws -> LocalStore {
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
    func detailsBaseline(_ store: LocalStore, revision: String = "1", details: String = "7") throws {
        let gen = generation(revision); try store.begin(gen)
        let email = "55555555-5555-4555-8555-555555555555", phone = "66666666-6666-4666-8666-666666666666"
        let summary = JSON.object(["id": .s(person), "display_name": .s("Ada Lovelace"), "first_name": .s("Ada"), "last_name": .s("Lovelace"), "details_revision": .s(details)])
        let contacts: [JSON] = [.object(["id": .s(email), "kind": .s("email"), "value": .s("ada@example.test"), "import_order": .number(0), "created_at": .s("2026-01-01T00:00:00Z")]), .object(["id": .s(phone), "kind": .s("phone"), "value": .s("555-0100"), "import_order": .null, "created_at": .s("2026-01-02T00:00:00Z")])]
        for section in ["summary", "notes", "tasks"] { try store.appendPage(Page(generation_id: gen.generation_id, person_id: person, revision: revision, section: section, summary: section == "summary" ? summary : nil, items: section == "summary" ? contacts : [], next_cursor: nil, complete: true), expected: revision) }
        try store.finishBundle(gen.generation_id, person, revision); try store.promote(seal(gen))
    }
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
    func testMobile005ImmutableDetailsEnvelopeMappingConflictAndFollowUp() throws {
        let store = try open(); try detailsBaseline(store)
        let baseline = try XCTUnwrap(store.editableDetails(person: person))
        let contact = baseline["contacts"].list[0]["id"].text
        let proposal: JSON = .object(["first_name": .s("Ada Updated"), "contact_operations": .array([.object(["op": .s("edit"), "id": .s(contact), "value": .s("ada.updated@example.test")]), .object(["op": .s("add"), "kind": .s("phone"), "value": .s("555-0101")]), .object(["op": .s("remove"), "id": .s(baseline["contacts"].list[1]["id"].text)])])])
        let saved = try store.saveDraft(Draft(id: UUID().uuidString, person: person, kind: "update_person_details", revision: 0, expectedRevision: "7", baseline: baseline, proposal: proposal))
        let envelope = try store.submit(saved)
        XCTAssertEqual(envelope.payload["expected_details_revision"].text, "7")
        XCTAssertEqual(envelope.payload["contact_operations"].list[1]["op"].text, "add")
        XCTAssertThrowsError(try store.submit(saved), "A submitted details envelope is immutable and cannot be duplicated locally")
        try store.recordConflict(envelope.operation_id, current: .object(["details_revision": .s("8"), "first_name": .s("Other"), "contacts": baseline["contacts"]]), contextID: context, person: person)
        let conflict = try XCTUnwrap(store.draftForOperation(envelope.operation_id)); XCTAssertEqual(conflict.mode, "conflict")
        let before = try XCTUnwrap(store.queue().last?.bytes)
        try store.markSuperseded(envelope.operation_id)
        let replacement = try store.saveDraft(Draft(id: UUID().uuidString, person: person, kind: "update_person_details", revision: 0, expectedRevision: "8", baseline: conflict.current, proposal: proposal, predecessor: envelope.operation_id))
        let second = try store.submit(replacement)
        XCTAssertNotEqual(second.operation_id, envelope.operation_id); XCTAssertEqual(try store.queue().first { $0.id == envelope.operation_id }?.bytes, before)
        let follow = try store.saveDraft(Draft(id: UUID().uuidString, person: person, kind: "update_person_details", revision: 0, expectedRevision: "8", baseline: conflict.current, proposal: proposal))
        XCTAssertThrowsError(try store.submit(follow), LocalError.waitingPredecessor.localizedDescription)
        XCTAssertEqual(try store.drafts().first { $0.id == follow.id }?.mode, "follow_up")
        let receipt = Receipt(operation_id: second.operation_id, outcome: "accepted", resource_type: "person_details", resource_id: person, committed_revision: "9", person_revision: "2", accepted_at: stamp(), changed: true, replayed: false, added_contact_ids: [AddedContactID(ordinal: 1, id: "77777777-7777-4777-8777-777777777777")])
        try store.acknowledge(receipt)
        XCTAssertEqual(try store.queue().first { $0.id == second.operation_id }?.status, "accepted")
    }
    func testMobile005NameOnlyPatchPreservesLargeImportedProfileAndRequiresExactEmptyReceiptMap() throws {
        let store = try open(); let gen = generation("4"); try store.begin(gen)
        let summary: JSON = .object(["id": .s(person), "display_name": .s("Imported profile"), "first_name": .s("Imported"), "last_name": .s(String(repeating: "L", count: 200)), "details_revision": .s("9")])
        let contacts = (0..<51).map { index in JSON.object(["id": .s(String(format: "00000000-0000-4000-8000-%012d", index + 1)), "kind": .s(index.isMultiple(of: 2) ? "email" : "phone"), "value": .s("imported-\(index)@example.test"), "import_order": .number(Double(index)), "created_at": .s("2026-01-01T00:00:00Z")]) }
        for section in ["summary", "notes", "tasks"] { try store.appendPage(Page(generation_id: gen.generation_id, person_id: person, revision: "4", section: section, summary: section == "summary" ? summary : nil, items: section == "summary" ? contacts : [], next_cursor: nil, complete: true), expected: "4") }
        try store.finishBundle(gen.generation_id, person, "4"); try store.promote(seal(gen))
        let baseline = try XCTUnwrap(store.editableDetails(person: person))
        let draft = try store.saveDraft(Draft(id: UUID().uuidString, person: person, kind: "update_person_details", revision: 0, expectedRevision: "9", baseline: baseline, proposal: .object(["first_name": .s("Changed only")])))
        let envelope = try store.submit(draft)
        XCTAssertEqual(envelope.payload["first_name"].text, "Changed only")
        XCTAssertEqual(envelope.payload["contact_operations"].list, [])
        XCTAssertEqual(envelope.payload["last_name"], .null, "Untouched imported names are not resubmitted.")
        XCTAssertEqual(envelope.payload["contacts"], .null, "Untouched imported methods are never sent back as replacement state.")
        XCTAssertEqual(try store.activeBundle(person)?.contacts.count, 51, "Large imported contact sets remain readable and intact.")
        let noAdds = Receipt(operation_id: envelope.operation_id, outcome: "accepted", resource_type: "person_details", resource_id: person, committed_revision: "10", person_revision: "4", accepted_at: stamp(), changed: true, replayed: false, added_contact_ids: [])
        try store.acknowledge(noAdds)
        XCTAssertEqual(try store.queue().first?.status, "accepted")

        let second = try store.saveDraft(Draft(id: UUID().uuidString, person: person, kind: "update_person_details", revision: 0, expectedRevision: "10", baseline: baseline, proposal: .object(["last_name": .s("Changed only")])))
        let secondEnvelope = try store.submit(second)
        let missingMap = Receipt(operation_id: secondEnvelope.operation_id, outcome: "accepted", resource_type: "person_details", resource_id: person, committed_revision: "11", person_revision: "4", accepted_at: stamp(), changed: true, replayed: false)
        XCTAssertThrowsError(try store.acknowledge(missingMap))
        XCTAssertEqual(try store.queue().first { $0.id == secondEnvelope.operation_id }?.status, "pending", "A malformed no-add receipt leaves the immutable operation available for exact replay.")
    }
    func testMobile005EditorProjectionReopensSparseDraftWithoutNormalizingUntouchedImportedValues() throws {
        let emailID = "55555555-5555-4555-8555-555555555555"
        let baseline: JSON = .object([
            "first_name": .s("  Imported First  "),
            "last_name": .s(String(repeating: "L", count: 201)),
            "contacts": .array([.object(["id": .s(emailID), "kind": .s("email"), "value": .s("  imported@example.test  ")])])
        ])
        let untouched = DetailsEditorProjection.fields(for: Draft(id: "draft-whitespace", person: person, kind: "update_person_details", revision: 1, expectedRevision: "7", baseline: baseline, proposal: .object([:])))
        let changedOnly = DetailsEditorProjection.proposal(firstName: "Changed", lastName: untouched.lastName, methods: untouched.methods, baseline: baseline)
        XCTAssertEqual(changedOnly["first_name"].text, "Changed")
        XCTAssertEqual(changedOnly["last_name"], .null, "An untouched oversized imported name remains readable but is not normalized into an edit.")
        XCTAssertEqual(changedOnly["contact_operations"].list, [], "An untouched whitespace-bearing imported contact is omitted.")

        let savedProposal: JSON = .object([
            "first_name": .s("Saved first"),
            "contact_operations": .array([
                .object(["op": .s("edit"), "id": .s(emailID), "value": .s("saved@example.test")]),
                .object(["op": .s("add"), "kind": .s("phone"), "value": .s("555-0109")])
            ])
        ])
        let reopenedDraft = Draft(id: "draft-reopen", person: person, kind: "update_person_details", revision: 2, expectedRevision: "7", baseline: baseline, proposal: savedProposal)
        let reopened = DetailsEditorProjection.fields(for: reopenedDraft)
        XCTAssertEqual(reopened.firstName, "Saved first")
        XCTAssertEqual(reopened.lastName, String(repeating: "L", count: 201))
        XCTAssertEqual(reopened.methods.map(\.id), ["draft:draft-reopen:add:1", emailID])
        XCTAssertEqual(reopened.methods.map(\.value), ["555-0109", "saved@example.test"])
        XCTAssertEqual(DetailsEditorProjection.proposal(firstName: reopened.firstName, lastName: reopened.lastName, methods: reopened.methods, baseline: baseline), savedProposal, "Reopening a protected draft retains its sparse proposal and stable local add ID.")
    }
    func testMobile005SchemaSevenInstalledUpgradePreservesOpaqueRowsAndRequiresCompleteDetails() throws {
        var old: LocalStore? = try open(version: 7)
        let oldGeneration = generation("1"); try stage(old!, oldGeneration); try old!.promote(seal(oldGeneration))
        let saved = try old!.saveDraft(draft("schema seven protected note")); _ = try old!.submit(saved)
        let oldBytes = try XCTUnwrap(old!.queue().first?.bytes); old = nil
        let upgraded = try open()
        XCTAssertEqual(try upgraded.rows("PRAGMA user_version")[0][0], "8"); XCTAssertEqual(try upgraded.queue().first?.bytes, oldBytes)
        XCTAssertNil(try upgraded.editableDetails(person: person))
        XCTAssertEqual(try upgraded.activeBundle(person)?.summary["display_name"].text, "Synthetic Person")
        try detailsBaseline(upgraded, revision: "1", details: "1")
        XCTAssertNotNil(try upgraded.editableDetails(person: person), "A fully traversed same-broad-revision summary replaces only the cache representation")
    }
    func testVersionOneMigrationPreservesSavedWorkAndFutureSchemaFailsClosed() throws {
        var old: LocalStore? = try open(version: 1)
        let saved = try old!.saveDraft(draft())
        try old!.submit(saved)
        old = nil
        var upgraded: LocalStore? = try open()
        XCTAssertEqual(try upgraded!.rows("PRAGMA user_version")[0][0], "8")
        XCTAssertEqual(try upgraded!.queue().count, 1)
        XCTAssertEqual(try upgraded!.queue()[0].attempts, 0)
        try upgraded!.run("PRAGMA user_version=999"); upgraded = nil
        XCTAssertThrowsError(try open())
        XCTAssertTrue(FileManager.default.fileExists(atPath: directory.appendingPathComponent("store.sqlite").path))
    }
    func testMobile004StageCatalogQualificationAndAtomicProposalUpgrade() throws {
        // This represents the opaque operation row written by the installed
        // Mobile003 schema. Its old bundle is deliberately not synthesized by
        // Mobile004 code; the actual installed-app upgrade is a separate UI
        // fixture run against the archived Mobile003 source.
        var legacy: LocalStore? = try open(version: 6)
        let oldDraft = try legacy!.saveDraft(draft("Mobile003 immutable saved note")); _ = try legacy!.submit(oldDraft)
        let oldBytes = try legacy!.queue().first!.bytes; legacy = nil

        let store = try open()
        XCTAssertEqual(try store.rows("PRAGMA user_version")[0][0], "8")
        XCTAssertEqual(try store.queue().first!.bytes, oldBytes)
        XCTAssertNil(try store.editableStage(person: person), "Old summaries cannot seed a stage CAS baseline")
        let stageID = "55555555-5555-4555-8555-555555555555"
        let catalog = StageCatalog(revision: "1", stages_url: "/api/mobile/v1/reconciliations/\(UUID().uuidString)/stages")
        // Use a generation whose URL is cryptographically-shaped for the local
        // route binding check, then promote a same-broad-revision new format.
        let genID = UUID().uuidString.lowercased()
        let qualified = Generation(generation_id: genID, context_id: context, evaluated_at: "2026-09-13T00:00:00Z", expires_at: "2026-09-13T00:30:00Z", complete: true, selected_count: 1, manifest: Manifest(items: [ManifestItem(person_id: person, revision: "1", reasons: ["assigned"])], next_cursor: nil, complete: true), stage_catalog: StageCatalog(revision: catalog.revision, stages_url: "/api/mobile/v1/reconciliations/\(genID)/stages"))
        try store.begin(qualified)
        for section in ["summary", "notes", "tasks"] {
            let summary: JSON? = section == "summary" ? .object(["id": .s(person), "display_name": .s("Synthetic Person"), "stage_revision": .s("1"), "stage": .object(["id": .s(stageID), "name": .s("Legacy label")])]) : nil
            try store.appendPage(Page(generation_id: genID, person_id: person, revision: "1", section: section, summary: summary, items: [], next_cursor: nil, complete: true), expected: "1")
        }
        try store.appendStagePage(StagePage(generation_id: genID, revision: "1", items: [Stage(id: stageID, name: "Renamed catalog label", position: 0)], next_cursor: nil, complete: true), expected: "1")
        try store.finishBundle(genID, person, "1"); try store.promote(seal(qualified))
        let baseline = try XCTUnwrap(store.editableStage(person: person))
        XCTAssertEqual(baseline.0.name, "Legacy label"); XCTAssertEqual(try store.activeStages().first?.name, "Renamed catalog label")
        let replacement = Stage(id: stageID, name: "Renamed catalog label", position: 0)
        let proposal = try store.queueStage(person: person, baseline: baseline.0, stage: replacement, expected: baseline.1)
        let op = try XCTUnwrap(store.queue().last)
        XCTAssertEqual(op.envelope.kind, "change_person_stage"); XCTAssertEqual(op.envelope.payload["expected_stage_revision"].text, "1")
        XCTAssertEqual(op.envelope.payload["stage_id"].text, stageID); XCTAssertEqual(try store.drafts().first { $0.id == proposal.id }?.mode, "submitted")
        let receipt = Receipt(operation_id: op.id, outcome: "accepted", resource_type: "person_stage", resource_id: person, committed_revision: "1", person_revision: "1", accepted_at: stamp(), changed: false, replayed: false)
        try store.acknowledge(receipt); XCTAssertFalse(try XCTUnwrap(store.queue().last).overlay)
    }
    func testMobile004StagePagesCannotPromotePartialCatalogAndCatalogCleanupKeepsActive() throws {
        let store = try open(); let genID = UUID().uuidString.lowercased()
        let gen = Generation(generation_id: genID, context_id: context, evaluated_at: stamp(), expires_at: stamp(Date().addingTimeInterval(1800)), complete: true, selected_count: 1, manifest: Manifest(items: [ManifestItem(person_id: person, revision: "1", reasons: [])], next_cursor: nil, complete: true), stage_catalog: StageCatalog(revision: "1", stages_url: "/api/mobile/v1/reconciliations/\(genID)/stages"))
        try store.begin(gen)
        for section in ["summary", "notes", "tasks"] { try store.appendPage(Page(generation_id: genID, person_id: person, revision: "1", section: section, summary: section == "summary" ? .object(["id": .s(person), "stage_revision": .s("1"), "stage": .object(["id": .s("55555555-5555-4555-8555-555555555555"), "name": .s("Lead")])]) : nil, items: [], next_cursor: nil, complete: true), expected: "1") }
        try store.finishBundle(genID, person, "1")
        let customLabel = String(repeating: "L", count: 2048)
        try store.appendStagePage(StagePage(generation_id: genID, revision: "1", items: [Stage(id: "55555555-5555-4555-8555-555555555555", name: customLabel, position: 0)], next_cursor: "opaque", complete: false), expected: "1")
        XCTAssertThrowsError(try store.promote(seal(gen)))
        XCTAssertNil(try store.meta("active_stage_catalog"))
        try store.appendStagePage(StagePage(generation_id: genID, revision: "1", items: [], next_cursor: nil, complete: true), expected: "1")
        try store.promote(seal(gen)); XCTAssertEqual(try store.activeStages().first?.name, customLabel)
    }
    func testMobile004StageConflictReplacementAndExplicitFollowUpDraft() throws {
        let store = try open(); let genID = UUID().uuidString.lowercased()
        let stageA = "55555555-5555-4555-8555-555555555555", stageB = "66666666-6666-4666-8666-666666666666", stageC = "77777777-7777-4777-8777-777777777777"
        let generation = Generation(generation_id: genID, context_id: context, evaluated_at: stamp(), expires_at: stamp(Date().addingTimeInterval(1800)), complete: true, selected_count: 1, manifest: Manifest(items: [ManifestItem(person_id: person, revision: "1", reasons: [])], next_cursor: nil, complete: true), stage_catalog: StageCatalog(revision: "1", stages_url: "/api/mobile/v1/reconciliations/\(genID)/stages"))
        try store.begin(generation)
        for section in ["summary", "notes", "tasks"] {
            let summary: JSON? = section == "summary" ? .object(["id": .s(person), "stage_revision": .s("1"), "stage": .object(["id": .s(stageA), "name": .s("A")])]) : nil
            try store.appendPage(Page(generation_id: genID, person_id: person, revision: "1", section: section, summary: summary, items: [], next_cursor: nil, complete: true), expected: "1")
        }
        try store.appendStagePage(StagePage(generation_id: genID, revision: "1", items: [Stage(id: stageA, name: "A", position: 0), Stage(id: stageB, name: "B", position: 1), Stage(id: stageC, name: "C", position: 2)], next_cursor: nil, complete: true), expected: "1")
        try store.finishBundle(genID, person, "1"); try store.promote(seal(generation))

        let first = try store.queueStage(person: person, baseline: Stage(id: stageA, name: "A", position: 0), stage: Stage(id: stageB, name: "B", position: 1), expected: "1")
        let conflict = try XCTUnwrap(store.queue().first); let immutableConflictBytes = conflict.bytes
        try store.recordStageConflict(conflict.id, current: CurrentStageResponse(context_id: context, person_id: person, person_revision: "2", stage_revision: "2", stage: Stage(id: stageA, name: "A", position: 0)), contextID: context, person: person)
        let replacement = try store.queueStage(person: person, baseline: Stage(id: stageA, name: "A", position: 0), stage: Stage(id: stageC, name: "C", position: 2), expected: "2", superseding: conflict.id)
        let replacementOp = try XCTUnwrap(store.queue().first { $0.id == replacement.predecessor })
        XCTAssertNotEqual(replacementOp.id, conflict.id); XCTAssertEqual(try store.queue().first { $0.id == conflict.id }?.bytes, immutableConflictBytes)
        XCTAssertEqual(try store.queue().first { $0.id == conflict.id }?.status, "superseded")
        XCTAssertEqual(replacement.mode, "submitted")

        let follow = try store.queueStage(person: person, baseline: Stage(id: stageA, name: "A", position: 0), stage: Stage(id: stageB, name: "B", position: 1), expected: "2")
        XCTAssertEqual(follow.mode, "follow_up"); XCTAssertEqual(follow.predecessor, replacementOp.id)
        XCTAssertEqual(try store.queue().count, 2, "A follow-up draft must not create another immutable outbox row")
        XCTAssertEqual(follow.baseline?["stage_revision"].text, "2")

        let receipt = Receipt(operation_id: replacementOp.id, outcome: "accepted", resource_type: "person_stage", resource_id: person, committed_revision: "2", person_revision: "2", accepted_at: stamp(), changed: true, replayed: false)
        try store.acknowledge(receipt)
        let submitted = try store.queueStage(person: person, baseline: Stage(id: stageA, name: "A", position: 0), stage: Stage(id: stageB, name: "B", position: 1), expected: "2", draftID: follow.id, draftRevision: follow.revision)
        let submittedOp = try XCTUnwrap(store.queue().first { $0.id == submitted.predecessor })
        XCTAssertEqual(submitted.mode, "submitted"); XCTAssertNotEqual(submittedOp.id, replacementOp.id)
        XCTAssertEqual(submitted.baseline?["stage_revision"].text, "2", "Explicit follow-up submit preserves its old baseline until a user reviews/rebases it.")
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
        let parts = name.split(separator: "/").map(String.init)
        let resource = parts.last ?? name
        let folder = (["contracts"] + parts.dropLast()).joined(separator: "/")
        let url = try XCTUnwrap(Foundation.Bundle(for: StorageTests.self).url(forResource: resource, withExtension: "json", subdirectory: folder))
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
        _ = try fixture(CurrentRecordResponse.self, "mobile002/mobile002_current_note")
        _ = try fixture(CurrentRecordResponse.self, "mobile002/mobile002_current_task")
        _ = try fixture(Receipt.self, "mobile002/mobile002_edit_note_receipt")
        _ = try fixture(Receipt.self, "mobile002/mobile002_update_task_receipt")
        _ = try fixture(Envelope.self, "mobile003/log_contact_attempt_request")
        _ = try fixture(Receipt.self, "mobile003/contact_attempt_receipt")
        XCTAssertEqual(try revision("9007199254740993"), 9007199254740993)
        XCTAssertThrowsError(try revision("01")); XCTAssertThrowsError(try revision("0"))
        XCTAssertEqual(try date("2026-09-13T01:27:15.217133+00:00"), try date("2026-09-13T01:27:15.217133Z"))
    }
    func testMobile002UpgradeKeepsLegacyBytesButRequiresVersionedNoteQualification() throws {
        var old: LocalStore? = try open(version: 3)
        let legacy = try old!.submit(old!.saveDraft(draft("Old envelope must remain byte exact")))
        let bytes = try old!.queue().first!.bytes
        let generation = self.generation(); try stage(old!, generation, notes: [.object(["id": .s("55555555-5555-4555-8555-555555555555"), "body": .s("Legacy readable note"), "can_manage": .bool(true)])]); try old!.promote(seal(generation))
        old = nil
        let upgraded = try open()
        XCTAssertEqual(try upgraded.rows("PRAGMA user_version")[0][0], "8")
        XCTAssertEqual(try upgraded.queue().first?.id, legacy.operation_id)
        XCTAssertEqual(try upgraded.queue().first?.bytes, bytes)
        XCTAssertNil(try upgraded.editableRecord(person: person, type: "note", id: "55555555-5555-4555-8555-555555555555"))
        XCTAssertEqual(try upgraded.activeBundle(person)?.notes.first?["body"].text, "Legacy readable note")
    }
    func testEditProposalCASFollowUpAndReceiptIdentityFences() throws {
        let store = try open()
        let note = "66666666-6666-4666-8666-666666666666"
        let initial = Draft(id: UUID().uuidString, person: person, kind: "edit_note", text: "Proposal A", revision: 0, targetID: note, expectedRevision: "2", baseline: .object(["body": .s("Baseline")]), proposal: .object(["body": .s("Proposal A")]))
        let saved = try store.saveDraft(initial)
        var revised = saved; revised.text = "Proposal B"; revised.proposal = .object(["body": .s("Proposal B")])
        let current = try store.saveDraft(revised)
        XCTAssertThrowsError(try store.submit(saved))
        let op = try store.submit(current)
        XCTAssertEqual(op.kind, "edit_note"); XCTAssertEqual(op.payload["expected_revision"].text, "2")
        let follow = try store.saveDraft(Draft(id: UUID().uuidString, person: person, kind: "edit_note", text: "Follow up", revision: 0, targetID: note, expectedRevision: "2", baseline: .object(["body": .s("Baseline")]), proposal: .object(["body": .s("Follow up")])) )
        XCTAssertThrowsError(try store.submit(follow)) { XCTAssertTrue($0 is LocalError) }
        let persisted = try XCTUnwrap(store.drafts().first { $0.id == follow.id }); XCTAssertEqual(persisted.mode, "follow_up"); XCTAssertEqual(persisted.predecessor, op.operation_id)
        let wrong = Receipt(operation_id: op.operation_id, outcome: "accepted", resource_type: "note", resource_id: UUID().uuidString, committed_revision: "3", person_revision: "3", accepted_at: stamp(), changed: true, replayed: false)
        XCTAssertThrowsError(try store.acknowledge(wrong))
        let accepted = Receipt(operation_id: op.operation_id, outcome: "accepted", resource_type: "note", resource_id: note, committed_revision: "3", person_revision: "3", accepted_at: stamp(), changed: true, replayed: false)
        XCTAssertNoThrow(try store.acknowledge(accepted))
    }
    func testOperationAndReceiptNotFoundKeepProtectedOriginalIdentityUnavailable() throws {
        let store = try open(); let note = "77777777-7777-4777-8777-777777777777"
        let saved = try store.saveDraft(Draft(id: UUID().uuidString, person: person, kind: "edit_note", text: "Protected uncertain proposal", revision: 0, targetID: note, expectedRevision: "1", baseline: .object(["body": .s("old")]), proposal: .object(["body": .s("Protected uncertain proposal")])))
        let op = try store.submit(saved); let bytes = try XCTUnwrap(store.queue().first?.bytes)
        // POST-operation 404 and a later receipt lookup 404 are both ambiguous:
        // no replacement ID is minted and private proposal/baseline survive.
        try store.failure(op.operation_id, code: "not_found", permanent: true, delay: 0)
        XCTAssertEqual(try store.queue().first?.status, "unavailable")
        XCTAssertEqual(try store.queue().first?.bytes, bytes)
        let protected = try XCTUnwrap(store.draftForOperation(op.operation_id))
        XCTAssertEqual(protected.text, "Protected uncertain proposal"); XCTAssertEqual(protected.baseline?["body"].text, "old")
    }
    func testContactUTCInstantsSurviveTimezoneRelaunchAndRejectInvalidCalendarDates() throws {
        let original = NSTimeZone.default
        defer { NSTimeZone.default = original }
        NSTimeZone.default = TimeZone(identifier: "America/Los_Angeles")!
        var store: LocalStore? = try open("time")
        // 10:30Z is during Los Angeles's skipped 02:30 wall hour. UTC selection
        // remains a real, explicit instant rather than a silently shifted local date.
        let skippedWallHour = try store!.saveDraft(Draft(id: UUID().uuidString, person: person, kind: "log_contact_attempt", revision: 0,
            contactChannel: "call", contactOutcome: "reached", occurredAt: "2026-03-08T10:30:00Z", deviceRecordedAt: "2000-01-01T00:00:00Z"))
        let first = try store!.submit(skippedWallHour)
        let repeatedDaylight = try store!.saveDraft(Draft(id: UUID().uuidString, person: person, kind: "log_contact_attempt", revision: 0,
            contactChannel: "call", contactOutcome: "reached", occurredAt: "2026-11-01T01:30:00-07:00", deviceRecordedAt: "2000-01-01T00:00:00Z"))
        let repeatedStandard = try store!.saveDraft(Draft(id: UUID().uuidString, person: person, kind: "log_contact_attempt", revision: 0,
            contactChannel: "call", contactOutcome: "reached", occurredAt: "2026-11-01T01:30:00-08:00", deviceRecordedAt: "2000-01-01T00:00:00Z"))
        let second = try store!.submit(repeatedDaylight), third = try store!.submit(repeatedStandard)
        XCTAssertNotEqual(second.payload["occurred_at"].text, third.payload["occurred_at"].text)
        XCTAssertNotEqual(first.device_recorded_at, "2000-01-01T00:00:00Z", "device time freezes when the immutable operation is saved")
        let invalid = try store!.saveDraft(Draft(id: UUID().uuidString, person: person, kind: "log_contact_attempt", revision: 0,
            contactChannel: "call", contactOutcome: "reached", occurredAt: "2026-02-30T12:00:00Z"))
        XCTAssertThrowsError(try store!.submit(invalid))
        let bytes = try store!.queue().map(\.bytes); store = nil
        NSTimeZone.default = TimeZone(identifier: "Asia/Tokyo")!
        store = try open("time")
        XCTAssertEqual(try store!.queue().map(\.bytes), bytes)
        XCTAssertEqual(try store!.queue()[0].envelope.payload["occurred_at"].text, "2026-03-08T10:30:00Z")
    }
    func testContactSQLiteFullKeepsSavedDraftAndMixedQueueBytes() throws {
        let store = try open("contact-full")
        let note = try store.submit(store.saveDraft(draft("Existing note remains immutable")))
        let noteBytes = try XCTUnwrap(store.queue().first?.bytes)
        let pageCount = try store.rows("PRAGMA page_count")[0][0]; try store.run("PRAGMA max_page_count=" + pageCount)
        let contactDraft = Draft(id: UUID().uuidString, person: person, kind: "log_contact_attempt", text: String(repeating: "protected", count: 6000), revision: 0,
            contactChannel: "email", contactOutcome: "sent", occurredAt: "2026-09-13T10:00:00Z")
        XCTAssertThrowsError(try store.saveDraft(contactDraft))
        XCTAssertEqual(try store.queue().count, 1); XCTAssertEqual(try store.queue().first?.id, note.operation_id); XCTAssertEqual(try store.queue().first?.bytes, noteBytes)
        XCTAssertTrue(try store.drafts().isEmpty)
    }
    func testMobile003UpgradeKeepsMobile002BytesAndContactNeedsFreshTodaySeal() throws {
        var old: LocalStore? = try open(version: 5)
        let prior = try old!.submit(old!.saveDraft(draft("Mobile002 envelope remains exact")))
        let priorBytes = try XCTUnwrap(old!.queue().first?.bytes)
        let initial = generation(); try stage(old!, initial); try old!.promote(seal(initial))
        old = nil

        let store = try open()
        XCTAssertEqual(try store.rows("PRAGMA user_version")[0][0], "8")
        XCTAssertEqual(try store.queue().first?.id, prior.operation_id)
        XCTAssertEqual(try store.queue().first?.bytes, priorBytes)

        let first = try store.saveDraft(Draft(id: UUID().uuidString, person: person, kind: "log_contact_attempt", revision: 0,
                                               contactChannel: "other", contactOutcome: "reached", occurredAt: "2026-09-12T13:00:00.123456789-07:00", deviceRecordedAt: "2026-09-13T12:00:00Z"))
        var edited = first; edited.contactOutcome = "sent"
        let second = try store.saveDraft(edited)
        XCTAssertThrowsError(try store.submit(first), "A stale editor cannot mint a second operation")
        let one = try store.submit(second)
        let distinctDraft = try store.saveDraft(Draft(id: UUID().uuidString, person: person, kind: "log_contact_attempt", revision: 0,
                                                       contactChannel: "email", contactOutcome: "sent", occurredAt: "2026-09-12T20:01:00Z", deviceRecordedAt: "2026-09-13T12:00:00Z"))
        let two = try store.submit(distinctDraft)
        XCTAssertNotEqual(one.operation_id, two.operation_id)
        XCTAssertEqual(try store.queue().filter(\.isContact).count, 2)
        XCTAssertEqual(one.payload["occurred_at"].text, "2026-09-12T13:00:00.123456789-07:00")

        let receipt = Receipt(operation_id: one.operation_id, outcome: "accepted", resource_type: "contact_attempt", resource_id: UUID().uuidString,
                              committed_revision: nil, person_revision: "1", accepted_at: stamp(), changed: true, replayed: false)
        try store.acknowledge(receipt)
        XCTAssertTrue(try XCTUnwrap(store.queue().first { $0.id == one.operation_id }).overlay)
        XCTAssertEqual(try store.meta("today_refresh_pending"), "1")
        let malformed = Receipt(operation_id: two.operation_id, outcome: "accepted", resource_type: "contact_attempt", resource_id: UUID().uuidString,
                                committed_revision: "2", person_revision: "1", accepted_at: stamp(), changed: true, replayed: false)
        XCTAssertThrowsError(try store.acknowledge(malformed))
        let unchanged = Receipt(operation_id: two.operation_id, outcome: "accepted", resource_type: "contact_attempt", resource_id: UUID().uuidString,
                                committed_revision: nil, person_revision: "1", accepted_at: stamp(), changed: false, replayed: false)
        XCTAssertThrowsError(try store.acknowledge(unchanged))

        // Contact facts do not fabricate a Person revision. A complete new seal,
        // even at revision 1, is the evidence that Today is fresh and clears the overlay.
        let refreshed = generation("1"); try stage(store, refreshed); try store.promote(seal(refreshed))
        XCTAssertFalse(try XCTUnwrap(store.queue().first { $0.id == one.operation_id }).overlay)
        XCTAssertNil(try store.meta("today_refresh_pending"))
    }
}
