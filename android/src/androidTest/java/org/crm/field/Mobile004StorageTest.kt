package org.crm.field

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.util.UUID
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/** The local Mobile004 contract: typed rows, immutable bytes and catalog-gated baselines. */
@RunWith(AndroidJUnit4::class)
class Mobile004StorageTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val key = ByteArray(32) { (it + 83).toByte() }
    private val person = "10000000-0000-4000-8000-000000000051"
    private val stageA = "20000000-0000-4000-8000-000000000051"
    private val stageB = "20000000-0000-4000-8000-000000000052"
    private lateinit var directory: File
    private lateinit var database: FieldDatabase
    private lateinit var store: FieldStore

    @Before fun open() {
        directory = File(context.noBackupFilesDir, "mobile004-${UUID.randomUUID()}").apply { mkdirs() }
        database = FieldDatabase.open(context, directory, key)
        store = FieldStore(database, stageBinding(), TestClock()); store.authorize("synthetic")
        store.dao.person(PersonRow(person, "7", json("id" to person, "stage" to json("id" to stageA, "name" to "Lead"), "stage_revision" to "1").toString(), "[]", "[]", "[]", "g", "now", true, true))
        store.dao.stageCatalog(listOf(StageCatalogRow(stageA, "Lead", 1, "g", "4"), StageCatalogRow(stageB, "Custom pipeline", 2, "g", "4")))
    }
    @After fun close() { database.close(); directory.deleteRecursively() }
    private fun save(id: String = UUID.randomUUID().toString(), stage: String = stageB) = store.saveStageDraft(id, person, stage)
    private fun receipt(op: OperationRow, changed: Boolean = true, committed: String = "2") = json("operation_id" to op.id, "outcome" to "accepted", "resource_type" to "person_stage", "resource_id" to person, "committed_revision" to committed, "person_revision" to "8", "changed" to changed, "accepted_at" to "2026-09-13T00:00:00Z", "replayed" to false)

    @Test fun stageDraftAndOperationCommitTogetherAndRemainImmutable() {
        val draft = save(); val operation = store.submitStageDraft(draft.id, draft.revision)
        assertEquals(operation.id, store.submitStageDraft(draft.id, draft.revision).id)
        assertEquals("change_person_stage", operation.kind)
        assertEquals("1", JSONObject(operation.envelope).getJSONObject("payload").getString("expected_stage_revision"))
        assertEquals(operation.id, store.dao.stageDraft(draft.id)!!.operation)
        assertNotNull(store.dao.stageContext(operation.id))
        assertThrows(Exception::class.java) { store.saveStageDraft(draft.id, person, stageA, draft.revision) }
    }

    @Test fun onlyOneUnresolvedStageOperationAllowsFollowupDraftWithoutCoalescing() {
        val first = save(); val operation = store.submitStageDraft(first.id, first.revision)
        val followup = save(UUID.randomUUID().toString(), stageA)
        assertThrows(IllegalArgumentException::class.java) { store.submitStageDraft(followup.id, followup.revision) }
        assertEquals("", store.dao.stageDraft(followup.id)!!.operation)
        assertEquals(operation.envelope, store.dao.operation(operation.id)!!.envelope)
    }

    @Test fun strictPersonStageReceiptRejectsTaskShapeAndNeedsFreshQualifiedSeal() {
        val draft = save(); val operation = store.submitStageDraft(draft.id, draft.revision)
        assertThrows(Exception::class.java) { store.acknowledge(operation.id, receipt(operation).put("resource_type", "task")) }
        assertThrows(IllegalArgumentException::class.java) { store.acknowledge(operation.id, receipt(operation).put("committed_revision", null)) }
        store.acknowledge(operation.id, receipt(operation))
        assertEquals("accepted", store.dao.operation(operation.id)!!.status)
        assertEquals("accepted", store.dao.stageDraft(draft.id)!!.state)
        assertNull(store.dao.meta("generation"))
    }

    @Test fun sameRevisionSameStageIsAReceiptedNoOp() {
        val draft = save(stage = stageA); val operation = store.submitStageDraft(draft.id, draft.revision)
        store.acknowledge(operation.id, receipt(operation, changed = false, committed = "1"))
        val receipt = JSONObject(requireNotNull(store.dao.operation(operation.id)!!.receipt))
        assertFalse(receipt.getBoolean("changed"))
        assertEquals("1", receipt.getString("committed_revision"))
    }

    @Test fun abaConflictStoresFreshCurrentAndRevisionCreatesNewProposal() {
        val original = save(); val operation = store.submitStageDraft(original.id, original.revision)
        store.dao.operationState(operation.id, "attention", 1, 0, "revision_conflict")
        store.recordCurrentStage(operation.id, json("context_id" to store.binding.context, "person_id" to person, "person_revision" to "11", "stage_revision" to "3", "stage" to json("id" to stageA, "name" to "Lead renamed")))
        val revised = store.reviseStageConflict(operation.id, UUID.randomUUID().toString())
        assertEquals("superseded", store.dao.operation(operation.id)!!.status)
        assertEquals("3", revised.baselineRevision)
        val newOperation = store.submitStageDraft(revised.id, revised.revision)
        assertNotEquals(operation.id, newOperation.id)
        assertEquals("3", JSONObject(newOperation.envelope).getJSONObject("payload").getString("expected_stage_revision"))
    }

    @Test fun v4UpgradePreservesLegacyBytesAndAddsOnlyStageTables() {
        val legacy = store.saveDraft("legacy", person, "add_note", json("person_id" to person, "body" to "protected v4 bytes"))
        val operation = store.submitDraft(legacy.id, legacy.revision); val bytes = operation.envelope
        val raw = database.openHelper.writableDatabase
        raw.execSQL("DROP TABLE stage_catalog_pages"); raw.execSQL("DROP TABLE stage_catalog"); raw.execSQL("DROP TABLE stage_context"); raw.execSQL("DROP TABLE stage_drafts")
        raw.execSQL("ALTER TABLE people RENAME TO people_v5")
        raw.execSQL("CREATE TABLE people (id TEXT NOT NULL PRIMARY KEY, revision TEXT NOT NULL, summary TEXT NOT NULL, contacts TEXT NOT NULL, notes TEXT NOT NULL, tasks TEXT NOT NULL, generation TEXT NOT NULL, evaluatedAt TEXT NOT NULL, noteRevisionsQualified INTEGER NOT NULL DEFAULT 0)")
        raw.execSQL("INSERT INTO people SELECT id,revision,summary,contacts,notes,tasks,generation,evaluatedAt,noteRevisionsQualified FROM people_v5"); raw.execSQL("DROP TABLE people_v5")
        raw.execSQL("DROP TABLE room_master_table"); raw.execSQL("PRAGMA user_version=4")
        database.close(); database = FieldDatabase.open(context, directory, key); store = FieldStore(database, stageBinding(), TestClock())
        assertEquals(bytes, store.dao.operation(operation.id)!!.envelope)
        assertFalse(store.dao.person(person)!!.stageRevisionsQualified)
        assertTrue(store.dao.stageDrafts().isEmpty())
    }

    private fun stageBinding(): Binding {
        val bootstrap = fixture("reconciliation_bootstrap").put("capabilities", org.json.JSONArray(listOf("add_note", "create_task", "complete_task", "reconciliation", "change_person_stage", "stage_revisions", "stage_catalog")))
        return Binding.parse(bootstrap, bootstrap.getString("actor_user_id"), bootstrap.getString("organization_id"), bootstrap.getString("installation_id"))
    }
}
