package org.crm.field

import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import java.time.Instant
import java.time.LocalDateTime
import java.time.OffsetDateTime
import java.time.ZoneId
import java.util.UUID
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject

class MainActivity : ComponentActivity() {
    private var clockWatch: kotlinx.coroutines.Job? = null
    private val repository
        get() = (application as FieldApplication).repository

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
        if (
            BuildConfig.DEBUG &&
                android.os.Build.VERSION.SDK_INT >= 37 &&
                checkSelfPermission("android.permission.ACCESS_LOCAL_NETWORK") !=
                    android.content.pm.PackageManager.PERMISSION_GRANTED
        ) {
            requestPermissions(arrayOf("android.permission.ACCESS_LOCAL_NETWORK"), 37)
        }
        setContent {
            MaterialTheme(
                colorScheme =
                    lightColorScheme(
                        primary = Color(0xFF006B60),
                        secondary = Color(0xFF51625D),
                        background = Color(0xFFF7FAF8),
                        surface = Color.White,
                    )
            ) {
                FieldApp(repository)
            }
        }
    }

    override fun onResume() {
        super.onResume()
        clockWatch =
            repository.scope.launch {
                (application as FieldApplication).ready.await()
                repository.foreground()
                while (kotlinx.coroutines.currentCoroutineContext().isActive) {
                    delay(30_000)
                    repository.foreground()
                }
            }
    }

    override fun onPause() {
        clockWatch?.cancel()
        super.onPause()
    }

    override fun onStop() {
        super.onStop()
        FieldSyncJob.schedule(this)
    }
}

private data class ComposerLaunch(val person: String, val kind: String, val target: JSONObject? = null)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun FieldApp(repository: FieldRepository) {
    val state by repository.ui.collectAsState()
    val scope = rememberCoroutineScope()
    var tab by remember { mutableStateOf("Today") }
    var composer by remember { mutableStateOf<ComposerLaunch?>(null) }
    var contactComposer by remember { mutableStateOf<Pair<String, String>?>(null) }
    var stageComposer by remember { mutableStateOf<Pair<String, String>?>(null) }
    var profileComposer by remember { mutableStateOf<Pair<String, String>?>(null) }
    var signOut by remember { mutableStateOf(false) }
    var notice by remember { mutableStateOf("") }
    LaunchedEffect(state.locked) {
        if (state.locked) {
            composer = null
            contactComposer = null
            stageComposer = null
            profileComposer = null
            signOut = false
            tab = "Today"
            notice = ""
        }
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Text(
                        if (state.person != null) "Person" else "CRM Field",
                        fontWeight = FontWeight.SemiBold,
                    )
                },
                navigationIcon = {
                    if (state.person != null)
                        TextButton(onClick = { repository.select(null) }) { Text("Back") }
                },
                actions = {
                    if (!state.locked) TextButton(onClick = { signOut = true }) { Text("Sign out") }
                },
            )
        },
        bottomBar = {
            if (!state.locked)
                NavigationBar {
                    listOf("Today", "People", "Saved work").forEach { label ->
                        NavigationBarItem(
                            modifier = Modifier.testTag("nav-$label"),
                            selected = tab == label && state.person == null,
                            onClick = {
                                tab = label
                                repository.select(null)
                            },
                            icon = {
                                Text(
                                    when (label) {
                                        "Today" -> "◷"
                                        "People" -> "◎"
                                        else -> "✓"
                                    }
                                )
                            },
                            label = { Text(label) },
                        )
                    }
                }
        },
    ) { padding ->
        if (state.locked) SignIn(repository, state, Modifier.padding(padding))
        else
            Column(Modifier.padding(padding).fillMaxSize()) {
                Surface(
                    color =
                        if (state.paused) MaterialTheme.colorScheme.tertiaryContainer
                        else MaterialTheme.colorScheme.primaryContainer
                ) {
                    Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp)) {
                        Text(state.message, style = MaterialTheme.typography.bodyMedium)
                        Text(
                            "${state.coverage} · Last sync ${displayTime(state.lastSync)}",
                            style = MaterialTheme.typography.labelSmall,
                        )
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            TextButton(
                                onClick = { repository.requestSync(true) },
                                enabled = !state.busy && !state.paused,
                            ) {
                                Text(if (state.busy) "Syncing…" else "Sync now")
                            }
                            TextButton(
                                onClick = { scope.launch { repository.pause(!state.paused) } }
                            ) {
                                Text(if (state.paused) "Resume sync" else "Pause sync")
                            }
                        }
                        if (state.busy) LinearProgressIndicator(Modifier.fillMaxWidth())
                    }
                }
                if (notice.isNotEmpty())
                    Text(notice, Modifier.padding(16.dp), color = MaterialTheme.colorScheme.error)
                when {
                    state.person != null ->
                        PersonScreen(
                            state,
                            repository,
                            onCompose = { kind, target -> composer = ComposerLaunch(state.person!!.id, kind, target) },
                            onContact = {
                                contactComposer = state.person!!.id to UUID.randomUUID().toString()
                            },
                            onStage = {
                                stageComposer = state.person!!.id to UUID.randomUUID().toString()
                            },
                            onProfile = { profileComposer = state.person!!.id to UUID.randomUUID().toString() },
                            onError = { notice = it },
                        )
                    tab == "People" -> PeopleScreen(state, repository, onError = { notice = it })
                    tab == "Saved work" ->
                        SavedWork(
                            state,
                            repository,
                            onDraft = {
                                if (state.people.any { person -> person.id == it.person })
                                    composer =
                                        ComposerLaunch(
                                            it.person,
                                            it.kind,
                                            it.baseline.takeIf { baseline -> baseline.isNotBlank() }
                                                ?.let(::JSONObject),
                                        )
                                else
                                    notice =
                                        "This Person is outside the current offline selection. Saved input remains protected; request availability before reopening it."
                            },
                            onRevise = { row, current ->
                                scope.launch {
                                    try {
                                        val fresh = repository.supersedeConflict(row.id)
                                        composer = ComposerLaunch(row.person, row.kind, fresh)
                                    } catch (_: Exception) {
                                        notice = "Current version could not be prepared. The original saved proposal remains protected."
                                    }
                                }
                            },
                            onContactDraft = { draft ->
                                if (state.people.any { person -> person.id == draft.person })
                                    contactComposer = draft.person to draft.id
                                else
                                    notice =
                                        "This Person is outside the current offline selection. Saved input remains protected; request availability before reopening it."
                            },
                            onRepairFutureContact = { row ->
                                scope.launch {
                                    try {
                                        val replacement = repository.reviseFutureContact(row.operation)
                                        contactComposer = replacement.person to replacement.id
                                    } catch (_: Exception) {
                                        notice = "The original contact remains protected; its corrected draft could not be prepared."
                                    }
                                }
                            },
                            onStageDraft = { draft -> stageComposer = draft.person to draft.id },
                            onReviseStage = { row ->
                                scope.launch {
                                    try { stageComposer = row.person to repository.reviseStageConflict(row.id).id }
                                    catch (_: Exception) { notice = "The original stage proposal remains protected; a revised proposal could not be prepared." }
                                }
                            },
                            onDiscardStage = { row -> scope.launch { repository.discardStageConflict(row.id) } },
                            onProfileDraft = { draft -> profileComposer = draft.person to draft.id },
                            onReviseProfile = { row ->
                                scope.launch { try { profileComposer = row.person to repository.reviseProfileConflict(row.id).id } catch (_: Exception) { notice = "The original profile proposal remains protected; a replacement could not be prepared." } }
                            },
                            onDiscardProfile = { row -> scope.launch { repository.discardProfileConflict(row.id) } },
                        )
                    else -> TodayScreen(state, repository)
                }
            }
    }
    composer?.let { launch ->
        Composer(
            repository,
            launch.person,
            launch.kind,
            launch.target,
            onClose = { composer = null },
            onSubmitted = {
                composer = null
                repository.requestSync()
            },
        )
    }
    contactComposer?.let { (person, draft) ->
        ContactComposer(
            repository,
            person,
            draft,
            onClose = { contactComposer = null },
            onSubmitted = {
                contactComposer = null
                repository.requestSync()
            },
        )
    }
    stageComposer?.let { (person, draft) ->
        StageComposer(repository, person, draft, onClose = { stageComposer = null }, onSubmitted = {
            stageComposer = null; repository.requestSync()
        })
    }
    profileComposer?.let { (person, draft) ->
        ProfileComposer(repository, person, draft, onClose = { profileComposer = null }, onSubmitted = {
            profileComposer = null; repository.requestSync()
        })
    }
    if (signOut)
        AlertDialog(
            onDismissRequest = { signOut = false },
            title = { Text("Lock this account?") },
            text = {
                Text(
                    "${state.pendingCount} drafts or pending actions will stay encrypted on this device. Sign in as the same agent and Organization to reopen them. Signing out works offline."
                )
            },
            confirmButton = {
                Button(
                    onClick = {
                        signOut = false
                        scope.launch {
                            repository.lockLocal("Signed out. Saved work remains protected.")
                        }
                    }
                ) {
                    Text("Sign out and lock")
                }
            },
            dismissButton = { TextButton(onClick = { signOut = false }) { Text("Keep working") } },
        )
}

@Composable
private fun SignIn(repository: FieldRepository, state: FieldUi, modifier: Modifier) {
    var email by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    val scope = rememberCoroutineScope()
    Column(
        modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Spacer(Modifier.height(28.dp))
        Text(
            "Your relationships,\nready for the field.",
            style = MaterialTheme.typography.headlineLarge,
            fontWeight = FontWeight.SemiBold,
        )
        Text("Synthetic development workspace", color = MaterialTheme.colorScheme.primary)
        Text(state.message)
        if (state.pendingCount > 0)
            Text("${state.pendingCount} saved items remain protected on this device.")
        OutlinedTextField(
            email,
            { email = it },
            label = { Text("Email") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth().testTag("email"),
        )
        OutlinedTextField(
            password,
            { password = it },
            label = { Text("Password") },
            singleLine = true,
            visualTransformation = PasswordVisualTransformation(),
            modifier = Modifier.fillMaxWidth().testTag("password"),
        )
        Button(
            onClick = {
                scope.launch {
                    val input = password
                    password = ""
                    repository.login(email, input)
                    repository.requestSync(true)
                }
            },
            enabled = !state.busy && email.isNotBlank() && password.isNotBlank(),
            modifier = Modifier.fillMaxWidth().testTag("sign-in"),
        ) {
            Text(if (state.busy) "Signing in…" else "Sign in")
        }
        Text(
            "After online authorization, complete downloads are available for up to 7 days. Reboot or uncertain clock boundaries require online authorization again.",
            style = MaterialTheme.typography.bodySmall,
        )
    }
}

@Composable
private fun TodayScreen(state: FieldUi, repository: FieldRepository) {
    val today = state.today?.let { JSONObject(it) }
    val entries = today?.optJSONArray("items")?.objects().orEmpty()
    LazyColumn(
        Modifier.fillMaxSize(),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            Text("Today", style = MaterialTheme.typography.headlineMedium)
            Text(
                "${state.displayName} · Evaluated ${displayTime(today?.optString("generated_at") ?: "Never")}",
                style = MaterialTheme.typography.bodySmall,
            )
        }
        if (today == null)
            item {
                Text(
                    "Your Today list will appear after a complete download. Incomplete downloads never replace an existing list."
                )
            }
        else if (entries.isEmpty())
            item {
                Text(
                    "Nothing in this saved Today list. Open People to prepare your next conversation."
                )
            }
        items(entries) { item ->
            val person = item.getJSONObject("person")
            val id = person.getString("id")
            OutlinedCard(Modifier.fillMaxWidth().clickable { repository.select(id) }) {
                Column(Modifier.padding(16.dp)) {
                    Text(
                        person.optString("display_name", "Person"),
                        fontWeight = FontWeight.SemiBold,
                    )
                    Text("Open notes and tasks")
                    if (state.operations.any { it.person == id })
                        Text("Local work pending", color = MaterialTheme.colorScheme.primary)
                    if (
                        state.contactDrafts.any {
                            it.person == id && (it.operation.isEmpty() || it.state != "accepted")
                        }
                    )
                        Text("Contact saved on device", color = MaterialTheme.colorScheme.primary)
                }
            }
        }
        if (today?.optBoolean("truncated") == true)
            item { Text("Today is a bounded list; additional work may exist.") }
    }
}

@Composable
private fun PeopleScreen(state: FieldUi, repository: FieldRepository, onError: (String) -> Unit) {
    var search by remember { mutableStateOf("") }
    var knownId by remember { mutableStateOf("") }
    val scope = rememberCoroutineScope()
    val filtered =
        state.people.filter {
            JSONObject(it.summary).optString("display_name").contains(search, true)
        }
    LazyColumn(
        Modifier.fillMaxSize(),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        item {
            Text("People", style = MaterialTheme.typography.headlineMedium)
            Text("${state.people.size} complete records on this device")
        }
        item {
            OutlinedTextField(
                search,
                { search = it },
                label = { Text("Search saved People") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth().testTag("people-search"),
            )
        }
        item {
            OutlinedTextField(
                knownId,
                { knownId = it },
                label = { Text("Known Person ID to save offline") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            TextButton(
                onClick = {
                    scope.launch {
                        try {
                            repository.pin(uuid(knownId.trim()), true)
                            knownId = ""
                        } catch (_: Exception) {
                            onError(
                                "Enter a valid Person ID from this workspace. Availability is verified during sync."
                            )
                        }
                    }
                },
                enabled = knownId.isNotBlank() && !state.paused,
            ) {
                Text("Request offline availability")
            }
            Text(
                "Online name search is not available in this version. Your request becomes available only after a complete authorized download.",
                style = MaterialTheme.typography.bodySmall,
            )
        }
        items(filtered, key = { it.id }) { person ->
            PersonTile(
                person,
                state.operations.any { it.person == person.id },
                state.contactDrafts.any {
                    it.person == person.id && (it.operation.isEmpty() || it.state != "accepted")
                },
                state.stageCatalog,
            ) {
                repository.select(person.id)
            }
        }
        if (filtered.isEmpty()) item { Text("No matching complete cached records.") }
    }
}

@Composable
private fun PersonTile(person: PersonCard, pending: Boolean, contactPending: Boolean, catalog: List<StageCatalogRow>, open: () -> Unit) {
    val summary = JSONObject(person.summary)
    OutlinedCard(Modifier.fillMaxWidth().testTag("person-${person.id}").clickable(onClick = open)) {
        Column(Modifier.padding(16.dp)) {
            Text(summary.getString("display_name"), fontWeight = FontWeight.SemiBold)
            Text(
                summary.optJSONObject("stage")?.optString("id")?.let { id -> catalog.firstOrNull { it.id == id }?.name }
                    ?: summary.optJSONObject("stage")?.optString("name") ?: "No stage",
                style = MaterialTheme.typography.bodySmall,
            )
            if (pending)
                Text(
                    "Saved work on device",
                    color = MaterialTheme.colorScheme.primary,
                    style = MaterialTheme.typography.labelMedium,
                )
            if (contactPending)
                Text(
                    "Contact saved on device",
                    color = MaterialTheme.colorScheme.primary,
                    style = MaterialTheme.typography.labelMedium,
                )
        }
    }
}

@Composable
internal fun PersonScreen(
    state: FieldUi,
    repository: FieldRepository,
    onCompose: (String, JSONObject?) -> Unit,
    onError: (String) -> Unit,
    onContact: () -> Unit = {},
    onStage: () -> Unit = {},
    onProfile: () -> Unit = {},
) {
    val row = state.person ?: return
    val summary = JSONObject(row.summary)
    val scope = rememberCoroutineScope()
    var section by remember(row.id) { mutableStateOf("Notes") }
    val local = state.operations.filter { it.person == row.id }
    val pendingCompletions = local.filter { it.kind == "complete_task" && it.status != "attention" }
    val localContacts = state.contactDrafts.filter { it.person == row.id }
    LazyColumn(
        Modifier.fillMaxSize(),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            Text(summary.getString("display_name"), style = MaterialTheme.typography.headlineMedium)
            val stageId = summary.optJSONObject("stage")?.optString("id")
            val catalogLabel = stageId?.let { id -> state.stageCatalog.firstOrNull { it.id == id }?.name }
            Text("${catalogLabel ?: summary.optJSONObject("stage")?.optString("name") ?: "No stage"} · ${summary.optJSONObject("assigned_user")?.optString("display_name") ?: "Unassigned"}")
            Text(
                "Complete saved record · ${displayTime(row.evaluatedAt)}",
                style = MaterialTheme.typography.bodySmall,
            )
        }
        items(JSONArray(row.contacts).objects()) { contact ->
            Text("${contact.optString("kind")}: ${contact.optString("value")}")
        }
        item {
            val profileOp = local.firstOrNull { it.kind == "update_person_details" && it.status !in setOf("covered", "superseded") }
            OutlinedCard(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text("Profile", fontWeight = FontWeight.SemiBold)
                    Text("Names and methods are saved separately from the server record until accepted. They never change call or message destinations.", style = MaterialTheme.typography.bodySmall)
                    if (profileOp != null) { Text("Pending profile proposal", color = MaterialTheme.colorScheme.primary); StatusBadge(profileOp) }
                    Button(onClick = onProfile, enabled = row.detailsRevisionsQualified && state.profileEditingEnabled && state.profileDrafts.none { it.person == row.id && it.operation.isEmpty() && profileOp != null }, modifier = Modifier.testTag("edit-profile")) {
                        Text(if (profileOp == null) "Edit names and contacts" else "Save follow-up profile")
                    }
                    if (!row.detailsRevisionsQualified) Text("Profile editing needs a complete current contact baseline.", style = MaterialTheme.typography.bodySmall)
                    else if (!state.profileEditingEnabled) Text("Profile editing is unavailable until this workspace restores its details capability. Existing saved profile work remains protected.", style = MaterialTheme.typography.bodySmall)
                }
            }
        }
        item {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(onClick = { onCompose("add_note", null) }) { Text("Add note") }
                OutlinedButton(onClick = { onCompose("create_task", null) }) { Text("Create task") }
            }
        }
        item {
            val stageOp = local.firstOrNull { it.kind == "change_person_stage" && it.status !in setOf("covered", "superseded") }
            OutlinedCard(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text("Stage", fontWeight = FontWeight.SemiBold)
                    Text("The server stage remains shown until a complete authorized refresh.", style = MaterialTheme.typography.bodySmall)
                    if (stageOp != null) {
                        val proposal = JSONObject(stageOp.envelope).getJSONObject("payload").getString("stage_id")
                        val label = state.stageCatalog.firstOrNull { it.id == proposal }?.name ?: "Saved stage"
                        Text("Pending proposal: $label", color = MaterialTheme.colorScheme.primary)
                        StatusBadge(stageOp)
                    }
                    Button(onClick = onStage, enabled = state.stageChangesEnabled && row.stageRevisionsQualified, modifier = Modifier.testTag("change-stage")) { Text(if (stageOp == null) "Change stage" else "Save follow-up stage") }
                    if (stageOp != null) Text("A follow-up can be saved on this device and will wait for the pending stage change.", style = MaterialTheme.typography.bodySmall)
                    if (!state.stageChangesEnabled || !row.stageRevisionsQualified)
                        Text("Stage changes need a complete online catalog and current Person baseline.", style = MaterialTheme.typography.bodySmall)
                }
            }
        }
        item {
            OutlinedCard(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text("Log a contact", fontWeight = FontWeight.SemiBold)
                    Text(
                        "Record a completed manual contact. This does not place a call, send a message, or change a task.",
                        style = MaterialTheme.typography.bodySmall,
                    )
                    Button(onClick = onContact) {
                        Text("Log contact")
                    }
                    if (!state.contactLoggingEnabled)
                        Text(
                            "Contact logging needs an updated online workspace capability. Existing saved contacts remain protected.",
                            style = MaterialTheme.typography.bodySmall,
                        )
                    localContacts.forEach { contact ->
                        Text(
                            "${contact.channel.replace('_', ' ')} · ${contact.outcome.replace('_', ' ')} · ${displayContactTime(contact.occurredAt)}",
                            style = MaterialTheme.typography.labelMedium,
                        )
                        if (contact.state != "accepted")
                            Text(
                                if (contact.state == "attention") "Contact needs attention" else "Contact saved on device",
                                color = MaterialTheme.colorScheme.primary,
                                style = MaterialTheme.typography.labelMedium,
                            )
                    }
                }
            }
        }
        item {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                FilterChip(section == "Notes", { section = "Notes" }, label = { Text("Notes") })
                FilterChip(section == "Tasks", { section = "Tasks" }, label = { Text("Tasks") })
            }
        }
        items(
            local.filter { it.kind == if (section == "Notes") "add_note" else "create_task" },
            key = { "local-${it.id}" },
        ) { operation ->
            val payload = JSONObject(operation.envelope).getJSONObject("payload")
            OutlinedCard(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(14.dp)) {
                    Text(payload.optString(if (operation.kind == "add_note") "body" else "title"))
                    StatusBadge(operation)
                    if (operation.kind == "create_task") {
                        val completed = pendingCompletions.any {
                            JSONObject(it.envelope)
                                .getJSONObject("payload")
                                .getJSONObject("target")
                                .optString("created_by_operation_id") == operation.id
                        }
                        TextButton(
                            onClick = {
                                scope.launch {
                                    try {
                                        repository.complete(row.id, creation = operation.id)
                                        repository.requestSync()
                                    } catch (_: Exception) {
                                        onError(
                                            "Completion could not be saved. Try again; the task is unchanged."
                                        )
                                    }
                                }
                            },
                            enabled = !completed && operation.status != "attention",
                        ) {
                            Text(
                                if (completed) "Completion saved on device"
                                else "Complete saved task"
                            )
                        }
                    }
                }
            }
        }
        if (section == "Notes")
            items(JSONArray(row.notes).objects(), key = { "note-${it.getString("id")}" }) { note ->
                OutlinedCard(Modifier.fillMaxWidth()) {
                    Column(Modifier.padding(14.dp)) {
                        Text(note.getString("body"))
                        Text(
                            "Synced · ${note.optJSONObject("author")?.optString("display_name") ?: "Unknown author"} · ${displayTime(note.getString("created_at"))}",
                            style = MaterialTheme.typography.labelSmall,
                        )
                        if (state.editsEnabled && row.noteRevisionsQualified && note.optBoolean("can_manage") && note.has("revision"))
                            TextButton(onClick = { onCompose("edit_note", note) }) { Text("Edit note") }
                    }
                }
            }
        else
            items(JSONArray(row.tasks).objects(), key = { "task-${it.getString("id")}" }) { task ->
                val completed = !task.isNull("completed_at")
                val conflictNeedsRefresh = local.any { op ->
                    if (op.kind != "complete_task" || op.status != "attention") false
                    else {
                        val target =
                            JSONObject(op.envelope).getJSONObject("payload").getJSONObject("target")
                        target.optString("task_id") == task.getString("id") &&
                            target.optString("expected_revision") == task.getString("revision")
                    }
                }
                val localComplete = pendingCompletions.firstOrNull { op ->
                    val target =
                        JSONObject(op.envelope).getJSONObject("payload").getJSONObject("target")
                    target.optString("task_id") == task.getString("id")
                }
                OutlinedCard(Modifier.fillMaxWidth()) {
                    Column(Modifier.padding(14.dp)) {
                        Text(task.getString("title"), fontWeight = FontWeight.Medium)
                        Text(
                            "${task.optString("kind").replace('_', ' ')} · Due ${displayTime(task.stringOrNull("due_at") ?: "None")}",
                            style = MaterialTheme.typography.bodySmall,
                        )
                        if (state.editsEnabled && task.optBoolean("can_manage") && task.has("revision"))
                            TextButton(onClick = { onCompose("update_task", task) }) { Text("Edit task") }
                        Text(
                            if (completed) "Synced · Completed" else "Synced · Open",
                            style = MaterialTheme.typography.labelSmall,
                        )
                        localComplete?.let { StatusBadge(it) }
                        if (!completed && task.optBoolean("can_manage"))
                            TextButton(
                                onClick = {
                                    scope.launch {
                                        try {
                                            repository.complete(row.id, task)
                                            repository.requestSync()
                                        } catch (_: Exception) {
                                            onError(
                                                "Completion could not be committed on this device. The task is unchanged."
                                            )
                                        }
                                    }
                                },
                                enabled = localComplete == null && !conflictNeedsRefresh,
                            ) {
                                Text(
                                    if (conflictNeedsRefresh) "Refresh changed task before retry"
                                    else if (localComplete == null) "Complete task"
                                    else "Completion saved on device"
                                )
                            }
                    }
                }
            }
    }
}

@Composable
private fun StatusBadge(row: OperationRow) {
    Text(
        when (row.status) {
            "accepted" -> "Synced · awaiting a current download"
            "attention" -> "Needs attention"
            "uploading" -> "Saved on device · checking acceptance"
            else -> "Saved on device · pending sync"
        },
        color =
            if (row.status == "attention") MaterialTheme.colorScheme.error
            else MaterialTheme.colorScheme.primary,
        style = MaterialTheme.typography.labelMedium,
    )
    if (row.lastError.isNotEmpty())
        Text(errorMessage(row.lastError), style = MaterialTheme.typography.bodySmall)
}

@Composable
internal fun SavedWork(
    state: FieldUi,
    repository: FieldRepository,
    onDraft: (DraftRow) -> Unit,
    onRevise: (OperationRow, JSONObject) -> Unit,
    onContactDraft: (ContactDraftRow) -> Unit,
    onRepairFutureContact: (ContactDraftRow) -> Unit,
    onStageDraft: (StageDraftRow) -> Unit,
    onReviseStage: (OperationRow) -> Unit,
    onDiscardStage: (OperationRow) -> Unit,
    onProfileDraft: (ProfileDraftRow) -> Unit,
    onReviseProfile: (OperationRow) -> Unit,
    onDiscardProfile: (OperationRow) -> Unit,
) {
    LazyColumn(
        Modifier.fillMaxSize(),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            Text("Saved work", style = MaterialTheme.typography.headlineMedium)
            Text(
                "Your saved work stays on this device until it syncs. Items needing attention keep what you entered."
            )
        }
        items(state.drafts, key = { "draft-${it.id}" }) { draft ->
            OutlinedCard(Modifier.fillMaxWidth().clickable { onDraft(draft) }) {
                Column(Modifier.padding(16.dp)) {
                    Text(if (draft.kind in setOf("add_note", "edit_note")) "Note draft" else "Task draft")
                    Text("Draft revision ${draft.revision} saved on device · Continue editing")
                }
            }
        }
        items(state.contactDrafts, key = { "contact-${it.id}" }) { draft ->
            val operation = draft.operation.takeIf { it.isNotEmpty() }?.let { id ->
                state.operations.firstOrNull { it.id == id }
            }
            OutlinedCard(
                Modifier.fillMaxWidth().then(
                    if (draft.operation.isEmpty()) Modifier.clickable { onContactDraft(draft) }
                    else Modifier
                )
            ) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text("Log contact", fontWeight = FontWeight.SemiBold)
                    Text(
                        "${draft.channel.replace('_', ' ')} · ${draft.outcome.replace('_', ' ')} · ${displayContactTime(draft.occurredAt)}"
                    )
                    if (operation != null) StatusBadge(operation)
                    else Text("Draft revision ${draft.revision} saved on device · Continue editing")
                    if (draft.lastError == "contact_time_in_future")
                        TextButton(onClick = { onRepairFutureContact(draft) }) {
                            Text("Correct reported time")
                        }
                }
            }
        }
        items(state.stageDrafts, key = { "stage-${it.id}" }) { draft ->
            val operation = draft.operation.takeIf { it.isNotEmpty() }?.let { id -> state.operations.firstOrNull { it.id == id } }
            val predecessor = state.operations.firstOrNull {
                it.person == draft.person &&
                    it.kind == "change_person_stage" &&
                    it.status !in setOf("covered", "superseded") &&
                    it.id != draft.operation
            }
            OutlinedCard(Modifier.fillMaxWidth().then(if (draft.operation.isEmpty()) Modifier.clickable { onStageDraft(draft) } else Modifier)) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text("Change stage", fontWeight = FontWeight.SemiBold)
                    Text("${draft.baselineStageName} → ${draft.proposedStageName}")
                    if (operation == null) Text("Draft revision ${draft.revision} saved on device · Continue editing") else StatusBadge(operation)
                    if (operation == null && predecessor != null)
                        Text(
                            "Waiting for the previous stage change. This follow-up remains a draft until you resolve or review that proposal.",
                            style = MaterialTheme.typography.bodySmall,
                        )
                }
            }
        }
        items(state.profileDrafts, key = { "profile-${it.id}" }) { draft ->
            val operation = draft.operation.takeIf { it.isNotEmpty() }?.let { id -> state.operations.firstOrNull { it.id == id } }
            OutlinedCard(Modifier.fillMaxWidth().then(if (draft.operation.isEmpty()) Modifier.clickable { onProfileDraft(draft) } else Modifier).testTag("profile-draft-${draft.id}")) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text("Profile details", fontWeight = FontWeight.SemiBold)
                    if (operation == null) Text("Draft revision ${draft.revision} saved on device · Continue editing") else StatusBadge(operation)
                    if (operation?.lastError == "revision_conflict") {
                        val comparison = state.profileContexts.firstOrNull { it.operation == operation.id }
                        Text("Version you started from", fontWeight = FontWeight.SemiBold)
                        Text(comparison?.baseline?.ifEmpty { "Protected baseline unavailable" } ?: "Protected baseline unavailable", style = MaterialTheme.typography.bodySmall)
                        Text("Current profile", fontWeight = FontWeight.SemiBold)
                        Text(comparison?.current?.ifEmpty { "Waiting for an authorized current-profile read" } ?: "Waiting for an authorized current-profile read", style = MaterialTheme.typography.bodySmall)
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            TextButton(onClick = { repository.requestSync(true) }) { Text("Keep for later") }
                            TextButton(onClick = { onDiscardProfile(operation) }) { Text("Use current") }
                            TextButton(onClick = { onReviseProfile(operation) }, enabled = !comparison?.current.isNullOrEmpty()) { Text("Review and replace") }
                        }
                    } else if (operation?.status == "attention") {
                        // A validation or transport result can leave a protected proposal that
                        // has no current-record comparison.  It remains durable until the user
                        // explicitly discards it through the same profile proposal lifecycle.
                        Text(operation.lastError.ifEmpty { "Profile proposal needs attention" }, style = MaterialTheme.typography.bodySmall)
                        TextButton(onClick = { onDiscardProfile(operation) }) { Text("Discard proposal") }
                    }
                }
            }
        }
        items(state.operations.filter { it.kind !in setOf("log_contact_attempt", "update_person_details") }, key = { it.id }) { row ->
            OutlinedCard(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(16.dp)) {
                    Text(
                        when (row.kind) {
                            "add_note" -> "Add note"
                            "edit_note" -> "Edit note"
                            "create_task" -> "Create task"
                            "update_task" -> "Edit task"
                            "change_person_stage" -> "Change stage"
                            else -> "Complete task"
                        },
                        fontWeight = FontWeight.SemiBold,
                    )
                    StatusBadge(row)
                    val comparison = state.editContexts.firstOrNull { it.operation == row.id }
                    if (row.lastError == "revision_conflict" && comparison != null) {
                        Text("Your saved edit", fontWeight = FontWeight.SemiBold)
                        Text(
                            JSONObject(row.envelope).getJSONObject("payload")
                                .optString(if (row.kind == "edit_note") "body" else "title"),
                            style = MaterialTheme.typography.bodySmall,
                        )
                        Text("Version you started from", fontWeight = FontWeight.SemiBold)
                        Text(
                            if (comparison.baseline.isEmpty()) "Protected baseline unavailable"
                            else comparison.baseline,
                            style = MaterialTheme.typography.bodySmall,
                        )
                        Text("Current version", fontWeight = FontWeight.SemiBold)
                        Text(
                            if (comparison.current.isEmpty()) "Waiting for an authorized current-record read"
                            else comparison.current,
                            style = MaterialTheme.typography.bodySmall,
                        )
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            TextButton(
                                onClick = { repository.requestSync(true) },
                                enabled = row.status == "attention",
                            ) { Text("Keep draft for later") }
                            TextButton(
                                onClick = {
                                    repository.scope.launch { repository.discardConflict(row.id) }
                                },
                            ) { Text("Discard and use current") }
                            TextButton(
                                onClick = {
                                    if (comparison.current.isNotEmpty()) onRevise(row, JSONObject(comparison.current))
                                },
                                enabled = comparison.current.isNotEmpty() && row.status == "attention",
                            ) { Text("Review and revise") }
                        }
                    }
                    val stageComparison = state.stageContexts.firstOrNull { it.operation == row.id }
                    if (row.kind == "change_person_stage" && row.lastError == "revision_conflict" && stageComparison != null) {
                        Text("Your saved proposal", fontWeight = FontWeight.SemiBold)
                        Text(stageComparison.proposal)
                        Text("Version you started from", fontWeight = FontWeight.SemiBold)
                        Text(stageComparison.baseline)
                        Text("Current stage", fontWeight = FontWeight.SemiBold)
                        Text(if (stageComparison.current.isEmpty()) "Waiting for an authorized current-stage read" else stageComparison.current)
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            TextButton(onClick = { repository.requestSync(true) }) { Text("Keep for later") }
                            TextButton(onClick = { onDiscardStage(row) }) { Text("Discard and use current") }
                            TextButton(onClick = { onReviseStage(row) }, enabled = stageComparison.current.isNotEmpty()) { Text("Review and revise") }
                        }
                    }
                    TextButton(
                        onClick = {
                            repository.select(row.person)
                            repository.requestSync(true)
                        }
                    ) {
                        Text("Review current Person")
                    }
                }
            }
        }
        if (state.operations.isEmpty() && state.drafts.isEmpty() && state.contactDrafts.isEmpty())
            item { Text("All saved work is covered by the current download.") }
    }
}

@Composable
private fun Composer(
    repository: FieldRepository,
    person: String,
    kind: String,
    targetRecord: JSONObject? = null,
    onClose: () -> Unit,
    onSubmitted: () -> Unit,
) {
    val id = "$person:$kind:${targetRecord?.optString("id").orEmpty()}"
    val scope = rememberCoroutineScope()
    val commits = remember(id) { Mutex() }
    val pickerContext = LocalContext.current
    var text by remember(id) { mutableStateOf("") }
    var due by remember(id) { mutableStateOf("") }
    var taskKind by remember(id) { mutableStateOf("follow_up") }
    var loaded by remember(id) { mutableStateOf(false) }
    var saving by remember(id) { mutableStateOf(false) }
    var discard by remember(id) { mutableStateOf(false) }
    var revision by remember(id) { mutableLongStateOf(0) }
    var committed by remember(id) { mutableStateOf("") }
    var retry by remember(id) { mutableIntStateOf(0) }
    var status by remember(id) { mutableStateOf("Loading saved draft…") }
    fun payload(): JSONObject =
        if (kind == "add_note") json("person_id" to person, "body" to text)
        else if (kind == "edit_note")
            json(
                "person_id" to person,
                "note_id" to targetRecord!!.getString("id"),
                "expected_revision" to targetRecord.getString("revision"),
                "body" to text,
            )
        else {
            val update = kind == "update_task"
            json(
                "person_id" to person,
                *(if (update) arrayOf("task_id" to targetRecord!!.getString("id"), "expected_revision" to targetRecord.getString("revision")) else emptyArray()),
                "title" to text,
                "kind" to taskKind,
                "due_at" to due.ifBlank { null },
            )
        }
    val current = payload().toString()
    val dirty = loaded && current != committed
    LaunchedEffect(id) {
        try {
            repository.draft(id)?.let { draft ->
                val body = JSONObject(draft.payload)
                text = body.optString(if (kind in setOf("add_note", "edit_note")) "body" else "title")
                due = body.stringOrNull("due_at") ?: ""
                taskKind = body.optString("kind", "follow_up")
                revision = draft.revision
                committed = payload().toString()
            }
            if (targetRecord != null && repository.draft(id) == null) {
                text = targetRecord.optString(if (kind == "edit_note") "body" else "title")
                due = targetRecord.stringOrNull("due_at") ?: ""
                taskKind = targetRecord.optString("kind", "follow_up")
                committed = payload().toString()
            }
            status = "Changes will autosave on this device"
            loaded = true
        } catch (_: Exception) {
            status = "Could not open this protected draft"
        }
    }
    LaunchedEffect(current, loaded, retry) {
        if (loaded && current != committed) {
            status = "Unsaved changes…"
            delay(350)
            commits.withLock {
                // Once a write starts, settle its revision before a newer edit can commit.
                withContext(NonCancellable) {
                    saving = true
                    try {
                        val target =
                            targetRecord?.let {
                                json(
                                    "person_id" to person,
                                    "resource_id" to it.getString("id"),
                                    "expected_revision" to it.getString("revision"),
                                )
                            }
                        val row =
                            repository.saveDraft(
                                id, person, kind, JSONObject(current), revision, targetRecord, target
                            )
                        revision = row.revision
                        committed = current
                        status = "Draft revision ${row.revision} saved on this device"
                    } catch (_: Exception) {
                        status =
                            "Not saved. Free storage and retry. Closing requires explicitly discarding these unsaved edits."
                    }
                    saving = false
                }
            }
        }
    }
    AlertDialog(
        onDismissRequest = { if (!dirty && !saving) onClose() },
        title = { Text(when (kind) { "add_note" -> "Add note"; "edit_note" -> "Edit note"; "update_task" -> "Edit task"; else -> "Create task" }) },
        text = {
            Column(
                Modifier.verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                OutlinedTextField(
                    text,
                    { text = it },
                    label = { Text(if (kind in setOf("add_note", "edit_note")) "Note" else "Task title") },
                    minLines = if (kind in setOf("add_note", "edit_note")) 4 else 1,
                    modifier = Modifier.fillMaxWidth().testTag("composer-text"),
                    enabled = !saving,
                )
                if (kind in setOf("create_task", "update_task")) {
                    Text(
                        if (kind == "create_task") "Assigned to you. Choose the kind:" else "Choose the kind:",
                        style = MaterialTheme.typography.bodySmall,
                    )
                    Row(
                        Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        listOf("follow_up", "call", "other").forEach { value ->
                            FilterChip(
                                taskKind == value,
                                { taskKind = value },
                                label = { Text(value.replace('_', ' ')) },
                                enabled = !saving,
                            )
                        }
                    }
                    Text(if (due.isBlank()) "No due date" else "Due ${displayTime(due)}")
                    Row {
                        TextButton(
                            onClick = {
                                val initial =
                                    try {
                                        Instant.parse(due).atZone(java.time.ZoneId.systemDefault())
                                    } catch (_: Exception) {
                                        java.time.ZonedDateTime.now()
                                            .plusDays(1)
                                            .withHour(9)
                                            .withMinute(0)
                                    }
                                android.app
                                    .DatePickerDialog(
                                        pickerContext,
                                        { _, year, month, day ->
                                            android.app
                                                .TimePickerDialog(
                                                    pickerContext,
                                                    { _, hour, minute ->
                                                        due =
                                                            java.time.LocalDateTime.of(
                                                                    year,
                                                                    month + 1,
                                                                    day,
                                                                    hour,
                                                                    minute,
                                                                )
                                                                .atZone(
                                                                    java.time.ZoneId.systemDefault()
                                                                )
                                                                .toInstant()
                                                                .toString()
                                                    },
                                                    initial.hour,
                                                    initial.minute,
                                                    android.text.format.DateFormat.is24HourFormat(
                                                        pickerContext
                                                    ),
                                                )
                                                .show()
                                        },
                                        initial.year,
                                        initial.monthValue - 1,
                                        initial.dayOfMonth,
                                    )
                                    .show()
                            },
                            enabled = !saving,
                        ) {
                            Text("Choose due date and time")
                        }
                        if (due.isNotBlank())
                            TextButton(onClick = { due = "" }, enabled = !saving) {
                                Text("Clear due date")
                            }
                    }
                }
                Text(status, style = MaterialTheme.typography.bodySmall)
                if (dirty && !saving)
                    Row {
                        TextButton(onClick = { retry++ }) { Text("Retry save") }
                        TextButton(onClick = { discard = true }) { Text("Discard unsaved edits") }
                    }
            }
        },
        confirmButton = {
            Button(
                onClick = {
                    saving = true
                    scope.launch {
                        commits.withLock {
                            try {
                                repository.submit(id, revision)
                                onSubmitted()
                            } catch (_: Exception) {
                                status =
                                    "Not submitted. Check the input and device storage; your committed draft remains available."
                                saving = false
                            }
                        }
                    }
                },
                enabled = loaded && !dirty && !saving && text.isNotBlank(),
            ) {
                Text("Save on device")
            }
        },
        dismissButton = {
            TextButton(onClick = onClose, enabled = loaded && !dirty && !saving) {
                Text("Close draft")
            }
        },
    )
    if (discard)
        AlertDialog(
            onDismissRequest = { discard = false },
            title = { Text("Discard unsaved edits?") },
            text = {
                Text(
                    "Only the latest uncommitted edits will be lost. Any previously saved draft stays encrypted on this device."
                )
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        discard = false
                        onClose()
                    }
                ) {
                    Text("Discard unsaved edits")
                }
            },
            dismissButton = { TextButton(onClick = { discard = false }) { Text("Keep editing") } },
        )
}

internal data class ProfileContact(
    val id: String?,
    val kind: String,
    val value: String,
    // The draft baseline is immutable. Never diff a reopened/replacement proposal against a
    // newer sealed cache that happens to be visible while this editor is open.
    val baselineValue: String? = value,
    val removed: Boolean = false,
)

@Composable
internal fun ProfileRemovalNotice(contacts: List<ProfileContact>, index: Int) {
    val removed = contacts[index]
    val firstExistingId = contacts.firstOrNull { it.id != null && it.kind == removed.kind }?.id
    if (firstExistingId != removed.id) {
        Text("${removed.kind} will be removed.", style = MaterialTheme.typography.bodySmall)
        return
    }
    val successor = contacts.drop(index + 1).firstOrNull {
        it.id != null && !it.removed && it.kind == removed.kind
    }
    Text(
        if (successor == null) "Removing the primary ${removed.kind}; no ${removed.kind} remains."
        else "Removing the primary ${removed.kind}; ${successor.value} becomes primary.",
        style = MaterialTheme.typography.bodySmall,
        modifier = Modifier.testTag("profile-primary-warning-${removed.kind}"),
    )
}

/** The editor keeps an operation list, never a replace-all contact array or local normalization. */
@Composable
private fun ProfileComposer(
    repository: FieldRepository,
    person: String,
    id: String,
    onClose: () -> Unit,
    onSubmitted: () -> Unit,
) {
    val scope = rememberCoroutineScope()
    val commits = remember(id) { Mutex() }
    val state by repository.ui.collectAsState()
    var first by remember(id) { mutableStateOf("") }
    var last by remember(id) { mutableStateOf("") }
    var originalFirst by remember(id) { mutableStateOf<String?>(null) }
    var originalLast by remember(id) { mutableStateOf<String?>(null) }
    val contacts = remember(id) { mutableStateListOf<ProfileContact>() }
    var revision by remember(id) { mutableLongStateOf(0) }
    var committed by remember(id) { mutableStateOf("") }
    var loaded by remember(id) { mutableStateOf(false) }
    var saving by remember(id) { mutableStateOf(false) }
    var status by remember(id) { mutableStateOf("Loading protected profile draft…") }
    var retry by remember(id) { mutableIntStateOf(0) }
    fun proposal(): JSONObject {
        val ops = JSONArray()
        contacts.forEach { contact ->
            when {
                contact.id == null && !contact.removed && contact.value.isNotBlank() -> ops.put(json("op" to "add", "kind" to contact.kind, "value" to contact.value))
                contact.id != null && contact.removed -> ops.put(json("op" to "remove", "id" to contact.id))
                contact.id != null -> {
                    if (contact.baselineValue != contact.value) ops.put(json("op" to "edit", "id" to contact.id, "value" to contact.value))
                }
            }
        }
        return json("contact_operations" to ops).apply {
            if (first != originalFirst.orEmpty()) put("first_name", first.trim().ifBlank { JSONObject.NULL })
            if (last != originalLast.orEmpty()) put("last_name", last.trim().ifBlank { JSONObject.NULL })
        }
    }
    fun applySaved(baseline: JSONObject, saved: JSONObject) {
        originalFirst = baseline.stringOrNull("first_name"); originalLast = baseline.stringOrNull("last_name")
        first = originalFirst.orEmpty(); last = originalLast.orEmpty()
        contacts.clear(); baseline.getJSONArray("contacts").objects().forEach { contacts += ProfileContact(it.getString("id"), it.getString("kind"), it.getString("value")) }
        if (saved.has("first_name")) first = saved.stringOrNull("first_name").orEmpty()
        if (saved.has("last_name")) last = saved.stringOrNull("last_name").orEmpty()
        saved.getJSONArray("contact_operations").objects().forEach { op ->
            when (op.getString("op")) {
                "add" -> contacts += ProfileContact(null, op.getString("kind"), op.getString("value"))
                "remove" -> contacts.indexOfFirst { it.id == op.getString("id") }.takeIf { it >= 0 }?.let { i -> contacts[i] = contacts[i].copy(removed = true) }
                "edit" -> contacts.indexOfFirst { it.id == op.getString("id") }.takeIf { it >= 0 }?.let { i -> contacts[i] = contacts[i].copy(value = op.getString("value")) }
            }
        }
    }
    LaunchedEffect(id) {
        try {
            val saved = repository.profileDraft(id)
            if (saved != null) { applySaved(JSONObject(saved.baseline), JSONObject(saved.proposal)); revision = saved.revision }
            else {
                val row = requireNotNull(state.person); val summary = JSONObject(row.summary)
                val baseline = json("first_name" to summary.stringOrNull("first_name"), "last_name" to summary.stringOrNull("last_name"), "details_revision" to summary.getString("details_revision"), "contacts" to JSONArray(row.contacts))
                applySaved(baseline, json("contact_operations" to JSONArray()))
            }
            committed = proposal().toString(); loaded = true
            status = "Changes autosave encrypted on this device. Contact order stays server-defined."
        } catch (_: Exception) { status = "Could not open this protected profile draft" }
    }
    val current = proposal().toString(); val dirty = loaded && current != committed
    val hasProposalChanges = JSONObject(current).let { it.has("first_name") || it.has("last_name") || it.getJSONArray("contact_operations").length() > 0 }
    LaunchedEffect(current, loaded, retry) {
        if (loaded && dirty) {
            delay(350); commits.withLock { withContext(NonCancellable) {
                saving = true
                try { val row = repository.saveProfileDraft(id, person, JSONObject(current), revision); revision = row.revision; committed = current; status = "Profile draft revision ${row.revision} saved on this device" }
                catch (_: Exception) { status = "Not saved. Free storage and retry; the last committed draft remains protected." }
                saving = false
            } }
        }
    }
    AlertDialog(
        onDismissRequest = { if (!dirty && !saving) onClose() },
        title = { Text("Edit profile") },
        text = { Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("Changes require the complete saved contact baseline and will be reviewed if the profile changed elsewhere.", style = MaterialTheme.typography.bodySmall)
            OutlinedTextField(first, { first = it }, label = { Text("First name") }, enabled = !saving, modifier = Modifier.fillMaxWidth().testTag("profile-first-name"))
            OutlinedTextField(last, { last = it }, label = { Text("Last name") }, enabled = !saving, modifier = Modifier.fillMaxWidth().testTag("profile-last-name"))
            Text("Contact methods", fontWeight = FontWeight.SemiBold)
            contacts.forEachIndexed { index, contact ->
                if (contact.id != null && !contact.removed) Row(horizontalArrangement = Arrangement.spacedBy(6.dp), modifier = Modifier.fillMaxWidth()) {
                    OutlinedTextField(contact.value, { contacts[index] = contact.copy(value = it) }, label = { Text(contact.kind) }, enabled = !saving, modifier = Modifier.weight(1f).testTag("profile-contact-$index"))
                    TextButton(onClick = { contacts[index] = contact.copy(removed = true) }, enabled = !saving, modifier = Modifier.testTag("profile-remove-$index")) { Text("Remove") }
                } else if (contact.id != null) {
                    ProfileRemovalNotice(contacts, index)
                }
            }
            if (contacts.any { it.id == null && !it.removed }) Text("New contact methods (the server appends them after existing methods)", fontWeight = FontWeight.SemiBold)
            contacts.forEachIndexed { index, contact ->
                if (contact.id == null && !contact.removed) Row(horizontalArrangement = Arrangement.spacedBy(6.dp), modifier = Modifier.fillMaxWidth()) {
                    OutlinedTextField(contact.value, { contacts[index] = contact.copy(value = it) }, label = { Text(contact.kind) }, enabled = !saving, modifier = Modifier.weight(1f).testTag("profile-contact-$index"))
                    TextButton(onClick = { contacts[index] = contact.copy(removed = true) }, enabled = !saving) { Text("Remove") }
                }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                TextButton(onClick = { contacts += ProfileContact(null, "email", "") }, enabled = !saving) { Text("Add email") }
                TextButton(onClick = { contacts += ProfileContact(null, "phone", "") }, enabled = !saving) { Text("Add phone") }
            }
            Text(status, style = MaterialTheme.typography.bodySmall)
            if (dirty && !saving) TextButton(onClick = { retry++ }) { Text("Retry save") }
        } },
        confirmButton = {
            Button(
                onClick = {
                    saving = true
                    scope.launch {
                        commits.withLock {
                            try {
                                repository.submitProfileDraft(id, revision)
                                onSubmitted()
                            } catch (_: Exception) {
                                status = "Not submitted. Your committed profile proposal remains protected."
                                saving = false
                            }
                        }
                    }
                },
                enabled = loaded && !dirty && !saving && hasProposalChanges,
                modifier = Modifier.testTag("profile-save"),
            ) { Text("Save profile on device") }
        },
        dismissButton = { TextButton(onClick = onClose, enabled = loaded && !dirty && !saving) { Text("Close draft") } },
    )
}

@Composable
private fun ContactComposer(
    repository: FieldRepository,
    person: String,
    id: String,
    onClose: () -> Unit,
    onSubmitted: () -> Unit,
) {
    val scope = rememberCoroutineScope()
    val commits = remember(id) { Mutex() }
    val pickerContext = LocalContext.current
    val zone = remember { ZoneId.systemDefault() }
    var channel by remember(id) { mutableStateOf("call") }
    var outcome by remember(id) { mutableStateOf("reached") }
    var occurredAt by
        remember(id) {
            mutableStateOf(OffsetDateTime.now(zone).withSecond(0).withNano(0).toString())
        }
    var resolvedOffset by remember(id) { mutableStateOf(OffsetDateTime.parse(occurredAt).offset.id) }
    var revision by remember(id) { mutableLongStateOf(0) }
    var committed by remember(id) { mutableStateOf("") }
    var loaded by remember(id) { mutableStateOf(false) }
    var saving by remember(id) { mutableStateOf(false) }
    var status by remember(id) { mutableStateOf("Loading saved contact…") }
    var retry by remember(id) { mutableIntStateOf(0) }
    var offsetChoices by remember(id) { mutableStateOf<List<OffsetDateTime>>(emptyList()) }
    var capability by remember(id) { mutableStateOf(false) }
    fun current() =
        json(
                "channel" to channel,
                "outcome" to outcome,
                "occurred_at" to occurredAt,
                "resolved_offset" to resolvedOffset,
            )
            .toString()
    fun setResolved(value: OffsetDateTime) {
        occurredAt = value.toString()
        resolvedOffset = value.offset.id
        offsetChoices = emptyList()
    }
    fun chooseLocal(year: Int, month: Int, day: Int, hour: Int, minute: Int) {
        val local = LocalDateTime.of(year, month, day, hour, minute)
        val choices = resolveReportedLocal(local, zone)
        when (choices.size) {
            0 -> status = "That local time does not exist because of a daylight-saving change. Choose another time."
            1 -> setResolved(choices.single())
            else -> {
                offsetChoices = choices
                status = "Choose the offset for this repeated local time."
            }
        }
    }
    val dirty = loaded && current() != committed
    LaunchedEffect(id) {
        try {
            repository.contactDraft(id)?.let { draft ->
                channel = draft.channel
                outcome = draft.outcome
                occurredAt = draft.occurredAt
                resolvedOffset = draft.resolvedOffset
                revision = draft.revision
                committed = current()
            }
            capability = repository.contactLoggingSupported()
            status =
                if (capability) "Changes will autosave on this device"
                else "This workspace has not enabled contact logging. You can keep the protected draft."
            loaded = true
        } catch (_: Exception) {
            status = "Could not open this protected contact draft"
        }
    }
    LaunchedEffect(current(), loaded, retry) {
        if (loaded && current() != committed) {
            status = "Unsaved contact changes…"
            delay(350)
            commits.withLock {
                withContext(NonCancellable) {
                    saving = true
                    try {
                        val row =
                            repository.saveContactDraft(
                                id,
                                person,
                                channel,
                                outcome,
                                occurredAt,
                                resolvedOffset,
                                revision,
                            )
                        revision = row.revision
                        committed = current()
                        status = "Contact draft revision ${row.revision} saved on this device"
                    } catch (_: Exception) {
                        status =
                            "Not saved. Free storage and retry. Closing keeps the last protected draft."
                    }
                    saving = false
                }
            }
        }
    }
    AlertDialog(
        onDismissRequest = { if (!dirty && !saving) onClose() },
        title = { Text("Log a contact") },
        text = {
            Column(
                Modifier.verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text(
                    "Record a completed manual contact. This form never places a call or sends a message.",
                    style = MaterialTheme.typography.bodySmall,
                )
                Text("Channel", fontWeight = FontWeight.SemiBold)
                Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    listOf("call", "text", "email", "other").forEach { value ->
                        FilterChip(
                            channel == value,
                            { channel = value },
                            label = { Text(value) },
                            enabled = !saving,
                            modifier = Modifier.testTag("contact-channel-$value"),
                        )
                    }
                }
                Text("Outcome", fontWeight = FontWeight.SemiBold)
                listOf("reached", "no_answer", "left_message", "sent", "busy", "wrong_number")
                    .chunked(3)
                    .forEach { row ->
                        Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                            row.forEach { value ->
                                FilterChip(
                                    outcome == value,
                                    { outcome = value },
                                    label = { Text(value.replace('_', ' ')) },
                                    enabled = !saving,
                                    modifier = Modifier.testTag("contact-outcome-$value"),
                                )
                            }
                        }
                    }
                Text("Reported time", fontWeight = FontWeight.SemiBold)
                Text(
                    "${displayContactTime(occurredAt)} (${resolvedOffset})",
                    style = MaterialTheme.typography.bodySmall,
                )
                TextButton(
                    onClick = {
                        val initial = OffsetDateTime.parse(occurredAt).atZoneSameInstant(zone)
                        android.app.DatePickerDialog(
                                pickerContext,
                                { _, year, month, day ->
                                    android.app.TimePickerDialog(
                                            pickerContext,
                                            { _, hour, minute ->
                                                chooseLocal(year, month + 1, day, hour, minute)
                                            },
                                            initial.hour,
                                            initial.minute,
                                            android.text.format.DateFormat.is24HourFormat(pickerContext),
                                        )
                                        .show()
                                },
                                initial.year,
                                initial.monthValue - 1,
                                initial.dayOfMonth,
                            )
                            .show()
                    },
                    enabled = !saving,
                    modifier = Modifier.testTag("contact-time-picker"),
                ) { Text("Choose reported date and time") }
                Text(status, style = MaterialTheme.typography.bodySmall)
                if (!capability)
                    Text(
                        "Contact logging is unavailable until an online capability update. This draft remains encrypted.",
                        color = MaterialTheme.colorScheme.error,
                        style = MaterialTheme.typography.bodySmall,
                    )
                if (dirty && !saving) TextButton(onClick = { retry++ }) { Text("Retry save") }
            }
        },
        confirmButton = {
            Button(
                onClick = {
                    saving = true
                    scope.launch {
                        commits.withLock {
                            try {
                                repository.submitContactDraft(id, revision)
                                onSubmitted()
                            } catch (error: Exception) {
                                status =
                                    if (error is ApiFailure) errorMessage(error.code)
                                    else "Not submitted. Your committed contact draft remains available."
                                saving = false
                            }
                        }
                    }
                },
                enabled = loaded && capability && !dirty && !saving,
                modifier = Modifier.testTag("contact-save"),
            ) { Text("Save contact on device") }
        },
        dismissButton = {
            TextButton(onClick = onClose, enabled = loaded && !dirty && !saving) {
                Text("Close draft")
            }
        },
    )
    if (offsetChoices.isNotEmpty())
        AlertDialog(
            onDismissRequest = { offsetChoices = emptyList() },
            title = { Text("Choose time offset") },
            text = { Text("This local time occurs twice. Choose when the contact happened.") },
            confirmButton = {
                TextButton(onClick = { setResolved(offsetChoices.first()) }) {
                    Text(offsetChoices.first().offset.id)
                }
            },
            dismissButton = {
                TextButton(onClick = { setResolved(offsetChoices.last()) }) {
                    Text(offsetChoices.last().offset.id)
                }
            },
        )
}

@Composable
internal fun StageComposer(
    repository: FieldRepository,
    person: String,
    id: String,
    onClose: () -> Unit,
    onSubmitted: () -> Unit,
) {
    val scope = rememberCoroutineScope()
    val commits = remember(id) { Mutex() }
    val state by repository.ui.collectAsState()
    var selectedStage by remember(id) { mutableStateOf("") }
    var revision by remember(id) { mutableLongStateOf(0) }
    var committed by remember(id) { mutableStateOf("") }
    var loaded by remember(id) { mutableStateOf(false) }
    var saving by remember(id) { mutableStateOf(false) }
    var status by remember(id) { mutableStateOf("Loading saved stage proposal…") }
    var retry by remember(id) { mutableIntStateOf(0) }
    val catalog = state.stageCatalog
    val predecessor = state.operations.firstOrNull {
        it.person == person &&
            it.kind == "change_person_stage" &&
            it.status !in setOf("covered", "superseded")
    }
    val dirty = loaded && selectedStage != committed
    LaunchedEffect(id) {
        try {
            repository.stageDraft(id)?.let { draft ->
                selectedStage = draft.proposedStageId; committed = draft.proposedStageId; revision = draft.revision
            }
            if (selectedStage.isEmpty()) {
                val current = state.person?.summary?.let(::JSONObject)?.getJSONObject("stage")?.getString("id")
                selectedStage = current.orEmpty(); committed = selectedStage
            }
            status = "Choose a stage. Saving keeps this proposal encrypted on this device."
            loaded = true
        } catch (_: Exception) { status = "Could not open this protected stage proposal" }
    }
    LaunchedEffect(selectedStage, loaded, retry) {
        if (loaded && selectedStage != committed && selectedStage.isNotEmpty()) {
            commits.withLock {
                withContext(NonCancellable) {
                    saving = true
                    try {
                        val row = repository.saveStageDraft(id, person, selectedStage, revision)
                        revision = row.revision; committed = selectedStage
                        status = "Stage proposal revision ${row.revision} saved on this device"
                    } catch (_: Exception) { status = "Not saved. Free storage or reconnect for a complete stage catalog, then retry." }
                    saving = false
                }
            }
        }
    }
    AlertDialog(
        onDismissRequest = { if (!dirty && !saving) onClose() },
        title = { Text("Change stage") },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Your downloaded server stage stays visible until a complete authorized refresh. This proposal does not recalculate Today.", style = MaterialTheme.typography.bodySmall)
                catalog.forEach { stage ->
                    FilterChip(selectedStage == stage.id, { selectedStage = stage.id }, label = { Text(stage.name) }, enabled = !saving, modifier = Modifier.fillMaxWidth().testTag("stage-${stage.id}"))
                }
                if (catalog.isEmpty()) Text("No complete stage catalog is available yet.", color = MaterialTheme.colorScheme.error)
                if (predecessor != null)
                    Text(
                        "Waiting for the previous stage change. You can keep editing this follow-up draft, but it will not submit until that proposal is resolved or reviewed.",
                        style = MaterialTheme.typography.bodySmall,
                    )
                Text(status, style = MaterialTheme.typography.bodySmall)
                if (dirty && !saving) TextButton(onClick = { retry++ }) { Text("Retry save") }
            }
        },
        confirmButton = {
            if (predecessor != null)
                Button(
                    onClick = {
                        saving = true
                        scope.launch {
                            commits.withLock {
                                try {
                                    // The initially selected server stage can equal `committed`,
                                    // so autosave deliberately has nothing to do. This explicit
                                    // action still creates the separate waiting draft, but never
                                    // creates or submits an immutable operation.
                                    val saved =
                                        repository.stageDraft(id)
                                            ?: repository.saveStageDraft(
                                                id,
                                                person,
                                                selectedStage,
                                                revision,
                                            )
                                    revision = saved.revision
                                    committed = saved.proposedStageId
                                    status = "Follow-up draft saved on this device. Waiting for the previous stage change."
                                    saving = false
                                } catch (_: Exception) {
                                    status = "Not saved. Your follow-up remains editable; free storage and retry."
                                    saving = false
                                }
                            }
                        }
                    },
                    enabled = loaded && !dirty && !saving && selectedStage.isNotEmpty() && catalog.isNotEmpty(),
                    modifier = Modifier.testTag("stage-save-followup"),
                ) { Text("Save follow-up draft") }
            else
                Button(onClick = {
                    saving = true
                    scope.launch {
                        commits.withLock {
                            try {
                                // Selecting the unchanged current stage is a supported, receipted
                                // no-op. It still needs a typed local draft before it can seal the
                                // immutable operation; do not make the no-op path disappear merely
                                // because the picker initially selected that value.
                                val saved = repository.stageDraft(id) ?: repository.saveStageDraft(id, person, selectedStage, revision)
                                repository.submitStageDraft(id, saved.revision)
                                onSubmitted()
                            }
                            catch (error: Exception) { status = if (error is ApiFailure) errorMessage(error.code) else "Not submitted. Your committed stage proposal remains available."; saving = false }
                        }
                    }
                }, enabled = loaded && !dirty && !saving && selectedStage.isNotEmpty() && catalog.isNotEmpty(), modifier = Modifier.testTag("stage-save")) { Text("Save stage on device") }
        },
        dismissButton = { TextButton(onClick = onClose, enabled = loaded && !dirty && !saving) { Text("Close proposal") } },
    )
}

private fun displayContactTime(value: String): String =
    try {
        OffsetDateTime.parse(value)
            .format(java.time.format.DateTimeFormatter.ofPattern("MMM d, uuuu HH:mm xxx"))
    } catch (_: Exception) {
        "Protected contact time unavailable"
    }

private fun displayTime(value: String): String =
    try {
        Instant.parse(value)
            .atZone(java.time.ZoneId.systemDefault())
            .format(java.time.format.DateTimeFormatter.ofPattern("MMM d, HH:mm"))
    } catch (_: Exception) {
        value
    }
