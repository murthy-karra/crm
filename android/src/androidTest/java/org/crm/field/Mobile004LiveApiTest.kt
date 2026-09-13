package org.crm.field

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.security.MessageDigest
import java.util.UUID
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.delay
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Real isolated-API proof. The runner intentionally force-stops the package between prepare and
 * relaunch; a reinstall cannot satisfy this test because the state file and encrypted store must
 * remain under the same Mobile004 QA identity.
 */
@RunWith(AndroidJUnit4::class)
class Mobile004LiveApiTest {
    @get:org.junit.Rule val localNetwork = androidx.test.rule.GrantPermissionRule.grant("android.permission.ACCESS_LOCAL_NETWORK")

    @Test fun offlineStageSaveSurvivesForceStopAndReceivesOneReceipt() = runBlocking {
        assumeTrue(InstrumentationRegistry.getArguments().getString("runMobile004Live") == "true")
        when (InstrumentationRegistry.getArguments().getString("mobile004Stage")) {
            "prepare" -> prepare()
            "relaunch" -> relaunch()
            else -> checkCatalogQualification()
        }
    }

    @Test fun lostResponseReplaysExactBytesAndSecondActorConflictRequiresNewProposal() = runBlocking {
        assumeTrue(InstrumentationRegistry.getArguments().getString("runMobile004Live") == "true")
        val app = app(); val repository = app.repository; authenticate(repository)
        repeat(100) { if (repository.ui.value.person?.id != PERSON) delay(100) }
        val active = FieldRepository::class.java.getDeclaredField("active").apply { isAccessible = true }.get(repository) as ActiveAccount
        val current = requireNotNull(repository.ui.value.person)
        val summary = JSONObject(current.summary); val oldStage = summary.getJSONObject("stage").getString("id")
        val target = repository.ui.value.stageCatalog.first { it.id != oldStage }.id
        repository.pause(true)
        val saved = repository.saveStageDraft(UUID.randomUUID().toString(), PERSON, target)
        val operation = repository.submitStageDraft(saved.id, saved.revision)
        val bytes = operation.envelope
        // Simulate the server accepting a request whose response was lost before local ack.
        val receipt = active.api.operation(active.store.binding, operation)
        assertEquals("accepted", receipt.getString("outcome")); assertEquals(bytes, active.store.dao.operation(operation.id)!!.envelope)
        repository.pause(false); repository.sync(true)
        val replayed = active.store.dao.operation(operation.id)!!
        assertEquals(bytes, replayed.envelope); assertTrue(replayed.status in setOf("accepted", "covered"))

        // A different actor changes the stage after the first operation, then the stale first
        // baseline must conflict and only a new operation may use the current-stage revision.
        val secondApi = FieldApi(BuildConfig.API_BASE)
        val secondSession = secondApi.call("POST", "/api/session", body = json("email" to "second@mobile.test", "password" to "Mobile-demo-only-123!").toString())
        val installation = UUID.randomUUID().toString()
        val binding = Binding.parse(secondApi.bootstrap(installation), secondSession.getJSONObject("user").getString("id"), active.store.binding.org, installation)
        // The fixture's second actor belongs to the same Organization; use the authenticated
        // context only for the direct command and keep the first device's bytes untouched.
        val stageNow = active.store.dao.person(PERSON)!!.let { JSONObject(it.summary).getJSONObject("stage").getString("id") }
        val other = repository.ui.value.stageCatalog.first { it.id != stageNow }.id
        val secondEnvelope = json(
            "context_id" to binding.context,
            "operation_id" to UUID.randomUUID().toString(),
            "kind" to "change_person_stage",
            "device_recorded_at" to "2026-09-13T16:20:00Z",
            "payload" to json(
                "person_id" to PERSON,
                "stage_id" to other,
                "expected_stage_revision" to JSONObject(active.store.dao.person(PERSON)!!.summary).getString("stage_revision"),
            ),
        ).toString()
        secondApi.call("POST", "/api/mobile/v1/operations", binding.context, secondEnvelope)
        val stale = repository.saveStageDraft(UUID.randomUUID().toString(), PERSON, target)
        val staleOp = repository.submitStageDraft(stale.id, stale.revision)
        repository.sync(true)
        assertEquals("revision_conflict", active.store.dao.operation(staleOp.id)!!.lastError)
        assertTrue(active.store.dao.stageContext(staleOp.id)!!.current.isNotEmpty())
        val revised = repository.reviseStageConflict(staleOp.id)
        val replacement = repository.submitStageDraft(revised.id, revised.revision)
        assertNotEquals(staleOp.id, replacement.id)
        repository.sync(true)
        assertTrue(active.store.dao.operation(replacement.id)!!.status in setOf("accepted", "covered"))
    }

    private suspend fun prepare() {
        val app = app(); val repository = app.repository
        authenticate(repository)
        repository.select(PERSON)
        repeat(20) { if (repository.ui.value.person != null) return@repeat; delay(250) }
        val cached = repository.ui.value.person
        assertNotNull("Android fixture Person must be fully downloaded: ${repository.ui.value.message}; ${repository.ui.value.coverage}", cached)
        val catalog = repository.ui.value.stageCatalog
        assertTrue("opted-in catalog must be promoted with the Person generation", catalog.size >= 2)
        val current = JSONObject(requireNotNull(repository.ui.value.person).summary).getJSONObject("stage").getString("id")
        val target = catalog.first { it.id != current }
        repository.pause(true)
        val draft = repository.saveStageDraft(UUID.randomUUID().toString(), PERSON, target.id)
        val operation = repository.submitStageDraft(draft.id, draft.revision)
        assertEquals("queued", operation.status)
        stage(app).writeText(JSONObject().put("operation", operation.id).put("sha", sha(operation.envelope)).put("target", target.id).toString())
        assertTrue(stage(app).exists())
    }

    private suspend fun relaunch() {
        val app = app(); val repository = app.repository; val saved = JSONObject(stage(app).readText())
        assertFalse("force-stopped encrypted account must reopen without a new login", repository.ui.value.locked)
        repository.pause(false); repository.sync(true)
        val vault = DeviceVault(app, BuildConfig.VAULT_NAMESPACE); val directory = vault.accountDirectory(vault.registry().getString("account"))
        val db = FieldDatabase.open(app, directory, vault.databaseKey(directory))
        try {
            val row = db.data().operation(saved.getString("operation"))!!
            assertEquals(saved.getString("sha"), sha(row.envelope))
            assertTrue(row.status in setOf("accepted", "covered"))
            val receipt = JSONObject(requireNotNull(row.receipt))
            assertEquals("person_stage", receipt.getString("resource_type"))
            assertEquals(PERSON, receipt.getString("resource_id"))
            assertTrue(receipt.getString("committed_revision").toLong() > 0)
        } finally { db.close() }
        stage(app).delete()
    }

    private suspend fun checkCatalogQualification() {
        val app = app(); val repository = app.repository; authenticate(repository)
        repository.select(PERSON)
        delay(300)
        assertTrue(repository.stageChangesSupported())
        assertTrue(repository.ui.value.stageCatalog.isNotEmpty())
        assertTrue(requireNotNull(repository.ui.value.person).stageRevisionsQualified)
    }

    private suspend fun authenticate(repository: FieldRepository) {
        repository.login("agent@mobile.test", "Mobile-demo-only-123!")
        assertFalse(repository.ui.value.message, repository.ui.value.locked)
        repository.pause(true); repository.pin(PERSON, true); repository.pause(false); repository.sync(true)
        repository.select(PERSON)
    }
    private suspend fun app(): FieldApplication = (InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as FieldApplication).also { it.ready.await() }
    private fun stage(app: FieldApplication) = File(app.filesDir, "mobile004-force-stop-stage.json")
    private fun sha(value: String) = MessageDigest.getInstance("SHA-256").digest(value.toByteArray()).joinToString("") { "%02x".format(it) }
    private companion object { const val PERSON = "60fafd49-9c9e-404d-869c-83817cd9fe0f" }
}
