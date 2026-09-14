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

/** Real API proof uses one isolated package and force-stop durable state, never a reinstall. */
@RunWith(AndroidJUnit4::class)
class Mobile005LiveApiTest {
    @get:org.junit.Rule val localNetwork = androidx.test.rule.GrantPermissionRule.grant("android.permission.ACCESS_LOCAL_NETWORK")

    @Test fun offlineProfileProposalSurvivesForceStopAndSettlesOnce() = runBlocking {
        assumeTrue(InstrumentationRegistry.getArguments().getString("runMobile005Live") == "true")
        if (InstrumentationRegistry.getArguments().getString("mobile005Profile") == "prepare") prepare() else relaunch()
    }

    @Test fun exactReplayAndCompetingProfileWriteRequireReplacement() = runBlocking {
        assumeTrue(InstrumentationRegistry.getArguments().getString("runMobile005Live") == "true")
        val app = app(); val repository = app.repository; authenticate(repository); awaitPerson(repository)
        val active = active(repository); val row = requireNotNull(repository.ui.value.person)
        assertTrue(row.detailsRevisionsQualified)
        repository.pause(true)
        val draft = repository.saveProfileDraft(UUID.randomUUID().toString(), PERSON, json("first_name" to "Android replay", "contact_operations" to JSONArray()))
        val op = repository.submitProfileDraft(draft.id, draft.revision); val bytes = op.envelope
        val receipt = active.api.operation(active.store.binding, op)
        assertEquals("accepted", receipt.getString("outcome")); assertEquals(bytes, active.store.dao.operation(op.id)!!.envelope)
        repository.pause(false); repository.sync(true)
        assertTrue(active.store.dao.operation(op.id)!!.status in setOf("accepted", "covered"))

        // A stale local details revision must conflict after a second actor changes the aggregate.
        repository.sync(true)
        val stale = repository.saveProfileDraft(UUID.randomUUID().toString(), PERSON, json("last_name" to "Stale", "contact_operations" to JSONArray()))
        val staleOp = repository.submitProfileDraft(stale.id, stale.revision)
        val other = FieldApi(BuildConfig.API_BASE)
        val session = other.call("POST", "/api/session", body = json("email" to "second@mobile.test", "password" to "Mobile-demo-only-123!").toString())
        val install = UUID.randomUUID().toString()
        val second = Binding.parse(other.bootstrap(install), session.getJSONObject("user").getString("id"), active.store.binding.org, install)
        val details = JSONObject(active.store.dao.person(PERSON)!!.summary).getString("details_revision")
        val competing = json("context_id" to second.context, "operation_id" to UUID.randomUUID().toString(), "kind" to "update_person_details", "device_recorded_at" to "2026-09-14T18:00:00Z", "payload" to json("person_id" to PERSON, "expected_details_revision" to details, "first_name" to "Second actor", "contact_operations" to JSONArray())).toString()
        other.call("POST", "/api/mobile/v1/operations", second.context, competing)
        repository.sync(true)
        assertEquals("revision_conflict", active.store.dao.operation(staleOp.id)!!.lastError)
        assertTrue(active.store.dao.profileContext(staleOp.id)!!.current.isNotEmpty())
        val replacement = repository.reviseProfileConflict(staleOp.id)
        assertNotEquals(staleOp.id, repository.submitProfileDraft(replacement.id, replacement.revision).id)
    }

    private suspend fun prepare() {
        val app = app(); val repository = app.repository; authenticate(repository); repository.select(PERSON)
        repeat(50) { if (repository.ui.value.person?.detailsRevisionsQualified == true) return@repeat; delay(100) }
        val row = requireNotNull(repository.ui.value.person); assertTrue(row.detailsRevisionsQualified)
        repository.pause(true)
        val draft = repository.saveProfileDraft(UUID.randomUUID().toString(), PERSON, json("first_name" to "Android offline", "contact_operations" to JSONArray()))
        val operation = repository.submitProfileDraft(draft.id, draft.revision)
        stage(app).writeText(json("operation" to operation.id, "sha" to sha(operation.envelope)).toString())
    }

    private suspend fun relaunch() {
        val app = app(); val repository = app.repository; val saved = JSONObject(stage(app).readText())
        assertFalse(repository.ui.value.locked)
        // The process restart intentionally recreates the connectivity callback; wait for its
        // initial network registration before turning a transient no-network startup into a retry.
        delay(1_500)
        repository.pause(false); repository.sync(true)
        val vault = DeviceVault(app, BuildConfig.VAULT_NAMESPACE); val db = FieldDatabase.open(app, vault.accountDirectory(vault.registry().getString("account")), vault.databaseKey(vault.accountDirectory(vault.registry().getString("account"))))
        try { val row = db.data().operation(saved.getString("operation"))!!; assertEquals(saved.getString("sha"), sha(row.envelope)); assertTrue("status=${row.status} error=${row.lastError} message=${repository.ui.value.message}", row.status in setOf("accepted", "covered")); assertEquals("person_details", JSONObject(requireNotNull(row.receipt)).getString("resource_type")) } finally { db.close() }
        stage(app).delete()
    }

    private suspend fun authenticate(repository: FieldRepository) { repository.login("agent@mobile.test", "Mobile-demo-only-123!"); assertFalse(repository.ui.value.locked); repository.pause(true); repository.pin(PERSON, true); repository.pause(false); repository.sync(true); repository.select(PERSON) }
    private suspend fun awaitPerson(repository: FieldRepository) { repeat(80) { if (repository.ui.value.person?.id == PERSON) return; delay(100) }; fail("Person unavailable: ${repository.ui.value.message} ${repository.ui.value.coverage}") }
    private suspend fun app() = (InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as FieldApplication).also { it.ready.await() }
    private fun active(repository: FieldRepository) = FieldRepository::class.java.getDeclaredField("active").apply { isAccessible = true }.get(repository) as ActiveAccount
    private fun stage(app: FieldApplication) = File(app.filesDir, "mobile005-profile-stage.json")
    private fun sha(value: String) = MessageDigest.getInstance("SHA-256").digest(value.toByteArray()).joinToString("") { "%02x".format(it) }
    private companion object { const val PERSON = "20f1bb9b-d502-48da-ae66-7990ba2cfef7" }
}
