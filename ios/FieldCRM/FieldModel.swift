import Foundation
import SwiftUI
import Network

@MainActor final class FieldModel: ObservableObject {
    @Published var people: [Bundle] = []
    @Published var queue: [Queued] = []
    @Published var drafts: [Draft] = []
    @Published var today: JSON = .null
    @Published var message = "Sign in to download your field workspace."
    @Published var lastSync = "Never"
    @Published var unlocked = false
    @Published var syncing = false
    @Published var updateRequired = false
    @Published var signingIn = false
    @Published var paused = UserDefaults.standard.bool(forKey: "pauseSync") { didSet { UserDefaults.standard.set(paused, forKey: "pauseSync") } }
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
    init(synthetic: Bool = false, startMonitor: Bool = true, restoreOnInit: Bool = true) {
        secure = SecureStorage(synthetic: synthetic); self.synthetic = secure.synthetic
        if let existing = UserDefaults.standard.string(forKey: "installation") { installation = existing }
        else { installation = UUID().uuidString.lowercased(); UserDefaults.standard.set(installation, forKey: "installation") }
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
            let base = "http://127.0.0.1:3101"
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
    @discardableResult func validateAccess() -> Bool {
        do {
            guard var saved = credential, !saved.signedOut, unlocked else { throw LocalError.locked }
            try saved.lease.validate(.current()); try secure.write("credential", encode(saved)); credential = saved
            return true
        } catch { lock(error.localizedDescription); return false }
    }
    func lock(_ reason: String, persist: Bool = true) {
        epoch = UUID(); unlocked = false; store = nil; api = nil
        people = []; queue = []; drafts = []; today = .null; selectedPerson = nil; account = ""
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
