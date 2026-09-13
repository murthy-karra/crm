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
                        Section("About this build") { Text("Mobile 001 · Synthetic development. Calls, messages, editing existing records and background delivery are outside this field workflow.").font(.footnote) }
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
                let pendingTasks = model.queue.filter { $0.envelope.kind != "add_note" && $0.overlay }.count
                Label("\(pendingTasks) local task changes · see Saved work", systemImage: "tray").font(.caption)
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
                }
                if !overlays.isEmpty {
                    Section("Saved changes on this device") {
                        ForEach(overlays) { op in
                            VStack(alignment: .leading, spacing: 6) {
                                Text(op.envelope.kind.replacingOccurrences(of: "_", with: " ").capitalized).font(.caption.bold())
                                if op.error != "not_found" && op.error != "forbidden" { Text(op.title.isEmpty ? "Task completion" : op.title) }
                                Text(op.status == "accepted" ? "Synced · waiting for a covering download" : op.status == "attention" ? "Needs attention" : "Saved on device").font(.caption).foregroundStyle(.secondary)
                                if op.envelope.kind == "create_task" && !model.queue.contains(where: { $0.envelope.payload["target"]["created_by_operation_id"].text == op.id }) {
                                    Button("Complete saved task") { model.complete(person: personID, target: .object(["created_by_operation_id": .s(op.id)])) }
                                }
                            }
                        }
                    }
                }
                Section("Tasks") {
                    ForEach(Array(bundle.tasks.enumerated()), id: \.offset) { _, task in
                        let pending = model.queue.last { $0.overlay && $0.envelope.payload["target"]["task_id"].text == task["id"].text }
                        VStack(alignment: .leading, spacing: 6) {
                            Text(task["title"].text)
                            Text(task["kind"].text.replacingOccurrences(of: "_", with: " ") + (task["due_at"].text.isEmpty ? "" : " · " + task["due_at"].text)).font(.caption).foregroundStyle(.secondary)
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
                Section("Notes") {
                    ForEach(Array(bundle.notes.enumerated()), id: \.offset) { _, note in
                        VStack(alignment: .leading, spacing: 7) {
                            Text(note["body"].text).textSelection(.enabled)
                            Text(note["author"]["display_name"].text + " · " + note["created_at"].text).font(.caption).foregroundStyle(.secondary)
                        }.padding(.vertical, 4)
                    }
                }
            } else { Text("This record is not in the complete downloaded selection. Saved actions remain in Saved work.") }
        }.navigationTitle("Person").navigationBarTitleDisplayMode(.inline)
            .sheet(item: $composer) { draft in ComposerView(initial: draft).environmentObject(model) }
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
                Section(draft.kind == "add_note" ? "Note" : "Task title") {
                    TextEditor(text: $draft.text).frame(minHeight: 150).accessibilityIdentifier("composerText")
                        .onChange(of: draft.text) { _, _ in autosave() }
                }
                if draft.kind == "create_task" {
                    Picker("Kind", selection: $draft.taskKind) { ForEach(["call", "email", "text", "follow_up", "other"], id: \.self) { Text($0.replacingOccurrences(of: "_", with: " ").capitalized).tag($0) } }.onChange(of: draft.taskKind) { _, _ in autosave() }
                    Toggle("Set a due date", isOn: $hasDue).onChange(of: hasDue) { _, value in draft.dueAt = value ? stamp(due) : nil; autosave() }
                    if hasDue { DatePicker("Due", selection: $due).onChange(of: due) { _, value in draft.dueAt = stamp(value); autosave() } }
                    Text("Assigned to you. Server validation applies when this task syncs.").font(.caption)
                }
                Section { Text(status).font(.caption).foregroundStyle(failed ? .red : .secondary).accessibilityIdentifier("draftStatus") }
                Button("Save action on device") {
                    do { if draft.revision == 0 { draft = try model.save(draft) }; try model.submit(draft); dismiss() }
                    catch { status = error.localizedDescription; failed = true }
                }.disabled(draft.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || failed).accessibilityIdentifier("saveAction")
                if failed { Button("Retry saving draft") { autosave() } }
            }.navigationTitle(draft.kind == "add_note" ? "New note" : "New task")
                .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Close") { dismiss() }.disabled(failed) } }
                .interactiveDismissDisabled(failed)
        }
    }
    private func autosave() {
        do { draft = try model.save(draft); status = "Draft saved on device · revision \(draft.revision)"; failed = false }
        catch { status = error.localizedDescription; failed = true }
    }
}
struct QueueView: View {
    @EnvironmentObject private var model: FieldModel
    @State private var composer: Draft?
    var body: some View {
        List {
            Section { Text("\(model.pendingCount) pending · \(model.drafts.count) saved drafts").accessibilityIdentifier("queueCount") }
            if !model.drafts.isEmpty {
                Section("Drafts") { ForEach(model.drafts) { draft in Button(draft.text.isEmpty ? "Empty draft" : draft.text) { composer = draft } } }
            }
            Section("Actions") {
                ForEach(model.queue.reversed()) { op in
                    VStack(alignment: .leading, spacing: 8) {
                        Label(op.status == "accepted" ? "Synced" : op.status == "attention" ? "Needs attention" : "Saved on device", systemImage: op.status == "accepted" ? "checkmark.circle" : op.status == "attention" ? "exclamationmark.triangle" : "tray")
                        Text(op.envelope.kind.replacingOccurrences(of: "_", with: " ").capitalized).font(.subheadline.bold())
                        if op.error != "not_found" && op.error != "forbidden" { Text(op.title.isEmpty ? "Task completion" : op.title).lineLimit(4) }
                        if let error = op.error { Text(APIError(status: 409, code: error).localizedDescription).font(.caption).foregroundStyle(.secondary) }
                        if op.status == "attention" && ["invalid_input", "invalid_assignee", "over_limit"].contains(op.error ?? "") && op.envelope.kind != "complete_task" {
                            Button("Prepare a separate revised draft") { composer = Draft(id: UUID().uuidString, person: op.envelope.person, kind: op.envelope.kind, text: op.title, revision: 0) }
                        }
                        DisclosureGroup("Sync details") { Text(op.id).font(.caption2).foregroundStyle(.secondary).textSelection(.enabled) }
                    }.padding(.vertical, 5)
                }
            }
        }.navigationTitle("Saved work").sheet(item: $composer) { draft in ComposerView(initial: draft).environmentObject(model) }
    }
}
