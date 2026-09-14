package org.crm.field

import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.util.UUID
import kotlinx.coroutines.runBlocking
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

/** Actual installed schema-5 source is seeded in a separate, never-cleared app identity. */
class Mobile005InstalledUpgradeTest {
    @Test fun preservesInstalledStoreThenQualifiesAtUnchangedBroadRevision() = runBlocking<Unit> {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        org.junit.Assume.assumeTrue(context.packageName == "org.crm.field.mobile005reviewupgradeqa")
        // Application.restore is the production owner of the upgrade open. Await it before
        // this inspector opens the same file; concurrent SQLiteOpenHelpers race migration.
        (context.applicationContext as FieldApplication).ready.join()
        val vault = DeviceVault(context, BuildConfig.VAULT_NAMESPACE)
        val before = JSONObject(File(context.filesDir,"mobile005-review-upgrade-before.json").readText())
        assertEquals(5, before.getInt("schema"))
        val directory = vault.accountDirectory(vault.registry().getString("account"))
        val key = vault.databaseKey(directory)
        val db = FieldDatabase.open(context, directory, key)
        val dao = db.data()
        val version = db.openHelper.readableDatabase.query("PRAGMA user_version").use { it.moveToFirst(); it.getInt(0) }
        assertEquals(6, version)
        val person = "10000000-0000-4000-8000-000000000091"
        val queuedId = "30000000-0000-4000-8000-000000000091"
        val acceptedId = "30000000-0000-4000-8000-000000000092"
        fun sha(value: ByteArray) = java.security.MessageDigest.getInstance("SHA-256").digest(value).joinToString("") { "%02x".format(it) }
        fun hash(value: String) = sha(value.toByteArray())
        val old = requireNotNull(dao.person(person))
        val inventory = json("schema" to version,"key" to sha(key),"wrapped_key" to sha(File(directory,"database.key").readBytes()),
            "queued" to hash(dao.operation(queuedId)!!.envelope),"receipt" to hash(dao.operation(acceptedId)!!.receipt!!),
            "draft" to hash(dao.draft("legacy-draft")!!.payload),"contact_draft" to hash(dao.contactDrafts().single().toString()),
            "summary" to hash(old.summary),"contacts" to hash(old.contacts),"notes" to hash(old.notes),"tasks" to hash(old.tasks),"revision" to old.revision)
        for (name in before.keys()) if (name != "schema") assertEquals("preserved $name",before.getString(name),inventory.getString(name))
        assertFalse(old.detailsRevisionsQualified)
        assertTrue(dao.profileDrafts().isEmpty()); assertTrue(dao.profileContexts().isEmpty())
        File(context.filesDir,"mobile005-review-upgrade-after.json").writeText(inventory.toString())
        println("MOBILE005_INSTALLED_AFTER $inventory")
        // The archived seed intentionally uses a deterministic clock. Production startup
        // correctly locks that synthetic lease on the emulator's different boot/clock. Model
        // a successful synthetic reauthorization before checking the modern download path;
        // the protected rows above have already been compared before this metadata update.
        FieldStore(db, fixtureBinding(), TestClock()).authorize("synthetic")
        db.close()

        val generation = UUID.randomUUID().toString()
        var summaries = 0
        val bootstrap = fixture("reconciliation_bootstrap").put("capabilities",JSONArray(listOf("add_note","create_task","complete_task","reconciliation","update_person_details","details_revisions")))
        val remote = Transport { _, _, path, _, _, _ ->
            val body = when {
                path.endsWith("bootstrap") -> bootstrap
                path.endsWith("/reconciliations") -> json("context_id" to bootstrap.getString("context_id"),"generation_id" to generation,"evaluated_at" to "2026-09-14T00:00:00Z","expires_at" to "2099-01-01T00:00:00Z","selected_count" to 1,"complete" to true,
                    "manifest" to json("items" to JSONArray().put(json("person_id" to person,"revision" to "7")),"next_cursor" to null,"complete" to true))
                path.endsWith("/seal") -> json("context_id" to bootstrap.getString("context_id"),"generation_id" to generation,"evaluated_at" to "2026-09-14T00:00:00Z","sealed_at" to "2026-09-14T00:01:00Z","selected_count" to 1,"today" to json("sources" to json("status" to "complete"),"items" to JSONArray()))
                path.contains("/people/") -> {
                    val section = path.substringAfterLast('/').substringBefore('?')
                    val summary = if (section == "summary") JSONObject(old.summary).put("details_revision","1") else JSONObject.NULL
                    val items = if (section == "summary") {
                        summaries++
                        JSONArray(old.contacts).apply { getJSONObject(0).put("import_order",1).put("created_at","2026-09-14T00:00:00Z") }
                    } else JSONArray()
                    json("generation_id" to generation,"person_id" to person,"revision" to "7","section" to section,"summary" to summary,"items" to items,"next_cursor" to null,"complete" to true)
                }
                else -> error("Unexpected path $path; upgrade must not upload retained queued work")
            }
            testHttpResult(200,body,null,30)
        }
        val repo = FieldRepository(context, namespace=BuildConfig.VAULT_NAMESPACE, clock=TestClock(), transport=remote)
        repo.restore()
        assertFalse("restore after synthetic authorization: ${repo.ui.value.message}", repo.ui.value.locked)
        repo.sync(false)
        assertFalse("qualification failed: ${repo.ui.value.message}",repo.ui.value.locked)
        val active = FieldRepository::class.java.getDeclaredField("active").apply { isAccessible=true }.get(repo) as ActiveAccount
        val qualified = requireNotNull(active.store.dao.person(person))
        assertEquals(1,summaries)
        assertEquals(before.getString("revision"),qualified.revision)
        assertTrue(qualified.detailsRevisionsQualified)
        assertTrue(repo.ui.value.profileEditingEnabled)
        assertEquals(before.getString("queued"),hash(active.store.dao.operation(queuedId)!!.envelope))
        assertEquals(before.getString("receipt"),hash(active.store.dao.operation(acceptedId)!!.receipt!!))
        println("MOBILE005_QUALIFIED same_broad_revision=${qualified.revision} details_revision=${JSONObject(qualified.summary).getString("details_revision")} fetched_summary_pages=$summaries exact_queued_and_receipt_preserved=true")
        File(context.filesDir,"mobile005-review-upgrade-qualified.json").writeText(json("revision" to qualified.revision,"details_qualified" to qualified.detailsRevisionsQualified,"summary_pages" to summaries).toString())
        repo.lockLocal("Upgrade proof complete")
        repo.scope.coroutineContext[kotlinx.coroutines.Job]?.cancel()
    }
}
