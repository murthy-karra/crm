import XCTest
@testable import FieldCRM

@MainActor final class LiveAPITests: XCTestCase {
    func connect(installation: String = UUID().uuidString.lowercased(), email: String = "agent@mobile.test") async throws -> (API, Bootstrap) {
        let api = try API(base: "http://127.0.0.1:3101")
        try await api.login(email: email, password: "Mobile-demo-only-123!")
        let boot: Bootstrap = try await api.call("/bootstrap", method: "POST", body: .object(["protocol": .s("mobile-v1"), "installation_id": .s(installation)]))
        return (api, boot)
    }
    func testRealAPILostAcknowledgement100DurableActionsDependencyConflictAndAccountIsolation() async throws {
        let (api, boot) = try await connect()
        let generation: Generation = try await api.call("/reconciliations", method: "POST", body: .object(["protocol": .s("mobile-v1"), "installation_id": .s(boot.installation_id), "pinned_person_ids": .array([])]), context: boot.context_id)
        XCTAssertEqual(generation.selected_count, 100)
        var target: String?
        for item in generation.manifest.items {
            let page: Page = try await api.call("/reconciliations/\(generation.generation_id)/people/\(item.person_id)/summary", context: boot.context_id)
            if page.summary?["last_name"].text == "080" { target = item.person_id; break }
        }
        let person = try XCTUnwrap(target)
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appendingPathComponent("live.sqlite"), key = Data(repeating: 77, count: 32)
        var store: LocalStore? = try LocalStore(url: url, key: key, identity: boot.identity, context: boot.context_id)
        let label = "iOS-" + UUID().uuidString
        for index in 0..<98 {
            let draft = Draft(id: UUID().uuidString, person: person, kind: "add_note", text: label + "-" + String(index), revision: 0)
            try store!.submit(store!.saveDraft(draft))
        }
        let create = try store!.submit(store!.saveDraft(Draft(id: UUID().uuidString, person: person, kind: "create_task", text: label + "-task", revision: 0)))
        try store!.complete(person: person, target: .object(["created_by_operation_id": .s(create.operation_id)]))
        let stable = try store!.queue().map(\.bytes)
        XCTAssertEqual(stable.count, 100)
        store = nil
        store = try LocalStore(url: url, key: key, identity: boot.identity, context: boot.context_id)
        XCTAssertEqual(try store!.queue().map(\.bytes), stable)
        api.dropNextOperationResponse = true
        do { _ = try await api.operation(stable[0], context: boot.context_id); XCTFail("Expected actual response drop") }
        catch LocalError.lostResponse { }
        let receipt = try await api.operation(stable[0], context: boot.context_id)
        XCTAssertTrue(receipt.replayed); try store!.acknowledge(receipt)
        let lookup: Receipt = try await api.call("/operations/" + receipt.operation_id, context: boot.context_id)
        XCTAssertEqual(lookup.resource_id, receipt.resource_id)
        for op in try store!.queue() where op.status == "pending" { try store!.acknowledge(await api.operation(op.bytes, context: boot.context_id)) }
        let accepted = try store!.queue()
        XCTAssertEqual(accepted.filter { $0.status == "accepted" }.count, 100)
        let created = try XCTUnwrap(accepted.first { $0.id == create.operation_id }?.receipt)
        let completion = try XCTUnwrap(accepted.last?.receipt)
        XCTAssertEqual(created.resource_id, completion.resource_id)
        XCTAssertNotEqual(created.committed_revision, completion.committed_revision)
        // A stale newly proposed completion must conflict; it cannot replay another ID's receipt.
        let stale = Envelope(context_id: boot.context_id, operation_id: UUID().uuidString.lowercased(), kind: "complete_task", device_recorded_at: stamp(), payload: .object(["person_id": .s(person), "target": .object(["task_id": .s(created.resource_id), "expected_revision": .s(created.committed_revision!)])]))
        do { _ = try await api.operation(encode(stale), context: boot.context_id); XCTFail("Expected revision conflict") }
        catch let error as APIError { XCTAssertEqual(error.code, "revision_conflict") }
        // Changed payload under the old ID never creates another note.
        let first = accepted[0].envelope
        let changed = Envelope(context_id: first.context_id, operation_id: first.operation_id, kind: first.kind, device_recorded_at: first.device_recorded_at, payload: .object(["person_id": .s(person), "body": .s("changed synthetic body")]))
        do { _ = try await api.operation(encode(changed), context: boot.context_id); XCTFail("Expected mismatch") }
        catch let error as APIError { XCTAssertEqual(error.code, "operation_payload_mismatch") }
        let (second, secondBoot) = try await connect(email: "second@mobile.test")
        do { let _: Receipt = try await second.call("/operations/" + receipt.operation_id, context: secondBoot.context_id); XCTFail("Cross-actor receipt must not be exposed") }
        catch let error as APIError { XCTAssertEqual(error.status, 404) }
        do { _ = try await second.operation(stable[0], context: secondBoot.context_id); XCTFail("Context mismatch must be denied") }
        catch let error as APIError { XCTAssertEqual(error.status, 401) }
        store = nil
        store = try LocalStore(url: url, key: key, identity: boot.identity, context: boot.context_id)
        XCTAssertEqual(try store!.queue().filter { $0.status == "accepted" }.count, 100)
        XCTAssertEqual(try store!.queue()[0].bytes, stable[0])
    }
}
