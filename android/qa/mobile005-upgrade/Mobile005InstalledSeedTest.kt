package org.crm.field

import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

/** Runs only in an unused QA identity, built against immutable Mobile004 schema-5 source. */
class Mobile005InstalledSeedTest {
    @Test fun seedActualMobile004StoreAndCaptureInventory() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        assertEquals("org.crm.field.mobile005reviewupgradeqa", context.packageName)
        val vault = DeviceVault(context, BuildConfig.VAULT_NAMESPACE)
        val directory = vault.accountDirectory("a".repeat(64))
        assertFalse("Never reseed an installed populated store", File(directory, "field.db").exists())
        val key = vault.databaseKey(directory)
        val db = FieldDatabase.open(context, directory, key)
        val store = FieldStore(db, fixtureBinding(), TestClock())
        store.authorize("synthetic")
        val binding = store.binding
        vault.saveRegistry(json("account" to "a".repeat(64), "locked" to false, "pending" to 4,
            "actor" to binding.actor, "org" to binding.org, "installation" to binding.installation))
        vault.setLocked(false)
        val person = "10000000-0000-4000-8000-000000000091"
        val contact = "20000000-0000-4000-8000-000000000091"
        store.dao.person(PersonRow(person, "7", json("id" to person, "display_name" to "Upgrade Person", "first_name" to "Upgrade", "last_name" to "Person").toString(),
            org.json.JSONArray().put(json("id" to contact, "kind" to "email", "value" to "upgrade@example.test")).toString(), "[]", "[]", "old", "2026-09-14T00:00:00Z", true, true))
        fun operation(id: String) = json("context_id" to binding.context,"operation_id" to id,"kind" to "add_note","device_recorded_at" to "2026-09-14T00:00:00Z","payload" to json("person_id" to person,"body" to "Synthetic upgrade $id")).toString()
        val queuedId = "30000000-0000-4000-8000-000000000091"
        val acceptedId = "30000000-0000-4000-8000-000000000092"
        store.dao.operation(OperationRow(queuedId, person, "add_note", operation(queuedId), 1, retryAt = Long.MAX_VALUE))
        val receipt = json("operation_id" to acceptedId,"outcome" to "accepted","resource_type" to "note","resource_id" to "40000000-0000-4000-8000-000000000091","committed_revision" to null,"person_revision" to "8","changed" to true,"accepted_at" to "2026-09-14T00:00:00Z","replayed" to false).toString()
        store.dao.operation(OperationRow(acceptedId, person, "add_note", operation(acceptedId), 2, "accepted", receipt))
        store.dao.draft(DraftRow("legacy-draft", person, "add_note", json("person_id" to person,"body" to "Protected old draft").toString(), 1))
        store.dao.insertContactDraft(ContactDraftRow("contact-draft", person, "phone", "connected", "2026-09-14T00:00:00Z", "Z", 1))
        val version = db.openHelper.readableDatabase.query("PRAGMA user_version").use { it.moveToFirst(); it.getInt(0) }
        assertEquals(5, version)
        fun sha(value: ByteArray) = java.security.MessageDigest.getInstance("SHA-256").digest(value).joinToString("") { "%02x".format(it) }
        fun hash(value: String) = sha(value.toByteArray())
        val row = requireNotNull(store.dao.person(person))
        val inventory = json("schema" to version,"key" to sha(key),"wrapped_key" to sha(File(directory,"database.key").readBytes()),
            "queued" to hash(operation(queuedId)),"receipt" to hash(receipt),"draft" to hash(store.dao.draft("legacy-draft")!!.payload),
            "contact_draft" to hash(store.dao.contactDrafts().single().toString()),"summary" to hash(row.summary),"contacts" to hash(row.contacts),"notes" to hash(row.notes),"tasks" to hash(row.tasks),"revision" to row.revision)
        File(context.filesDir,"mobile005-review-upgrade-before.json").writeText(inventory.toString())
        println("MOBILE004_INSTALLED_BEFORE $inventory")
        db.close()
    }
}
