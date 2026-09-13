package org.crm.field

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.time.LocalDateTime
import java.time.ZoneId
import java.util.UUID
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/** Mobile 003 local invariants: contact input is protected separately from legacy work. */
@RunWith(AndroidJUnit4::class)
class Mobile003StorageTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val key = ByteArray(32) { (it + 61).toByte() }
    private val person = "10000000-0000-4000-8000-000000000051"
    private lateinit var directory: File
    private lateinit var database: FieldDatabase
    private lateinit var store: FieldStore

    @Before
    fun open() {
        directory = File(context.noBackupFilesDir, "mobile003-${UUID.randomUUID()}").apply { mkdirs() }
        database = FieldDatabase.open(context, directory, key)
        store = FieldStore(database, fixtureBinding(), TestClock())
        store.authorize("synthetic")
        store.dao.person(
            PersonRow(person, "7", json("id" to person).toString(), "[]", "[]", "[]", "g", "now")
        )
    }

    @After
    fun close() {
        database.close()
        directory.deleteRecursively()
    }

    private fun save(
        id: String = UUID.randomUUID().toString(),
        occurredAt: String = "2026-11-01T01:30:00-07:00",
        offset: String = "-07:00",
    ) =
        store.saveContactDraft(id, person, "call", "left_message", occurredAt, offset)

    private fun receipt(operation: OperationRow, changed: Boolean = true, committed: Any? = null) =
        json(
            "operation_id" to operation.id,
            "outcome" to "accepted",
            "resource_type" to "contact_attempt",
            "resource_id" to UUID.randomUUID().toString(),
            "committed_revision" to committed,
            "person_revision" to "7",
            "changed" to changed,
            "accepted_at" to "2026-09-13T00:00:00Z",
            "replayed" to false,
        )

    @Test
    fun duplicateSaveConvergesWhileDistinctContactsForOnePersonStayDistinct() {
        val first = save()
        val once = store.submitContactDraft(first.id, first.revision)
        val twice = store.submitContactDraft(first.id, first.revision)
        assertEquals(once.id, twice.id)
        assertEquals(once.envelope, twice.envelope)

        val second = save(occurredAt = "2026-11-01T01:31:00-07:00")
        val separate = store.submitContactDraft(second.id, second.revision)
        assertNotEquals(once.id, separate.id)
        assertEquals(2, store.dao.operations().size)
        assertEquals(
            "2026-11-01T01:30:00-07:00",
            JSONObject(once.envelope).getJSONObject("payload").getString("occurred_at"),
        )
    }

    @Test
    fun strictContactReceiptHasNullRevisionAndWaitsForFreshSeal() {
        val draft = save()
        val operation = store.submitContactDraft(draft.id, draft.revision)
        assertThrows(IllegalArgumentException::class.java) {
            store.acknowledge(operation.id, receipt(operation, committed = "8"))
        }
        assertThrows(IllegalArgumentException::class.java) {
            store.acknowledge(operation.id, receipt(operation, changed = false))
        }
        // This generation started before the receipt. Even with equal Person revision it cannot
        // supply the Today seal that covers a newly accepted append-only contact.
        store.beginGeneration(fixture("reconciliation"))
        assertNotNull(store.dao.meta("generation"))
        store.acknowledge(operation.id, receipt(operation))
        // The local cached Person already has revision 7. Contact acceptance must not settle just
        // because the append-only fact intentionally leaves that revision unchanged.
        assertEquals("accepted", store.dao.operation(operation.id)!!.status)
        assertEquals("accepted", store.dao.contactDraft(draft.id)!!.state)
        assertEquals(null, store.dao.meta("generation"))
        assertEquals(null, store.dao.meta("manifest_cursor"))
        assertEquals(null, store.dao.meta("manifest_complete"))
    }

    @Test
    fun futureRepairClonesDraftWithoutChangingUncertainOperationBytes() {
        val originalDraft = save()
        val original = store.submitContactDraft(originalDraft.id, originalDraft.revision)
        store.dao.operationState(original.id, "attention", 1, 0, "contact_time_in_future")
        store.dao.contactState(original.id, "attention", "contact_time_in_future")
        val corrected = store.reviseFutureContact(original.id)
        assertNotEquals(originalDraft.id, corrected.id)
        assertEquals("", corrected.operation)
        val changed =
            store.saveContactDraft(
                corrected.id,
                person,
                corrected.channel,
                corrected.outcome,
                "2026-10-31T23:30:00-07:00",
                "-07:00",
                corrected.revision,
            )
        val replacement = store.submitContactDraft(changed.id, changed.revision)
        assertNotEquals(original.id, replacement.id)
        assertEquals(original.envelope, store.dao.operation(original.id)!!.envelope)
        assertEquals("attention", store.dao.operation(original.id)!!.status)
    }

    @Test
    fun migrationThreeToFourPreservesLegacyBytesAndAddsOnlyContactTable() {
        val oldDraft =
            store.saveDraft("legacy-draft", person, "add_note", json("person_id" to person, "body" to "legacy"))
        val oldOperation = store.submitDraft(oldDraft.id, oldDraft.revision)
        store.saveDraft(
            "legacy-draft",
            person,
            "add_note",
            json("person_id" to person, "body" to "legacy retained draft"),
        )
        val legacyEnvelope = oldOperation.envelope
        val raw = database.openHelper.writableDatabase
        raw.execSQL("DROP TABLE contact_drafts")
        raw.execSQL("DROP TABLE room_master_table")
        raw.execSQL("PRAGMA user_version=3")
        database.close()
        database = FieldDatabase.open(context, directory, key)
        store = FieldStore(database, fixtureBinding(), TestClock())
        assertEquals(legacyEnvelope, store.dao.operation(oldOperation.id)!!.envelope)
        assertEquals(
            "legacy retained draft",
            JSONObject(store.dao.draft("legacy-draft")!!.payload).getString("body"),
        )
        assertTrue(store.dao.contactDrafts().isEmpty())
        val contact = save()
        assertNotNull(store.dao.contactDraft(contact.id))
    }

    @Test
    fun dstResolutionRequiresExplicitChoiceAndNeverShiftsNonexistentTime() {
        val zone = ZoneId.of("America/Los_Angeles")
        val repeated = resolveReportedLocal(LocalDateTime.of(2026, 11, 1, 1, 30), zone)
        assertEquals(2, repeated.size)
        assertEquals(setOf("-07:00", "-08:00"), repeated.map { it.offset.id }.toSet())
        assertTrue(resolveReportedLocal(LocalDateTime.of(2026, 3, 8, 2, 30), zone).isEmpty())
        val chosen = save(occurredAt = repeated.last().toString(), offset = repeated.last().offset.id)
        assertEquals(repeated.last().toString(), store.dao.contactDraft(chosen.id)!!.occurredAt)
    }

    @Test
    fun fullDatabaseFailureLeavesContactDraftAndCreatesNoOperation() {
        val draft = save()
        database.openHelper.writableDatabase.execSQL(
            "CREATE TRIGGER reject_contact_outbox BEFORE INSERT ON operations BEGIN SELECT RAISE(ABORT, 'synthetic full storage'); END"
        )
        assertThrows(Exception::class.java) { store.submitContactDraft(draft.id, draft.revision) }
        assertEquals("", store.dao.contactDraft(draft.id)!!.operation)
        assertFalse(store.dao.operations().any { it.kind == "log_contact_attempt" })
    }
}
