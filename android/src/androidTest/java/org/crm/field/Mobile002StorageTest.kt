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

/** Mobile 002's local protocol invariants; the API conflict run has separate live evidence. */
@RunWith(AndroidJUnit4::class)
class Mobile002StorageTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val key = ByteArray(32) { (it + 31).toByte() }
    private val person = "10000000-0000-4000-8000-000000000051"
    private lateinit var directory: File
    private lateinit var database: FieldDatabase
    private lateinit var store: FieldStore

    @Before
    fun open() {
        directory = File(context.noBackupFilesDir, "mobile002-${UUID.randomUUID()}").apply { mkdirs() }
        database = FieldDatabase.open(context, directory, key)
        store = FieldStore(database, fixtureBinding(), TestClock())
        store.authorize("synthetic")
        store.dao.person(
            PersonRow(person, "1", json("id" to person).toString(), "[]", "[]", "[]", "g", "now")
        )
    }

    @After
    fun close() {
        database.close()
        directory.deleteRecursively()
    }

    private fun noteTarget(id: String) =
        json("person_id" to person, "resource_id" to id, "expected_revision" to "7")

    private fun edit(id: String, draft: String, note: String, body: String): DraftRow =
        store.saveDraft(
            draft,
            person,
            "edit_note",
            json("person_id" to person, "note_id" to note, "expected_revision" to "7", "body" to body),
            baseline = json("id" to note, "person_id" to person, "revision" to "7", "body" to "baseline"),
            target = noteTarget(note),
        )

    @Test
    fun editEnvelopeBaselineProposalAndFollowUpSurviveReopen() {
        val note = "20000000-0000-4000-8000-000000000051"
        val first = edit("first", "first", note, "first saved proposal")
        val operation = store.submitDraft(first.id, first.revision)
        assertEquals("edit_note", operation.kind)
        assertEquals("first saved proposal", JSONObject(operation.envelope).getJSONObject("payload").getString("body"))
        assertEquals("baseline", JSONObject(store.dao.editContext(operation.id)!!.baseline).getString("body"))

        // Subsequent typing can be safely persisted while the first action remains immutable.
        val followUp = edit("follow", "follow", note, "later typing")
        assertNotNull(store.dao.draft(followUp.id))
        assertThrows(IllegalArgumentException::class.java) { store.submitDraft(followUp.id, followUp.revision) }
        database.close()
        database = FieldDatabase.open(context, directory, key)
        store = FieldStore(database, fixtureBinding(), TestClock())
        assertEquals(operation.envelope, store.dao.operation(operation.id)!!.envelope)
        assertEquals("later typing", JSONObject(store.dao.draft(followUp.id)!!.payload).getString("body"))
        assertEquals("baseline", JSONObject(store.dao.editContext(operation.id)!!.baseline).getString("body"))
    }

    @Test
    fun editNoteReceiptRequiresRevisionButLegacyAddNoteStillRequiresNull() {
        val note = "20000000-0000-4000-8000-000000000052"
        val draft = edit("receipt", "receipt", note, "proposal")
        val operation = store.submitDraft(draft.id, draft.revision)
        val accepted =
            json(
                "operation_id" to operation.id,
                "outcome" to "accepted",
                "resource_type" to "note",
                "resource_id" to note,
                "committed_revision" to "8",
                "person_revision" to "2",
                "changed" to true,
                "accepted_at" to "2026-09-13T00:00:00Z",
                "replayed" to false,
            )
        store.acknowledge(operation.id, accepted)
        assertEquals("accepted", store.dao.operation(operation.id)!!.status)

        val legacy = store.saveDraft("legacy", person, "add_note", json("person_id" to person, "body" to "old"))
        val oldOperation = store.submitDraft(legacy.id, legacy.revision)
        assertThrows(IllegalArgumentException::class.java) {
            store.acknowledge(oldOperation.id, accepted.put("operation_id", oldOperation.id))
        }
    }
}
