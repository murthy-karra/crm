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
    private companion object { const val PERSON = "13e8d47a-5648-4d88-9358-fe5ed400078d" }
}
