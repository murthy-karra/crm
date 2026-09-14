import XCTest
@testable import FieldCRM

@MainActor final class ModelTests: XCTestCase {
    var directory: URL!
    var originalPause = false
    override func setUp() async throws {
        originalPause = UserDefaults.standard.bool(forKey: "pauseSync")
        directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    }
    override func tearDown() async throws {
        UserDefaults.standard.set(originalPause, forKey: "pauseSync")
        try FileManager.default.removeItem(at: directory)
    }
    func response(_ request: URLRequest, _ status: Int, _ json: JSON, cookie: Bool = false) throws -> (Data, HTTPURLResponse) {
        (try encode(json), HTTPURLResponse(url: request.url!, statusCode: status, httpVersion: "HTTP/1.1", headerFields: cookie ? ["Set-Cookie": "crm_session=synthetic-model-test; Path=/; HttpOnly"] : [:])!)
    }
    func setupModel(details: Bool = false) async throws -> (FieldModel, API, SecureStorage, Bootstrap, LocalStore) {
        let model = FieldModel(synthetic: true, startMonitor: false, restoreOnInit: false)
        let secure = SecureStorage(synthetic: true, testingNamespace: UUID().uuidString); secure.testDirectory = directory
        #if MOBILE005_QA || MOBILE005_UPGRADE_QA
        let api = try API(base: "http://127.0.0.1:3103")
        #elseif MOBILE002_QA || MOBILE003_QA || MOBILE004_QA || MOBILE004_UPGRADE_QA
        let api = try API(base: "http://127.0.0.1:3102")
        #else
        let api = try API(base: "http://127.0.0.1:3101")
        #endif
        let boot = Bootstrap(context_id: UUID().uuidString, installation_id: model.installation, actor_user_id: UUID().uuidString, organization_id: UUID().uuidString,
                             workspace_revision: "1", authorized_at: stamp(), offline_access_expires_at: stamp(Date().addingTimeInterval(7 * 86400)), server_time: stamp(), capabilities: ["add_note", "create_task", "complete_task", "reconciliation", "log_contact_attempt"] + (details ? ["update_person_details", "details_revisions"] : []), protocol: "mobile-v1")
        api.responseForTesting = { request in
            if request.url!.path == "/api/session" { return try self.response(request, 200, .object([:]), cookie: true) }
            return (try encode(boot), HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: nil)!)
        }
        model.configureForTesting(secure: secure, api: api, directory: directory); model.paused = true
        await model.signIn(email: "controlled@synthetic.test", password: "synthetic")
        XCTAssertTrue(model.unlocked, model.message)
        let store = try XCTUnwrap(model.store)
        let person = UUID().uuidString, gen = UUID().uuidString
        try store.begin(Generation(generation_id: gen, context_id: boot.context_id, evaluated_at: stamp(), expires_at: stamp(Date().addingTimeInterval(1800)), complete: true, selected_count: 1, manifest: Manifest(items: [ManifestItem(person_id: person, revision: "1", reasons: ["assigned"])], next_cursor: nil, complete: true)))
        for section in ["summary", "notes", "tasks"] {
            try store.appendPage(Page(generation_id: gen, person_id: person, revision: "1", section: section, summary: section == "summary" ? .object(["id": .s(person), "display_name": .s("Controlled Synthetic Person")]) : nil, items: [], next_cursor: nil, complete: true), expected: "1")
        }
        try store.finishBundle(gen, person, "1")
        try store.promote(Seal(generation_id: gen, context_id: boot.context_id, sealed_at: stamp(), evaluated_at: stamp(), selected_count: 1, today: .object(["items": .array([])])))
        try model.reload()
        return (model, api, secure, boot, store)
    }
    func reopen(_ secure: SecureStorage, _ api: API) -> FieldModel {
        let next = FieldModel(synthetic: true, startMonitor: false, restoreOnInit: false)
        next.configureForTesting(secure: secure, api: api, directory: directory); next.restore(); return next
    }
    func queue(_ store: LocalStore) throws {
        let person = try XCTUnwrap(store.activePeople().first?.person)
        try store.submit(store.saveDraft(Draft(id: UUID().uuidString, person: person, kind: "add_note", text: "Controlled synthetic saved input", revision: 0)))
    }
    func queuedEdit(_ store: LocalStore, text: String = "Protected edit proposal") throws -> (Draft, Data) {
        let person = try XCTUnwrap(store.activePeople().first?.person)
        let draft = try store.saveDraft(Draft(id: UUID().uuidString.lowercased(), person: person, kind: "edit_note", text: text, revision: 0,
                                              targetID: "11111111-1111-4111-8111-111111111111", expectedRevision: "1",
                                              baseline: .object(["body": .s("Baseline text")]), proposal: .object(["body": .s(text)])))
        _ = try store.submit(draft)
        return (draft, try XCTUnwrap(store.queue().last?.bytes))
    }
    func queuedContact(_ store: LocalStore) throws -> (Draft, Data) {
        let person = try XCTUnwrap(store.activePeople().first?.person)
        let draft = try store.saveDraft(Draft(id: UUID().uuidString.lowercased(), person: person, kind: "log_contact_attempt", revision: 0,
                                              contactChannel: "other", contactOutcome: "reached", occurredAt: "2026-09-12T20:00:00Z", deviceRecordedAt: "2026-09-13T00:00:00Z"))
        _ = try store.submit(draft)
        return (draft, try XCTUnwrap(store.queue().last?.bytes))
    }

    func profileDraft(_ store: LocalStore) throws -> Draft {
        let person = try XCTUnwrap(store.activePeople().first?.person)
        return try store.saveDraft(Draft(id: UUID().uuidString, person: person, kind: "update_person_details", revision: 0, expectedRevision: "7",
            baseline: .object(["first_name": .s("Original"), "last_name": .null, "details_revision": .s("7"), "contacts": .array([])]),
            proposal: .object(["first_name": .s("Protected proposal"), "contact_operations": .array([])])))
    }
    func profileContact(value: String = "synthetic@example.test") -> JSON {
        .object(["id": .s(UUID().uuidString), "kind": .s("email"), "value": .s(value), "import_order": .null, "created_at": .s("2026-09-14T12:00:00Z")])
    }
    func profilePage(_ boot: Bootstrap, _ person: String, _ items: [JSON], next: String?, broad: String = "9") -> JSON {
        .object(["context_id": .s(boot.context_id), "person_id": .s(person), "person_revision": .s(broad), "details_revision": .s("8"),
            "first_name": .s("Current"), "last_name": .null, "items": .array(items), "next_cursor": next.map(JSON.s) ?? .null, "complete": .bool(next == nil)])
    }
    func testMobile005CurrentProfileBoundsAndCursorCyclePreserveProtectedDraft() async throws {
        let (model, api, _, boot, store) = try await setupModel(details: true)
        let draft = try profileDraft(store)
        let original = try encode(draft)
        for scenario in ["rows", "bytes", "cursor", "cycle"] {
            var calls = 0
            api.responseForTesting = { request in
                calls += 1
                if calls > 4 { XCTFail("Unbounded current-profile loop"); return try self.response(request, 503, .object([:])) }
                let items = scenario == "rows" ? (0..<101).map { _ in self.profileContact() }
                    : [self.profileContact(value: scenario == "bytes" ? String(repeating: "日", count: 180000) : "synthetic@example.test")]
                let next: String? = scenario == "cursor" ? String(repeating: "x", count: 2049)
                    : scenario == "cycle" ? (calls == 2 ? "second" : "first") : nil
                return try self.response(request, 200, self.profilePage(boot, draft.person, items, next: next))
            }
            do { _ = try await model.requalifyDetails(draft); XCTFail("Must reject \(scenario)") }
            catch { XCTAssertTrue(error is LocalError, "\(error)") }
            XCTAssertEqual(calls, scenario == "cycle" ? 3 : 1)
            XCTAssertEqual(try encode(XCTUnwrap(store.drafts().first { $0.id == draft.id })), original)
        }
    }
    func testMobile005MalformedCurrentContactPreservesProtectedDraft() async throws {
        let (model, api, _, boot, store) = try await setupModel(details: true)
        let draft = try profileDraft(store), before = try encode(draft)
        let malformedOrders: [JSON?] = [nil, .s("1"), .number(1.5), .number(Double(Int32.max) + 1)]
        for value in malformedOrders {
            guard case .object(var fields) = profileContact() else { return XCTFail("fixture contact") }
            fields["import_order"] = value
            let malformed = JSON.object(fields)
            api.responseForTesting = { request in
                try self.response(request, 200, self.profilePage(boot, draft.person, [malformed], next: nil))
            }
            do { _ = try await model.requalifyDetails(draft); XCTFail("Malformed ordering must not qualify") }
            catch { XCTAssertTrue(error is LocalError, "\(error)") }
            XCTAssertEqual(try encode(XCTUnwrap(store.drafts().first { $0.id == draft.id })), before)
        }
    }
    func testMobile005CompleteCurrentProfileAllowsUnrelatedBroadRevisionChange() async throws {
        let (model, api, _, boot, store) = try await setupModel(details: true)
        let draft = try profileDraft(store)
        let first = profileContact(), second = profileContact()
        var calls = 0
        api.responseForTesting = { request in
            calls += 1
            let cursor = URLComponents(url: request.url!, resolvingAgainstBaseURL: false)?.queryItems?.first { $0.name == "cursor" }?.value
            XCTAssertEqual(cursor, calls == 1 ? nil : "next")
            return try self.response(request, 200, self.profilePage(boot, draft.person, [calls == 1 ? first : second], next: calls == 1 ? "next" : nil, broad: calls == 1 ? "9" : "10"))
        }
        let refreshed = try await model.requalifyDetails(draft)
        XCTAssertEqual(calls, 2)
        XCTAssertEqual(refreshed.current?["contacts"].list, [first, second])
        XCTAssertEqual(refreshed.proposal, draft.proposal)
        XCTAssertEqual(refreshed.baseline, draft.baseline)
        XCTAssertEqual(try store.activePeople().first?.revision, "1", "Conflict reads never promote the downloaded Person.")
    }
    func testMobile005ConflictWithInvalidCurrentPagesRetainsExactEnvelope() async throws {
        let (model, api, _, boot, store) = try await setupModel(details: true)
        let draft = try profileDraft(store)
        let envelope = try store.submit(draft)
        let original = try XCTUnwrap(store.queue().first { $0.id == envelope.operation_id }?.bytes)
        var detailCalls = 0
        api.responseForTesting = { request in
            if request.url!.path.hasSuffix("/operations") { return try self.response(request, 409, .object(["error": .s("revision_conflict")])) }
            if request.url!.path.hasSuffix("/details") {
                detailCalls += 1
                if detailCalls > 3 { XCTFail("Unbounded conflict loop"); return try self.response(request, 503, .object([:])) }
                return try self.response(request, 200, self.profilePage(boot, draft.person, [self.profileContact()], next: "same"))
            }
            return try self.response(request, 503, .object(["error": .s("unavailable")]))
        }
        model.paused = false; await model.sync()
        XCTAssertEqual(detailCalls, 2)
        let queued = try XCTUnwrap(store.queue().first { $0.id == envelope.operation_id })
        XCTAssertEqual(queued.bytes, original); XCTAssertEqual(queued.status, "conflict")
        let protected = try XCTUnwrap(store.draftForOperation(envelope.operation_id))
        XCTAssertEqual(protected.proposal, draft.proposal); XCTAssertEqual(protected.baseline, draft.baseline)
        XCTAssertNil(protected.current)
    }
    func testAmbiguousOperationAndReceipt404KeepExactEditEnvelopeAndProtectedDraft() async throws {
        let (model, api, _, _, store) = try await setupModel()
        let (draft, bytes) = try queuedEdit(store, text: "Protected 404 proposal")
        // A POST 404 and a later receipt route 404 are deliberately ambiguous.
        // The production model may fence uploads, but it must not mint an ID,
        // discard the encrypted baseline/proposal, or expose a replacement.
        api.responseForTesting = { request in
            let operation = request.url!.path.hasSuffix("/operations")
            let receipt = request.url!.path.contains("/operations/")
            return try self.response(request, operation || receipt ? 404 : 503, .object(["error": .s(operation || receipt ? "not_found" : "unavailable")]))
        }
        model.paused = false; await model.sync()
        XCTAssertTrue(model.unlocked)
        XCTAssertEqual(try store.queue().last?.id, try decode(Envelope.self, bytes).operation_id)
        XCTAssertEqual(try store.queue().last?.bytes, bytes)
        XCTAssertEqual(try store.queue().last?.status, "unavailable")
        let retained = try XCTUnwrap(store.drafts().first { $0.id == draft.id })
        XCTAssertEqual(retained.text, "Protected 404 proposal")
        XCTAssertEqual(retained.baseline?["body"].text, "Baseline text")
    }
    func testPermissionDenialHidesModelPlaintextButRetainsProtectedEdit() async throws {
        let (model, api, secure, _, store) = try await setupModel()
        let (draft, bytes) = try queuedEdit(store, text: "Permission retained proposal")
        api.responseForTesting = { try self.response($0, 403, .object(["error": .s("forbidden")])) }
        model.paused = false; await model.sync()
        XCTAssertFalse(model.unlocked)
        XCTAssertTrue(model.people.isEmpty); XCTAssertTrue(model.queue.isEmpty); XCTAssertTrue(model.drafts.isEmpty)
        XCTAssertTrue(try secure.isLocked())
        XCTAssertEqual(try store.queue().last?.bytes, bytes)
        XCTAssertEqual(try store.drafts().first { $0.id == draft.id }?.text, "Permission retained proposal")
    }
    func testAcceptedContactForcesSameRevisionTodaySealWithoutSecondUpload() async throws {
        let (model, api, _, boot, store) = try await setupModel()
        let (_, bytes) = try queuedContact(store)
        let contact = try decode(Envelope.self, bytes)
        let person = contact.person
        let fresh = Generation(generation_id: UUID().uuidString, context_id: boot.context_id, evaluated_at: stamp(), expires_at: stamp(Date().addingTimeInterval(1800)), complete: true, selected_count: 1, manifest: Manifest(items: [ManifestItem(person_id: person, revision: "1", reasons: ["assigned"])], next_cursor: nil, complete: true))
        var operationCalls = 0
        api.responseForTesting = { request in
            if request.url!.path.hasSuffix("/operations") {
                operationCalls += 1
                return try self.response(request, 200, .object(["operation_id": .s(contact.operation_id), "outcome": .s("accepted"), "resource_type": .s("contact_attempt"), "resource_id": .s(UUID().uuidString), "committed_revision": .null, "person_revision": .s("1"), "accepted_at": .s(stamp()), "changed": .bool(true), "replayed": .bool(false)]))
            }
            if request.url!.path.hasSuffix("/reconciliations") { return try self.response(request, 200, try decode(JSON.self, encode(fresh))) }
            if request.url!.path.hasSuffix("/seal") { return try self.response(request, 200, try decode(JSON.self, encode(Seal(generation_id: fresh.generation_id, context_id: boot.context_id, sealed_at: stamp(), evaluated_at: fresh.evaluated_at, selected_count: 1, today: .object(["items": .array([])])))) ) }
            XCTFail("Unexpected request \(request.url!.path)")
            return try self.response(request, 500, .object([:]))
        }
        model.paused = false; await model.sync()
        XCTAssertEqual(operationCalls, 1)
        XCTAssertEqual(try store.queue().last?.status, "accepted")
        XCTAssertFalse(model.todayIsStale)
        XCTAssertEqual(try store.meta("active"), fresh.generation_id)
    }
    func testContactReceiptSupersedesPreReceiptStagingAndRelaunchDoesNotResubmit() async throws {
        let (model, api, secure, boot, store) = try await setupModel()
        let (_, bytes) = try queuedContact(store); let contact = try decode(Envelope.self, bytes)
        let preReceipt = Generation(generation_id: UUID().uuidString, context_id: boot.context_id, evaluated_at: stamp(), expires_at: stamp(Date().addingTimeInterval(1800)), complete: true, selected_count: 1, manifest: Manifest(items: [ManifestItem(person_id: contact.person, revision: "1", reasons: ["assigned"])], next_cursor: nil, complete: true))
        try store.begin(preReceipt)
        var uploads = 0, reconciliation = 0, failSeal = true; var issuedGeneration = ""
        api.responseForTesting = { request in
            if request.url!.path.hasSuffix("/operations") { uploads += 1; return try self.response(request, 200, .object(["operation_id": .s(contact.operation_id), "outcome": .s("accepted"), "resource_type": .s("contact_attempt"), "resource_id": .s(UUID().uuidString), "committed_revision": .null, "person_revision": .s("1"), "accepted_at": .s(stamp()), "changed": .bool(true), "replayed": .bool(false)])) }
            if request.url!.path.hasSuffix("/reconciliations") { reconciliation += 1; let fresh = Generation(generation_id: UUID().uuidString, context_id: boot.context_id, evaluated_at: stamp(), expires_at: stamp(Date().addingTimeInterval(1800)), complete: true, selected_count: 1, manifest: preReceipt.manifest); issuedGeneration = fresh.generation_id; return try self.response(request, 200, try decode(JSON.self, encode(fresh))) }
            if request.url!.path.hasSuffix("/seal") { if failSeal { return try self.response(request, 503, .object(["error": .s("unavailable")])) }; return try self.response(request, 200, try decode(JSON.self, encode(Seal(generation_id: issuedGeneration, context_id: boot.context_id, sealed_at: stamp(), evaluated_at: stamp(), selected_count: 1, today: .object(["items": .array([])]))))) }
            return try self.response(request, 500, .object([:]))
        }
        model.paused = false; await model.sync(); XCTAssertEqual(uploads, 1); XCTAssertTrue(model.todayIsStale); XCTAssertNotEqual(try store.generation()?.generation_id, preReceipt.generation_id)
        failSeal = false; let next = reopen(secure, api); next.paused = false; await next.sync(manual: true)
        XCTAssertEqual(uploads, 1); XCTAssertGreaterThanOrEqual(reconciliation, 2); XCTAssertFalse(next.todayIsStale)
    }
    func testContactRefreshFailureRetainsAcceptedReceiptAndDoesNotResubmit() async throws {
        let (model, api, _, boot, store) = try await setupModel()
        let (_, bytes) = try queuedContact(store)
        let contact = try decode(Envelope.self, bytes)
        var operationCalls = 0
        api.responseForTesting = { request in
            if request.url!.path.hasSuffix("/operations") {
                operationCalls += 1
                return try self.response(request, 200, .object(["operation_id": .s(contact.operation_id), "outcome": .s("accepted"), "resource_type": .s("contact_attempt"), "resource_id": .s(UUID().uuidString), "committed_revision": .null, "person_revision": .s("1"), "accepted_at": .s(stamp()), "changed": .bool(true), "replayed": .bool(false)]))
            }
            if request.url!.path.hasSuffix("/reconciliations") { return try self.response(request, 503, .object(["error": .s("unavailable")])) }
            return try self.response(request, 500, .object([:]))
        }
        model.paused = false; await model.sync()
        XCTAssertEqual(operationCalls, 1); XCTAssertEqual(try store.queue().last?.status, "accepted")
        XCTAssertTrue(model.todayIsStale); XCTAssertEqual(try store.queue().last?.bytes, bytes)
        await model.sync(manual: true)
        XCTAssertEqual(operationCalls, 1, "A failed Today refresh must never upload an accepted contact again")
    }
    func testContactFutureRejectionKeepsOriginalAndUsesNewDraftIdentity() async throws {
        let (model, api, _, _, store) = try await setupModel()
        let (_, bytes) = try queuedContact(store)
        api.responseForTesting = { request in
            if request.url!.path.hasSuffix("/operations") { return try self.response(request, 422, .object(["error": .s("contact_time_in_future")])) }
            return try self.response(request, 503, .object(["error": .s("unavailable")]))
        }
        model.paused = false; await model.sync()
        let original = try XCTUnwrap(store.queue().last)
        XCTAssertEqual(original.status, "attention"); XCTAssertEqual(original.error, "contact_time_in_future"); XCTAssertEqual(original.bytes, bytes)
        let revised = try model.revisedContactDraft(from: original)
        XCTAssertNotEqual(revised.id, original.id); XCTAssertEqual(revised.occurredAt, original.envelope.payload["occurred_at"].text)
    }
    func testReconciliationAuthority403HidesOldCacheAndPersistsLockAcrossRelaunch() async throws {
        let (model, api, secure, _, store) = try await setupModel()
        api.responseForTesting = { try self.response($0, 403, .object(["error": .s("forbidden")])) }
        model.paused = false; await model.sync()
        XCTAssertFalse(model.unlocked); XCTAssertTrue(model.people.isEmpty); XCTAssertTrue(try secure.isLocked())
        XCTAssertEqual(try store.activePeople().count, 1)
        XCTAssertFalse(reopen(secure, api).unlocked)
    }
    func testTaskPermission403RetainsVisibleCacheAndNeedsAttentionAfterValidAuthorityRecheck() async throws {
        let (model, api, _, boot, store) = try await setupModel(); try queue(store)
        var authorityChecks = 0
        api.responseForTesting = { request in
            if request.url!.path == "/api/me" {
                authorityChecks += 1
                return try self.response(request, 200, .object(["user": .object(["id": .s(boot.actor_user_id)]), "organization": .object(["id": .s(boot.organization_id), "workspace_mode": .s("operational")])]))
            }
            let operation = request.url!.path.hasSuffix("/operations")
            return try self.response(request, operation ? 403 : 503, .object(["error": .s(operation ? "forbidden" : "unavailable")]))
        }
        model.paused = false; await model.sync()
        XCTAssertEqual(authorityChecks, 1); XCTAssertTrue(model.unlocked); XCTAssertEqual(model.people.count, 1)
        XCTAssertEqual(try store.queue()[0].status, "unavailable"); XCTAssertEqual(try store.queue()[0].error, "forbidden")
    }
    func testOperation403WithRevokedAuthorityLocksAndPreservesExactEnvelope() async throws {
        let (model, api, secure, _, store) = try await setupModel(); try queue(store)
        let before = try store.queue()[0].bytes
        api.responseForTesting = { try self.response($0, 403, .object(["error": .s("forbidden")])) }
        model.paused = false; await model.sync()
        XCTAssertFalse(model.unlocked); XCTAssertTrue(model.people.isEmpty); XCTAssertTrue(model.queue.isEmpty)
        XCTAssertEqual(try store.queue()[0].bytes, before); XCTAssertFalse(reopen(secure, api).unlocked)
    }
    func testUnsupportedProtocolCannotBeClearedByPauseToggleOrRelaunch() async throws {
        let (model, api, secure, _, _) = try await setupModel()
        var requests = 0
        api.responseForTesting = { request in requests += 1; return try self.response(request, 409, .object(["error": .s("protocol_unsupported")])) }
        model.paused = false; await model.sync(); XCTAssertTrue(model.updateRequired)
        let before = requests
        model.paused = true; model.paused = false; await model.sync(); XCTAssertEqual(requests, before)
        let next = reopen(secure, api); XCTAssertTrue(next.unlocked); XCTAssertTrue(next.updateRequired)
        next.paused = false; await next.sync(); XCTAssertEqual(requests, before)
    }
    func testLockSurvivesActualModelCredentialReplacementFailureWithPendingWork() async throws {
        let (model, api, secure, _, store) = try await setupModel(); try queue(store)
        let bytes = try store.queue()[0].bytes
        secure.failCredentialWritesForTesting = true
        model.lock("Controlled sign-out during credential write failure")
        XCTAssertFalse(model.unlocked); XCTAssertTrue(try secure.isLocked())
        XCTAssertFalse(reopen(secure, api).unlocked)
        XCTAssertEqual(try store.queue()[0].bytes, bytes)
        secure.failCredentialWritesForTesting = false
    }
    func testRetiredGeneration404DiscardsOnlyStagingThenReusesCompleteBundle() async throws {
        let (model, api, _, boot, store) = try await setupModel()
        let person = try XCTUnwrap(store.activePeople().first?.person)
        _ = try store.saveDraft(Draft(id: UUID().uuidString, person: person, kind: "add_note", text: "Retained synthetic draft", revision: 0))
        let old = Generation(generation_id: UUID().uuidString, context_id: boot.context_id, evaluated_at: stamp(), expires_at: stamp(Date().addingTimeInterval(1800)), complete: true, selected_count: 1, manifest: Manifest(items: [ManifestItem(person_id: person, revision: "1", reasons: ["assigned"])], next_cursor: nil, complete: true))
        try store.begin(old)
        api.responseForTesting = { try self.response($0, 404, .object(["error": .s("not_found")])) }
        model.paused = false; await model.sync()
        XCTAssertNil(try store.generation()); XCTAssertTrue(model.unlocked)
        XCTAssertEqual(try store.activePeople().count, 1); XCTAssertEqual(try store.drafts().count, 1)
        let fresh = Generation(generation_id: UUID().uuidString, context_id: boot.context_id, evaluated_at: stamp(), expires_at: stamp(Date().addingTimeInterval(1800)), complete: true, selected_count: 1, manifest: old.manifest)
        var componentReads = 0
        api.responseForTesting = { request in
            let payload: Data
            if request.url!.path.hasSuffix("/seal") { payload = try encode(Seal(generation_id: fresh.generation_id, context_id: boot.context_id, sealed_at: stamp(), evaluated_at: stamp(), selected_count: 1, today: .object(["items": .array([])]))) }
            else if request.url!.path.hasSuffix("/reconciliations") { payload = try encode(fresh) }
            else { componentReads += 1; return try self.response(request, 500, .object([:])) }
            return (payload, HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: nil)!)
        }
        await model.sync(manual: true)
        XCTAssertEqual(componentReads, 0); XCTAssertEqual(try store.meta("active"), fresh.generation_id)
        XCTAssertEqual(try store.drafts().count, 1)
    }

    #if MOBILE003_QA
    func testHistoricalSchemaFiveInstalledInventoryMigratesWithoutChangingBytes() async throws {
        guard ProcessInfo.processInfo.environment["CRM_MOBILE003_INSTALLED_UPGRADE"] == "approved-synthetic" else {
            throw XCTSkip("Requires the coordinator-created isolated installed schema-5 QA fixture.")
        }
        let originalInstallation = appDefaults.string(forKey: "installation")
        defer { if let originalInstallation { appDefaults.set(originalInstallation, forKey: "installation") } else { appDefaults.removeObject(forKey: "installation") } }
        appDefaults.set("c1f05e41-0581-4a64-80b0-8547b576399b", forKey: "installation")
        let model = FieldModel(synthetic: true, startMonitor: false, restoreOnInit: false)
        model.paused = true
        await model.signIn(email: "agent@mobile.test", password: "Mobile-demo-only-123!")
        XCTAssertTrue(model.unlocked, model.message)
        let store = try XCTUnwrap(model.store)
        let pre = try String(contentsOf: model.mobile003QAInventoryURL())
        XCTAssertTrue(pre.contains("schema=5"), pre)
        let active = try XCTUnwrap(pre.components(separatedBy: " active=").last?.components(separatedBy: " ops=").first)
        XCTAssertEqual(try store.meta("active"), active)
        XCTAssertEqual(try store.rows("PRAGMA user_version")[0][0], "6")
        func digest(_ data: Data) -> String { String(data.reduce(1469598103934665603) { ($0 ^ UInt64($1)) &* 1099511628211 }, radix: 16) }
        let expectedOps = pre.components(separatedBy: " ops=").dropFirst().first?.components(separatedBy: " drafts=").first?.split(separator: ",") ?? []
        let queue = try store.queue(); XCTAssertEqual(queue.count, expectedOps.count)
        XCTAssertEqual(queue.filter { $0.receipt != nil }.count, expectedOps.filter { $0.hasSuffix(":1") }.count)
        XCTAssertEqual(queue.filter { $0.status == "pending" }.count, expectedOps.filter { $0.hasSuffix(":0") }.count)
        for part in expectedOps {
            let fields = part.split(separator: ":")
            let op = try XCTUnwrap(queue.first { $0.id == fields[0] })
            XCTAssertEqual(digest(op.bytes), String(fields[1])); XCTAssertEqual(op.receipt == nil ? "0" : "1", String(fields[2]))
        }
        let draftPart = pre.components(separatedBy: " drafts=").last!.split(separator: ":")
        XCTAssertEqual(try store.drafts().count, 1)
        let draft = try XCTUnwrap(try store.drafts().first { $0.id == draftPart[0] })
        XCTAssertEqual(digest(try encode(draft)), String(draftPart[1]))
    }
    #endif

    #if MOBILE004_UPGRADE_QA
    func testMobile004OpensActualInstalledMobile003StoreWithoutChangingProtectedRows() async throws {
        let inventoryURL = try SecureStorage.directory(synthetic: true).appendingPathComponent("mobile004-upgrade-inventory.txt")
        let pre = try String(contentsOf: inventoryURL)
        XCTAssertTrue(pre.contains("schema=6"), pre)
        appDefaults.set("2f6db9e9-f1cc-4b90-8a8b-46572cdc4768", forKey: "installation")
        let model = FieldModel(synthetic: true, startMonitor: false, restoreOnInit: false)
        model.paused = true
        await model.signIn(email: "agent@mobile.test", password: "Mobile-demo-only-123!")
        XCTAssertTrue(model.unlocked, model.message)
        let store = try XCTUnwrap(model.store)
        XCTAssertEqual(try store.rows("PRAGMA user_version")[0][0], "7")
        func digest(_ data: Data) -> String { String(data.reduce(1469598103934665603) { ($0 ^ UInt64($1)) &* 1099511628211 }, radix: 16) }
        let identity = try XCTUnwrap(model.credential?.bootstrap.identity)
        let protectedKey = try SecureStorage(synthetic: true).key(for: identity, existingFile: true)
        let expectedKey = pre.components(separatedBy: " key=").dropFirst().first?.components(separatedBy: " ops=").first
        XCTAssertEqual(expectedKey, digest(protectedKey), "The existing SQLCipher key must be retained; migration must not replace it.")
        let expectedOps = pre.components(separatedBy: " ops=").dropFirst().first?.components(separatedBy: " drafts=").first?.split(separator: ",") ?? []
        let queue = try store.queue(); XCTAssertEqual(queue.count, expectedOps.count)
        for part in expectedOps {
            let fields = part.split(separator: ":")
            let op = try XCTUnwrap(queue.first { $0.id == fields[0] })
            XCTAssertEqual(digest(op.bytes), String(fields[1]))
            XCTAssertEqual(op.receipt == nil ? "0" : "1", String(fields[2]))
        }
        let expectedDrafts = pre.components(separatedBy: " drafts=").dropFirst().first?.components(separatedBy: " person=").first?.split(separator: ",") ?? []
        XCTAssertEqual(try store.drafts().count, expectedDrafts.count)
        for part in expectedDrafts {
            let fields = part.split(separator: ":")
            let draft = try XCTUnwrap(try store.drafts().first { $0.id == fields[0] })
            XCTAssertEqual(digest(try encode(draft)), String(fields[1]))
        }
        let person = pre.components(separatedBy: " person=").last ?? ""
        XCTAssertNotNil(try store.activeBundle(person), "The installed encrypted cache and its original key remain available.")
    }
    #endif

    #if MOBILE005_UPGRADE_QA
    /// Opens the data written by the archived Mobile004 app after this app has
    /// been installed over the same isolated bundle.  The schema-8 migration is
    /// additive, so this asserts the prior SQLCipher key and every protected
    /// operation/draft byte exactly, rather than merely checking row counts.
    func testMobile005OpensActualInstalledSchemaSevenStoreWithoutChangingProtectedRows() throws {
        let identity = "aa6004f1-894f-448b-aa1a-bde28cc66acd_856340c3-3e5b-4776-89eb-e0c225306505"
        let context = "mobile005-installed-schema-seven-context"
        let directory = try SecureStorage.directory(synthetic: true)
        let pre = try String(contentsOf: directory.appendingPathComponent("mobile005-schema-seven-inventory.txt"))
        XCTAssertTrue(pre.hasPrefix("schema=7 key="), pre)
        func field(_ left: String, _ right: String? = nil) -> String {
            let suffix = pre.components(separatedBy: left).dropFirst().first ?? ""
            return right.map { String(suffix.components(separatedBy: $0).first ?? "") } ?? String(suffix)
        }
        let secure = SecureStorage(synthetic: true)
        let path = directory.appendingPathComponent(identity + ".sqlite")
        XCTAssertTrue(FileManager.default.fileExists(atPath: path.path), "The archived app must be installed and create its protected store first.")
        let key = try secure.key(for: identity, existingFile: true)
        let store = try LocalStore(url: path, key: key, identity: identity, context: context)
        XCTAssertEqual(try store.rows("PRAGMA user_version")[0][0], "8")
        func digest(_ data: Data) -> String { String(data.reduce(1469598103934665603) { ($0 ^ UInt64($1)) &* 1099511628211 }, radix: 16) }
        XCTAssertEqual(field("key=", " ops="), digest(key), "The installed SQLCipher key must survive the schema-8 migration.")

        let expectedOperations = field(" ops=", " drafts=").split(separator: ",").map(String.init)
        let queue = try store.queue(); XCTAssertEqual(queue.count, expectedOperations.count)
        for expected in expectedOperations {
            let parts = expected.split(separator: ":", maxSplits: 2).map(String.init)
            let op = try XCTUnwrap(queue.first { $0.id == parts[0] })
            XCTAssertEqual(op.bytes.base64EncodedString(), parts[1], "Immutable schema-7 envelope bytes changed during migration.")
            XCTAssertEqual(op.receipt == nil ? "0" : "1", parts[2])
        }
        let expectedDrafts = field(" drafts=", " person=").split(separator: ",").map(String.init)
        let drafts = try store.drafts(); XCTAssertEqual(drafts.count, expectedDrafts.count)
        for expected in expectedDrafts {
            let parts = expected.split(separator: ":", maxSplits: 1).map(String.init)
            let draft = try XCTUnwrap(drafts.first { $0.id == parts[0] })
            XCTAssertEqual((try encode(draft)).base64EncodedString(), parts[1], "Protected schema-7 draft bytes changed during migration.")
        }
        let person = field(" person=", " generation=")
        XCTAssertNotNil(try store.activeBundle(person), "The old cached bundle remains readable, although it is not details-qualified.")
        XCTAssertFalse(try store.hasQualifiedDetailsBundle(person, "2"), "Schema-7 cache cannot silently acquire a details qualification token.")
    }
    #endif

}
