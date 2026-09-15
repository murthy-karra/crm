package org.crm.field

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.security.MessageDigest
import java.util.UUID
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Opt-in API3106 proof. It writes only the parent-owned synthetic workspace. */
@RunWith(AndroidJUnit4::class)
class Mobile006LiveApiTest {
    @get:org.junit.Rule val localNetwork = androidx.test.rule.GrantPermissionRule.grant("android.permission.ACCESS_LOCAL_NETWORK")

    @Test fun offlineMixedMetadataSurvivesRestartAndSettlesOnce() = runBlocking {
        assumeTrue(InstrumentationRegistry.getArguments().getString("runMobile006Live") == "true")
        val app = app(); val repository = app.repository
        if (InstrumentationRegistry.getArguments().getString("mobile006Stage") == "relaunch") return@runBlocking relaunch(app, repository)
        authenticate(repository); awaitMetadata(repository)
        val active = active(repository); val person = requireNotNull(repository.ui.value.person)
        val fields = repository.ui.value.metadataFields.associateBy { it.fieldType }
        val choice = requireNotNull(fields["choice"]); val option = repository.ui.value.metadataOptions.first { it.fieldId == choice.id && it.archivedAt == null }
        val tag = repository.ui.value.metadataTags.first()
        repository.pause(true)
        val actions = JSONArray()
            .put(json("kind" to "add_tag", "tag_id" to tag.id))
            .put(json("kind" to "set_field", "field_id" to requireNotNull(fields["text"]).id, "value" to json("text" to "Android metadata ${UUID.randomUUID()}")))
            .put(json("kind" to "set_field", "field_id" to requireNotNull(fields["number"]).id, "value" to json("number" to "123.4500")))
            .put(json("kind" to "set_field", "field_id" to requireNotNull(fields["date"]).id, "value" to json("date" to "2026-09-14")))
            .put(json("kind" to "set_field", "field_id" to choice.id, "value" to json("option_id" to option.id)))
        val draft = repository.saveMetadataDraft(UUID.randomUUID().toString(), person.id, actions)
        val operation = repository.submitMetadataDraft(draft.id, draft.revision)
        val persisted = active.store.dao.operation(operation.id)!!
        stage(app).writeText(json("operation" to operation.id, "sha" to sha(persisted.envelope), "person" to person.id).toString())
        assertEquals("update_person_metadata", persisted.kind)
        assertEquals("123.4500", JSONObject(persisted.envelope).getJSONObject("payload").getJSONArray("actions").getJSONObject(2).getJSONObject("value").getString("number"))
    }

    /**
     * Three deliberately separate invocations leave a reviewable boundary for the fixture-only
     * second actor: first the server accepts a request whose response is lost, then the primary
     * device holds a stale immutable proposal while that actor advances only the catalog token.
     */
    @Test fun exactLostResponseReplayThenCatalogConflictFetchesCurrentMetadata() = runBlocking {
        assumeTrue(InstrumentationRegistry.getArguments().getString("runMobile006Live") == "true")
        when (InstrumentationRegistry.getArguments().getString("mobile006ConflictStage")) {
            "lost-response" -> lostResponse()
            "queue-catalog" -> queueCatalogConflict()
            "resume-catalog" -> resumeCatalogConflict()
            else -> fail("set mobile006ConflictStage to lost-response, queue-catalog, or resume-catalog")
        }
    }

    /** Reads the retained lost-response operation after its exact retry; it creates no new work. */
    @Test fun stagedLostResponseReplayRetainsExactEnvelopeAndReceipt() = runBlocking {
        assumeTrue(InstrumentationRegistry.getArguments().getString("mobile006ReplayEvidence") == "true")
        val app = app()
        val saved = JSONObject(conflictStage(app).readText())
        val row = active(app.repository).store.dao.operation(saved.getString("replay"))!!
        assertEquals("update_person_metadata", row.kind)
        assertEquals(saved.getString("replay_sha"), sha(row.envelope))
        assertTrue("replay status=${row.status} error=${row.lastError}", row.status in setOf("accepted", "covered"))
        val receipt = JSONObject(requireNotNull(row.receipt))
        assertEquals("person_metadata", receipt.getString("resource_type"))
        assertTrue("the retained operation must have been exact-replayed", receipt.getBoolean("replayed"))
        File(app.filesDir, "mobile006-ui-evidence.txt").appendText(
            "lost_response_replay=${row.id} status=${row.status} sha256=${saved.getString("replay_sha")} receipt_replayed=${receipt.getBoolean("replayed")}\n",
        )
    }

    private suspend fun lostResponse() {
        val app = app(); val repository = app.repository
        authenticate(repository); awaitMetadata(repository)
        val active = active(repository); val person = requireNotNull(repository.ui.value.person)
        repository.pause(true)
        val draft = repository.saveMetadataDraft(UUID.randomUUID().toString(), person.id, proposal(repository, "lost response"))
        val operation = repository.submitMetadataDraft(draft.id, draft.revision)
        val bytes = operation.envelope
        // The network response is intentionally not acknowledged into the encrypted store.
        val receipt = active.api.operation(active.store.binding, operation)
        assertEquals("accepted", receipt.getString("outcome"))
        assertEquals(bytes, active.store.dao.operation(operation.id)!!.envelope)
        conflictStage(app).writeText(json("person" to person.id, "replay" to operation.id, "replay_sha" to sha(bytes)).toString())
    }

    private suspend fun queueCatalogConflict() {
        val app = app(); val repository = app.repository
        val saved = JSONObject(conflictStage(app).readText())
        assertFalse("the protected store must reopen without a new login", repository.ui.value.locked)
        repository.pause(false); settle(repository, saved.getString("replay"))
        val active = active(repository); val replay = active.store.dao.operation(saved.getString("replay"))!!
        assertEquals(saved.getString("replay_sha"), sha(replay.envelope))
        assertTrue(replay.status in setOf("accepted", "covered"))
        repository.select(saved.getString("person")); awaitMetadata(repository)
        repository.pause(true)
        val draft = repository.saveMetadataDraft(UUID.randomUUID().toString(), saved.getString("person"), proposal(repository, "stale catalog"))
        val stale = repository.submitMetadataDraft(draft.id, draft.revision)
        conflictStage(app).writeText(saved.put("stale", stale.id).put("stale_sha", sha(stale.envelope)).put("state", "queued_for_catalog_rename").toString())
        assertEquals("queued", stale.status)
    }

    private suspend fun resumeCatalogConflict() {
        val app = app(); val repository = app.repository; val saved = JSONObject(conflictStage(app).readText())
        require(saved.getString("state") == "queued_for_catalog_rename")
        val active = active(repository)
        repository.pause(false); repository.sync(true)
        val stale = active.store.dao.operation(saved.getString("stale"))!!
        assertEquals(saved.getString("stale_sha"), sha(stale.envelope))
        assertEquals("catalog_revision_conflict", stale.lastError)
        val current = requireNotNull(active.store.dao.metadataContext(stale.id)).current
        assertTrue("authorized current metadata/catalog read is required", current.isNotEmpty())
        val currentJson = JSONObject(current)
        assertNotEquals(
            JSONObject(stale.envelope).getJSONObject("payload").getString("expected_catalog_revision"),
            currentJson.getString("catalog_revision"),
        )
        conflictStage(app).writeText(saved.put("state", "catalog_conflict_ready_for_ui_review").toString())
    }

    private suspend fun settle(repository: FieldRepository, operation: String) {
        repeat(5) {
            repository.sync(true)
            if (active(repository).store.dao.operation(operation)?.status == "covered") return
            delay(1_000)
        }
        val row = active(repository).store.dao.operation(operation)!!
        assertTrue("replay status=${row.status} error=${row.lastError}", row.status in setOf("accepted", "covered"))
    }

    private fun proposal(repository: FieldRepository, marker: String): JSONArray {
        val person = requireNotNull(repository.ui.value.person)
        val baseline = JSONObject(person.metadata)
        val tag = repository.ui.value.metadataTags.first()
        val hasTag = baseline.getJSONArray("tags").objects().any { it.getString("id") == tag.id }
        val text = requireNotNull(repository.ui.value.metadataFields.firstOrNull { it.fieldType == "text" })
        return JSONArray()
            .put(json("kind" to if (hasTag) "remove_tag" else "add_tag", "tag_id" to tag.id))
            .put(json("kind" to "set_field", "field_id" to text.id, "value" to json("text" to "Android $marker ${UUID.randomUUID()}")))
    }

    private suspend fun relaunch(app: FieldApplication, repository: FieldRepository) {
        val saved = JSONObject(stage(app).readText()); delay(1_500)
        repository.pause(false); repository.sync(true)
        val active = active(repository); val row = active.store.dao.operation(saved.getString("operation"))!!
        assertEquals(saved.getString("sha"), sha(row.envelope))
        assertTrue("status=${row.status} ${row.lastError} ${repository.ui.value.message}", row.status in setOf("accepted", "covered"))
        assertEquals("person_metadata", JSONObject(requireNotNull(row.receipt)).getString("resource_type"))
        stage(app).delete()
    }

    private suspend fun authenticate(repository: FieldRepository) { repository.login("agent@mobile.test", "Mobile-demo-only-123!"); assertFalse(repository.ui.value.locked); repository.pause(true); repository.pause(false); repository.sync(true); repository.select(repository.ui.value.people.first().id) }
    private suspend fun awaitMetadata(repository: FieldRepository) { repeat(100) { if (repository.ui.value.person?.metadataRevisionsQualified == true && repository.ui.value.metadataFields.size >= 4) return; delay(100) }; fail("metadata unavailable: ${repository.ui.value.message} ${repository.ui.value.coverage}") }
    private suspend fun app() = (InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as FieldApplication).also { it.ready.await() }
    private fun active(repository: FieldRepository) = FieldRepository::class.java.getDeclaredField("active").apply { isAccessible = true }.get(repository) as ActiveAccount
    private fun stage(app: FieldApplication) = File(app.filesDir, "mobile006-stage.json")
    private fun conflictStage(app: FieldApplication) = File(app.filesDir, "mobile006-conflict-stage.json")
    private fun sha(value: String) = MessageDigest.getInstance("SHA-256").digest(value.toByteArray()).joinToString("") { "%02x".format(it) }
}
