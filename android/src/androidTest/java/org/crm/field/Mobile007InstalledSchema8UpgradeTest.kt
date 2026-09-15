package org.crm.field

import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.security.MessageDigest
import kotlinx.coroutines.runBlocking
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test

/**
 * Installed Mobile006 -> Mobile007 proof. The seed phase runs with the exact
 * HEAD Mobile006 APK (schema 8); it never edits user_version or downgrades a
 * physical database. The verify phase runs after current APK -r installation.
 */
class Mobile007InstalledSchema8UpgradeTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val personPrefix = "10000000-0000-4000-8000-000000000"
    private val person = "10000000-0000-4000-8000-000000000096"
    private val queued = "30000000-0000-4000-8000-000000000096"
    private val accepted = "30000000-0000-4000-8000-000000000097"

    @Test
    fun seedSchemaEightProtectedStore() {
        assumeTrue(
            InstrumentationRegistry.getArguments().getString("mobile007UpgradePhase") == "seed"
        )
        val vault = DeviceVault(context, BuildConfig.VAULT_NAMESPACE)
        val account = "e".repeat(64)
        val directory = vault.accountDirectory(account)
        val key = vault.databaseKey(directory)
        val binding = binding()
        val database = FieldDatabase.open(context, directory, key)
        val store = FieldStore(database, binding, TestClock()).also { it.authorize("synthetic") }
        val version = database.openHelper.readableDatabase.query("PRAGMA user_version").use {
            it.moveToFirst(); it.getInt(0)
        }
        assertEquals("seed must use the installed Mobile006 schema", 8, version)
        repeat(100) { index ->
            val id = "$personPrefix%03d".format(index)
            store.dao.person(
                PersonRow(
                    id,
                    "7",
                    json("id" to id, "display_name" to "Schema Eight Person %03d".format(index)).toString(),
                    "[]",
                    "[]",
                    "[]",
                    "mobile006-generation",
                    "2026-09-15T00:00:00Z",
                    true,
                    true,
                    true,
                )
            )
        }
        val envelope = json(
            "context_id" to binding.context,
            "operation_id" to queued,
            "kind" to "add_note",
            "device_recorded_at" to "2026-09-15T00:00:00Z",
            "payload" to json("person_id" to person, "body" to "Schema eight queued note"),
        ).toString()
        val draft = json("person_id" to person, "body" to "Schema eight protected draft").toString()
        val receipt = json(
            "operation_id" to accepted,
            "outcome" to "accepted",
            "resource_type" to "note",
            "resource_id" to "81000000-0000-4000-8000-000000000096",
            "committed_revision" to JSONObject.NULL,
            "person_revision" to "7",
            "accepted_at" to "2026-09-15T00:01:00Z",
            "changed" to true,
            "replayed" to false,
        ).toString()
        store.dao.operation(OperationRow(queued, person, "add_note", envelope, 1, retryAt = Long.MAX_VALUE))
        store.dao.operation(OperationRow(accepted, person, "add_note", envelope.replace(queued, accepted), 2, "covered", receipt))
        store.dao.draft(DraftRow("schema-eight-draft", person, "add_note", draft, 1))
        vault.saveRegistry(
            json(
                "account" to account,
                "locked" to false,
                "pending" to 2,
                "actor" to binding.actor,
                "org" to binding.org,
                "installation" to binding.installation,
            )
        )
        vault.setLocked(false)
        File(context.filesDir, "mobile007-schema8-before.json").writeText(
            json(
                "schema" to version,
                "key" to digest(key),
                "envelope" to envelope,
                "receipt" to receipt,
                "draft" to draft,
            ).toString()
        )
        database.close()
    }

    @Test
    fun verifySchemaNineUpgradePreservesProtectedRows() = runBlocking {
        assumeTrue(
            InstrumentationRegistry.getArguments().getString("mobile007UpgradePhase") == "verify"
        )
        (context.applicationContext as FieldApplication).ready.join()
        val before = org.json.JSONObject(File(context.filesDir, "mobile007-schema8-before.json").readText())
        val vault = DeviceVault(context, BuildConfig.VAULT_NAMESPACE)
        val directory = vault.accountDirectory(vault.registry().getString("account"))
        val key = vault.databaseKey(directory)
        val database = FieldDatabase.open(context, directory, key)
        val version = database.openHelper.readableDatabase.query("PRAGMA user_version").use {
            it.moveToFirst(); it.getInt(0)
        }
        val dao = database.data()
        assertEquals(9, version)
        assertEquals(before.getString("key"), digest(key))
        assertEquals(100, dao.people().size)
        assertEquals(before.getString("envelope"), dao.operation(queued)!!.envelope)
        assertEquals(before.getString("receipt"), dao.operation(accepted)!!.receipt)
        assertEquals(before.getString("draft"), dao.draft("schema-eight-draft")!!.payload)
        assertFalse(dao.person(person)!!.metadataRevisionsQualified)
        assertTrue(dao.metadataDrafts().isEmpty())
        database.close()
    }

    private fun digest(bytes: ByteArray): String =
        MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }

    private fun binding(): Binding {
        val actor = "80000000-0000-4000-8000-000000000096"
        val org = "90000000-0000-4000-8000-000000000096"
        val installation = "a0000000-0000-4000-8000-000000000096"
        return Binding.parse(
            json(
                "protocol" to PROTOCOL,
                "actor_user_id" to actor,
                "organization_id" to org,
                "installation_id" to installation,
                "context_id" to "b0000000-0000-4000-8000-000000000096",
                "workspace_revision" to "1",
                "capabilities" to JSONArray(listOf("add_note", "create_task", "complete_task", "reconciliation")),
                "server_time" to "2026-09-15T00:00:00Z",
                "offline_access_expires_at" to "2026-09-20T00:00:00Z",
            ),
            actor,
            org,
            installation,
        )
    }
}
