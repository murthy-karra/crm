package org.crm.field

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.util.UUID
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Opt-in real API 3102 contact receipt/reconciliation proof using the production repository. */
@RunWith(AndroidJUnit4::class)
class Mobile003LiveApiTest {
    @get:org.junit.Rule
    val localNetwork = androidx.test.rule.GrantPermissionRule.grant("android.permission.ACCESS_LOCAL_NETWORK")

    @Test
    fun offlineContactSurvivesRepositoryRelaunchThenReceivesFreshSeal() = runBlocking {
        assumeTrue(InstrumentationRegistry.getArguments().getString("runMobile003Live") == "true")
        val app = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as FieldApplication
        app.ready.await()
        val repository = app.repository
        repository.login("android@mobile.test", "Mobile-demo-only-123!")
        assertFalse(repository.ui.value.message, repository.ui.value.locked)
        repository.pause(true)
        val person = "f2553108-d87f-4876-86c3-59eb1f7cf3b2"
        repository.pin(person, true)
        repository.pause(false)
        repository.sync(true)
        assertTrue(repository.ui.value.people.any { it.id == person })
        repository.pause(true)
        val draft =
            repository.saveContactDraft(
                UUID.randomUUID().toString(), person, "call", "left_message",
                "2026-09-12T13:00:00.123456789-07:00", "-07:00",
            )
        val operation = repository.submitContactDraft(draft.id, draft.revision)
        val bytes = operation.envelope
        assertEquals("log_contact_attempt", operation.kind)
        assertEquals("queued", operation.status)
        // This reproduces the protected-store reopen boundary without clearing the package.
        repository.lockLocal("Mobile003 process-death acceptance", false)
        repository.restore()
        repository.login("android@mobile.test", "Mobile-demo-only-123!")
        repository.pause(false)
        repository.sync(true)
        val persisted = repository.ui.value.operations.firstOrNull { it.id == operation.id }
        assertTrue(persisted == null || persisted.status in setOf("accepted", "covered"))
        // The operation is either covered after the fresh seal or remains a durable receipt
        // overlay; its envelope is never rebuilt during retry/reconciliation.
        val account = DeviceVault(app, BuildConfig.VAULT_NAMESPACE).registry().getString("account")
        val vault = DeviceVault(app, BuildConfig.VAULT_NAMESPACE)
        val directory = vault.accountDirectory(account)
        val db = FieldDatabase.open(app, directory, vault.databaseKey(directory))
        try {
            val row = db.data().operation(operation.id)!!
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
}
