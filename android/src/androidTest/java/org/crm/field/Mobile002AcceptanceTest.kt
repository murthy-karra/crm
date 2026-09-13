package org.crm.field

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.security.MessageDigest
import java.util.UUID
import kotlinx.coroutines.runBlocking
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Opt-in, live Mobile 002 acceptance against only the isolated API 3102 fixture.
 *
 * The note case intentionally runs in two instrumentation invocations: the host force-stops
 * the QA package between `note_prepare` and `note_relaunch`.  That makes process death part of
 * the proof instead of approximating it by recreating a Compose screen.  Every edit still goes
 * through the production repository, encrypted store and mobile operation route.
 */
@RunWith(AndroidJUnit4::class)
class Mobile002AcceptanceTest {
    private val primaryEmail = "android@mobile.test"
    private val syntheticPassword = "Mobile-demo-only-123!"
    private val reservedPeople =
        listOf(
            "54c956e3-ab11-4d1f-b448-1521330347d7",
            "d5016b41-938e-4fdb-a892-48df077d8b1b",
            "341d4f9d-bfc8-4b6f-9965-21e7d6edbc29",
        )
    private val conflictOriginalOperation = "e3774872-8062-43e3-b39f-ccbdc3d9f314"
    private val conflictRevisedOperation = "2bffbe9a-c336-4985-8300-1583a48b5460"
    private val conflictNote = "0a8a4c5f-d279-4b98-a6f7-62a65b4bd272"

    @get:Rule(order = 0)
    val localNetwork =
        androidx.test.rule.GrantPermissionRule.grant("android.permission.ACCESS_LOCAL_NETWORK")

    @get:Rule(order = 1) val compose = createAndroidComposeRule<MainActivity>()

    private val repository
        get() = (compose.activity.application as FieldApplication).repository

    private val stage get() = InstrumentationRegistry.getArguments().getString("acceptanceStage")

    private fun only(wanted: String) = assumeTrue(stage == wanted)

    private fun sha256(value: String) =
        MessageDigest.getInstance("SHA-256")
            .digest(value.toByteArray())
            .joinToString("") { "%02x".format(it) }

    private fun log(event: String) = android.util.Log.i("Mobile002Acceptance", event)

    private suspend fun primaryReady() {
        compose.waitUntil(180_000) {
            !(compose.activity.application as FieldApplication).ready.isActive
        }
        // This dedicated synthetic actor has its own API 3102 admission quota. Rebind the
        // existing installation to it so previous protected accounts remain untouched while
        // this acceptance run has an unambiguous 100-Person authoritative cache.
        repository.login(primaryEmail, syntheticPassword)
        assertFalse("primary authorization: ${repository.ui.value.message}", repository.ui.value.locked)
        repository.pause(true)
        // A dedicated synthetic actor starts without assigned People. Pin only the records
        // allocated to this Android lane through the same repository API the UI uses; the
        // following reconcile remains the normal authenticated production download.
        reservedPeople.forEach { repository.pin(it, true) }
        repository.pause(false)
        repository.sync(true)
        assertEquals("complete selected cache: ${repository.ui.value.message}", 3, repository.ui.value.people.size)
        assertTrue("Mobile 002 edit capabilities", repository.ui.value.editsEnabled)
        repository.pause(true)
    }

    private suspend fun person(suffix: String): PersonRow {
        val card =
            repository.ui.value.people.single {
                JSONObject(it.summary).optString("last_name") == suffix
            }
        repository.select(card.id)
        repository.refreshView()
        return requireNotNull(repository.ui.value.person) { "selected Mobile Person $suffix" }
    }

    private fun JSONArray.items() = (0 until length()).map { getJSONObject(it) }

    private fun stableJsonValue(value: Any?) =
        when (value) {
            is JSONObject -> value.toString()
            is JSONArray -> value.toString()
            else -> value
        }

    private fun note(row: PersonRow) =
        JSONArray(row.notes).items().first {
            it.optBoolean("can_manage") && it.has("revision") && it.getString("revision") != "0"
        }

    private fun completedTask(row: PersonRow) =
        JSONArray(row.tasks).items().first {
            it.optBoolean("can_manage") && it.has("revision") && !it.isNull("completed_at")
        }

    /**
     * The isolated API fixture intentionally begins with open tasks only. Create and complete a
     * setup task through the normal mobile operation path, then re-fetch it as the existing
     * completed record whose Mobile 002 update must preserve its completion and ownership facts.
     */
    private suspend fun existingCompletedTask(row: PersonRow): JSONObject {
        JSONArray(row.tasks).items().firstOrNull {
            it.optBoolean("can_manage") && it.has("revision") && !it.isNull("completed_at")
        }?.let { return it }

        val title = "Mobile002 setup completed task 097"
        val draft =
            repository.saveDraft(
                "acceptance-task-setup-${UUID.randomUUID()}",
                row.id,
                "create_task",
                json(
                    "person_id" to row.id,
                    "title" to title,
                    "kind" to "follow_up",
                    "due_at" to "2026-09-30T12:00:00Z",
                ),
            )
        val create = repository.submit(draft.id, draft.revision)
        repository.pause(false)
        repository.sync(true)
        val createReceipt = receiptAfterLock(create.id)
        assertTrue(createReceipt.status in setOf("accepted", "covered"))
        assertEquals("accepted", JSONObject(requireNotNull(createReceipt.receipt)).getString("outcome"))

        primaryReady()
        val created = JSONArray(person("097").tasks).items().single { it.getString("title") == title }
        val known = repository.ui.value.operations.map { it.id }.toSet()
        repository.complete(row.id, created)
        val completion =
            repository.ui.value.operations.single {
                it.id !in known && it.kind == "complete_task" && it.person == row.id
            }
        repository.pause(false)
        repository.sync(true)
        val completionReceipt = receiptAfterLock(completion.id)
        assertTrue(completionReceipt.status in setOf("accepted", "covered"))
        assertEquals(
            "accepted",
            JSONObject(requireNotNull(completionReceipt.receipt)).getString("outcome"),
        )
        log("TASK_SETUP create_operation=${create.id} complete_operation=${completion.id}")

        primaryReady()
        return completedTask(person("097"))
    }

    private suspend fun saveNoteEdit(
        draftId: String,
        row: PersonRow,
        note: JSONObject,
        body: String,
    ): OperationRow {
        val payload =
            json(
                "person_id" to row.id,
                "note_id" to note.getString("id"),
                "expected_revision" to note.getString("revision"),
                "body" to body,
            )
        val target =
            json(
                "person_id" to row.id,
                "resource_id" to note.getString("id"),
                "expected_revision" to note.getString("revision"),
            )
        val saved = repository.saveDraft(draftId, row.id, "edit_note", payload, 0, note, target)
        return repository.submit(saved.id, saved.revision)
    }

    private suspend fun saveTaskEdit(
        draftId: String,
        row: PersonRow,
        task: JSONObject,
        title: String,
    ): OperationRow {
        val payload =
            json(
                "person_id" to row.id,
                "task_id" to task.getString("id"),
                "expected_revision" to task.getString("revision"),
                "title" to title,
                "kind" to "other",
                "due_at" to null,
            )
        val target =
            json(
                "person_id" to row.id,
                "resource_id" to task.getString("id"),
                "expected_revision" to task.getString("revision"),
            )
        val saved = repository.saveDraft(draftId, row.id, "update_task", payload, 0, task, target)
        return repository.submit(saved.id, saved.revision)
    }

    private suspend fun receiptAfterLock(operationId: String): OperationRow {
        repository.lockLocal("Inspecting Mobile 002 acceptance receipt", false)
        val vault = DeviceVault(compose.activity, BuildConfig.VAULT_NAMESPACE)
        val registry = vault.registry()
        val directory = vault.accountDirectory(registry.getString("account"))
        val db = FieldDatabase.open(compose.activity, directory, vault.databaseKey(directory))
        return try {
            requireNotNull(db.data().operation(operationId))
        } finally {
            db.close()
        }
    }

    @Test
    fun notePrepareQueuesImmutableExistingEditOffline() {
        runBlocking {
        only("note_prepare")
        primaryReady()
        val row = person("096")
        val existing = note(row)
        val proposal = "Mobile002 Android acceptance note 096"
        val operation = saveNoteEdit("acceptance-note-096", row, existing, proposal)
        val envelope = JSONObject(operation.envelope)
        assertEquals("edit_note", operation.kind)
        assertEquals("queued", operation.status)
        assertEquals(proposal, envelope.getJSONObject("payload").getString("body"))
        compose.waitUntil(15_000) { repository.ui.value.operations.any { it.id == operation.id } }
        compose.onNodeWithTag("nav-Saved work").performClick()
        compose.onNodeWithText("Saved on device · pending sync").assertExists()
        log(
            "NOTE_PREPARED person=${row.id} note=${existing.getString("id")} operation=${operation.id} " +
                "expected_revision=${existing.getString("revision")} envelope_sha256=${sha256(operation.envelope)}"
        )
        }
    }

    @Test
    fun noteRelaunchReplaysExactQueuedEnvelopeAndRecordsReceipt() {
        runBlocking {
        only("note_relaunch")
        compose.waitUntil(180_000) {
            !(compose.activity.application as FieldApplication).ready.isActive && !repository.ui.value.locked
        }
        val personId = person("096").id
        val operation =
            repository.ui.value.operations.single {
                it.kind == "edit_note" && it.person == personId
            }
        val before = operation.envelope
        val beforeJson = JSONObject(before)
        assertEquals("Mobile002 Android acceptance note 096", beforeJson.getJSONObject("payload").getString("body"))
        repository.pause(false)
        repository.sync(true)
        val persisted = receiptAfterLock(operation.id)
        val receipt = JSONObject(requireNotNull(persisted.receipt))
        assertTrue(
            "receipt must be accepted before a completed download may cover it: ${persisted.status}",
            persisted.status in setOf("accepted", "covered"),
        )
        assertEquals(operation.id, receipt.getString("operation_id"))
        assertEquals("accepted", receipt.getString("outcome"))
        assertEquals("edit_note", persisted.kind)
        assertEquals(before, persisted.envelope)
        assertTrue(receipt.getString("committed_revision").toLong() > 0)
        log(
            "NOTE_ACCEPTED operation=${operation.id} envelope_sha256=${sha256(before)} " +
                "receipt_revision=${receipt.getString("committed_revision")} person_revision=${receipt.getString("person_revision")}" +
                " replayed=${receipt.getBoolean("replayed")}"
        )
        }
    }

    @Test
    fun completedTaskEditPreservesCompletionAssigneeAndCreator() {
        runBlocking {
        only("task")
        primaryReady()
        val row = person("097")
        val original = existingCompletedTask(row)
        val operation = saveTaskEdit("acceptance-task-097", row, original, "Mobile002 Android completed task 097")
        repository.pause(false)
        repository.sync(true)
        val persisted = receiptAfterLock(operation.id)
        val receipt = JSONObject(requireNotNull(persisted.receipt))
        assertTrue(persisted.status in setOf("accepted", "covered"))
        assertEquals("update_task", persisted.kind)
        assertEquals(operation.id, receipt.getString("operation_id"))
        assertEquals("accepted", receipt.getString("outcome"))

        repository.login(primaryEmail, syntheticPassword)
        repository.pause(false)
        repository.sync(true)
        val refreshed = JSONArray(person("097").tasks).items().single { it.getString("id") == original.getString("id") }
        assertEquals("Mobile002 Android completed task 097", refreshed.getString("title"))
        assertEquals("other", refreshed.getString("kind"))
        assertTrue("due date explicitly cleared", refreshed.isNull("due_at"))
        assertEquals(stableJsonValue(original.opt("completed_at")), stableJsonValue(refreshed.opt("completed_at")))
        assertEquals(stableJsonValue(original.opt("assignee")), stableJsonValue(refreshed.opt("assignee")))
        assertEquals(stableJsonValue(original.opt("created_by")), stableJsonValue(refreshed.opt("created_by")))
        log(
            "TASK_ACCEPTED person=${row.id} task=${original.getString("id")} operation=${operation.id} " +
                "receipt_revision=${receipt.getString("committed_revision")} completed_preserved=true assignee_preserved=true creator_preserved=true"
        )
        }
    }

    @Test
    fun secondActorConflictShowsComparisonThenCreatesExplicitRevisedOperation() {
        runBlocking {
        only("conflict")
        primaryReady()
        val primaryRow = person("098")
        val original = note(primaryRow)
        val initial = saveNoteEdit("acceptance-conflict-primary-098", primaryRow, original, "Mobile002 primary proposal 098")

        // Same allocated QA installation, switched through the normal account binding. The
        // second actor is the synthetic Organization admin and performs a real accepted edit.
        repository.lockLocal("Switching to synthetic second actor", false)
        repository.login("second@mobile.test", syntheticPassword)
        repository.pause(true)
        repository.pin(reservedPeople.single { it == "341d4f9d-bfc8-4b6f-9965-21e7d6edbc29" }, true)
        repository.pause(false)
        repository.sync(true)
        val secondRow = person("098")
        val remote = JSONArray(secondRow.notes).items().single { it.getString("id") == original.getString("id") }
        val remoteOperation = saveNoteEdit("acceptance-conflict-remote-098", secondRow, remote, "Mobile002 second actor change 098")
        repository.pause(false)
        repository.sync(true)
        val remoteReceipt = receiptAfterLock(remoteOperation.id)
        assertTrue(remoteReceipt.status in setOf("accepted", "covered"))
        assertEquals("accepted", JSONObject(requireNotNull(remoteReceipt.receipt)).getString("outcome"))

        repository.login(primaryEmail, syntheticPassword)
        repository.pause(false)
        repository.sync(true)
        // The failed immutable operation and its separately fetched current record are both
        // visible to the production Compose Saved work surface.
        val conflict = repository.ui.value.operations.single { it.id == initial.id }
        assertEquals("attention", conflict.status)
        assertEquals("revision_conflict", conflict.lastError)
        val comparison = repository.ui.value.editContexts.single { it.operation == initial.id }
        assertEquals(original.getString("body"), JSONObject(comparison.baseline).getString("body"))
        assertEquals("Mobile002 primary proposal 098", JSONObject(conflict.envelope).getJSONObject("payload").getString("body"))
        assertEquals("Mobile002 second actor change 098", JSONObject(comparison.current).getString("body"))
        compose.onNodeWithTag("nav-Saved work").performClick()
        compose.onNodeWithText("Your saved edit").assertExists()
        compose.onNodeWithText("Version you started from").assertExists()
        compose.onNodeWithText("Current version").assertExists()
        compose.onNodeWithText("Review and revise").assertExists()

        val current = repository.supersedeConflict(initial.id)
        val revised =
            saveNoteEdit(
                "acceptance-conflict-revised-098",
                primaryRow,
                current,
                "Mobile002 revised primary proposal 098",
            )
        assertTrue("explicit revised operation gets a fresh identity", revised.id != initial.id)
        assertEquals("superseded", repository.ui.value.operations.single { it.id == initial.id }.status)
        assertEquals(current.getString("revision"), JSONObject(revised.envelope).getJSONObject("payload").getString("expected_revision"))
        log(
            "CONFLICT_REVISED person=${primaryRow.id} note=${original.getString("id")} old_operation=${initial.id} " +
                "remote_operation=${remoteOperation.id} revised_operation=${revised.id} current_revision=${current.getString("revision")}"
        )
        }
    }

    /** Drains the already-created P098 revised operation without replaying the conflict setup. */
    @Test
    fun revisedConflictOperationReceivesReceiptAndUpdatesAuthoritativeNote() {
        runBlocking {
            only("conflict_relaunch")
            primaryReady()
            repository.pause(false)
            repository.sync(true)
            val revised = receiptAfterLock(conflictRevisedOperation)
            val receipt = JSONObject(requireNotNull(revised.receipt))
            assertTrue(revised.status in setOf("accepted", "covered"))
            assertEquals("accepted", receipt.getString("outcome"))
            assertEquals(conflictRevisedOperation, receipt.getString("operation_id"))
            assertEquals("edit_note", revised.kind)
            assertEquals(
                "Mobile002 revised primary proposal 098",
                JSONObject(revised.envelope).getJSONObject("payload").getString("body"),
            )

            repository.login(primaryEmail, syntheticPassword)
            repository.pause(false)
            repository.sync(true)
            assertEquals(
                "superseded",
                repository.ui.value.operations.single { it.id == conflictOriginalOperation }.status,
            )
            val authoritative =
                JSONArray(person("098").notes).items().single { it.getString("id") == conflictNote }
            assertEquals("Mobile002 revised primary proposal 098", authoritative.getString("body"))
            log(
                "CONFLICT_REVISED_ACCEPTED operation=$conflictRevisedOperation " +
                    "receipt_revision=${receipt.getString("committed_revision")} " +
                    "authoritative_revision=${authoritative.getString("revision")} old_operation=$conflictOriginalOperation"
            )
        }
    }
}
