import XCTest
@testable import FieldCRM

@MainActor final class LiveAPITests: XCTestCase {
    func connect(installation: String = UUID().uuidString.lowercased(), email: String = "agent@mobile.test") async throws -> (API, Bootstrap) {
        #if MOBILE006_QA || MOBILE006_UPGRADE_QA
        let api = try API(base: "http://127.0.0.1:3106")
        #elseif MOBILE005_QA || MOBILE005_UPGRADE_QA
        let api = try API(base: "http://127.0.0.1:3103")
        #elseif MOBILE002_QA || MOBILE003_QA || MOBILE004_QA
        let api = try API(base: "http://127.0.0.1:3102")
        #else
        let api = try API(base: "http://127.0.0.1:3101")
        #endif
        try await api.login(email: email, password: "Mobile-demo-only-123!")
        let boot: Bootstrap = try await api.call("/bootstrap", method: "POST", body: .object(["protocol": .s("mobile-v1"), "installation_id": .s(installation)]))
        return (api, boot)
    }
    #if MOBILE005_QA
    func testMobile005RealDetailsLostResponseReplayConflictAndCurrentTraversal() async throws {
        let installation = "6c8637a4-99f7-45d6-9c11-201f5c4d447a"
        let (api, boot): (API, Bootstrap)
        do { (api, boot) = try await connect(installation: installation) }
        catch { XCTFail("bootstrap failed: \(error)"); return }
        XCTAssertTrue(boot.capabilities.contains("update_person_details")); XCTAssertTrue(boot.capabilities.contains("details_revisions"))
        // The coordinator seeds Person001 only for this lane; the configured
        // installation is stable so retries do not consume reconciliation capacity.
        let person = "07cb08d0-56c3-43c1-a538-68573e5a24f0"
        let current: CurrentDetailsResponse
        do { current = try await api.currentDetails(person: person, context: boot.context_id) }
        catch { XCTFail("current profile read failed: \(error)"); return }
        XCTAssertTrue(current.complete); _ = try revision(current.details_revision)
        let original = current.items.first
        let operations: [JSON] = [
            .object(["op": .s("add"), "kind": .s("email"), "value": .s("ios.mobile005." + UUID().uuidString.lowercased() + "@example.test")]),
            .object(["op": .s("add"), "kind": .s("phone"), "value": .s("(555) 555-0109")])
        ]
        let envelope = Envelope(context_id: boot.context_id, operation_id: UUID().uuidString.lowercased(), kind: "update_person_details", device_recorded_at: stamp(), payload: .object(["person_id": .s(person), "expected_details_revision": .s(current.details_revision), "first_name": .s("iOS Mobile005"), "contact_operations": .array(operations)]))
        let bytes = try encode(envelope)
        api.dropNextOperationResponse = true
        do { _ = try await api.operation(bytes, context: boot.context_id); XCTFail("accepted response must be deliberately lost") } catch LocalError.lostResponse { } catch { XCTFail("initial details submission failed: \(error)"); return }
        let replay: Receipt
        do { replay = try await api.operation(bytes, context: boot.context_id) } catch { XCTFail("exact replay failed: \(error)"); return }
        XCTAssertTrue(replay.replayed); XCTAssertEqual(replay.resource_type, "person_details"); XCTAssertEqual(replay.added_contact_ids?.map(\.ordinal), [0, 1])
        let after: CurrentDetailsResponse
        do { after = try await api.currentDetails(person: person, context: boot.context_id) } catch { XCTFail("post-accept read failed: \(error)"); return }
        XCTAssertEqual(after.details_revision, replay.committed_revision)
        let other = try await connect(installation: "b9a38d7e-3f64-4b6a-8cdf-211a220d5be3", email: "second@mobile.test")
        let competing = Envelope(context_id: other.1.context_id, operation_id: UUID().uuidString.lowercased(), kind: "update_person_details", device_recorded_at: stamp(), payload: .object(["person_id": .s(person), "expected_details_revision": .s(after.details_revision), "last_name": .s("Second Actor"), "contact_operations": .array([])]))
        _ = try await other.0.operation(try encode(competing), context: other.1.context_id)
        let stale = Envelope(context_id: boot.context_id, operation_id: UUID().uuidString.lowercased(), kind: "update_person_details", device_recorded_at: stamp(), payload: .object(["person_id": .s(person), "expected_details_revision": .s(after.details_revision), "last_name": .s("Must conflict"), "contact_operations": .array([])]))
        do { _ = try await api.operation(try encode(stale), context: boot.context_id); XCTFail("stale profile revision must conflict") }
        catch let error as APIError { XCTAssertEqual(error.code, "revision_conflict") }
        if let original {
            let final = try await api.currentDetails(person: person, context: boot.context_id)
            XCTAssertTrue(final.items.contains { $0["id"].text == original["id"].text })
        }
    }
    #endif
    #if MOBILE006_QA
    func testMobile006RealMetadataLostResponseReplayCurrentAndRevisionConflict() async throws {
        let installation = "f37e2c31-99b5-41f6-846e-8f16b1e4c013"
        let (api, boot) = try await connect(installation: installation)
        XCTAssertTrue(["update_person_metadata", "metadata_revisions", "metadata_catalog"].allSatisfy(boot.capabilities.contains))
        let person = "923e2821-8644-487f-8fda-115027ddc226"
        let before = try await api.currentMetadata(person: person, context: boot.context_id)
        XCTAssertTrue(before.complete)
        _ = try revision(before.metadata_revision); _ = try revision(before.catalog_revision)
        let tag = "28eefc0b-87a2-49bc-ac36-9f0e30985113"
        let text = "d9d979a3-5015-4cc9-9252-a58e1d109548"
        let number = "b162603c-06a2-4325-9481-7bb5538fa907"
        let date = "a7d8c964-1423-42a1-ac32-d34446f96a0a"
        let choice = "ecc89596-43a6-40d9-99ca-17bc4afbd0b1"
        let north = "be0f7bee-a376-47ba-8faf-d17563bf5625"
        let unique = UUID().uuidString.lowercased()
        var actions: [JSON] = [
            .object(["kind": .s("set_field"), "field_id": .s(text), "value": .object(["text": .s("iOS Mobile006 " + unique)])]),
            .object(["kind": .s("set_field"), "field_id": .s(number), "value": .object(["number": .s("123.4500")])]),
            .object(["kind": .s("set_field"), "field_id": .s(date), "value": .object(["date": .s("2026-09-14")])]),
            .object(["kind": .s("set_field"), "field_id": .s(choice), "value": .object(["option_id": .s(north)])])
        ]
        if !before.tags.contains(where: { $0["id"].text == tag }) { actions.insert(.object(["kind": .s("add_tag"), "tag_id": .s(tag)]), at: 0) }
        let envelope = Envelope(context_id: boot.context_id, operation_id: UUID().uuidString.lowercased(), kind: "update_person_metadata", device_recorded_at: stamp(), payload: .object(["person_id": .s(person), "expected_metadata_revision": .s(before.metadata_revision), "expected_catalog_revision": .s(before.catalog_revision), "actions": .array(actions)]))
        let bytes = try encode(envelope)
        api.dropNextOperationResponse = true
        do { _ = try await api.operation(bytes, context: boot.context_id); XCTFail("accepted metadata response must be deliberately lost") }
        catch LocalError.lostResponse { }
        let replay = try await api.operation(bytes, context: boot.context_id)
        XCTAssertTrue(replay.replayed); XCTAssertEqual(replay.resource_type, "person_metadata")
        let after = try await api.currentMetadata(person: person, context: boot.context_id)
        XCTAssertEqual(after.metadata_revision, replay.committed_revision)
        XCTAssertTrue(after.values.contains { $0["field_id"].text == text && $0["value"]["text"].text == "iOS Mobile006 " + unique })
        XCTAssertTrue(after.values.contains { $0["field_id"].text == number && $0["value"]["number"].text == "123.4500" })
        XCTAssertTrue(after.values.contains { $0["field_id"].text == date && $0["value"]["date"].text == "2026-09-14" })
        XCTAssertTrue(after.values.contains { $0["field_id"].text == choice && $0["value"]["option_id"].text == north })
        let other = try await connect(installation: "8a4f3c1d-46f2-4afc-aa90-e0d2ab581d76", email: "second@mobile.test")
        let competing = Envelope(context_id: other.1.context_id, operation_id: UUID().uuidString.lowercased(), kind: "update_person_metadata", device_recorded_at: stamp(), payload: .object(["person_id": .s(person), "expected_metadata_revision": .s(after.metadata_revision), "expected_catalog_revision": .s(after.catalog_revision), "actions": .array([.object(["kind": .s("set_field"), "field_id": .s(text), "value": .object(["text": .s("second actor " + unique)])])])]))
        _ = try await other.0.operation(try encode(competing), context: other.1.context_id)
        let staleActions: [JSON] = [.object(["kind": .s("set_field"), "field_id": .s(number), "value": .object(["number": .s("9")])])]
        let stalePayload: JSON = .object(["person_id": .s(person), "expected_metadata_revision": .s(after.metadata_revision), "expected_catalog_revision": .s(after.catalog_revision), "actions": .array(staleActions)])
        let stale = Envelope(context_id: boot.context_id, operation_id: UUID().uuidString.lowercased(), kind: "update_person_metadata", device_recorded_at: stamp(), payload: stalePayload)
        do { _ = try await api.operation(try encode(stale), context: boot.context_id); XCTFail("stale metadata revision must conflict") }
        catch let error as APIError { XCTAssertEqual(error.code, "revision_conflict") }
    }
    #endif
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
    #if MOBILE004_QA
    func testMobile004RealStageLostResponseReplayConflictAndCurrentRead() async throws {
        let installation = "6a3fdca6-981a-4f20-a1fe-5bcf0d115b77"
        let (api, boot) = try await connect(installation: installation)
        let request: JSON = .object(["protocol": .s("mobile-v1"), "installation_id": .s(boot.installation_id), "pinned_person_ids": .array([]), "include_stage_catalog": .bool(true)])
        let generation: Generation = try await api.call("/reconciliations", method: "POST", body: request, context: boot.context_id)
        let catalog = try XCTUnwrap(generation.stage_catalog)
        XCTAssertEqual(catalog.stages_url, "/api/mobile/v1/reconciliations/\(generation.generation_id)/stages")
        var stages: [Stage] = [], cursor: String? = nil
        repeat {
            let suffix = cursor.map { "?cursor=" + API.cursor($0) } ?? ""
            let page: StagePage = try await api.call("/reconciliations/\(generation.generation_id)/stages\(suffix)", context: boot.context_id)
            XCTAssertEqual(page.revision, catalog.revision); stages += page.items; cursor = page.next_cursor
        } while cursor != nil
        XCTAssertFalse(stages.isEmpty)
        // A sealed server generation is reclaimable; the locally protected
        // outbox test below does not depend on retaining an unsealed server
        // download reservation.
        let _: Seal = try await api.call("/reconciliations/\(generation.generation_id)/seal", method: "POST", body: .object([:]), context: boot.context_id)
        let person = "13e8d47a-5648-4d88-9358-fe5ed400078d"
        let summary: Page = try await api.call("/reconciliations/\(generation.generation_id)/people/\(person)/summary", context: boot.context_id)
        let currentID = summary.summary?["stage"]["id"].text ?? ""
        let target = try XCTUnwrap(stages.first { $0.id != currentID })
        let expected = try XCTUnwrap(summary.summary?["stage_revision"].text)
        _ = try revision(expected)
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("mobile004-live-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true); defer { try? FileManager.default.removeItem(at: directory) }
        var store: LocalStore? = try LocalStore(url: directory.appendingPathComponent("store.sqlite"), key: Data(repeating: 83, count: 32), identity: boot.identity, context: boot.context_id)
        let draft = try store!.queueStage(person: person, baseline: Stage(id: currentID, name: summary.summary?["stage"]["name"].text ?? "", position: 0), stage: target, expected: expected)
        let bytes = try XCTUnwrap(store!.queue().last?.bytes)
        store = nil // process termination leaves the immutable operation intact.
        store = try LocalStore(url: directory.appendingPathComponent("store.sqlite"), key: Data(repeating: 83, count: 32), identity: boot.identity, context: boot.context_id)
        XCTAssertEqual(try store!.queue().last?.bytes, bytes); XCTAssertEqual(try store!.drafts().first { $0.id == draft.id }?.mode, "submitted")
        api.dropNextOperationResponse = true
        do { _ = try await api.operation(bytes, context: boot.context_id); XCTFail("accepted response is deliberately lost") } catch LocalError.lostResponse { }
        let replay = try await api.operation(bytes, context: boot.context_id)
        XCTAssertTrue(replay.replayed); XCTAssertEqual(replay.resource_type, "person_stage"); XCTAssertTrue(replay.changed)
        try store!.acknowledge(replay)
        let current = try await api.currentStage(person: person, context: boot.context_id)
        XCTAssertEqual(current.stage.id, target.id); XCTAssertEqual(current.stage_revision, replay.committed_revision)

        let (other, otherBoot) = try await connect(installation: "f3352fb4-93de-4301-9df6-9de80de12ae4", email: "second@mobile.test")
        let otherTarget = try XCTUnwrap(stages.first { $0.id != target.id })
        let replacement = Envelope(context_id: otherBoot.context_id, operation_id: UUID().uuidString.lowercased(), kind: "change_person_stage", device_recorded_at: stamp(), payload: .object(["person_id": .s(person), "stage_id": .s(otherTarget.id), "expected_stage_revision": .s(current.stage_revision)]))
        _ = try await other.operation(try encode(replacement), context: otherBoot.context_id)
        let stale = Envelope(context_id: boot.context_id, operation_id: UUID().uuidString.lowercased(), kind: "change_person_stage", device_recorded_at: stamp(), payload: .object(["person_id": .s(person), "stage_id": .s(target.id), "expected_stage_revision": .s(current.stage_revision)]))
        do { _ = try await api.operation(try encode(stale), context: boot.context_id); XCTFail("stale stage revision must conflict") }
        catch let error as APIError { XCTAssertEqual(error.code, "revision_conflict") }
        let conflictCurrent = try await api.currentStage(person: person, context: boot.context_id)
        XCTAssertEqual(conflictCurrent.stage.id, otherTarget.id)
    }
    #endif
    #if MOBILE003_QA
    func testMobile003RealContactReplayTwoIndependentLogsAndFutureRetention() async throws {
        // A fixed QA installation avoids consuming a new bounded server context
        // on test retries. Person001 is reserved exclusively for this iOS lane.
        let (api, boot) = try await connect(installation: "1a372241-5d93-4af5-88d7-3f21604b2a91")
        let person = "f40f5132-9822-4bdf-ba34-76affb181195"
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("mobile003-live-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = try LocalStore(url: directory.appendingPathComponent("store.sqlite"), key: Data(repeating: 63, count: 32), identity: boot.identity, context: boot.context_id)
        let first = try store.saveDraft(Draft(id: UUID().uuidString.lowercased(), person: person, kind: "log_contact_attempt", revision: 0,
                                              contactChannel: "other", contactOutcome: "reached", occurredAt: stamp(Date().addingTimeInterval(-3600)), deviceRecordedAt: stamp()))
        let firstEnvelope = try store.submit(first)
        let bytes = try XCTUnwrap(store.queue().first?.bytes)
        api.dropNextOperationResponse = true
        do { _ = try await api.operation(bytes, context: boot.context_id); XCTFail("The test seam must interrupt the accepted response") }
        catch LocalError.lostResponse { }
        let replay = try await api.operation(bytes, context: boot.context_id)
        XCTAssertTrue(replay.replayed); XCTAssertEqual(replay.operation_id, firstEnvelope.operation_id)
        XCTAssertEqual(replay.resource_type, "contact_attempt"); XCTAssertNil(replay.committed_revision); XCTAssertTrue(replay.changed)
        try store.acknowledge(replay); XCTAssertEqual(try store.queue().first?.status, "accepted")

        let second = try store.saveDraft(Draft(id: UUID().uuidString.lowercased(), person: person, kind: "log_contact_attempt", revision: 0,
                                               contactChannel: "email", contactOutcome: "sent", occurredAt: stamp(Date().addingTimeInterval(-1800)), deviceRecordedAt: stamp()))
        let secondEnvelope = try store.submit(second)
        let secondReceipt = try await api.operation(try XCTUnwrap(store.queue().last?.bytes), context: boot.context_id)
        XCTAssertFalse(secondReceipt.replayed); XCTAssertNotEqual(secondReceipt.operation_id, replay.operation_id); XCTAssertEqual(secondReceipt.operation_id, secondEnvelope.operation_id)
        XCTAssertEqual(try store.queue().filter(\.isContact).count, 2)

        let futureDraft = try store.saveDraft(Draft(id: UUID().uuidString.lowercased(), person: person, kind: "log_contact_attempt", revision: 0,
                                                    contactChannel: "call", contactOutcome: "no_answer", occurredAt: "2099-01-01T00:00:00Z", deviceRecordedAt: stamp()))
        let rejected = try store.submit(futureDraft); let rejectedBytes = try XCTUnwrap(store.queue().last?.bytes)
        do { _ = try await api.operation(rejectedBytes, context: boot.context_id); XCTFail("future reported contact must be rejected") }
        catch let error as APIError { XCTAssertEqual(error.status, 422); XCTAssertEqual(error.code, "contact_time_in_future") }
        try store.failure(rejected.operation_id, code: "contact_time_in_future", permanent: true, delay: 0)
        XCTAssertEqual(try store.queue().last?.bytes, rejectedBytes); XCTAssertEqual(try store.queue().last?.status, "attention")
    }
    #endif
    #if MOBILE002_QA
    func testMobile002AcceptedSeedAppearsInCurrentRoutePageAndQualifiedEncryptedBundle() async throws {
        let (api, boot) = try await connect()
        // Person001 is the coordinator-reserved iOS fixture, verified by the
        // actual UI test's accessibility identifier.
        let target = "f40f5132-9822-4bdf-ba34-76affb181195"
        let seed = Envelope(context_id: boot.context_id, operation_id: UUID().uuidString.lowercased(), kind: "add_note", device_recorded_at: stamp(), payload: .object(["person_id": .s(target), "body": .s("iOS exact-seed " + UUID().uuidString)]))
        let receipt = try await api.operation(try encode(seed), context: boot.context_id)
        print("MOBILE002_QA_SEED_NOTE=" + receipt.resource_id)
        let exact = try await api.currentNote(person: target, note: receipt.resource_id, context: boot.context_id)
        XCTAssertEqual(exact.note?["id"].text, receipt.resource_id); XCTAssertEqual(exact.note?["revision"].text, "1")
        let fresh: Generation = try await api.call("/reconciliations", method: "POST", body: .object(["protocol": .s("mobile-v1"), "installation_id": .s(boot.installation_id), "pinned_person_ids": .array([])]), context: boot.context_id)
        print("MOBILE002_QA_SEALED_GENERATION=" + fresh.generation_id)
        let matching = try XCTUnwrap(fresh.manifest.items.first { $0.person_id == target })
        let summary: Page = try await api.call("/reconciliations/\(fresh.generation_id)/people/\(target)/summary", context: boot.context_id)
        let notes: Page = try await api.call("/reconciliations/\(fresh.generation_id)/people/\(target)/notes", context: boot.context_id)
        let tasks: Page = try await api.call("/reconciliations/\(fresh.generation_id)/people/\(target)/tasks", context: boot.context_id)
        XCTAssertTrue(notes.items.contains { $0["id"].text == receipt.resource_id })
        let _: Seal = try await api.call("/reconciliations/\(fresh.generation_id)/seal", method: "POST", body: .object([:]), context: boot.context_id)
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString); try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true); defer { try? FileManager.default.removeItem(at: dir) }
        let store = try LocalStore(url: dir.appendingPathComponent("exact.sqlite"), key: Data(repeating: 71, count: 32), identity: boot.identity, context: boot.context_id)
        let local = Generation(generation_id: fresh.generation_id, context_id: boot.context_id, evaluated_at: fresh.evaluated_at, expires_at: fresh.expires_at, complete: true, selected_count: 1, manifest: Manifest(items: [matching], next_cursor: nil, complete: true))
        try store.begin(local); try store.appendPage(summary, expected: matching.revision); try store.appendPage(notes, expected: matching.revision); try store.appendPage(tasks, expected: matching.revision); try store.finishBundle(fresh.generation_id, target, matching.revision)
        try store.promote(Seal(generation_id: fresh.generation_id, context_id: boot.context_id, sealed_at: stamp(), evaluated_at: fresh.evaluated_at, selected_count: 1, today: .object(["items": .array([])])))
        XCTAssertNotNil(try store.editableRecord(person: target, type: "note", id: receipt.resource_id))
    }

    func testMobile002RealLostResponseReplayAndTwoActorConflictOnReservedPerson001() async throws {
        let (first, firstBoot) = try await connect()
        let generation: Generation = try await first.call("/reconciliations", method: "POST", body: .object(["protocol": .s("mobile-v1"), "installation_id": .s(firstBoot.installation_id), "pinned_person_ids": .array([])]), context: firstBoot.context_id)
        var person: String?
        for item in generation.manifest.items {
            let page: Page = try await first.call("/reconciliations/\(generation.generation_id)/people/\(item.person_id)/summary", context: firstBoot.context_id)
            if page.summary?["last_name"].text == "001" { person = item.person_id; break }
        }
        let target = try XCTUnwrap(person)
        let create = Envelope(context_id: firstBoot.context_id, operation_id: UUID().uuidString.lowercased(), kind: "add_note", device_recorded_at: stamp(), payload: .object(["person_id": .s(target), "body": .s("iOS Mobile002 QA " + UUID().uuidString)]))
        let created = try await first.operation(try encode(create), context: firstBoot.context_id)
        XCTAssertNil(created.committed_revision)
        let current = try await first.currentNote(person: target, note: created.resource_id, context: firstBoot.context_id)
        let note = try XCTUnwrap(current.note); XCTAssertEqual(note["revision"].text, "1")
        let edit = Envelope(context_id: firstBoot.context_id, operation_id: UUID().uuidString.lowercased(), kind: "edit_note", device_recorded_at: stamp(), payload: .object(["person_id": .s(target), "note_id": .s(created.resource_id), "expected_revision": .s("1"), "body": .s("iOS accepted edit")]))
        first.dropNextOperationResponse = true
        do { _ = try await first.operation(try encode(edit), context: firstBoot.context_id); XCTFail("deliberately dropped accepted response") }
        catch LocalError.lostResponse { }
        let replay = try await first.operation(try encode(edit), context: firstBoot.context_id)
        XCTAssertTrue(replay.replayed); XCTAssertEqual(replay.committed_revision, "2")
        let (second, secondBoot) = try await connect(email: "second@mobile.test")
        let secondEdit = Envelope(context_id: secondBoot.context_id, operation_id: UUID().uuidString.lowercased(), kind: "edit_note", device_recorded_at: stamp(), payload: .object(["person_id": .s(target), "note_id": .s(created.resource_id), "expected_revision": .s("2"), "body": .s("second actor edit")]))
        _ = try await second.operation(try encode(secondEdit), context: secondBoot.context_id)
        let stale = Envelope(context_id: firstBoot.context_id, operation_id: UUID().uuidString.lowercased(), kind: "edit_note", device_recorded_at: stamp(), payload: .object(["person_id": .s(target), "note_id": .s(created.resource_id), "expected_revision": .s("2"), "body": .s("must conflict")]))
        do { _ = try await first.operation(try encode(stale), context: firstBoot.context_id); XCTFail("stale revision must conflict") }
        catch let error as APIError { XCTAssertEqual(error.code, "revision_conflict") }
        let refreshed = try await first.currentNote(person: target, note: created.resource_id, context: firstBoot.context_id)
        XCTAssertEqual(refreshed.note?["body"].text, "second actor edit"); XCTAssertEqual(refreshed.note?["revision"].text, "3")
    }
    #endif
}
