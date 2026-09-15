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

/** Mobile006 local trust boundary: complete metadata/catalog, CAS and immutable receipt bytes. */
@RunWith(AndroidJUnit4::class)
class Mobile006StorageTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val key = ByteArray(32) { (it + 17).toByte() }
    private val person = "10000000-0000-4000-8000-000000000006"
    private val tag = "20000000-0000-4000-8000-000000000006"
    private val text = "30000000-0000-4000-8000-000000000006"
    private val number = "40000000-0000-4000-8000-000000000006"
    private val date = "50000000-0000-4000-8000-000000000006"
    private val choice = "60000000-0000-4000-8000-000000000006"
    private val option = "70000000-0000-4000-8000-000000000006"
    private lateinit var directory: File
    private lateinit var database: FieldDatabase
    private lateinit var store: FieldStore
    private lateinit var clock: TestClock

    @Before fun open() {
        directory = File(context.noBackupFilesDir, "mobile006-${UUID.randomUUID()}").apply { mkdirs() }
        database = FieldDatabase.open(context, directory, key)
        clock = TestClock()
        store = FieldStore(database, binding(), clock); store.authorize("synthetic")
        store.dao.metadataTags(listOf(MetadataTagRow(tag, "Buyer", "g", "3")))
        store.dao.metadataFields(listOf(
            MetadataFieldRow(text, "Text", "text", 1, null, "g", "3"),
            MetadataFieldRow(number, "Number", "number", 2, null, "g", "3"),
            MetadataFieldRow(date, "Date", "date", 3, null, "g", "3"),
            MetadataFieldRow(choice, "Choice", "choice", 4, null, "g", "3"),
        ))
        store.dao.metadataOptions(listOf(MetadataOptionRow(option, choice, "North", 1, null, "g", "3")))
        store.dao.person(PersonRow(person, "9", summary().toString(), "[]", "[]", "[]", "g", "now", true, true, true, true, baseline().toString()))
    }
    @After fun close() { database.close(); directory.deleteRecursively() }

    @Test fun mixedDraftRestartSafeEnvelopeAndReceiptAreAtomic() {
        val draft = store.saveMetadataDraft(UUID.randomUUID().toString(), person, actions())
        val operation = store.submitMetadataDraft(draft.id, draft.revision)
        assertEquals(operation.id, store.submitMetadataDraft(draft.id, draft.revision).id)
        val payload = JSONObject(operation.envelope).getJSONObject("payload")
        assertEquals("2", payload.getString("expected_metadata_revision")); assertEquals("3", payload.getString("expected_catalog_revision"))
        assertEquals(4, payload.getJSONArray("actions").length())
        assertEquals(operation.id, store.dao.metadataDraft(draft.id)!!.operation)
        store.acknowledge(operation.id, receipt(operation))
        assertEquals("accepted", store.dao.operation(operation.id)!!.status)
        assertEquals("accepted", store.dao.metadataDraft(draft.id)!!.state)
    }

    @Test fun exactDecimalDateChoiceAndExplicitClearArePreservedWithoutCoercion() {
        val values = JSONArray()
            .put(json("kind" to "set_field", "field_id" to number, "value" to json("number" to "12.3400")))
            .put(json("kind" to "set_field", "field_id" to date, "value" to json("date" to "2026-09-14")))
            .put(json("kind" to "set_field", "field_id" to choice, "value" to json("option_id" to option)))
            .put(json("kind" to "clear_field", "field_id" to text))
        val draft = store.saveMetadataDraft(UUID.randomUUID().toString(), person, values)
        val payload = JSONObject(store.submitMetadataDraft(draft.id, draft.revision).envelope).getJSONObject("payload")
        assertEquals("12.3400", payload.getJSONArray("actions").getJSONObject(0).getJSONObject("value").getString("number"))
        assertEquals("2026-09-14", payload.getJSONArray("actions").getJSONObject(1).getJSONObject("value").getString("date"))
    }

    @Test fun malformedDuplicateAndPartialBaselinesNeverCreateOperations() {
        val invalid = JSONArray().put(json("kind" to "set_field", "field_id" to number, "value" to json("number" to "12.34567")))
        assertThrows(Exception::class.java) { store.saveMetadataDraft(UUID.randomUUID().toString(), person, invalid) }
        val duplicate = JSONArray().put(json("kind" to "add_tag", "tag_id" to tag)).put(json("kind" to "remove_tag", "tag_id" to tag))
        assertThrows(Exception::class.java) { store.saveMetadataDraft(UUID.randomUUID().toString(), person, duplicate) }
        val old = store.dao.person(person)!!
        store.dao.person(old.copy(metadataRevisionsQualified = false))
        assertThrows(Exception::class.java) { store.saveMetadataDraft(UUID.randomUUID().toString(), person, actions()) }
        assertTrue(store.dao.operations().isEmpty())
    }

    @Test fun catalogConflictRetainsOriginalAndCreatesNewCasDraftOnlyAfterCurrentRead() {
        val original = store.saveMetadataDraft(UUID.randomUUID().toString(), person, actions())
        val operation = store.submitMetadataDraft(original.id, original.revision)
        store.dao.operationState(operation.id, "attention", 1, 0, "catalog_revision_conflict")
        val current = baseline().put("metadata_revision", "4").put("catalog_revision", "5")
        store.recordCurrentMetadata(operation.id, json("context_id" to store.binding.context, "person_id" to person, "person_revision" to "10", "metadata_revision" to "4", "catalog_revision" to "5", "tags" to current.getJSONArray("tags"), "values" to current.getJSONArray("values"), "complete" to true))
        // A conflict read alone must not rebind the editor to labels/options from catalog 3.
        store.dao.meta(MetaRow("metadata_catalog_revision", "3"))
        assertThrows(IllegalArgumentException::class.java) {
            store.reviseMetadataConflict(operation.id, UUID.randomUUID().toString())
        }
        assertEquals("attention", store.dao.operation(operation.id)!!.status)
        // Model the causally later sealed reconciliation: exact catalog and Person metadata
        // component have both been promoted before the user can create a replacement.
        store.dao.meta(MetaRow("metadata_catalog_revision", "5"))
        store.dao.clearMetadataTags(); store.dao.clearMetadataFields(); store.dao.clearMetadataOptions()
        store.dao.metadataTags(listOf(MetadataTagRow(tag, "Buyer", "g2", "5")))
        store.dao.metadataFields(listOf(MetadataFieldRow(text, "Text", "text", 1, null, "g2", "5")))
        store.dao.person(store.dao.person(person)!!.copy(revision = "10", metadata = current.toString(), metadataRevisionsQualified = true))
        val replacement = store.reviseMetadataConflict(operation.id, UUID.randomUUID().toString())
        assertEquals("superseded", store.dao.operation(operation.id)!!.status)
        val second = store.submitMetadataDraft(replacement.id, replacement.revision)
        assertNotEquals(operation.id, second.id)
        assertEquals("5", JSONObject(second.envelope).getJSONObject("payload").getString("expected_catalog_revision"))
    }

    @Test fun frozenMetadataDescriptorAndComponentWithoutItemsAreAcceptedOnlyWhenPinned() {
        val generation = "d0000000-0000-4000-8000-000000000006"
        val response = json(
            "context_id" to store.binding.context, "generation_id" to generation,
            "evaluated_at" to "2026-09-14T00:00:00Z", "expires_at" to "2026-09-14T00:30:00Z",
            "complete" to true, "selected_count" to 1,
            "metadata" to json("representation" to "metadata-v1", "catalog_revision" to "3", "catalog_url" to "/api/mobile/v1/reconciliations/$generation/metadata/catalog"),
            "manifest" to json("items" to JSONArray().put(json("person_id" to person, "revision" to "9", "metadata_revision" to "2")), "next_cursor" to null, "complete" to true),
        )
        store.beginGeneration(response)
        store.metadataCatalogPage(generation, "tags", "", json("generation_id" to generation, "section" to "tags", "revision" to "3", "items" to JSONArray(), "next_cursor" to null, "complete" to true))
        store.stagePage(generation, person, "metadata", "", json("generation_id" to generation, "person_id" to person, "section" to "metadata", "revision" to "9", "metadata_revision" to "2", "catalog_revision" to "3", "tags" to JSONArray(), "values" to JSONArray(), "complete" to true))
        assertNull(store.nextPage(generation, person, "metadata"))
    }

    @Test fun catalogOnlyRevisionChangeRefetchesAndRequalifiesSamePersonRevision() {
        val old = store.dao.person(person)!!
        store.dao.meta(MetaRow("metadata_catalog_revision", "3"))
        assertFalse(store.metadataQualified(old, "4", "2"))
        val generation = "e0000000-0000-4000-8000-000000000006"
        store.beginGeneration(json(
            "context_id" to store.binding.context, "generation_id" to generation,
            "evaluated_at" to "2026-09-14T01:00:00Z", "expires_at" to "2026-09-14T01:30:00Z",
            "complete" to true, "selected_count" to 1,
            "metadata" to json("representation" to "metadata-v1", "catalog_revision" to "4", "catalog_url" to "/api/mobile/v1/reconciliations/$generation/metadata/catalog"),
            "manifest" to json("items" to JSONArray().put(json("person_id" to person, "revision" to "9", "metadata_revision" to "2")), "next_cursor" to null, "complete" to true),
        ))
        listOf("tags", "fields", "options").forEach { section ->
            store.metadataCatalogPage(generation, section, "", json("generation_id" to generation, "section" to section, "revision" to "4", "items" to JSONArray(), "next_cursor" to null, "complete" to true))
        }
        fun page(section: String, summary: Any = JSONObject.NULL) =
            json("generation_id" to generation, "person_id" to person, "revision" to "9", "section" to section, "summary" to summary, "items" to JSONArray(), "next_cursor" to null, "complete" to true)
        store.stagePage(generation, person, "summary", "", page("summary", summary()))
        store.stagePage(generation, person, "notes", "", page("notes"))
        store.stagePage(generation, person, "tasks", "", page("tasks"))
        store.stagePage(generation, person, "metadata", "", json("generation_id" to generation, "person_id" to person, "revision" to "9", "section" to "metadata", "metadata_revision" to "2", "catalog_revision" to "4", "tags" to JSONArray(), "values" to JSONArray(), "complete" to true))
        store.promote(json("context_id" to store.binding.context, "generation_id" to generation, "evaluated_at" to "2026-09-14T01:00:00Z", "sealed_at" to "2026-09-14T01:01:00Z", "selected_count" to 1, "today" to json("sources" to json("status" to "complete"), "items" to JSONArray())))
        val refreshed = store.dao.person(person)!!
        assertEquals("4", JSONObject(refreshed.metadata).getString("catalog_revision"))
        assertTrue(store.metadataQualified(refreshed, "4", "2"))
        assertFalse(store.metadataQualified(refreshed, "3", "2"))
    }

    @Test fun matchingMetadataTokenCannotReusePersonWhenCatalogRowsAreMissing() {
        val cached = store.dao.person(person)!!
        store.dao.meta(MetaRow("metadata_catalog_revision", "3"))
        assertTrue(store.metadataQualified(cached, "3", "2"))
        store.dao.clearMetadataTags(); store.dao.clearMetadataFields(); store.dao.clearMetadataOptions()
        assertFalse("a token without renderable typed rows must refetch", store.metadataQualified(cached, "3", "2"))
        store.dao.metadataTags(listOf(MetadataTagRow(tag, "Buyer", "g", "3")))
        store.dao.metadataFields(listOf(MetadataFieldRow(text, "Text", "text", 1, null, "g", "3")))
        assertTrue(store.metadataQualified(cached, "3", "2"))
    }

    @Test fun archivedFieldsCanBeClearedButDeletedAndArchivedSetTargetsAreRejected() {
        // A catalog refresh can archive a field or option while an older baseline still contains
        // its value. Clearing remains a valid explicit intent; selecting it again is not.
        store.dao.metadataFields(listOf(MetadataFieldRow(text, "Text", "text", 1, "2026-09-14T00:00:00Z", "g", "3")))
        assertThrows(IllegalArgumentException::class.java) {
            store.saveMetadataDraft(UUID.randomUUID().toString(), person,
                JSONArray().put(json("kind" to "set_field", "field_id" to text, "value" to json("text" to "cannot restore"))))
        }
        val clear = store.saveMetadataDraft(UUID.randomUUID().toString(), person,
            JSONArray().put(json("kind" to "clear_field", "field_id" to text)))
        assertEquals(1L, clear.revision)

        store.dao.metadataFields(listOf(MetadataFieldRow(choice, "Choice", "choice", 4, null, "g", "3")))
        store.dao.metadataOptions(listOf(MetadataOptionRow(option, choice, "North", 1, "2026-09-14T00:00:00Z", "g", "3")))
        assertThrows(IllegalArgumentException::class.java) {
            store.saveMetadataDraft(UUID.randomUUID().toString(), person,
                JSONArray().put(json("kind" to "set_field", "field_id" to choice, "value" to json("option_id" to option))))
        }
        // The removed tag is deliberately not accepted as an empty/unknown target.
        store.dao.clearMetadataTags()
        assertThrows(IllegalArgumentException::class.java) {
            store.saveMetadataDraft(UUID.randomUUID().toString(), person,
                JSONArray().put(json("kind" to "add_tag", "tag_id" to tag)))
        }
        assertTrue(store.dao.operations().isEmpty())
    }

    @Test fun expiredLeaseLocksMetadataAndLateBootstrapIdentityCannotEnterAccount() {
        clock.elapsedValue += 86_400_001
        assertThrows(AccessLocked::class.java) {
            store.saveMetadataDraft(UUID.randomUUID().toString(), person, actions())
        }
        assertEquals("true", store.dao.meta("locked"))
        assertTrue(store.dao.metadataDrafts().isEmpty())
        val bootstrap = JSONObject(binding().bootstrap).put("actor_user_id", "80000000-0000-4000-8000-000000000099")
        assertThrows(ProtocolFailure::class.java) {
            Binding.parse(bootstrap, "80000000-0000-4000-8000-000000000006", "90000000-0000-4000-8000-000000000006", "a0000000-0000-4000-8000-000000000006")
        }
    }

    @Test fun metadataStoreKeyIsWrappedAndCannotBeRecoveredAfterKeystoreLoss() {
        val name = "mobile006-vault-${UUID.randomUUID()}"
        val vault = DeviceVault(context, name)
        try {
            val account = vault.accountId("https://metadata.example", "actor-a", "org-a")
            val directory = vault.accountDirectory(account)
            val secret = vault.databaseKey(directory)
            assertArrayEquals(secret, DeviceVault(context, name).databaseKey(directory))
            assertFalse(File(directory, "database.key").readBytes().contentEquals(secret))
            java.security.KeyStore.getInstance("AndroidKeyStore").apply {
                load(null); deleteEntry("crm.$name.device-wrap.v1")
            }
            assertThrows(Exception::class.java) { vault.databaseKey(directory) }
            assertTrue("ciphertext stays protected for an explicit reauthorization path", File(directory, "database.key").exists())
        } finally {
            vault.root.deleteRecursively()
        }
    }

    private fun actions() = JSONArray()
        .put(json("kind" to "add_tag", "tag_id" to tag))
        .put(json("kind" to "set_field", "field_id" to text, "value" to json("text" to "Saved offline")))
        .put(json("kind" to "set_field", "field_id" to number, "value" to json("number" to "12.3400")))
        .put(json("kind" to "set_field", "field_id" to date, "value" to json("date" to "2026-09-14")))
    private fun baseline() = json("metadata_revision" to "2", "catalog_revision" to "3", "tags" to JSONArray(), "values" to JSONArray().put(json("field_id" to text, "value" to json("text" to "Old"))), "complete" to true)
    private fun summary() = json("id" to person, "display_name" to "Metadata Person", "details_revision" to "1", "stage_revision" to "1")
    private fun receipt(op: OperationRow) = json("operation_id" to op.id, "outcome" to "accepted", "resource_type" to "person_metadata", "resource_id" to person, "committed_revision" to "3", "person_revision" to "10", "changed" to true, "accepted_at" to "2026-09-14T00:00:00Z", "replayed" to false)
    private fun binding(): Binding {
        val actor = "80000000-0000-4000-8000-000000000006"; val org = "90000000-0000-4000-8000-000000000006"; val installation = "a0000000-0000-4000-8000-000000000006"
        return Binding.parse(json("protocol" to PROTOCOL, "actor_user_id" to actor, "organization_id" to org, "installation_id" to installation, "context_id" to "b0000000-0000-4000-8000-000000000006", "workspace_revision" to "1", "capabilities" to JSONArray(listOf("add_note", "create_task", "complete_task", "reconciliation", "update_person_metadata", "metadata_revisions", "metadata_catalog")), "server_time" to "2026-09-14T00:00:00Z", "offline_access_expires_at" to "2026-09-15T00:00:00Z"), actor, org, installation)
    }
}
