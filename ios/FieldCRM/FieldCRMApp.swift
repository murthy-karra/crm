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
    var filtered: [Bundle] { model.people.filter { search.isEmpty || $0.summary["display_name"].text.localizedCaseInsensitiveContains(search) } }
    var body: some View {
        List {
            Section { Text("\(model.people.count) downloaded people · assigned, Today and saved records").font(.caption).foregroundStyle(.secondary) }
            ForEach(filtered, id: \.person) { person in
                NavigationLink { PersonView(personID: person.person) } label: {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(person.summary["display_name"].text).font(.headline)
                        Text(person.summary["stage"]["name"].text).font(.caption).foregroundStyle(.secondary)
                    }.padding(.vertical, 3)
                }.accessibilityIdentifier("person_" + person.person)
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
    var bundle: Bundle? { model.people.first { $0.person == personID } }
    var overlays: [Queued] { model.queue.filter { $0.envelope.person == personID && $0.overlay } }
    var body: some View {
        List {
            if let bundle {
                Section {
                    Text(bundle.summary["display_name"].text).font(.title2.bold())
                    Label(bundle.summary["stage"]["name"].text, systemImage: "flag")
                    Text("Responsible: " + (bundle.summary["assigned_user"]["display_name"].text.isEmpty ? "Unassigned" : bundle.summary["assigned_user"]["display_name"].text)).font(.subheadline)
                    ForEach(Array(bundle.contacts.enumerated()), id: \.offset) { _, contact in Text(contact["value"].text).textSelection(.enabled) }
                }
                Section {
                    Button("Add note") { composer = Draft(id: UUID().uuidString, person: personID, kind: "add_note", text: "", revision: 0) }.accessibilityIdentifier("addNote")
                    Button("Create task") { composer = Draft(id: UUID().uuidString, person: personID, kind: "create_task", text: "", revision: 0) }.accessibilityIdentifier("createTask")
                    Button("Log contact") { contactComposer = try? model.newContactDraft(person: personID) }.disabled(!model.canLogContact).accessibilityIdentifier("logContact")
                    Text(model.canLogContact ? "For a manual interaction that already happened. Calls made through the CRM already have a contact record." : "Contact logging is unavailable for this account. Existing saved work is retained.").font(.caption).foregroundStyle(.secondary)
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
struct QueueView: View {
    @EnvironmentObject private var model: FieldModel
    @State private var composer: Draft?
    @State private var contactComposer: Draft?
    var body: some View {
        List {
            Section { Text("\(model.pendingCount) pending · \(model.drafts.count) saved drafts").accessibilityIdentifier("queueCount") }
            if !model.drafts.isEmpty {
                Section("Drafts") { ForEach(model.drafts) { draft in
                    Button(draft.kind == "log_contact_attempt" ? "Manual contact · " + [draft.contactChannel, draft.contactOutcome, draft.occurredAt].compactMap { $0 }.joined(separator: " · ") : (draft.text.isEmpty ? "Empty draft" : draft.text)) {
                        if draft.kind == "log_contact_attempt" { contactComposer = draft } else { composer = draft }
                    }
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
                            Button("Review conflict") { composer = draft }
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
    }
}
