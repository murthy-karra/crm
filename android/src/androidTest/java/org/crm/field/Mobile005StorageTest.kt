package org.crm.field

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.util.UUID
import org.json.JSONArray
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/** Mobile005's local trust boundary: complete profile baselines, CAS drafts and strict receipts. */
@RunWith(AndroidJUnit4::class)
class Mobile005StorageTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val key = ByteArray(32) { (it + 109).toByte() }
    private val person = "10000000-0000-4000-8000-000000000061"
    private val email = "20000000-0000-4000-8000-000000000061"
    private lateinit var directory: File
    private lateinit var database: FieldDatabase
    private lateinit var store: FieldStore

    @Before fun open() {
        directory = File(context.noBackupFilesDir, "mobile005-${UUID.randomUUID()}").apply { mkdirs() }
        database = FieldDatabase.open(context, directory, key)
        store = FieldStore(database, binding(), TestClock()); store.authorize("synthetic")
        store.dao.person(PersonRow(person, "7", summary().toString(), contacts().toString(), "[]", "[]", "g", "now", true, true, true))
    }
    @After fun close() { database.close(); directory.deleteRecursively() }

    @Test fun draftAndImmutableOperationCommitTogetherWithOriginalOperationArrayOrdinals() {
        val draft = store.saveProfileDraft("30000000-0000-4000-8000-000000000061", person, proposal())
        val op = store.submitProfileDraft(draft.id, draft.revision)
        assertEquals(op.id, store.submitProfileDraft(draft.id, draft.revision).id)
        assertEquals("7", JSONObject(op.envelope).getJSONObject("payload").getString("expected_details_revision"))
        assertEquals(op.id, store.dao.profileDraft(draft.id)!!.operation)
        val receipt = receipt(op)
        store.acknowledge(op.id, receipt)
        assertEquals("accepted", store.dao.operation(op.id)!!.status)
        assertEquals("accepted", store.dao.profileDraft(draft.id)!!.state)
        assertNull(store.dao.meta("generation"))
    }

    @Test fun strictReceiptRejectsWrongOrdinalOrAddedMappingOnNoop() {
        val draft = store.saveProfileDraft(UUID.randomUUID().toString(), person, proposal())
        val op = store.submitProfileDraft(draft.id, draft.revision)
        assertThrows(Exception::class.java) { store.acknowledge(op.id, receipt(op).put("added_contact_ids", JSONArray().put(json("ordinal" to 0, "id" to UUID.randomUUID().toString())))) }
        assertThrows(Exception::class.java) { store.acknowledge(op.id, receipt(op, false)) }
        store.acknowledge(op.id, receipt(op))
        store.dao.cover(op.id)

        val noAdd = store.saveProfileDraft(UUID.randomUUID().toString(), person, json("first_name" to "Names only", "contact_operations" to JSONArray()))
        val noAddOperation = store.submitProfileDraft(noAdd.id, noAdd.revision)
        store.acknowledge(noAddOperation.id, receipt(noAddOperation).put("added_contact_ids", JSONArray()))
        assertEquals("accepted", store.dao.operation(noAddOperation.id)!!.status)
    }

    @Test fun incompleteOrOldSameBroadRevisionCannotCreateProfileBaseline() {
        val old = store.dao.person(person)!!
        store.dao.person(old.copy(summary = json("id" to person, "first_name" to "Old", "last_name" to "Cache").toString(), contacts = JSONArray().put(json("id" to email, "kind" to "email", "value" to "old@example.test", "created_at" to "2026-09-14T00:00:00Z")).toString(), detailsRevisionsQualified = false))
        assertThrows(Exception::class.java) { store.saveProfileDraft(UUID.randomUUID().toString(), person, proposal()) }
    }

    @Test fun unchangedLegacyValuesOverEditLimitsRemainFrozenAndAreNotResubmitted() {
        val imported = "x".repeat(1_025) + "@legacy.test"
        val old = store.dao.person(person)!!
        store.dao.person(old.copy(contacts = JSONArray().put(json("id" to email, "kind" to "email", "value" to imported, "import_order" to 1, "created_at" to "2026-09-14T00:00:00Z")).toString()))
        val draft = store.saveProfileDraft(UUID.randomUUID().toString(), person, json("last_name" to "Changed", "contact_operations" to JSONArray()))
        val operation = store.submitProfileDraft(draft.id, draft.revision)
        val payload = JSONObject(operation.envelope).getJSONObject("payload")
        assertEquals("Changed", payload.getString("last_name"))
        assertEquals(0, payload.getJSONArray("contact_operations").length())
        assertFalse(operation.envelope.contains(imported))
    }

    @Test fun conflictRetainsOriginalAndBuildsSeparateReplacementAgainstCurrent() {
        val original = store.saveProfileDraft(UUID.randomUUID().toString(), person, proposal())
        val op = store.submitProfileDraft(original.id, original.revision)
        store.dao.operationState(op.id, "attention", 1, 0, "revision_conflict")
        store.recordCurrentProfile(op.id, json("context_id" to store.binding.context, "person_id" to person, "person_revision" to "9", "details_revision" to "8", "first_name" to "Current", "last_name" to "Name", "items" to contacts(), "next_cursor" to null, "complete" to true))
        val replacement = store.reviseProfileConflict(op.id, UUID.randomUUID().toString())
        assertEquals("superseded", store.dao.operation(op.id)!!.status)
        assertEquals("8", replacement.detailsRevision)
        val second = store.submitProfileDraft(replacement.id, replacement.revision)
        assertNotEquals(op.id, second.id)
        assertEquals("8", JSONObject(second.envelope).getJSONObject("payload").getString("expected_details_revision"))
    }

    @Test fun currentProfileProjectionOrdersScrambledContactIdsByDeclaredServerOrder() {
        val original = store.saveProfileDraft(UUID.randomUUID().toString(), person, proposal())
        val op = store.submitProfileDraft(original.id, original.revision)
        store.dao.operationState(op.id, "attention", 1, 0, "revision_conflict")
        val nullImport = "20000000-0000-4000-8000-000000000001"
        val laterImport = "10000000-0000-4000-8000-000000000999"
        val firstImportLaterCreated = "f0000000-0000-4000-8000-000000000001"
        val scrambled = JSONArray()
            .put(json("id" to nullImport, "kind" to "phone", "value" to "555-555-0103", "import_order" to JSONObject.NULL, "created_at" to "2026-09-14T00:00:00Z"))
            .put(json("id" to laterImport, "kind" to "email", "value" to "later@example.test", "import_order" to 2, "created_at" to "2026-09-14T00:00:00Z"))
            .put(json("id" to firstImportLaterCreated, "kind" to "email", "value" to "first@example.test", "import_order" to 1, "created_at" to "2026-09-14T01:00:00Z"))
            .put(json("id" to email, "kind" to "phone", "value" to "555-555-0102", "import_order" to 1, "created_at" to "2026-09-14T00:00:00Z"))
        store.recordCurrentProfile(op.id, json("context_id" to store.binding.context, "person_id" to person, "person_revision" to "9", "details_revision" to "8", "first_name" to "Current", "last_name" to "Name", "items" to scrambled, "next_cursor" to null, "complete" to true))
        val ordered = JSONObject(store.dao.profileContext(op.id)!!.current).getJSONArray("contacts")
        assertEquals(listOf(email, firstImportLaterCreated, laterImport, nullImport), (0 until ordered.length()).map { ordered.getJSONObject(it).getString("id") })
    }

    @Test fun incompleteCurrentProfileCannotEnableReplacement() {
        val draft = store.saveProfileDraft(UUID.randomUUID().toString(), person, proposal())
        val op = store.submitProfileDraft(draft.id, draft.revision)
        store.dao.operationState(op.id, "attention", 1, 0, "revision_conflict")
        val partial = json("context_id" to store.binding.context, "person_id" to person, "person_revision" to "9", "details_revision" to "8", "first_name" to "Current", "last_name" to "Name", "items" to contacts(), "next_cursor" to "later", "complete" to false)
        assertThrows(Exception::class.java) { store.recordCurrentProfile(op.id, partial) }
        assertThrows(Exception::class.java) { store.reviseProfileConflict(op.id, UUID.randomUUID().toString()) }
    }

    @Test fun schemaFiveUpgradePreservesExistingOutboxBytesAndOnlyMarksDetailsUnqualified() {
        val legacy = store.saveDraft("legacy", person, "add_note", json("person_id" to person, "body" to "protected old envelope"))
        val op = store.submitDraft(legacy.id, legacy.revision); val bytes = op.envelope
        val raw = database.openHelper.writableDatabase
        raw.execSQL("DROP TABLE profile_context"); raw.execSQL("DROP TABLE profile_drafts")
        raw.execSQL("ALTER TABLE people RENAME TO people_v6")
        raw.execSQL("CREATE TABLE people (id TEXT NOT NULL PRIMARY KEY, revision TEXT NOT NULL, summary TEXT NOT NULL, contacts TEXT NOT NULL, notes TEXT NOT NULL, tasks TEXT NOT NULL, generation TEXT NOT NULL, evaluatedAt TEXT NOT NULL, noteRevisionsQualified INTEGER NOT NULL DEFAULT 0, stageRevisionsQualified INTEGER NOT NULL DEFAULT 0)")
        raw.execSQL("INSERT INTO people SELECT id,revision,summary,contacts,notes,tasks,generation,evaluatedAt,noteRevisionsQualified,stageRevisionsQualified FROM people_v6")
        raw.execSQL("DROP TABLE people_v6"); raw.execSQL("DROP TABLE room_master_table"); raw.execSQL("PRAGMA user_version=5")
        database.close(); database = FieldDatabase.open(context, directory, key); store = FieldStore(database, binding(), TestClock())
        assertEquals(bytes, store.dao.operation(op.id)!!.envelope)
        assertFalse(store.dao.person(person)!!.detailsRevisionsQualified)
        assertTrue(store.dao.profileDrafts().isEmpty())
    }

    private fun proposal() = json("first_name" to "Changed", "contact_operations" to JSONArray().put(json("op" to "edit", "id" to email, "value" to "changed@example.test")).put(json("op" to "add", "kind" to "phone", "value" to "555-555-0100")))
    private fun receipt(op: OperationRow, changed: Boolean = true) = json("operation_id" to op.id, "outcome" to "accepted", "resource_type" to "person_details", "resource_id" to person, "committed_revision" to "8", "person_revision" to "9", "changed" to changed, "accepted_at" to "2026-09-14T00:00:00Z", "replayed" to false, "added_contact_ids" to if (changed) JSONArray().put(json("ordinal" to 1, "id" to "40000000-0000-4000-8000-000000000061")) else JSONArray())
    private fun summary() = json("id" to person, "first_name" to "First", "last_name" to "Last", "display_name" to "First Last", "details_revision" to "7", "stage" to json("id" to "50000000-0000-4000-8000-000000000061", "name" to "Lead"), "stage_revision" to "1")
    private fun contacts() = JSONArray().put(json("id" to email, "kind" to "email", "value" to "first@example.test", "import_order" to 1, "created_at" to "2026-09-14T00:00:00Z"))
    private fun binding(): Binding {
        val bootstrap = fixture("reconciliation_bootstrap").put("capabilities", JSONArray(listOf("add_note", "create_task", "complete_task", "reconciliation", "update_person_details", "details_revisions")))
        return Binding.parse(bootstrap, bootstrap.getString("actor_user_id"), bootstrap.getString("organization_id"), bootstrap.getString("installation_id"))
    }
}
