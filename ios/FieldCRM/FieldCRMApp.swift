import SwiftUI

@main struct FieldCRMApp: App {
    @StateObject private var model = FieldModel(synthetic: ProcessInfo.processInfo.arguments.contains("--synthetic-keychain"))
    @Environment(\.scenePhase) private var scenePhase
    var body: some Scene {
        WindowGroup {
            RootView().environmentObject(model)
                .tint(Color(red: 0.03, green: 0.43, blue: 0.39))
                .overlay { if scenePhase != .active { Color(.systemBackground).ignoresSafeArea().overlay(Label("Field CRM · Protected", systemImage: "lock.fill")) } }
                .onChange(of: scenePhase) { _, phase in
                    if phase == .active { model.restore(); Task { await model.sync() } }
                }
                .onReceive(NotificationCenter.default.publisher(for: UIApplication.protectedDataWillBecomeUnavailableNotification)) { _ in model.protectedDataUnavailable() }
                .task {
                    while !Task.isCancelled {
                        try? await Task.sleep(for: .seconds(60))
                        if scenePhase == .active && model.unlocked { _ = model.validateAccess(); await model.sync() }
                    }
                }
        }
    }
}
struct RootView: View {
    @EnvironmentObject private var model: FieldModel
    var body: some View {
        VStack(spacing: 0) {
            if model.synthetic { Text("SYNTHETIC SIMULATOR · Test keys · No customer data").font(.caption2).padding(6).frame(maxWidth: .infinity).background(.yellow.opacity(0.25)).accessibilityIdentifier("syntheticBanner") }
            if model.unlocked { WorkspaceView() } else { SignInView() }
        }
    }
}
struct SignInView: View {
    @EnvironmentObject private var model: FieldModel
    @State private var email = ""
    @State private var password = ""
    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 22) {
                    Image(systemName: "house.and.flag.fill").font(.system(size: 46)).foregroundStyle(.tint)
                    Text("Your relationships,\nwherever work takes you.").font(.largeTitle.bold())
                    Text("Download your workspace, save notes and tasks on this device, and sync when you’re connected.").foregroundStyle(.secondary)
                    TextField("Email", text: $email).textContentType(.username).keyboardType(.emailAddress).textInputAutocapitalization(.never).autocorrectionDisabled().accessibilityIdentifier("email")
                    SecureField("Password", text: $password).textContentType(.password).accessibilityIdentifier("password")
                    Button(model.signingIn ? "Signing in…" : "Sign in") { let candidate = password; password = ""; Task { await model.signIn(email: email, password: candidate) } }
                        .buttonStyle(.borderedProminent).controlSize(.large).disabled(model.signingIn || email.isEmpty || password.isEmpty).accessibilityIdentifier("signIn")
                    Text(model.message).font(.callout).accessibilityIdentifier("statusMessage")
                    Label("Protected offline access lasts up to 7 days. After a device reboot, sign in online again.", systemImage: "lock.shield").font(.footnote).foregroundStyle(.secondary)
                }.padding(28).textFieldStyle(.roundedBorder)
            }.navigationTitle("Field CRM").navigationBarTitleDisplayMode(.inline)
        }
    }
}
struct WorkspaceView: View {
    @EnvironmentObject private var model: FieldModel
    @State private var signOutPrompt = false
    var body: some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 4) {
                HStack {
                    Label(model.paused ? "Working offline" : model.connected ? "Connected" : "Offline", systemImage: model.paused || !model.connected ? "wifi.slash" : "wifi")
                    Spacer()
                    if model.syncing { ProgressView() }
                    Button("Sync") { Task { await model.sync(manual: true) } }.disabled(model.syncing || model.paused || model.updateRequired).accessibilityIdentifier("sync")
                }.font(.subheadline.bold())
                Text(model.updateRequired ? "Update required before sync. Saved work is retained." : model.message).font(.caption).lineLimit(3).accessibilityIdentifier("statusMessage")
            }.padding(12).background(.teal.opacity(0.08))
            TabView {
                NavigationStack { TodayView() }.tabItem { Label("Today", systemImage: "sun.max") }
                NavigationStack { PeopleView() }.tabItem { Label("People", systemImage: "person.2") }
                NavigationStack { QueueView() }.tabItem { Label("Saved work", systemImage: "tray.and.arrow.up") }.badge(model.pendingCount)
                NavigationStack {
                    Form {
                        #if MOBILE002_QA
                        // Keep the synthetic-only fixture control in the first visible rows.
                        // Form realizes lower sections lazily, which would make an accessibility
                        // probe unable to observe an otherwise present test control.
                        Section("QA fixture stage") {
                            Text(model.qaFixtureStage).font(.caption2).accessibilityIdentifier("qaFixtureStage")
                            Button("Load exact test note") { Task { await model.loadQANoteFixture() } }.accessibilityIdentifier("loadQANoteFixture")
                                .disabled(model.syncing)
                        }
                        Section("QA migration probe") {
                            Text(model.qaMigrationStage).font(.caption2).accessibilityIdentifier("qaMigrationStage")
                            Button("Inspect protected migration") { model.inspectQAMigration() }.accessibilityIdentifier("inspectQAMigration")
                        }
                        Section("QA conflict actor") {
                            Text(model.qaConflictStage).font(.caption2).accessibilityIdentifier("qaConflictStage")
                            Button("Create fresh conflict note") { Task { await model.createQAFreshConflictNote() } }.accessibilityIdentifier("qaFreshConflict")
                            Button("Prepare pending note conflict") { model.prepareQAPendingConflictEdit() }.accessibilityIdentifier("qaPrepareConflict")
                            Button("Advance pending note as second actor") { Task { await model.advanceQAPendingEditAsSecondActor() } }.accessibilityIdentifier("qaAdvanceConflict")
                                .disabled(model.syncing)
                            Button("Resume QA sync") { model.resumeQASync() }.accessibilityIdentifier("qaResumeSync")
                            Button("Drain target conflict") { Task { await model.drainQAConflict() } }.accessibilityIdentifier("qaDrainConflict")
                        }
                        #endif
                        #if MOBILE003_QA
                        Section("QA Mobile003 migration probe") {
                            Text(model.qaMobile003MigrationStage).font(.caption2).accessibilityIdentifier("qaMobile003MigrationStage")
                            Button("Inspect Mobile003 protected migration") { model.inspectMobile003Migration() }.accessibilityIdentifier("inspectMobile003Migration")
                        }
                        #endif
                        #if MOBILE005_QA
                        Section("QA Mobile005 profile conflict") {
                            Text(model.qaProfileConflictStage).font(.caption2).accessibilityIdentifier("qaProfileConflictStage")
                            Button("Prepare pending profile conflict") { model.prepareQAProfileConflict() }.accessibilityIdentifier("qaPrepareProfileConflict")
                            Button("Advance profile as second actor") { Task { await model.advanceQAProfileConflictAsSecondActor() } }.accessibilityIdentifier("qaAdvanceProfileConflict")
                                .disabled(model.syncing)
                            Button("Drain profile conflict") { Task { await model.drainQAProfileConflict() } }.accessibilityIdentifier("qaDrainProfileConflict")
                            Button("Inspect profile outbox") { model.inspectQAProfileOutbox() }.accessibilityIdentifier("qaInspectProfileOutbox")
                            Button("Inspect profile cache") { model.inspectQAProfileCache() }.accessibilityIdentifier("qaInspectProfileCache")
                            Button("Verify profile receipt") { Task { await model.verifyQAProfileReceipt() } }.accessibilityIdentifier("qaVerifyProfileReceipt")
                        }
                        #endif
                        #if MOBILE006_QA
                        Section("QA Mobile006 metadata conflict") {
                            Text(model.qaMetadataConflictStage).font(.caption2).accessibilityIdentifier("qaMetadataConflictStage")
                            Button("Prepare local metadata conflict review") { model.prepareQAMetadataConflictReview() }.accessibilityIdentifier("qaPrepareMetadataConflict")
                                .disabled(model.syncing)
                        }
                        #endif
                        #if MOBILE006_UPGRADE_QA
                        Section("QA Mobile006 installed upgrade") {
                            Text(model.qaMobile006UpgradeStage).font(.caption2).accessibilityIdentifier("qaMobile006UpgradeStage")
                            Button("Inspect installed Mobile005 upgrade") { model.inspectMobile006Upgrade() }.accessibilityIdentifier("inspectMobile006Upgrade")
                        }
                        #endif
                        Section("Connection") {
                            Toggle("Work offline · pause sync", isOn: $model.paused).accessibilityIdentifier("offlineToggle")
                            Text("Last complete sync: \(model.lastSync)").font(.footnote)
                            Text("\(model.people.count) people available offline")
                            Text("\(model.people.reduce(0) { $0 + $1.notes.count }) notes · \(model.people.reduce(0) { $0 + $1.tasks.count }) tasks in the complete cache").font(.caption).accessibilityIdentifier("coverageCounts")
                        }
                        Section("Protected access") {
                            Text("Online authorization lasts up to 7 days, ending sooner if the device restarts or its clock becomes unverifiable. Saved work is retained when access locks.")
                            Text(model.account).font(.caption).textSelection(.enabled)
                            Button("Sign out", role: .destructive) { signOutPrompt = true }.accessibilityIdentifier("signOut")
                        }
                        Section("About this build") { Text("Mobile 003 · Synthetic development. Log a manual contact that already happened, or save notes and tasks for later synchronization. Calls, messages and background delivery are outside this field workflow.").font(.footnote) }
                    }.navigationTitle("Settings")
                }.tabItem { Label("Settings", systemImage: "gearshape") }
            }
        }
        .confirmationDialog("\(model.pendingCount) actions are still saved on this device. Sign out now?", isPresented: $signOutPrompt, titleVisibility: .visible) {
            Button("Sign out and protect saved work", role: .destructive) { Task { await model.signOut() } }
        } message: { Text("Sign in with this same account and Organization to reopen them. A network connection is not required to sign out locally.") }
    }
}
struct TodayView: View {
    @EnvironmentObject private var model: FieldModel
    var body: some View {
        List {
            Section {
                Text("Server-ranked work from your last complete download.").font(.footnote).foregroundStyle(.secondary)
                Text("Evaluated: " + (model.today["generated_at"].text.isEmpty ? "No complete download" : model.today["generated_at"].text)).font(.caption)
                let pendingTasks = model.queue.filter { $0.envelope.kind != "add_note" && !$0.isContact && $0.overlay }.count
                Label("\(pendingTasks) local task changes · see Saved work", systemImage: "tray").font(.caption)
                if model.pendingContactCount > 0 { Label("\(model.pendingContactCount) contact \(model.pendingContactCount == 1 ? "attempt" : "attempts") saved on this device", systemImage: "person.crop.circle.badge.clock").font(.caption).accessibilityIdentifier("pendingContactBadge") }
                if model.todayIsStale { Label("Today will refresh from the server before its ranking changes.", systemImage: "arrow.triangle.2.circlepath").font(.caption).foregroundStyle(.orange).accessibilityIdentifier("todayStale") }
            }
            if model.today["items"].list.isEmpty { ContentUnavailableView("No downloaded Today items", systemImage: "sun.max", description: Text("Open People to see your downloaded relationships.")) }
            ForEach(Array(model.today["items"].list.enumerated()), id: \.offset) { _, item in
                let person = item["person"]
                if let bundle = model.people.first(where: { $0.person == person["id"].text }) {
                    NavigationLink { PersonView(personID: bundle.person) } label: {
                        VStack(alignment: .leading) {
                            Text(person["display_name"].text).font(.headline)
                            ForEach(Array(item["reasons"].list.enumerated()), id: \.offset) { _, reason in Text(reason["label"].text.isEmpty ? reason["code"].text.replacingOccurrences(of: "_", with: " ") : reason["label"].text).font(.caption).foregroundStyle(.secondary) }
                        }
                    }
                }
            }
        }.navigationTitle("Today")
    }
}
struct PeopleView: View {
    @EnvironmentObject private var model: FieldModel
    @State private var search = ""
    @State private var pin = ""
    @State private var organizationTerm = ""
    var filtered: [Bundle] { model.people.filter { search.isEmpty || $0.summary["display_name"].text.localizedCaseInsensitiveContains(search) } }
    var body: some View {
        List {
            Section { Text("\(model.people.count) downloaded people · assigned, Today and saved records").font(.caption).foregroundStyle(.secondary) }
            ForEach(filtered, id: \.person) { person in
                NavigationLink { PersonView(personID: person.person) } label: {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(person.summary["display_name"].text).font(.headline)
                        Text(model.displayedStageName(person)).font(.caption).foregroundStyle(.secondary)
                        Text("Last synced: \(model.selectionReason(person.person))").font(.caption2).foregroundStyle(.secondary)
                    }.padding(.vertical, 3)
                }.accessibilityIdentifier("person_" + person.person)
            }
            if !model.requestedPins.isEmpty {
                Section("Requested offline People") {
                    if model.pinReviewRequired { Text("The requested set needs review. No individual Person was identified.").font(.caption).foregroundStyle(.red) }
                    Button("Retry requested downloads") { model.retryPinRequests() }
                    ForEach(model.requestedPins) { request in
                        HStack {
                            Text(request.label).accessibilityIdentifier("requested_\(request.personID)")
                            Spacer()
                            Button("Cancel") { model.unpin(request.personID) }.accessibilityIdentifier("cancelRequested_\(request.personID)")
                        }
                    }
                }
            }
            Section("Search Organization") {
                TextField("Name, exact email or phone", text: $organizationTerm).textInputAutocapitalization(.never).autocorrectionDisabled().onSubmit { Task { await model.searchOrganization(organizationTerm) } }
                HStack { Button("Search") { Task { await model.searchOrganization(organizationTerm) } }.disabled(model.organizationSearching || organizationTerm.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty); if model.organizationSearching { ProgressView().controlSize(.small) }; if !model.organizationSearchResults.isEmpty { Button("Clear") { organizationTerm = ""; model.clearOrganizationSearch() } } }
                if !model.organizationSearchMessage.isEmpty { Text(model.organizationSearchMessage).font(.caption).foregroundStyle(.secondary) }
                if model.organizationSearchHasMore { Text("More matches — refine your search.").font(.caption).foregroundStyle(.secondary) }
                ForEach(model.organizationSearchResults) { result in
                    let downloaded = model.people.contains { $0.person == result.person_id }
                    VStack(alignment: .leading, spacing: 4) { Text(result.display_name).font(.headline); if let stage = result.stage { Text(stage.name).font(.caption).foregroundStyle(.secondary) }; if let assigned = result.assigned_user { Text("Responsible: \(assigned.display_name)").font(.caption).foregroundStyle(.secondary) }; if let email = result.primary_email { Text(email).font(.caption) }; if let phone = result.primary_phone { Text(phone).font(.caption) }; HStack { if downloaded { NavigationLink("Open") { PersonView(personID: result.person_id) }; Text("Available offline").font(.caption).foregroundStyle(.secondary) } else if model.isPinned(result.person_id) { Text("Requested offline — manage above").font(.caption).foregroundStyle(.secondary) } else { Button("Save offline") { model.pin(result.person_id) } } } }.padding(.vertical, 3)
                }
            }
            Section("Save a known CRM record offline") {
                TextField("Person ID", text: $pin).textInputAutocapitalization(.never).autocorrectionDisabled()
                Button("Add to next download") { model.pin(pin); pin = "" }.disabled(UUID(uuidString: pin) == nil)
                Text("Only records the server permits you to view can be downloaded.").font(.caption)
            }
        }.searchable(text: $search, prompt: "Search downloaded people").navigationTitle("People")
    }
}
struct PersonView: View {
    @EnvironmentObject private var model: FieldModel
    let personID: String
    @State private var composer: Draft?
    @State private var contactComposer: Draft?
    @State private var stageProposal: StageProposal?
    @State private var detailsComposer: Draft?
    @State private var metadataComposer: Draft?
    var bundle: Bundle? { model.people.first { $0.person == personID } }
    var overlays: [Queued] { model.queue.filter { $0.envelope.person == personID && $0.overlay } }
    var body: some View {
        List {
            if let bundle {
                Section {
                    Text(bundle.summary["display_name"].text).font(.title2.bold())
                    Label(model.displayedStageName(bundle), systemImage: "flag")
                    Text("Responsible: " + (bundle.summary["assigned_user"]["display_name"].text.isEmpty ? "Unassigned" : bundle.summary["assigned_user"]["display_name"].text)).font(.subheadline)
                }
                Section {
                    Button("Add note") { composer = Draft(id: UUID().uuidString, person: personID, kind: "add_note", text: "", revision: 0) }.accessibilityIdentifier("addNote")
                    Button("Create task") { composer = Draft(id: UUID().uuidString, person: personID, kind: "create_task", text: "", revision: 0) }.accessibilityIdentifier("createTask")
                    Button("Log contact") { contactComposer = try? model.newContactDraft(person: personID) }.disabled(!model.canLogContact).accessibilityIdentifier("logContact")
                    Button("Change stage") { stageProposal = try? model.newStageProposal(person: personID) }.disabled(!model.canChangeStage).accessibilityIdentifier("changeStage")
                    Button("Edit profile") { detailsComposer = try? model.startDetails(person: personID) }
                        .disabled(!model.canEditDetails(person: personID))
                        .accessibilityIdentifier("editProfile")
                    Button("Edit tags and custom fields") { metadataComposer = try? model.newMetadataDraft(person: personID) }
                        .accessibilityIdentifier("editMetadata")
                        .disabled(!model.canEditMetadata(person: personID))
                        .accessibilityIdentifier("editMetadata")
                    Text(model.canLogContact ? "For a manual interaction that already happened. Calls made through the CRM already have a contact record." : "Contact logging is unavailable for this account. Existing saved work is retained.").font(.caption).foregroundStyle(.secondary)
                }
                Section("Contact methods") {
                    // Keep imported/server ordering stable while placing action
                    // controls before a long contact history.
                    ForEach(Array(bundle.orderedContacts.enumerated()), id: \.offset) { _, contact in Text(contact["value"].text).textSelection(.enabled) }
                }
                if let metadata = model.displayedMetadata(person: personID) {
                    Section("Tags") {
                        Text(metadata["tags"].list.map { $0["name"].text }.filter { !$0.isEmpty }.joined(separator: ", ").isEmpty ? "No tags" : metadata["tags"].list.map { $0["name"].text }.filter { !$0.isEmpty }.joined(separator: ", "))
                    }
                    Section("Custom fields") {
                        ForEach(Array(metadata["values"].list.enumerated()), id: \.offset) { _, value in
                            let label = metadata["fields"].list.first(where: { $0["id"].text == value["field_id"].text })?["label"].text ?? "Archived field"
                            Text(label + ": " + MetadataEditorProjection.display(value["value"]))
                        }
                    }
                }
                if !overlays.isEmpty {
                    Section("Saved changes on this device") {
                        ForEach(overlays) { op in
                            VStack(alignment: .leading, spacing: 6) {
                                Text(op.envelope.kind.replacingOccurrences(of: "_", with: " ").capitalized).font(.caption.bold())
                                if op.error != "not_found" && op.error != "forbidden" { Text(op.title.isEmpty ? (op.isContact ? "Manual contact attempt" : "Task completion") : op.title) }
                                Text(op.status == "accepted" ? "Synced · waiting for a covering download" : op.status == "attention" ? "Needs attention" : "Saved on device").font(.caption).foregroundStyle(.secondary)
                                if op.envelope.kind == "create_task" && !model.queue.contains(where: { $0.envelope.payload["target"]["created_by_operation_id"].text == op.id }) {
                                    Button("Complete saved task") { model.complete(person: personID, target: .object(["created_by_operation_id": .s(op.id)])) }
                                }
                            }
                        }
                    }
                }
                // Notes precede the potentially long task history so an editable note
                // remains reachable on a fully reconciled Person without requiring a
                // user to traverse every task row.
                Section("Notes") {
                    ForEach(Array(bundle.notes.enumerated()), id: \.offset) { _, note in
                        VStack(alignment: .leading, spacing: 7) {
                            Text(note["body"].text).textSelection(.enabled)
                            Text(note["author"]["display_name"].text + " · " + note["created_at"].text).font(.caption).foregroundStyle(.secondary)
                            if note["can_manage"].flag && !note["revision"].text.isEmpty {
                                Button("Edit note") { composer = try? model.startEdit(person: personID, type: "note", record: note) }.accessibilityIdentifier("editNote_" + note["id"].text)
                            } else if note["can_manage"].flag { Button("Refresh note to edit") { Task { composer = try? await model.startEditFromCurrent(person: personID, type: "note", id: note["id"].text) } }.font(.caption) }
                        }.padding(.vertical, 4)
                    }
                }
                Section("Tasks") {
                    ForEach(Array(bundle.tasks.enumerated()), id: \.offset) { _, task in
                        let pending = model.queue.last { $0.overlay && $0.envelope.payload["target"]["task_id"].text == task["id"].text }
                        VStack(alignment: .leading, spacing: 6) {
                            Text(task["title"].text)
                            Text(task["kind"].text.replacingOccurrences(of: "_", with: " ") + (task["due_at"].text.isEmpty ? "" : " · " + task["due_at"].text)).font(.caption).foregroundStyle(.secondary)
                            if task["can_manage"].flag && !task["revision"].text.isEmpty {
                                Button("Edit task") { composer = try? model.startEdit(person: personID, type: "task", record: task) }.accessibilityIdentifier("editTask_" + task["id"].text)
                            } else if task["can_manage"].flag { Button("Refresh task to edit") { Task { composer = try? await model.startEditFromCurrent(person: personID, type: "task", id: task["id"].text) } }.font(.caption) }
                            if task["completed_at"] != .null { Label("Completed", systemImage: "checkmark.circle.fill").font(.caption) }
                            else if let pending {
                                if pending.status == "attention" {
                                    Text("Earlier completion needs attention. Its saved proposal is retained.").font(.caption).foregroundStyle(.orange)
                                    if pending.error == "revision_conflict" && pending.envelope.payload["target"]["expected_revision"].text != task["revision"].text && task["can_manage"].flag {
                                        Button("Complete this refreshed task") { model.complete(person: personID, target: .object(["task_id": task["id"], "expected_revision": task["revision"]])) }
                                        Text("This creates a separate action using the version shown above.").font(.caption)
                                    }
                                } else { Text("Completion saved on device").font(.caption) }
                            }
                            else if task["can_manage"].flag { Button("Complete task") { model.complete(person: personID, target: .object(["task_id": task["id"], "expected_revision": task["revision"]])) } }
                        }
                    }
                }
            } else { Text("This record is not in the complete downloaded selection. Saved actions remain in Saved work.") }
        }.navigationTitle("Person").navigationBarTitleDisplayMode(.inline)
            .sheet(item: $composer) { draft in ComposerView(initial: draft).environmentObject(model) }
            .sheet(item: $contactComposer) { draft in ContactComposerView(initial: draft).environmentObject(model) }
            .sheet(item: $stageProposal) { proposal in StageProposalView(initial: proposal).environmentObject(model) }
            .sheet(item: $detailsComposer) { draft in DetailsComposerView(initial: draft).environmentObject(model) }
            .sheet(item: $metadataComposer) { draft in MetadataComposerView(initial: draft).environmentObject(model) }
    }
}
struct DetailsEditorMethod: Identifiable, Equatable {
    var id: String, kind: String, value: String, original: String, removed: Bool, isNew: Bool
    /// Existing sparse operations retain their original serialized ordinal when
    /// a draft is reopened.  Fresh local edits append after that preserved set.
    var operationOrdinal: Int? = nil
}

/// Keeps the editor's on-device draft faithful to the downloaded baseline.  The
/// draft stores only a sparse proposal, so reopening it must apply that sparse
/// proposal rather than treating the baseline as an edit.
enum DetailsEditorProjection {
    static func fields(for draft: Draft) -> (firstName: String, lastName: String, methods: [DetailsEditorMethod]) {
        let baseline = draft.baseline ?? .object([:])
        let savedProposal = draft.proposal ?? .object([:])
        var firstName = baseline["first_name"].text
        var lastName = baseline["last_name"].text
        var methods = ContactDisplayOrder.sorted(baseline["contacts"].list).map {
            DetailsEditorMethod(id: $0["id"].text, kind: $0["kind"].text, value: $0["value"].text, original: $0["value"].text, removed: false, isNew: false)
        }
        var added: [DetailsEditorMethod] = []
        for (ordinal, operation) in savedProposal["contact_operations"].list.enumerated() {
            switch operation["op"].text {
            case "edit":
                if let index = methods.firstIndex(where: { $0.id == operation["id"].text }) { methods[index].value = operation["value"].text; methods[index].operationOrdinal = ordinal }
            case "remove":
                if let index = methods.firstIndex(where: { $0.id == operation["id"].text }) { methods[index].removed = true; methods[index].operationOrdinal = ordinal }
            case "add":
                // The server has no ID until acceptance.  Deriving the local ID
                // from draft plus operation ordinal makes it stable across a
                // relaunch while retaining the serialized add order.
                added.append(DetailsEditorMethod(id: "draft:\(draft.id):add:\(ordinal)", kind: operation["kind"].text, value: operation["value"].text, original: "", removed: false, isNew: true, operationOrdinal: ordinal))
            default: break
            }
        }
        if case .object(let proposal) = savedProposal, let first = proposal["first_name"] {
            firstName = first == .null ? "" : first.text
        }
        if case .object(let proposal) = savedProposal, let last = proposal["last_name"] {
            lastName = last == .null ? "" : last.text
        }
        return (firstName, lastName, added + methods)
    }

    static func primaryRemovalPreviews(_ methods: [DetailsEditorMethod]) -> [String] {
        ["email", "phone"].compactMap { kind in
            let existing = methods.filter { !$0.isNew && $0.kind == kind }
            guard existing.first?.removed == true else { return nil }
            if let next = existing.first(where: { !$0.removed }) {
                return "After sync, the first \(kind) will be \(next.value)."
            }
            if methods.contains(where: { $0.isNew && $0.kind == kind && !$0.value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }) {
                return "After sync, a newly added \(kind) will become the first \(kind)."
            }
            return "After sync, this profile will have no \(kind) contact method."
        }
    }

    static func proposal(firstName: String, lastName: String, methods: [DetailsEditorMethod], baseline: JSON) -> JSON {
        var out: [String: JSON] = [:]
        // Compare the raw editor value first.  An untouched imported value may
        // contain whitespace or exceed today's edit limits and must remain a
        // readable baseline, not be silently normalized into this submission.
        if firstName != baseline["first_name"].text {
            let normalized = firstName.trimmingCharacters(in: .whitespacesAndNewlines)
            out["first_name"] = normalized.isEmpty ? .null : .s(normalized)
        }
        if lastName != baseline["last_name"].text {
            let normalized = lastName.trimmingCharacters(in: .whitespacesAndNewlines)
            out["last_name"] = normalized.isEmpty ? .null : .s(normalized)
        }
        var operations: [JSON] = []
        let preserved = methods.enumerated().filter { $0.element.operationOrdinal != nil }.sorted {
            ($0.element.operationOrdinal!, $0.offset) < ($1.element.operationOrdinal!, $1.offset)
        }.map(\.element)
        let fresh = methods.filter { $0.operationOrdinal == nil }
        for method in preserved + fresh {
            if method.isNew {
                let normalized = method.value.trimmingCharacters(in: .whitespacesAndNewlines)
                if !normalized.isEmpty { operations.append(.object(["op": .s("add"), "kind": .s(method.kind), "value": .s(normalized)])) }
            } else if method.removed {
                operations.append(.object(["op": .s("remove"), "id": .s(method.id)]))
            } else if method.value != method.original {
                let normalized = method.value.trimmingCharacters(in: .whitespacesAndNewlines)
                operations.append(.object(["op": .s("edit"), "id": .s(method.id), "value": .s(normalized)]))
            }
        }
        if !operations.isEmpty { out["contact_operations"] = .array(operations) }
        return .object(out)
    }
}

struct DetailsComposerView: View {
    @EnvironmentObject private var model: FieldModel
    @Environment(\.dismiss) private var dismiss
    @State private var draft: Draft
    @State private var firstName: String
    @State private var lastName: String
    @State private var methods: [DetailsEditorMethod]
    @State private var status: String
    @State private var failed = false
    init(initial: Draft) {
        _draft = State(initialValue: initial)
        let fields = DetailsEditorProjection.fields(for: initial)
        _firstName = State(initialValue: fields.firstName)
        _lastName = State(initialValue: fields.lastName)
        _methods = State(initialValue: fields.methods)
        _status = State(initialValue: initial.mode == "conflict" ? "Review your saved profile proposal." : "Profile proposal is protected on this device.")
    }
    var body: some View {
        NavigationStack {
            Form {
                Section("Downloaded profile") {
                    Text("Profile revision \(draft.expectedRevision ?? "?")").font(.caption).foregroundStyle(.secondary)
                }
                if draft.mode == "conflict" {
                    Section("Your saved proposal") { Text(summary(draft.proposal)).textSelection(.enabled) }
                    Section {
                        Text(status).font(.caption).foregroundStyle(failed ? .red : .secondary).accessibilityIdentifier("profileDraftStatus")
                        Button("Use current profile and discard my saved proposal", role: .destructive) { do { try model.resolveUsingCurrent(draft); dismiss() } catch { fail(error) } }
                        Button("Prepare replacement from current profile") {
                            do { draft = try model.revisedDetailsDraft(draft); applyDraft(); status = "Replacement draft saved on device."; failed = false } catch { fail(error) }
                        }.disabled(draft.current == nil).accessibilityIdentifier("prepareProfileReplacement")
                        if draft.current == nil { Button("Fetch current profile") { fetchCurrent() } }
                    }
                    Section("Current profile") { Text(summary(draft.current)).textSelection(.enabled) }
                } else {
                    Section("Name") {
                        TextField("First name", text: $firstName).onChange(of: firstName) { _, _ in autosave() }.accessibilityIdentifier("profileFirstName")
                        TextField("Last name", text: $lastName).onChange(of: lastName) { _, _ in autosave() }.accessibilityIdentifier("profileLastName")
                        Button("Add email") { methods.append(DetailsEditorMethod(id: UUID().uuidString, kind: "email", value: "", original: "", removed: false, isNew: true)) }.accessibilityIdentifier("profileAddEmail")
                        Button("Add phone") { methods.append(DetailsEditorMethod(id: UUID().uuidString, kind: "phone", value: "", original: "", removed: false, isNew: true)) }.accessibilityIdentifier("profileAddPhone")
                    }
                    if methods.contains(where: { $0.isNew }) {
                        Section("New contact methods") {
                            Text("New methods appear after existing methods once accepted by the server.").font(.caption).foregroundStyle(.secondary)
                            ForEach($methods) { $method in
                                if method.isNew {
                                    VStack(alignment: .leading) {
                                        Text(method.kind.capitalized).font(.caption).foregroundStyle(.secondary)
                                        TextField(method.kind == "email" ? "Email" : "Phone", text: $method.value).onChange(of: method.value) { _, _ in autosave() }
                                            .accessibilityIdentifier("profileNew\(method.kind.capitalized)")
                                    }
                                }
                            }
                        }
                    }
                    Section {
                        Text(status).font(.caption).foregroundStyle(failed ? .red : .secondary).accessibilityIdentifier("profileDraftStatus")
                        ForEach(DetailsEditorProjection.primaryRemovalPreviews(methods), id: \.self) { preview in
                            Text(preview).font(.caption).foregroundStyle(.orange)
                        }
                        Button(draft.mode == "follow_up" ? "Save profile draft — waiting for previous change" : "Save profile change on device") { submit() }
                            .disabled(failed).accessibilityIdentifier("saveProfile")
                        if failed { Button("Retry saving profile draft") { autosave() } }
                    }
                    Section("Contact methods") {
                        ForEach($methods) { $method in
                            if !method.isNew {
                                VStack(alignment: .leading) {
                                    Text(method.kind.capitalized).font(.caption).foregroundStyle(.secondary)
                                    TextField(method.kind == "email" ? "Email" : "Phone", text: $method.value).onChange(of: method.value) { _, _ in autosave() }
                                        .accessibilityIdentifier("profileContact_\(method.id)")
                                    Toggle("Remove this method", isOn: $method.removed).onChange(of: method.removed) { _, _ in autosave() }
                                }
                            }
                        }
                    }
                    Text("Saved profile changes stay separate from call and message destinations until the server accepts them.").font(.caption).foregroundStyle(.secondary)
                }
            }.navigationTitle("Edit profile")
                .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Close") { dismiss() }.disabled(failed) } }
                .interactiveDismissDisabled(failed)
        }
    }
    private func proposal() -> JSON {
        DetailsEditorProjection.proposal(firstName: firstName, lastName: lastName, methods: methods, baseline: draft.baseline ?? .object([:]))
    }
    private func autosave() {
        do { draft.proposal = proposal(); draft = try model.save(draft); status = "Draft saved on device · revision \(draft.revision)"; failed = false }
        catch { fail(error) }
    }
    private func submit() {
        do { if draft.revision == 0 { autosave() }; guard !failed else { return }; try model.submit(draft); dismiss() }
        catch LocalError.waitingPredecessor { status = LocalError.waitingPredecessor.localizedDescription; failed = false }
        catch { fail(error) }
    }
    private func fetchCurrent() { Task { do { draft = try await model.requalifyDetails(draft); status = "Current profile fetched. Choose replacement or current."; failed = false } catch { fail(error) } } }
    private func applyDraft() {
        let fields = DetailsEditorProjection.fields(for: draft)
        firstName = fields.firstName; lastName = fields.lastName; methods = fields.methods
    }
    private func summary(_ value: JSON?) -> String { guard let value else { return "Current profile could not be fetched; your saved proposal is protected." }; return [value["first_name"].text, value["last_name"].text, value["contacts"].list.map { $0["kind"].text + ": " + $0["value"].text }.joined(separator: "\n")].filter { !$0.isEmpty }.joined(separator: "\n") }
    private func fail(_ error: Error) { status = error.localizedDescription; failed = true }
}
struct StageProposalView: View {
    @EnvironmentObject private var model: FieldModel
    @Environment(\.dismiss) private var dismiss
    @State private var proposal: StageProposal
    @State private var status = "Choose a stage. The downloaded server stage remains in place until sync succeeds."
    @State private var hasError = false
    init(initial: StageProposal) { _proposal = State(initialValue: initial) }
    var stages: [Stage] { model.availableStages() }
    var selected: Stage? { stages.first(where: { $0.id == proposal.selectedID }) }
    var body: some View {
        NavigationStack {
            Form {
                Section("Downloaded server stage") { Text(proposal.baseline.name); Text("Stage revision \(proposal.expected)").font(.caption).foregroundStyle(.secondary) }
                Section("Proposed stage") {
                    Picker("Stage", selection: $proposal.selectedID) { ForEach(stages) { stage in Text(stage.name).tag(stage.id) } }.accessibilityIdentifier("stagePicker")
                    Text("This saves a proposal on this device. Today is not recomputed locally.").font(.caption).foregroundStyle(.secondary)
                }
                Section { Text(status).font(.caption).foregroundStyle(hasError ? .red : .secondary).accessibilityIdentifier("stageDraftStatus") }
                Button("Save stage proposal on device") {
                    do {
                        guard let selected else { throw LocalError.invalidInput }
                        try model.queueStage(person: proposal.person, baseline: proposal.baseline, expected: proposal.expected, proposal: selected, superseding: proposal.superseding, draftID: proposal.draftID, draftRevision: proposal.draftRevision)
                        dismiss()
                    } catch { status = error.localizedDescription; hasError = true }
                }.disabled(selected == nil).accessibilityIdentifier("saveStage")
            }.navigationTitle("Change stage")
                .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Close") { dismiss() } } }
        }
    }
}
struct ComposerView: View {
    @EnvironmentObject private var model: FieldModel
    @Environment(\.dismiss) private var dismiss
    @State private var draft: Draft
    @State private var status = "Not yet saved"
    @State private var failed = false
    @State private var hasDue = false
    @State private var due = Date()
    init(initial: Draft) {
        _draft = State(initialValue: initial)
        let savedDue = initial.dueAt.flatMap { try? date($0) }
        _hasDue = State(initialValue: savedDue != nil)
        _due = State(initialValue: savedDue ?? Date())
        _status = State(initialValue: initial.revision > 0 ? "Draft saved on device · revision \(initial.revision)" : "Not yet saved")
    }
    var body: some View {
        NavigationStack {
            Form {
                Section(draft.kind == "add_note" || draft.kind == "edit_note" ? "Note" : "Task title") {
                    TextEditor(text: $draft.text).frame(minHeight: 150).accessibilityIdentifier("composerText")
                        .onChange(of: draft.text) { _, _ in autosave() }
                }
                if draft.kind == "create_task" || draft.kind == "update_task" {
                    Picker("Kind", selection: $draft.taskKind) { ForEach(["call", "email", "text", "follow_up", "other"], id: \.self) { Text($0.replacingOccurrences(of: "_", with: " ").capitalized).tag($0) } }.onChange(of: draft.taskKind) { _, _ in autosave() }
                    Toggle("Set a due date", isOn: $hasDue).onChange(of: hasDue) { _, value in draft.dueAt = value ? stamp(due) : nil; autosave() }
                    if hasDue { DatePicker("Due", selection: $due).onChange(of: due) { _, value in draft.dueAt = stamp(value); autosave() } }
                    Text(draft.kind == "update_task" ? "Assignee and completion state are preserved by the server." : "Assigned to you. Server validation applies when this task syncs.").font(.caption)
                }
                if let baseline = draft.baseline, draft.isEdit {
                    Section("Version you started from") { Text(editableText(baseline)).font(.caption).textSelection(.enabled) }
                }
                if draft.mode == "conflict" {
                    Section("Your saved edit") { Text(draft.text).textSelection(.enabled) }
                    Section("Current version") { Text(draft.current.map(editableText) ?? "Current version could not be fetched yet. Your saved edit is protected.").textSelection(.enabled) }
                    Text("Choosing the current version discards your saved proposal.").font(.caption).foregroundStyle(.orange)
                    Button("Use current version and discard my saved edit", role: .destructive) { do { try model.resolveUsingCurrent(draft); dismiss() } catch { status = error.localizedDescription; failed = true } }
                    Button("Prepare revised edit against current version") { do { draft = try model.revisedDraft(draft); status = "Revised draft saved on device" } catch { status = error.localizedDescription; failed = true } }
                }
                Section { Text(status).font(.caption).foregroundStyle(failed ? .red : .secondary).accessibilityIdentifier("draftStatus") }
                Button(draft.mode == "follow_up" ? "Save draft — waiting for the previous change" : "Save action on device") {
                    do { if draft.revision == 0 { draft = try model.save(draft) }; try model.submit(draft); dismiss() }
                    catch LocalError.waitingPredecessor { status = LocalError.waitingPredecessor.localizedDescription; failed = false }
                    catch { status = error.localizedDescription; failed = true }
                }.disabled(draft.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || failed).accessibilityIdentifier("saveAction")
                if failed { Button("Retry saving draft") { autosave() } }
            }.navigationTitle(draft.kind == "add_note" ? "New note" : draft.kind == "edit_note" ? "Edit note" : draft.kind == "update_task" ? "Edit task" : "New task")
                .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Close") { dismiss() }.disabled(failed) } }
                .interactiveDismissDisabled(failed)
        }
    }
    private func autosave() {
        do { draft = try model.save(draft); status = "Draft saved on device · revision \(draft.revision)"; failed = false }
        catch { status = error.localizedDescription; failed = true }
    }
    private func editableText(_ value: JSON) -> String {
        if !value["body"].text.isEmpty { return value["body"].text }
        let due = value["due_at"] == .null ? "No due date" : value["due_at"].text
        return value["title"].text + "\n" + value["kind"].text + " · " + due
    }
}
struct ContactComposerView: View {
    @EnvironmentObject private var model: FieldModel
    @Environment(\.dismiss) private var dismiss
    @State private var draft: Draft
    @State private var occurredAt: String
    @State private var pickerDate: Date
    @State private var status: String
    @State private var failed = false
    init(initial: Draft) {
        let value = initial.occurredAt ?? stamp()
        _draft = State(initialValue: initial)
        _occurredAt = State(initialValue: value)
        _pickerDate = State(initialValue: (try? date(value)) ?? Date())
        _status = State(initialValue: initial.revision > 0 ? "Draft saved on device · revision \(initial.revision)" : "Not yet saved")
    }
    var body: some View {
        NavigationStack {
            Form {
                Section("Manual contact") {
                    Picker("Channel", selection: binding(\.contactChannel, fallback: "call")) {
                        ForEach(["call", "text", "email", "other"], id: \.self) { Text($0.capitalized).tag($0) }
                    }.onChange(of: draft.contactChannel) { _, _ in autosave() }
                    Picker("Outcome", selection: binding(\.contactOutcome, fallback: "reached")) {
                        ForEach(["reached", "no_answer", "left_message", "sent", "busy", "wrong_number"], id: \.self) { outcome in Text(outcome.replacingOccurrences(of: "_", with: " ").capitalized).tag(outcome) }
                    }.onChange(of: draft.contactOutcome) { _, _ in autosave() }
                    Text("Calls made through the CRM already have a contact record. Use this form only for a manual interaction that already happened.").font(.caption).foregroundStyle(.secondary)
                }
                Section("When it happened") {
                    DatePicker("UTC date and time", selection: $pickerDate, displayedComponents: [.date, .hourAndMinute])
                        .environment(\.timeZone, TimeZone(secondsFromGMT: 0)!)
                        .accessibilityIdentifier("contactOccurredAtUTC")
                        .onChange(of: pickerDate) { _, value in occurredAt = stamp(value); draft.occurredAt = occurredAt; autosave() }
                    LabeledContent("Recorded instant") { Text(occurredAt).font(.caption.monospaced()).textSelection(.enabled) }
                    Text("Time zone: UTC (fixed). UTC has no daylight-saving gaps or repeated wall-clock times. The selected instant remains unchanged if this device’s time zone changes.").font(.caption).foregroundStyle(.secondary)
                }
                Section { Text(status).font(.caption).foregroundStyle(failed ? .red : .secondary).accessibilityIdentifier("contactDraftStatus") }
                Button("Save contact on device") {
                    do { if draft.revision == 0 { draft = try model.save(draft) }; try model.submit(draft); dismiss() }
                    catch { status = error.localizedDescription; failed = true }
                }.disabled(occurredAt.isEmpty || failed).accessibilityIdentifier("saveContact")
                if failed { Button("Retry saving contact draft") { autosave() } }
            }.navigationTitle("Log contact")
                .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Close") { dismiss() }.disabled(failed) } }
                .interactiveDismissDisabled(failed)
        }
    }
    private func binding(_ keyPath: WritableKeyPath<Draft, String?>, fallback: String) -> Binding<String> {
        Binding(get: { draft[keyPath: keyPath] ?? fallback }, set: { draft[keyPath: keyPath] = $0 })
    }
    private func autosave() {
        do { draft = try model.save(draft); status = "Draft saved on device · revision \(draft.revision)"; failed = false }
        catch { status = error.localizedDescription; failed = true }
    }
}
enum MetadataEditorProjection {
    static func dateValue(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(secondsFromGMT: 0)
        formatter.dateFormat = "yyyy-MM-dd"
        return formatter.string(from: date)
    }
    static func pickerDate(_ raw: String) -> Date {
        let formatter = DateFormatter()
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(secondsFromGMT: 0)
        formatter.dateFormat = "yyyy-MM-dd"
        return formatter.date(from: raw) ?? Date()
    }
    static func display(_ value: JSON) -> String {
        for key in ["text", "number", "date", "option_id"] where !value[key].text.isEmpty { return value[key].text }
        return ""
    }
    static func valueMap(_ baseline: JSON) -> [String: JSON] {
        Dictionary(uniqueKeysWithValues: baseline["values"].list.map { ($0["field_id"].text, $0["value"]) })
    }
    static func actions(tags: Set<String>, baseline: JSON, text: [String: String], number: [String: String], dates: [String: String], choices: [String: String], clears: Set<String>) -> [JSON] {
        let originalTags = Set(baseline["tags"].list.map { $0["id"].text })
        var actions = tags.subtracting(originalTags).sorted().map { JSON.object(["kind": .s("add_tag"), "tag_id": .s($0)]) }
        actions += originalTags.subtracting(tags).sorted().map { JSON.object(["kind": .s("remove_tag"), "tag_id": .s($0)]) }
        let original = valueMap(baseline)
        for field in baseline["fields"].list {
            let id = field["id"].text, type = field["field_type"].text
            guard !id.isEmpty else { continue }
            if clears.contains(id) {
                if original[id] != nil { actions.append(.object(["kind": .s("clear_field"), "field_id": .s(id)])) }
                continue
            }
            let raw: String? = type == "text" ? text[id] : type == "number" ? number[id] : type == "date" ? dates[id] : choices[id]
            guard let raw, !raw.isEmpty else { continue }
            let value: JSON = type == "text" ? .object(["text": .s(raw)]) : type == "number" ? .object(["number": .s(raw)]) : type == "date" ? .object(["date": .s(raw)]) : .object(["option_id": .s(raw)])
            if original[id] != value { actions.append(.object(["kind": .s("set_field"), "field_id": .s(id), "value": value])) }
        }
        return actions
    }
}

struct MetadataComposerView: View {
    @EnvironmentObject private var model: FieldModel
    @Environment(\.dismiss) private var dismiss
    @State private var draft: Draft
    @State private var tags: Set<String>
    @State private var text: [String: String] = [:]
    @State private var number: [String: String] = [:]
    @State private var dates: [String: String] = [:]
    @State private var choices: [String: String] = [:]
    @State private var clears: Set<String> = []
    @State private var status = "Not yet saved"
    @State private var failed = false
    @State private var excludedConflictActions = Set<Int>()
    init(initial: Draft) {
        _draft = State(initialValue: initial)
        let baseline = initial.baseline ?? .object([:])
        var selectedTags = Set(baseline["tags"].list.map { $0["id"].text })
        var restoredClears = Set<String>()
        let values = MetadataEditorProjection.valueMap(baseline)
        var t: [String: String] = [:], n: [String: String] = [:], d: [String: String] = [:], c: [String: String] = [:]
        for field in baseline["fields"].list {
            let id = field["id"].text, value = values[id]
            switch field["field_type"].text { case "text": t[id] = value?["text"].text ?? ""; case "number": n[id] = value?["number"].text ?? ""; case "date": d[id] = value?["date"].text ?? ""; case "choice": c[id] = value?["option_id"].text ?? ""; default: break }
        }
        // Reopening a saved metadata draft must reconstruct its sparse action
        // list before another control autosaves; otherwise that later edit
        // would silently erase earlier tag/field choices.
        for action in initial.proposal?["actions"].list ?? [] {
            let id = action["tag_id"].text.isEmpty ? action["field_id"].text : action["tag_id"].text
            switch action["kind"].text {
            case "add_tag": selectedTags.insert(id)
            case "remove_tag": selectedTags.remove(id)
            case "clear_field": restoredClears.insert(id)
            case "set_field":
                restoredClears.remove(id)
                if let field = baseline["fields"].list.first(where: { $0["id"].text == id }) {
                    switch field["field_type"].text {
                    case "text": t[id] = action["value"]["text"].text
                    case "number": n[id] = action["value"]["number"].text
                    case "date": d[id] = action["value"]["date"].text
                    case "choice": c[id] = action["value"]["option_id"].text
                    default: break
                    }
                }
            default: break
            }
        }
        _tags = State(initialValue: selectedTags)
        _text = State(initialValue: t); _number = State(initialValue: n); _dates = State(initialValue: d); _choices = State(initialValue: c)
        _clears = State(initialValue: restoredClears)
        _status = State(initialValue: initial.revision > 0 ? "Draft saved on device · revision \(initial.revision)" : "Not yet saved")
    }
    var baseline: JSON { draft.baseline ?? .object([:]) }
    var body: some View {
        NavigationStack { Form {
            if draft.mode == "conflict" {
                Section("Conflict requires review") {
                    Text("Your saved proposal is protected. The catalog or metadata changed on the server.")
                    metadataComparison("Version you started from", draft.baseline, identifier: "metadataComparison_baseline")
                    metadataComparison("Your proposed changes", draft.proposal, identifier: "metadataComparison_proposed")
                    metadataComparison("Current server values", draft.current, identifier: "metadataComparison_current")
                    Button("Fetch current values") { Task { do { draft = try await model.requalifyMetadata(draft); status = "Current values fetched. Create the revised proposal when the sealed catalog revision matches."; failed = false } catch LocalError.invalidProtocol { status = "Refresh the workspace to download the catalog matching these current values, then fetch again."; failed = false } catch { fail(error) } } }
                    let incompatible = (try? model.invalidMetadataActionIndexes(draft)) ?? []
                    if !incompatible.isEmpty {
                        Text("Some saved actions no longer target the refreshed catalog. Choose each action to exclude; the original conflict remains protected until a valid replacement saves.").font(.caption).foregroundStyle(.orange)
                        ForEach(Array(incompatible).sorted(), id: \.self) { index in
                            Toggle("Exclude: \(metadataActionLabel(draft.proposal?["actions"].list[index] ?? .null))", isOn: conflictExclusionBinding(index))
                                .accessibilityIdentifier("excludeMetadataAction_\(index)")
                        }
                    }
                    Button("Prepare revised proposal against current values") { do {
                        let all = draft.proposal?["actions"].list.indices ?? 0..<0
                        let revised = try model.revisedMetadataDraft(draft, retaining: Set(all).subtracting(excludedConflictActions))
                        draft = revised; restoreControls(revised); status = "Revised proposal saved on device"; failed = false
                    } catch { fail(error) } }.disabled(!incompatible.isSubset(of: excludedConflictActions))
                    Button("Use current values and discard my saved proposal", role: .destructive) { do { try model.resolveUsingCurrent(draft); dismiss() } catch { fail(error) } }
                }
            } else {
                Section("Tags") {
                    ForEach(Array(allTags.enumerated()), id: \.offset) { _, tag in Toggle(tag["name"].text, isOn: Binding(get: { tags.contains(tag["id"].text) }, set: { selected in if selected { tags.insert(tag["id"].text) } else { tags.remove(tag["id"].text) }; autosave() })).accessibilityIdentifier("metadataTag_\(tag["id"].text)") }
                }
                #if MOBILE006_UPGRADE_QA
                Button("Stage metadata upgrade proof") {
                    guard let tag = allTags.first else { return }
                    if let missing = allTags.first(where: { !tags.contains($0["id"].text) }) { tags.insert(missing["id"].text) }
                    else { tags.remove(tag["id"].text) }
                    autosave()
                }.accessibilityIdentifier("stageMetadataUpgradeProof")
                #endif
                ForEach(Array(baseline["fields"].list.enumerated()), id: \.offset) { _, field in fieldEditor(field) }
            }
            Section { Text(status).font(.caption).foregroundStyle(failed ? .red : .secondary).accessibilityIdentifier("metadataDraftStatus") }
            if draft.mode != "conflict" { Button("Save metadata proposal on device") { do { if draft.revision == 0 { autosave() }; guard !failed else { return }; try model.submit(draft); dismiss() } catch { fail(error) } }.disabled(failed).accessibilityIdentifier("saveMetadata") }
        }.navigationTitle("Tags and fields").toolbar { ToolbarItem(placement: .cancellationAction) { Button("Close") { dismiss() }.disabled(failed) } }.interactiveDismissDisabled(failed) }
    }
    var allTags: [JSON] { baseline["catalog_tags"].list.isEmpty ? baseline["tags"].list : baseline["catalog_tags"].list }
    @ViewBuilder func fieldEditor(_ field: JSON) -> some View {
        let id = field["id"].text, archived = field["archived_at"] != .null
        Section(field["label"].text + (archived ? " · archived" : "")) {
            if field["field_type"].text == "text" { TextField("Text", text: binding($text, id)).disabled(archived).onChange(of: text[id] ?? "") { _, _ in clears.remove(id); autosave() }.accessibilityIdentifier("metadataText_\(id)") }
            else if field["field_type"].text == "number" { TextField("Exact decimal", text: binding($number, id)).keyboardType(.decimalPad).disabled(archived).onChange(of: number[id] ?? "") { _, _ in clears.remove(id); autosave() }.accessibilityIdentifier("metadataNumber_\(id)") }
            else if field["field_type"].text == "date" { DatePicker("Date", selection: Binding(get: { MetadataEditorProjection.pickerDate(dates[id] ?? "") }, set: { dates[id] = MetadataEditorProjection.dateValue($0) }), displayedComponents: .date).environment(\.timeZone, TimeZone(secondsFromGMT: 0)!).disabled(archived).onChange(of: dates[id] ?? "") { _, _ in clears.remove(id); autosave() }.accessibilityIdentifier("metadataDate_\(id)") }
            else if field["field_type"].text == "choice" { Picker("Choice", selection: binding($choices, id)) { Text("Choose").tag(""); ForEach(Array(options(for: id).enumerated()), id: \.offset) { _, option in Text(option["label"].text + (option["archived_at"] == .null ? "" : " · archived")).tag(option["id"].text).disabled(option["archived_at"] != .null) } }.disabled(archived).onChange(of: choices[id] ?? "") { _, _ in clears.remove(id); autosave() }.accessibilityIdentifier("metadataChoice_\(id)") }
            if MetadataEditorProjection.valueMap(baseline)[id] != nil { Button("Clear value", role: .destructive) { clears.insert(id); autosave() }.accessibilityIdentifier("metadataClear_\(id)") }
        }
    }
    func options(for field: String) -> [JSON] { baseline["options"].list.filter { $0["field_id"].text == field } }
    @ViewBuilder func metadataComparison(_ title: String, _ value: JSON?, identifier: String) -> some View {
        if let value {
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.caption.bold())
                Text(metadataSummary(value)).font(.caption.monospaced()).textSelection(.enabled).accessibilityIdentifier(identifier)
            }.padding(.vertical, 2)
        }
    }
    func metadataSummary(_ value: JSON) -> String {
        // A requalified current read carries the matching fresh catalog. Use it
        // over the original baseline so labels never describe a newer value
        // using definitions from the conflicted catalog revision.
        let catalog = value["catalog_tags"].list.isEmpty && value["fields"].list.isEmpty ? (draft.baseline ?? value) : value
        let tagName: (String) -> String = { id in catalog["catalog_tags"].list.first(where: { $0["id"].text == id })?["name"].text ?? id }
        let fieldName: (String) -> String = { id in catalog["fields"].list.first(where: { $0["id"].text == id })?["label"].text ?? id }
        let optionName: (String) -> String = { id in catalog["options"].list.first(where: { $0["id"].text == id })?["label"].text ?? id }
        var lines: [String] = []
        let names = value["tags"].list.map { tagName($0["id"].text) }
        lines.append("tags: " + (names.isEmpty ? "none" : names.joined(separator: ", ")))
        let displayValue: (JSON) -> String = { item in
            let raw = MetadataEditorProjection.display(item)
            return item["option_id"].text.isEmpty ? raw : optionName(item["option_id"].text)
        }
        if value["values"].list.isEmpty { lines.append("custom fields: none") }
        for item in value["values"].list { lines.append("\(fieldName(item["field_id"].text)): \(displayValue(item["value"]))") }
        for action in value["actions"].list {
            switch action["kind"].text {
            case "add_tag": lines.append("add tag: \(tagName(action["tag_id"].text))")
            case "remove_tag": lines.append("remove tag: \(tagName(action["tag_id"].text))")
            case "clear_field": lines.append("clear: \(fieldName(action["field_id"].text))")
            case "set_field": lines.append("set \(fieldName(action["field_id"].text)): \(displayValue(action["value"]))")
            default: break
            }
        }
        return lines.joined(separator: "\n")
    }
    func metadataActionLabel(_ action: JSON) -> String {
        let catalog: JSON
        if let current = draft.current, !current["catalog_tags"].list.isEmpty { catalog = current }
        else { catalog = draft.baseline ?? .null }
        let tag = catalog["catalog_tags"].list.first(where: { $0["id"].text == action["tag_id"].text })?["name"].text ?? action["tag_id"].text
        let field = catalog["fields"].list.first(where: { $0["id"].text == action["field_id"].text })?["label"].text ?? action["field_id"].text
        switch action["kind"].text {
        case "add_tag": return "add tag \(tag)"
        case "remove_tag": return "remove tag \(tag)"
        case "clear_field": return "clear \(field)"
        case "set_field": return "set \(field)"
        default: return "invalid saved action"
        }
    }
    func conflictExclusionBinding(_ index: Int) -> Binding<Bool> {
        Binding(get: { excludedConflictActions.contains(index) }, set: { selected in
            if selected { excludedConflictActions.insert(index) }
            else { excludedConflictActions.remove(index) }
        })
    }
    func restoreControls(_ source: Draft) {
        let base = source.baseline ?? .object([:]); let values = MetadataEditorProjection.valueMap(base)
        tags = Set(base["tags"].list.map { $0["id"].text }); clears = []
        text = [:]; number = [:]; dates = [:]; choices = [:]
        for field in base["fields"].list { let id = field["id"].text, value = values[id]; switch field["field_type"].text { case "text": text[id] = value?["text"].text ?? ""; case "number": number[id] = value?["number"].text ?? ""; case "date": dates[id] = value?["date"].text ?? ""; case "choice": choices[id] = value?["option_id"].text ?? ""; default: break } }
        for action in source.proposal?["actions"].list ?? [] { let id = action["tag_id"].text.isEmpty ? action["field_id"].text : action["tag_id"].text; switch action["kind"].text { case "add_tag": tags.insert(id); case "remove_tag": tags.remove(id); case "clear_field": clears.insert(id); case "set_field": clears.remove(id); let kind = base["fields"].list.first(where: { $0["id"].text == id })?["field_type"].text; if kind == "text" { text[id] = action["value"]["text"].text } else if kind == "number" { number[id] = action["value"]["number"].text } else if kind == "date" { dates[id] = action["value"]["date"].text } else if kind == "choice" { choices[id] = action["value"]["option_id"].text }; default: break } }
    }
    func binding(_ source: Binding<[String: String]>, _ id: String) -> Binding<String> { Binding(get: { source.wrappedValue[id] ?? "" }, set: { source.wrappedValue[id] = $0 }) }
    func autosave() { do { let actions = MetadataEditorProjection.actions(tags: tags, baseline: baseline, text: text, number: number, dates: dates, choices: choices, clears: clears); draft.proposal = .object(["actions": .array(actions)]); draft = try model.save(draft); status = "Draft saved on device · revision \(draft.revision)"; failed = false } catch { fail(error) } }
    func fail(_ error: Error) { status = error.localizedDescription; failed = true }
}

struct QueueView: View {
    @EnvironmentObject private var model: FieldModel
    @State private var composer: Draft?
    @State private var contactComposer: Draft?
    @State private var stageProposal: StageProposal?
    @State private var detailsComposer: Draft?
    @State private var metadataComposer: Draft?
    var body: some View {
        List {
            Section { Text("\(model.pendingCount) pending · \(model.drafts.count) saved drafts").accessibilityIdentifier("queueCount") }
            if !model.drafts.isEmpty {
                Section("Drafts") { ForEach(model.drafts) { draft in
                    Button(draft.kind == "log_contact_attempt" ? "Manual contact · " + [draft.contactChannel, draft.contactOutcome, draft.occurredAt].compactMap { $0 }.joined(separator: " · ") : (draft.kind == "change_person_stage" ? "Stage proposal · " + (draft.proposal?["name"].text ?? "") : (draft.text.isEmpty ? "Empty draft" : draft.text))) {
                        if draft.kind == "log_contact_attempt" { contactComposer = draft }
                        else if draft.kind == "change_person_stage" { stageProposal = try? model.revisedStageProposal(draft) }
                        else if draft.kind == "update_person_details" { detailsComposer = draft }
                        else if draft.kind == "update_person_metadata" { metadataComposer = draft }
                        else { composer = draft }
                    }.accessibilityIdentifier(draft.kind == "change_person_stage" && draft.mode == "follow_up" ? "stageFollowUp_" + draft.id : "draft_" + draft.id)
                    if draft.mode == "follow_up" { Text("Saved draft — waiting for the previous change").font(.caption).foregroundStyle(.orange) }
                    if draft.mode == "conflict" { Text("Conflict requires review").font(.caption).foregroundStyle(.orange) }
                } }
            }
            Section("Actions") {
                ForEach(model.queue.reversed()) { op in
                    VStack(alignment: .leading, spacing: 8) {
                        Label(op.status == "accepted" ? "Synced" : op.status == "attention" ? "Needs attention" : "Saved on device", systemImage: op.status == "accepted" ? "checkmark.circle" : op.status == "attention" ? "exclamationmark.triangle" : "tray")
                        Text(op.envelope.kind.replacingOccurrences(of: "_", with: " ").capitalized).font(.subheadline.bold())
                        if op.error != "not_found" && op.error != "forbidden" { Text(op.title.isEmpty ? (op.isContact ? "Manual contact attempt" : "Task completion") : op.title).lineLimit(4) }
                        if let error = op.error { Text(APIError(status: 409, code: error).localizedDescription).font(.caption).foregroundStyle(.secondary) }
                        if op.status == "conflict", let draft = model.drafts.first(where: { $0.predecessor == op.id }) {
                            Button("Review conflict") {
                                if op.isStage { stageProposal = try? model.revisedStageProposal(draft) }
                                else if op.isDetails { detailsComposer = draft }
                                else if op.isMetadata { metadataComposer = draft }
                                else { composer = draft }
                            }
                        } else if op.status == "attention" && op.error == "contact_time_in_future" && op.isContact {
                            Button("Correct reported time in a new contact") { contactComposer = try? model.revisedContactDraft(from: op) }
                            Text("The original saved action remains unchanged because the server rejected its reported future time.").font(.caption).foregroundStyle(.secondary)
                        } else if op.status == "attention" && ["invalid_input", "invalid_assignee", "over_limit"].contains(op.error ?? "") && op.envelope.kind != "complete_task" && !op.isContact {
                            Button("Prepare a separate revised draft") { composer = Draft(id: UUID().uuidString, person: op.envelope.person, kind: op.envelope.kind, text: op.title, revision: 0) }
                        }
                        DisclosureGroup("Sync details") { Text(op.id).font(.caption2).foregroundStyle(.secondary).textSelection(.enabled) }
                    }.padding(.vertical, 5)
                }
            }
        }.navigationTitle("Saved work").sheet(item: $composer) { draft in ComposerView(initial: draft).environmentObject(model) }
            .sheet(item: $contactComposer) { draft in ContactComposerView(initial: draft).environmentObject(model) }
            .sheet(item: $stageProposal) { proposal in StageProposalView(initial: proposal).environmentObject(model) }
            .sheet(item: $detailsComposer) { draft in DetailsComposerView(initial: draft).environmentObject(model) }
            .sheet(item: $metadataComposer) { draft in MetadataComposerView(initial: draft).environmentObject(model) }
    }
}
