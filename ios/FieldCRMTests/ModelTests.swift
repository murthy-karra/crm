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
    func setupModel() async throws -> (FieldModel, API, SecureStorage, Bootstrap, LocalStore) {
        let model = FieldModel(synthetic: true, startMonitor: false, restoreOnInit: false)
        let secure = SecureStorage(synthetic: true, testingNamespace: UUID().uuidString); secure.testDirectory = directory
        let api = try API(base: "http://127.0.0.1:3101")
        let boot = Bootstrap(context_id: UUID().uuidString, installation_id: model.installation, actor_user_id: UUID().uuidString, organization_id: UUID().uuidString,
                             workspace_revision: "1", authorized_at: stamp(), offline_access_expires_at: stamp(Date().addingTimeInterval(7 * 86400)), server_time: stamp(), capabilities: ["add_note", "create_task", "complete_task", "reconciliation"], protocol: "mobile-v1")
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
        XCTAssertEqual(try store.queue()[0].status, "attention"); XCTAssertEqual(try store.queue()[0].error, "forbidden")
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

}
