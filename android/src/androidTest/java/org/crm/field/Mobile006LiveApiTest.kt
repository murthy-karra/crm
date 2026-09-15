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
    private fun sha(value: String) = MessageDigest.getInstance("SHA-256").digest(value.toByteArray()).joinToString("") { "%02x".format(it) }
}
