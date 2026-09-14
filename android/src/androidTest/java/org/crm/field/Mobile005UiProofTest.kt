package org.crm.field

import android.view.WindowManager
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.util.UUID
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test

/**
 * Native acceptance proof. The only profile saves/submits/replacement actions in this class
 * are Compose gestures; the direct second actor creates the deliberate server-side conflict.
 */
class Mobile005UiProofTest {
    @get:Rule(order = 0)
    val localNetwork = androidx.test.rule.GrantPermissionRule.grant("android.permission.ACCESS_LOCAL_NETWORK")
    @get:Rule(order = 1) val compose = createAndroidComposeRule<MainActivity>()

    private val repository get() = (compose.activity.application as FieldApplication).repository

    @Test fun profileEditorSavesMultiFieldProposalOfflineForActualRestart(): Unit = runBlocking<Unit> {
        assumeTrue(InstrumentationRegistry.getArguments().getString("uiMobile005Profile") == "prepare")
        waitReady()
        signInIfNeeded()
        preparePerson()
        if (!repository.ui.value.paused) compose.onNodeWithText("Pause sync").performClick()
        compose.waitUntil { repository.ui.value.paused }

        repository.select(PERSON)
        compose.waitUntil(20_000) { repository.ui.value.person?.id == PERSON }
        val contacts = JSONArray(requireNotNull(repository.ui.value.person).contacts).length()
        compose.onNodeWithTag("edit-profile").performClick()
        compose.onNodeWithTag("profile-first-name").performTextReplacement("UI offline")
        compose.onNodeWithTag("profile-last-name").performTextReplacement("proposal")
        compose.onNodeWithText("Add email").performClick()
        compose.waitUntil(5_000) { compose.onAllNodesWithTag("profile-contact-$contacts").fetchSemanticsNodes().isNotEmpty() }
        // Add both blank rows before typing: typing triggers the editor's CAS autosave and
        // temporarily disables mutation controls until that encrypted save settles.
        compose.onNodeWithText("Add phone").performClick()
        compose.waitUntil(5_000) { compose.onAllNodesWithTag("profile-contact-${contacts + 1}").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag("profile-contact-$contacts").performTextInput("ui-offline-${UUID.randomUUID()}@example.test")
        compose.onNodeWithTag("profile-contact-${contacts + 1}").performTextInput("555-555-0105")
        compose.waitUntil(15_000) {
            repository.ui.value.profileDrafts.any { it.person == PERSON && it.operation.isEmpty() }
        }
        screenshot("mobile005-ui-offline-profile")
        compose.waitUntil(15_000) { runCatching { compose.onNodeWithTag("profile-save").assertIsEnabled() }.isSuccess }
        compose.onNodeWithTag("profile-save").assertIsEnabled().performClick()
        compose.waitUntil(15_000) {
            profileOperation()?.status == "queued"
        }
        compose.onNodeWithTag("nav-Saved work").performClick()
        compose.waitUntil { compose.onAllNodesWithText("Profile details").fetchSemanticsNodes().isNotEmpty() }
        screenshot("mobile005-ui-queued-profile")
        val operation = requireNotNull(profileOperation())
        stage().writeText(json("operation" to operation.id, "sha" to sha(operation.envelope)).toString())
    }

    @Test fun restartedProfileQueueSyncsThenConflictIsReviewedAndReplacedInCompose(): Unit = runBlocking<Unit> {
        assumeTrue(InstrumentationRegistry.getArguments().getString("uiMobile005Profile") == "relaunch")
        waitReady()
        assertFalse(repository.ui.value.locked)
        val staged = JSONObject(stage().readText())
        compose.waitUntil(20_000) { active().store.dao.operation(staged.getString("operation")) != null }
        val queued = requireNotNull(active().store.dao.operation(staged.getString("operation")))
        assertEquals(staged.getString("sha"), sha(queued.envelope))
        compose.onNodeWithTag("nav-Saved work").performClick()
        screenshot("mobile005-ui-restarted-profile")
        if (repository.ui.value.paused) compose.onNodeWithText("Resume sync").performClick() else compose.onNodeWithText("Sync now").performClick()
        compose.waitUntil(90_000) { active().store.dao.operation(staged.getString("operation"))?.status in setOf("accepted", "covered") }
        // The receipt may arrive after the in-flight sealed download. Use the visible manual
        // control to obtain the later seal that is allowed to cover the accepted proposal.
        if (active().store.dao.operation(staged.getString("operation"))?.status != "covered") compose.onNodeWithText("Sync now").performClick()
        compose.waitUntil(90_000) { active().store.dao.operation(staged.getString("operation"))?.status == "covered" }
        screenshot("mobile005-ui-profile-synced")

        // Build the next proposal through the editor, then change the current profile as the
        // second authorized actor while the device is offline.
        repository.select(PERSON)
        compose.waitUntil(20_000) { repository.ui.value.person?.id == PERSON }
        compose.onNodeWithText("Pause sync").performClick()
        compose.waitUntil { repository.ui.value.paused }
        compose.onNodeWithTag("edit-profile").performClick()
        compose.onNodeWithTag("profile-last-name").performTextReplacement("UI stale")
        compose.waitUntil(15_000) {
            repository.ui.value.profileDrafts.any { it.person == PERSON && it.operation.isEmpty() }
        }
        compose.onNodeWithTag("profile-save").assertIsEnabled().performClick()
        compose.waitUntil(15_000) { profileOperation()?.status == "queued" }
        val stale = requireNotNull(profileOperation())
        writeCompetingProfile()

        compose.onNodeWithText("Resume sync").performClick()
        compose.waitUntil(90_000) {
            profileOperation()?.id == stale.id && profileOperation()?.lastError == "revision_conflict" &&
                repository.ui.value.profileContexts.any { it.operation == stale.id && it.current.isNotEmpty() }
        }
        compose.onNodeWithTag("nav-Saved work").performClick()
        compose.onNodeWithText("Current profile").assertExists()
        compose.onNodeWithText("Review and replace").assertIsEnabled()
        screenshot("mobile005-ui-profile-conflict")

        compose.onNodeWithText("Pause sync").performClick()
        compose.waitUntil { repository.ui.value.paused }
        compose.onNodeWithText("Review and replace").performClick()
        compose.onNodeWithTag("profile-first-name").performTextReplacement("UI replacement")
        compose.waitUntil(15_000) {
            repository.ui.value.profileDrafts.any { it.person == PERSON && it.operation.isEmpty() }
        }
        screenshot("mobile005-ui-profile-replacement")
        compose.waitUntil(15_000) { runCatching { compose.onNodeWithTag("profile-save").assertIsEnabled() }.isSuccess }
        compose.onNodeWithTag("profile-save").assertIsEnabled().performClick()
        compose.waitUntil(15_000) { profileOperation()?.id != stale.id && profileOperation()?.status == "queued" }
        compose.onNodeWithText("Resume sync").performClick()
        compose.waitUntil(90_000) { profileOperation()?.status in setOf("accepted", "covered") }
        stage().delete()
        Unit
    }

    private suspend fun waitReady() {
        compose.waitUntil(20_000) { !(compose.activity.application as FieldApplication).ready.isActive }
    }

    private suspend fun signInIfNeeded() {
        if (!repository.ui.value.locked) return
        compose.onNodeWithTag("email").performTextInput("agent@mobile.test")
        compose.onNodeWithTag("password").performTextInput("Mobile-demo-only-123!")
        compose.onNodeWithTag("sign-in").performClick()
        compose.waitUntil(90_000) { !repository.ui.value.locked && !repository.ui.value.busy }
    }

    private suspend fun preparePerson() {
        // A prior proof may have deliberately retained a queued replacement. Settle it before
        // beginning this independent UI scenario; pause is restored by the caller before edits.
        if (repository.ui.value.paused) repository.pause(false)
        repository.pin(PERSON, true)
        repeat(8) {
            repository.sync(true)
            val active = active()
            if (active.store.dao.operations().none { it.person == PERSON && it.kind == "update_person_details" && it.status !in setOf("covered", "superseded") }) return@repeat
            delay(1_000)
        }
        repository.select(PERSON)
        compose.waitUntil(60_000) { repository.ui.value.person?.detailsRevisionsQualified == true }
        assertTrue(repository.ui.value.profileDrafts.none { it.person == PERSON && it.operation.isNotEmpty() })
    }

    private suspend fun writeCompetingProfile() {
        val active = active()
        val details = JSONObject(requireNotNull(active.store.dao.person(PERSON)).summary).getString("details_revision")
        val other = FieldApi(BuildConfig.API_BASE)
        val session = other.call("POST", "/api/session", body = json("email" to "second@mobile.test", "password" to "Mobile-demo-only-123!").toString())
        val installation = UUID.randomUUID().toString()
        val second = Binding.parse(other.bootstrap(installation), session.getJSONObject("user").getString("id"), active.store.binding.org, installation)
        val envelope = json(
            "context_id" to second.context,
            "operation_id" to UUID.randomUUID().toString(),
            "kind" to "update_person_details",
            "device_recorded_at" to "2026-09-14T19:00:00Z",
            "payload" to json("person_id" to PERSON, "expected_details_revision" to details, "first_name" to "Other actor", "contact_operations" to JSONArray()),
        ).toString()
        assertEquals("accepted", other.call("POST", "/api/mobile/v1/operations", second.context, envelope).getString("outcome"))
    }

    private fun profileOperation(): OperationRow? = active().store.dao.operations().lastOrNull {
        it.person == PERSON && it.kind == "update_person_details" && it.status !in setOf("covered", "superseded")
    }

    private fun active(): ActiveAccount = FieldRepository::class.java.getDeclaredField("active").apply { isAccessible = true }.get(repository) as ActiveAccount

    private fun stage() = File(compose.activity.filesDir, "mobile005-ui-profile-stage.json")

    private fun screenshot(name: String) {
        // Test-only clearing. MainActivity retains FLAG_SECURE in every production flavor.
        compose.runOnUiThread { compose.activity.window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE) }
        compose.waitForIdle()
        val file = File(compose.activity.getExternalFilesDir(null), "$name.png")
        compose.onRoot().captureToImage().asAndroidBitmap().compress(android.graphics.Bitmap.CompressFormat.PNG, 100, file.outputStream())
    }

    private fun sha(value: String) = java.security.MessageDigest.getInstance("SHA-256").digest(value.toByteArray()).joinToString("") { "%02x".format(it) }

    private companion object { const val PERSON = "20f1bb9b-d502-48da-ae66-7990ba2cfef7" }
}
