package org.crm.field

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.util.UUID
import org.json.JSONArray
import org.json.JSONObject
import org.junit.*
import org.junit.Assert.*
import org.junit.runner.RunWith

class TestClock(
    var bootValue: Int = 3,
    var elapsedValue: Long = 1_000,
    var wallValue: Long = 1_800_000_000_000,
) : DeviceClock {
    override fun boot() = bootValue

    override fun elapsed() = elapsedValue

    override fun wall() = wallValue
}

fun fixture(name: String) =
    JSONObject(
        InstrumentationRegistry.getInstrumentation()
            .context
            .assets
            .open("$name.json")
            .bufferedReader()
            .use { it.readText() }
    )

fun fixtureBinding(): Binding =
    fixture("reconciliation_bootstrap").let {
        Binding.parse(
            it,
            it.getString("actor_user_id"),
            it.getString("organization_id"),
            it.getString("installation_id"),
        )
    }

@RunWith(AndroidJUnit4::class)
class StorageTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private lateinit var directory: File
    private lateinit var database: FieldDatabase
    private lateinit var store: FieldStore
    private val key = ByteArray(32) { (it + 11).toByte() }
    private val clock = TestClock()
    private val person = "10000000-0000-4000-8000-000000000020"

    @Before
    fun open() {
        directory = File(context.noBackupFilesDir, "test-${UUID.randomUUID()}").apply { mkdirs() }
        database = FieldDatabase.open(context, directory, key)
        store = FieldStore(database, fixtureBinding(), clock)
        store.authorize("synthetic-session")
        store.dao.person(
            PersonRow(
                person,
                "1",
                json("id" to person, "display_name" to "Synthetic encryption marker").toString(),
                "[]",
                "[]",
                "[]",
                UUID.randomUUID().toString(),
                "2026-09-13T00:00:00Z",
            )
        )
    }

    @After
    fun close() {
        database.close()
        directory.deleteRecursively()
    }

    private fun reopen() {
        database.close()
        database = FieldDatabase.open(context, directory, key)
        store = FieldStore(database, fixtureBinding(), clock)
    }

    private fun draft(id: String = "draft") =
        store.saveDraft(
            id,
            person,
            "add_note",
            json("person_id" to person, "body" to "Durable secret marker $id"),
        )

    private fun receipt(op: OperationRow, revision: String) =
        json(
            "operation_id" to op.id,
            "outcome" to "accepted",
            "resource_type" to "note",
            "resource_id" to UUID.randomUUID().toString(),
            "committed_revision" to null,
            "person_revision" to revision,
            "changed" to true,
            "accepted_at" to "2026-09-13T00:00:00Z",
            "replayed" to false,
        )

    @Test
    fun encryptedDatabaseWalAndWrongKeyNeverFallBack() {
        draft()
        database.openHelper.writableDatabase.query("PRAGMA wal_autocheckpoint=0").use {
            it.moveToFirst()
        }
        repeat(20) { draft("marker-$it") }
        val wal = File(directory, "field.db-wal")
        assertTrue(wal.exists() && wal.length() > 0)
        for (file in listOf(File(directory, "field.db"), wal)) {
            val bytes = file.readBytes()
            assertFalse(String(bytes, Charsets.ISO_8859_1).contains("Durable secret marker"))
            assertFalse(String(bytes, Charsets.ISO_8859_1).contains("Synthetic encryption marker"))
            assertFalse(
                bytes.take(16).toByteArray().contentEquals("SQLite format 3\u0000".toByteArray())
            )
        }
        reopen()
        assertNotNull(store.dao.draft("draft"))
        database.close()
        assertThrows(Exception::class.java) {
            FieldDatabase.open(context, directory, ByteArray(32) { 7 })
        }
        database = FieldDatabase.open(context, directory, key)
        assertNotNull(database.data().draft("draft"))
    }

    @Test
    fun hundredImmutableActionsAndDraftSurviveReopenAndFailedSubmission() {
        val expected =
            (0 until 100).associate {
                val saved = draft("action-$it")
                val op = store.submitDraft(saved.id, saved.revision)
                op.id to op.envelope
            }
        val saved = draft("failure")
        database.openHelper.writableDatabase.execSQL(
            "CREATE TRIGGER reject_outbox BEFORE INSERT ON operations BEGIN SELECT RAISE(ABORT, 'synthetic storage failure'); END"
        )
        assertThrows(Exception::class.java) { store.submitDraft(saved.id, saved.revision) }
        assertEquals(saved, store.dao.draft(saved.id))
        assertEquals(100, store.dao.operations().size)
        database.openHelper.writableDatabase.execSQL("DROP TRIGGER reject_outbox")
        val before = store.dao.operations().first()
        store.dao.operationState(before.id, "uploading", 1, 9000, "unavailable")
        reopen()
        store.dao.recoverUploads()
        assertEquals(expected, store.dao.operations().associate { it.id to it.envelope })
        assertEquals("queued", store.dao.operation(before.id)!!.status)
        assertNotNull(store.dao.draft("failure"))
        assertThrows(IllegalArgumentException::class.java) {
            store.submitDraft(saved.id, saved.revision - 1)
        }
    }

    @Test
    fun leaseAndContextFailClosedWithoutDroppingWork() {
        val saved = draft()
        store.submitDraft(saved.id, saved.revision)
        clock.bootValue++
        assertThrows(AccessLocked::class.java) { store.requireAccess() }
        reopen()
        assertEquals(1, store.dao.operations().size)
        assertThrows(AccessLocked::class.java) { store.requireAccess() }
        store.authorize("fresh-synthetic-session")
        store.requireAccess()
        val changed =
            JSONObject(store.binding.bootstrap).put("context_id", UUID.randomUUID().toString())
        val other =
            Binding.parse(
                changed,
                store.binding.actor,
                store.binding.org,
                store.binding.installation,
            )
        assertThrows(AccessLocked::class.java) {
            FieldStore(database, other, clock).authorize("another")
        }
        store.lock()
        assertThrows(AccessLocked::class.java) { draft("after-lock") }
        assertEquals(1, store.dao.operations().size)
    }

    @Test
    fun partialPagesAndFailedPromotionPreserveActiveCacheAndReceiptOverlay() {
        val generation = fixture("reconciliation")
        val chosen = fixture("summary_page").getString("person_id")
        generation
            .put("selected_count", 1)
            .put(
                "manifest",
                json(
                    "items" to JSONArray().put(json("person_id" to chosen, "revision" to "2001")),
                    "complete" to true,
                    "next_cursor" to null,
                ),
            )
        store.beginGeneration(generation)
        val id = generation.getString("generation_id")
        val summary = fixture("summary_page")
        store.stagePage(id, chosen, "summary", "", summary)
        assertEquals("", store.nextPage(id, chosen, "notes"))
        reopen()
        assertNull(store.nextPage(id, chosen, "summary"))
        assertNotNull(store.dao.person(person))
        for (section in listOf("notes", "tasks")) {
            val page = fixture("${section}_page").put("next_cursor", null).put("complete", true)
            store.stagePage(id, chosen, section, "", page)
        }
        val seal = fixture("seal").put("selected_count", 1)
        database.openHelper.writableDatabase.execSQL(
            "CREATE TRIGGER reject_promotion BEFORE INSERT ON people BEGIN SELECT RAISE(ABORT, 'synthetic storage failure'); END"
        )
        assertThrows(Exception::class.java) { store.promote(seal) }
        assertNotNull(store.dao.person(person))
        assertNull(store.dao.person(chosen))
        assertNotNull(store.dao.meta("generation"))
        database.openHelper.writableDatabase.execSQL("DROP TRIGGER reject_promotion")
        store.promote(seal)
        assertNull(store.dao.person(person))
        assertEquals("2001", store.dao.person(chosen)!!.revision)
        val saved =
            store.saveDraft(
                "race",
                chosen,
                "add_note",
                json("person_id" to chosen, "body" to "Accepted after old generation"),
            )
        val op = store.submitDraft(saved.id, saved.revision)
        store.acknowledge(op.id, receipt(op, "2002"))
        // Generation and receipt use the same transaction lock; an older complete seal cannot cover
        // revision 2002.
        store.beginGeneration(generation)
        store.promote(seal)
        assertEquals("accepted", store.dao.operation(op.id)!!.status)
        store.dao.person(store.dao.person(chosen)!!.copy(revision = "2002"))
        store.acknowledge(op.id, receipt(op, "2002"))
        assertEquals("covered", store.dao.operation(op.id)!!.status)
    }

    @Test
    fun migrationOneToTwoPreservesDraftsOperationsAndCheckpoint() {
        val saved = draft()
        val op = store.submitDraft(saved.id, saved.revision)
        draft("remaining")
        store.dao.meta(MetaRow("manifest_cursor", "durable-checkpoint"))
        // Construct the actual v1 shape from v2, without destructive Room fallback.
        val raw = database.openHelper.writableDatabase
        raw.execSQL("ALTER TABLE operations DROP COLUMN lastError")
        raw.execSQL("DROP TABLE room_master_table")
        raw.execSQL("PRAGMA user_version=1")
        reopen()
        assertEquals(op.envelope, store.dao.operation(op.id)!!.envelope)
        assertEquals("", store.dao.operation(op.id)!!.lastError)
        assertNotNull(store.dao.draft("remaining"))
        assertEquals("durable-checkpoint", store.dao.meta("manifest_cursor"))
        database.openHelper.writableDatabase.query("PRAGMA user_version").use {
            it.moveToFirst()
            assertEquals(2, it.getInt(0))
        }
    }

    @Test
    fun deviceKeysAreWrappedAndIsolatedAcrossActorsOrganizationsAndOrigins() {
        val name = "vault-${UUID.randomUUID()}"
        val vault = DeviceVault(context, name)
        try {
            val registry = vault.registry()
            val a = vault.accountId("https://a.example", "actor-a", "org-a")
            assertNotEquals(a, vault.accountId("https://a.example", "actor-b", "org-a"))
            assertNotEquals(a, vault.accountId("https://a.example", "actor-a", "org-b"))
            assertNotEquals(a, vault.accountId("https://b.example", "actor-a", "org-a"))
            val dir = vault.accountDirectory(a)
            val secret = vault.databaseKey(dir)
            assertArrayEquals(secret, DeviceVault(context, name).databaseKey(dir))
            assertFalse(File(dir, "database.key").readBytes().contentEquals(secret))
            assertFalse(
                String(File(vault.root, "registry.bin").readBytes())
                    .contains(registry.getString("installation"))
            )
            java.security.KeyStore.getInstance("AndroidKeyStore").apply {
                load(null)
                deleteEntry("crm.$name.device-wrap.v1")
            }
            assertThrows(Exception::class.java) { vault.databaseKey(dir) }
            assertTrue(File(dir, "database.key").exists())
        } finally {
            vault.root.deleteRecursively()
        }
    }

    @Test
    fun staleAutosaveCannotOverwriteAndRemovedPersonCannotReopenOrSubmit() {
        val first = draft("cas")
        val second =
            store.saveDraft(
                first.id,
                person,
                "add_note",
                json("person_id" to person, "body" to "newer committed edit"),
                first.revision,
            )
        assertThrows(IllegalArgumentException::class.java) {
            store.saveDraft(first.id, person, "add_note", JSONObject(first.payload), first.revision)
        }
        assertEquals(second, store.dao.draft(first.id))
        val opDraft = draft("queued")
        val op = store.submitDraft(opDraft.id, opDraft.revision)
        val generation =
            fixture("reconciliation")
                .put("selected_count", 0)
                .put(
                    "manifest",
                    json("items" to JSONArray(), "complete" to true, "next_cursor" to null),
                )
        store.beginGeneration(generation)
        store.promote(fixture("seal").put("selected_count", 0))
        assertNull(store.dao.person(person))
        assertEquals(second, store.dao.draft(first.id))
        assertEquals(op.envelope, store.dao.operation(op.id)!!.envelope)
        assertThrows(IllegalArgumentException::class.java) {
            store.submitDraft(second.id, second.revision)
        }
        assertTrue(store.dao.meta("coverage")!!.contains("protected saved-work"))
    }

    @Test
    fun sharedFixtureMalformedIdentityCannotEnterOtherAccount() {
        val atomic = fixture("bootstrap")
        assertThrows(ProtocolFailure::class.java) {
            Binding.parse(
                atomic,
                store.binding.actor,
                store.binding.org,
                store.binding.installation,
            )
        }
        assertThrows(ProtocolFailure::class.java) {
            store.beginGeneration(
                fixture("reconciliation").put("context_id", UUID.randomUUID().toString())
            )
        }
        assertEquals(1, store.dao.people().size)
    }
}
