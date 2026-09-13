package org.crm.field

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.security.MessageDigest
import java.util.UUID
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Opt-in real API 3102 contact receipt/reconciliation proof using the production repository.
 *
 * The `prepare` and `relaunch` stages are intentionally separate instrumentation invocations.
 * The acceptance command force-stops the target package between them, so the second stage opens
 * the same encrypted database and Android Keystore identity after a real process termination.
 */
@RunWith(AndroidJUnit4::class)
class Mobile003LiveApiTest {
    @get:org.junit.Rule
    val localNetwork = androidx.test.rule.GrantPermissionRule.grant("android.permission.ACCESS_LOCAL_NETWORK")

    @Test
    fun offlineContactSurvivesRepositoryRelaunchThenReceivesFreshSeal() = runBlocking {
        assumeTrue(InstrumentationRegistry.getArguments().getString("runMobile003Live") == "true")
        when (InstrumentationRegistry.getArguments().getString("mobile003Stage")) {
            "prepare" -> prepareOfflineMixedQueue()
            "relaunch" -> relaunchAndVerifyMixedQueue()
            else -> repositoryLockReopenRegression()
        }
    }

    private suspend fun prepareOfflineMixedQueue() {
        val app = app()
        val repository = app.repository
        authenticateAndPin(repository)
        // Mobile 002 intentionally permits only one unresolved target-less legacy operation.
        // Preserve that legacy rule: seal the note first, then retain its receipt while the
        // legacy task and Mobile 003 contact wait together offline.
        repository.pause(true)
        val legacyDraft =
            repository.saveDraft(
                UUID.randomUUID().toString(),
                PERSON,
                "add_note",
                json("person_id" to PERSON, "body" to "Mobile003 mixed queue ${UUID.randomUUID()}"),
            )
        val legacy = repository.submit(legacyDraft.id, legacyDraft.revision)
        repository.pause(false)
        repository.sync(true)
        repository.pause(true)
        val taskDraft =
            repository.saveDraft(
                UUID.randomUUID().toString(),
                PERSON,
                "create_task",
                json(
                    "person_id" to PERSON,
                    "title" to "Mobile003 mixed queue ${UUID.randomUUID()}",
                    "kind" to "follow_up",
                ),
            )
        val task = repository.submit(taskDraft.id, taskDraft.revision)
        val contactDraft =
            repository.saveContactDraft(
                UUID.randomUUID().toString(),
                PERSON,
                "call",
                "left_message",
                "2026-09-12T13:00:00.123456789-07:00",
                "-07:00",
            )
        val contact = repository.submitContactDraft(contactDraft.id, contactDraft.revision)
        assertEquals("queued", task.status)
        assertEquals("queued", contact.status)
        val state =
            JSONObject()
                .put("legacy_operation", legacy.id)
                .put("legacy_envelope_sha256", sha256(legacy.envelope))
                .put("task_operation", task.id)
                .put("task_envelope_sha256", sha256(task.envelope))
                .put("contact_operation", contact.id)
                .put("contact_envelope_sha256", sha256(contact.envelope))
                .put("today_before", todayHash(app))
        stageFile(app).writeText(state.toString())
        assertTrue(stageFile(app).exists())
    }

    private suspend fun relaunchAndVerifyMixedQueue() {
        val app = app()
        val state = JSONObject(stageFile(app).readText())
        val repository = app.repository
        // restore() reopened the preserved protected account store in FieldApplication.onCreate.
        assertFalse("The force-stopped account must reopen without a new login", repository.ui.value.locked)
        repository.pause(false)
        repository.sync(true)
        val account = DeviceVault(app, BuildConfig.VAULT_NAMESPACE).registry().getString("account")
        val vault = DeviceVault(app, BuildConfig.VAULT_NAMESPACE)
        val directory = vault.accountDirectory(account)
        val db = FieldDatabase.open(app, directory, vault.databaseKey(directory))
        try {
            val legacy = db.data().operation(state.getString("legacy_operation"))
            val task = db.data().operation(state.getString("task_operation"))
            val contact = db.data().operation(state.getString("contact_operation"))
            assertNotNull(legacy)
            assertNotNull(task)
            assertNotNull(contact)
            assertEquals(state.getString("legacy_envelope_sha256"), sha256(legacy!!.envelope))
            assertEquals(state.getString("task_envelope_sha256"), sha256(task!!.envelope))
            assertEquals(state.getString("contact_envelope_sha256"), sha256(contact!!.envelope))
            assertEquals("legacy note receipt was not retained", "covered", legacy.status)
            assertTrue("task operation was not accepted", task.status in setOf("accepted", "covered"))
            assertTrue("contact operation was not accepted", contact.status in setOf("accepted", "covered"))
            val receipt = JSONObject(requireNotNull(contact.receipt))
            assertEquals("contact_attempt", receipt.getString("resource_type"))
            assertTrue(receipt.isNull("committed_revision"))
            assertTrue(receipt.getBoolean("changed"))
            // A contact receipt discards pre-receipt staging, requiring this post-receipt seal.
            assertTrue("fresh sealed Today is absent", db.data().meta("today") != null)
            assertTrue("staging remained after the fresh seal", db.data().meta("generation") == null)
            assertTrue("manifest staging remained after the fresh seal", db.data().meta("manifest_cursor") == null)
        } finally {
            db.close()
        }
        stageFile(app).delete()
    }

    private suspend fun repositoryLockReopenRegression() {
        val app = app()
        val repository = app.repository
        authenticateAndPin(repository)
        repository.pause(true)
        val draft =
            repository.saveContactDraft(
                UUID.randomUUID().toString(), PERSON, "call", "left_message",
                "2026-09-12T13:00:00.123456789-07:00", "-07:00",
            )
        val operation = repository.submitContactDraft(draft.id, draft.revision)
        val bytes = operation.envelope
        assertEquals("log_contact_attempt", operation.kind)
        repository.lockLocal("Mobile003 protected-store reopen", false)
        repository.restore()
        repository.login("android@mobile.test", "Mobile-demo-only-123!")
        repository.pause(false)
        repository.sync(true)
        verifyContactReceipt(app, operation.id, bytes)
    }

    private suspend fun authenticateAndPin(repository: FieldRepository) {
        repository.login("android@mobile.test", "Mobile-demo-only-123!")
        assertFalse(repository.ui.value.message, repository.ui.value.locked)
        repository.pause(true)
        repository.pin(PERSON, true)
        repository.pause(false)
        repository.sync(true)
        assertTrue(repository.ui.value.people.any { it.id == PERSON })
    }

    private fun verifyContactReceipt(app: FieldApplication, operation: String, bytes: String) {
        val account = DeviceVault(app, BuildConfig.VAULT_NAMESPACE).registry().getString("account")
        val vault = DeviceVault(app, BuildConfig.VAULT_NAMESPACE)
        val directory = vault.accountDirectory(account)
        val db = FieldDatabase.open(app, directory, vault.databaseKey(directory))
        try {
            val row = db.data().operation(operation)!!
            assertEquals(bytes, row.envelope)
            assertTrue(row.status in setOf("accepted", "covered"))
            val receipt = JSONObject(requireNotNull(row.receipt))
            assertEquals("contact_attempt", receipt.getString("resource_type"))
            assertTrue(receipt.isNull("committed_revision"))
            assertTrue(receipt.getBoolean("changed"))
            assertTrue(db.data().meta("today") != null)
        } finally {
            db.close()
        }
    }

    private suspend fun app(): FieldApplication {
        val app = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as FieldApplication
        app.ready.await()
        return app
    }

    private fun todayHash(app: FieldApplication): String {
        val vault = DeviceVault(app, BuildConfig.VAULT_NAMESPACE)
        val account = vault.registry().optString("account")
        if (account.isEmpty()) return ""
        val directory = vault.accountDirectory(account)
        val db = FieldDatabase.open(app, directory, vault.databaseKey(directory))
        return try { sha256(db.data().meta("today").orEmpty()) } finally { db.close() }
    }

    private fun stageFile(app: FieldApplication): File = File(app.filesDir, "mobile003-force-stop-stage.json")

    private fun sha256(value: String): String =
        MessageDigest.getInstance("SHA-256").digest(value.toByteArray()).joinToString("") { "%02x".format(it) }

    private companion object {
        const val PERSON = "f2553108-d87f-4876-86c3-59eb1f7cf3b2"
    }
}
