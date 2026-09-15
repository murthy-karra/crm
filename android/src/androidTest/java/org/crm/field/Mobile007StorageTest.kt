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

/** Mobile007 pin intent durability: revision fences, restart recovery and old-store seeding. */
@RunWith(AndroidJUnit4::class)
class Mobile007StorageTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val key = ByteArray(32) { (it + 41).toByte() }
    private val person = "10000000-0000-4000-8000-000000000007"
    private val second = "20000000-0000-4000-8000-000000000007"
    private lateinit var directory: File
    private lateinit var database: FieldDatabase
    private lateinit var store: FieldStore

    @Before fun open() {
        directory = File(context.noBackupFilesDir, "mobile007-${UUID.randomUUID()}").apply { mkdirs() }
        database = FieldDatabase.open(context, directory, key)
        store = FieldStore(database, binding(), TestClock()).also { it.authorize("synthetic") }
    }

    @After fun close() {
        if (::database.isInitialized) database.close()
        directory.deleteRecursively()
    }

    @Test fun addCancelIsRevisionedAndInvalidPinReviewSurvivesRestart() {
        store.setPinIntent(person, true)
        assertEquals(listOf(person), store.requestedPins())
        assertEquals("1", store.dao.pinSetRevision())
        store.setPinIntent(person, true)
        assertEquals("1", store.dao.pinSetRevision())
        store.setPinIntent(person, false)
        assertTrue(store.requestedPins().isEmpty())
        assertEquals("2", store.dao.pinSetRevision())
        store.setPinIntent(person, false)
        assertEquals("2", store.dao.pinSetRevision())

        store.setPinIntent(person, true)
        val frozen = store.preparePinStaging()
        assertEquals(3L, frozen.first)
        assertEquals(listOf(person), frozen.second)
        store.setPinIntent(second, true)
        assertEquals("4", store.dao.pinSetRevision())
        assertEquals("3", store.dao.meta("staging_pin_revision"))
        assertEquals(JSONArray().put(person).toString(), store.dao.meta("staging_pins"))

        store.markPinFailure("not_found")
        assertEquals("not_found", store.dao.pinIntents().single { it.person == person }.failure)
        database.close()
        database = FieldDatabase.open(context, directory, key)
        store = FieldStore(database, binding(), TestClock()).also { it.authorize("synthetic") }
        assertEquals("not_found", store.dao.pinIntents().single { it.person == person }.failure)
        store.retryPinRequests()
        assertNull(store.dao.pinIntents().single { it.person == person }.failure)
    }

    @Test fun promotionAdmitsFrozenRevisionAndFollowupRevisionRemainsPending() {
        store.setPinIntent(person, true)
        val frozen = store.preparePinStaging()
        val generation = "30000000-0000-4000-8000-000000000007"
        store.beginGeneration(generationResponse(generation), frozen.first)
        listOf("summary", "notes", "tasks").forEach { section ->
            store.stagePage(generation, person, section, "", page(generation, section))
        }
        store.setPinIntent(second, true)
        assertEquals(frozen.first + 1, store.dao.pinSetRevision()!!.toLong())
        store.promote(seal(generation))
        assertEquals(frozen.first.toString(), store.dao.admittedPinRevision())
        assertEquals(frozen.first + 1, store.dao.pinSetRevision()!!.toLong())
        assertEquals(listOf(person, second), store.requestedPins())
        assertEquals(listOf("pinned"), store.activeReasons()[person])
    }

    @Test fun populatedVersionEightShapeMigratesAndSeedsPinIntent() {
        store.setPinIntent(person, true)
        database.close()

        val raw = FieldDatabase.open(context, directory, key)
        val sql = raw.openHelper.writableDatabase
        sql.execSQL("DROP TABLE pin_intents")
        sql.execSQL("CREATE TABLE old_manifest(generation TEXT NOT NULL, person TEXT NOT NULL, revision TEXT NOT NULL, metadataRevision TEXT NOT NULL DEFAULT '', PRIMARY KEY(generation,person))")
        sql.execSQL("INSERT INTO old_manifest(generation,person,revision,metadataRevision) SELECT generation,person,revision,metadataRevision FROM manifest")
        sql.execSQL("DROP TABLE manifest")
        sql.execSQL("ALTER TABLE old_manifest RENAME TO manifest")
        sql.execSQL("PRAGMA user_version=8")
        raw.close()

        database = FieldDatabase.open(context, directory, key)
        store = FieldStore(database, binding(), TestClock()).also { it.authorize("synthetic") }
        assertEquals(9, database.openHelper.readableDatabase.query("PRAGMA user_version").use { it.moveToFirst(); it.getInt(0) })
        assertEquals(listOf(person), store.requestedPins())
        assertEquals("1", store.dao.pinSetRevision())
    }

    private fun generationResponse(id: String) = json(
        "context_id" to store.binding.context, "generation_id" to id, "evaluated_at" to "2026-09-15T00:00:00Z",
        "expires_at" to "2026-09-15T00:30:00Z", "complete" to true, "selected_count" to 1,
        "manifest" to json("items" to JSONArray().put(json("person_id" to person, "revision" to "1", "reasons" to JSONArray().put("pinned"))), "next_cursor" to null, "complete" to true),
    )

    private fun page(generation: String, section: String) = json(
        "generation_id" to generation, "person_id" to person, "revision" to "1", "section" to section,
        "summary" to if (section == "summary") json("id" to person, "display_name" to "Synthetic Person") else JSONObject.NULL,
        "items" to JSONArray(), "next_cursor" to null, "complete" to true,
    )

    private fun seal(generation: String) = json(
        "context_id" to store.binding.context, "generation_id" to generation, "evaluated_at" to "2026-09-15T00:00:00Z",
        "sealed_at" to "2026-09-15T00:01:00Z", "selected_count" to 1,
        "today" to json("sources" to json("status" to "complete"), "items" to JSONArray()),
    )

    private fun binding(): Binding {
        val actor = "80000000-0000-4000-8000-000000000007"
        val org = "90000000-0000-4000-8000-000000000007"
        val installation = "a0000000-0000-4000-8000-000000000007"
        return Binding.parse(
            json("protocol" to PROTOCOL, "actor_user_id" to actor, "organization_id" to org, "installation_id" to installation,
                "context_id" to "b0000000-0000-4000-8000-000000000007", "workspace_revision" to "1",
                "capabilities" to JSONArray(listOf("add_note", "create_task", "complete_task", "reconciliation")), "server_time" to "2026-09-15T00:00:00Z",
                "offline_access_expires_at" to "2026-09-16T00:00:00Z"), actor, org, installation,
        )
    }
}
