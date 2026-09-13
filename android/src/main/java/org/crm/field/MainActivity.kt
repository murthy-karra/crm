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

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun FieldApp(repository: FieldRepository) {
    val state by repository.ui.collectAsState()
    val scope = rememberCoroutineScope()
    var tab by remember { mutableStateOf("Today") }
    var composer by remember { mutableStateOf<Pair<String, String>?>(null) }
    var signOut by remember { mutableStateOf(false) }
    var notice by remember { mutableStateOf("") }
    LaunchedEffect(state.locked) {
        if (state.locked) {
            composer = null
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
                            onCompose = { kind -> composer = state.person!!.id to kind },
                            onError = { notice = it },
                        )
                    tab == "People" -> PeopleScreen(state, repository, onError = { notice = it })
                    tab == "Saved work" ->
                        SavedWork(
                            state,
                            repository,
                            onDraft = {
                                if (state.people.any { person -> person.id == it.person })
                                    composer = it.person to it.kind
                                else
                                    notice =
                                        "This Person is outside the current offline selection. Saved input remains protected; request availability before reopening it."
                            },
                        )
                    else -> TodayScreen(state, repository)
                }
            }
    }
    composer?.let { (person, kind) ->
        Composer(
            repository,
            person,
            kind,
            onClose = { composer = null },
            onSubmitted = {
                composer = null
                repository.requestSync()
            },
        )
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
            PersonTile(person, state.operations.any { it.person == person.id }) {
                repository.select(person.id)
            }
        }
        if (filtered.isEmpty()) item { Text("No matching complete cached records.") }
    }
}

@Composable
private fun PersonTile(person: PersonCard, pending: Boolean, open: () -> Unit) {
    val summary = JSONObject(person.summary)
    OutlinedCard(Modifier.fillMaxWidth().testTag("person-${person.id}").clickable(onClick = open)) {
        Column(Modifier.padding(16.dp)) {
            Text(summary.getString("display_name"), fontWeight = FontWeight.SemiBold)
            Text(
                summary.optJSONObject("stage")?.optString("name") ?: "No stage",
                style = MaterialTheme.typography.bodySmall,
            )
            if (pending)
                Text(
                    "Saved work on device",
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
    onCompose: (String) -> Unit,
    onError: (String) -> Unit,
) {
    val row = state.person ?: return
    val summary = JSONObject(row.summary)
    val scope = rememberCoroutineScope()
    var section by remember(row.id) { mutableStateOf("Notes") }
    val local = state.operations.filter { it.person == row.id }
    val pendingCompletions = local.filter { it.kind == "complete_task" && it.status != "attention" }
    LazyColumn(
        Modifier.fillMaxSize(),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            Text(summary.getString("display_name"), style = MaterialTheme.typography.headlineMedium)
            Text(
                "${summary.optJSONObject("stage")?.optString("name") ?: "No stage"} · ${summary.optJSONObject("assigned_user")?.optString("display_name") ?: "Unassigned"}"
            )
            Text(
                "Complete saved record · ${displayTime(row.evaluatedAt)}",
                style = MaterialTheme.typography.bodySmall,
            )
        }
        items(JSONArray(row.contacts).objects()) { contact ->
            Text("${contact.optString("kind")}: ${contact.optString("value")}")
        }
        item {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(onClick = { onCompose("add_note") }) { Text("Add note") }
                OutlinedButton(onClick = { onCompose("create_task") }) { Text("Create task") }
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
private fun SavedWork(state: FieldUi, repository: FieldRepository, onDraft: (DraftRow) -> Unit) {
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
                    Text(if (draft.kind == "add_note") "Note draft" else "Task draft")
                    Text("Draft revision ${draft.revision} saved on device · Continue editing")
                }
            }
        }
        items(state.operations, key = { it.id }) { row ->
            OutlinedCard(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(16.dp)) {
                    Text(
                        when (row.kind) {
                            "add_note" -> "Add note"
                            "create_task" -> "Create task"
                            else -> "Complete task"
                        },
                        fontWeight = FontWeight.SemiBold,
                    )
                    StatusBadge(row)
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
        if (state.operations.isEmpty() && state.drafts.isEmpty())
            item { Text("All saved work is covered by the current download.") }
    }
}

@Composable
private fun Composer(
    repository: FieldRepository,
    person: String,
    kind: String,
    onClose: () -> Unit,
    onSubmitted: () -> Unit,
) {
    val id = "$person:$kind"
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
        else
            json(
                "person_id" to person,
                "title" to text,
                "kind" to taskKind,
                "due_at" to due.ifBlank { null },
                "assignee_user_id" to null,
            )
    val current = payload().toString()
    val dirty = loaded && current != committed
    LaunchedEffect(id) {
        try {
            repository.draft(id)?.let { draft ->
                val body = JSONObject(draft.payload)
                text = body.optString(if (kind == "add_note") "body" else "title")
                due = body.stringOrNull("due_at") ?: ""
                taskKind = body.optString("kind", "follow_up")
                revision = draft.revision
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
                        val row =
                            repository.saveDraft(id, person, kind, JSONObject(current), revision)
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
        title = { Text(if (kind == "add_note") "Add note" else "Create task") },
        text = {
            Column(
                Modifier.verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                OutlinedTextField(
                    text,
                    { text = it },
                    label = { Text(if (kind == "add_note") "Note" else "Task title") },
                    minLines = if (kind == "add_note") 4 else 1,
                    modifier = Modifier.fillMaxWidth().testTag("composer-text"),
                    enabled = !saving,
                )
                if (kind == "create_task") {
                    Text(
                        "Assigned to you. Choose the kind:",
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

private fun displayTime(value: String): String =
    try {
        Instant.parse(value)
            .atZone(java.time.ZoneId.systemDefault())
            .format(java.time.format.DateTimeFormatter.ofPattern("MMM d, HH:mm"))
    } catch (_: Exception) {
        value
    }
