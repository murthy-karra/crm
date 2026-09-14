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
        compose.onNodeWithTag("profile-contact-${contacts + 1}").performTextInput(uniquePhone(requireNotNull(repository.ui.value.person).contacts))
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
        val replayed = staged.getString("operation")
        // Do not treat an initially-idle view as evidence that the click finished: Compose
        // posts the sync state after input dispatch.  Wait for a durable operation transition
        // (or the visible busy state) before considering another user-visible sync control.
        if (active().store.dao.operation(replayed)?.status !in setOf("accepted", "covered")) syncThroughVisibleControl(replayed)
        if (active().store.dao.operation(replayed)?.status !in setOf("accepted", "covered")) syncThroughVisibleControl(replayed)
        for (attempt in 0 until 90) {
            if (active().store.dao.operation(replayed)?.status in setOf("accepted", "covered")) break
            delay(1_000)
        }
        val replayState = requireNotNull(active().store.dao.operation(replayed))
        assertTrue(
            "replay status=${replayState.status} error=${replayState.lastError} attempts=${replayState.attempts} paused=${repository.ui.value.paused} coverage=${repository.ui.value.coverage} message=${repository.ui.value.message}",
            replayState.status in setOf("accepted", "covered"),
        )
        // The receipt may arrive after the in-flight sealed download. Use the visible manual
        // control to obtain the later seal that is allowed to cover the accepted proposal.
        if (active().store.dao.operation(staged.getString("operation"))?.status != "covered") syncThroughVisibleControl(replayed)
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
        compose.waitUntil(15_000) { runCatching { compose.onNodeWithTag("profile-save").assertIsEnabled() }.isSuccess }
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
        compose.waitUntil(15_000) { compose.onAllNodesWithText("Current profile").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText("Current profile").assertExists()
        compose.onNodeWithText("Review and replace").assertIsEnabled()
        screenshot("mobile005-ui-profile-conflict")

        compose.onNodeWithText("Pause sync").performClick()
        compose.waitUntil { repository.ui.value.paused }
        compose.onNodeWithText("Review and replace").performScrollTo().assertHasClickAction().performClick()
        compose.waitUntil(15_000) { compose.onAllNodesWithTag("profile-first-name").fetchSemanticsNodes().isNotEmpty() }
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
    }

    @Test fun retainedProfileConflictIsReviewedAndReplacedInCompose(): Unit = runBlocking<Unit> {
        assumeTrue(InstrumentationRegistry.getArguments().getString("uiMobile005Profile") == "conflict-replace")
        waitReady()
        signInIfNeeded()
        compose.onNodeWithTag("nav-Saved work").performClick()
        val retainedDraft = repository.ui.value.profileDrafts.lastOrNull { it.person == PERSON && it.operation.isEmpty() }
        val retained = if (retainedDraft == null) {
            val stale = active().store.dao.operations().lastOrNull {
                it.person == PERSON && it.kind == "update_person_details" && it.status == "attention" && it.lastError == "revision_conflict"
            }
            assertNotNull("expected the retained revision-conflict proposal", stale)
            val conflict = requireNotNull(stale)
            assertTrue(active().store.dao.profileContext(conflict.id)?.current?.isNotEmpty() == true)
            compose.waitUntil(15_000) { compose.onAllNodesWithText("Current profile").fetchSemanticsNodes().isNotEmpty() }
            compose.onNodeWithText("Review and replace").assertIsEnabled()
            screenshot("mobile005-ui-profile-conflict")
            if (!repository.ui.value.paused) compose.onNodeWithText("Pause sync").assertIsEnabled().performClick()
            compose.waitUntil { repository.ui.value.paused }
            compose.onNodeWithText("Review and replace").performScrollTo().assertHasClickAction().performClick()
            conflict
        } else {
            compose.onNodeWithTag("profile-draft-${retainedDraft.id}").performScrollTo().assertHasClickAction().performClick()
            screenshot("mobile005-ui-profile-draft-open-attempt")
            null
        }
        compose.waitUntil(15_000) { compose.onAllNodesWithTag("profile-first-name").fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag("profile-first-name").performTextReplacement("UI replacement")
        compose.waitUntil(15_000) { repository.ui.value.profileDrafts.any { it.person == PERSON && it.operation.isEmpty() } }
        screenshot("mobile005-ui-profile-replacement")
        if (!repository.ui.value.paused) compose.onNodeWithText("Pause sync").assertIsEnabled().performClick()
        compose.waitUntil { repository.ui.value.paused }
        compose.waitUntil(15_000) { runCatching { compose.onNodeWithTag("profile-save").assertIsEnabled() }.isSuccess }
        compose.onNodeWithTag("profile-save").assertIsEnabled().performClick()
        compose.waitUntil(15_000) { profileOperation()?.id != retained?.id && profileOperation()?.status == "queued" }
        compose.onNodeWithText("Resume sync").assertIsEnabled().performClick()
        compose.waitUntil(90_000) { profileOperation()?.status in setOf("accepted", "covered") }
        stage().delete()
    }

    @Test fun rejectedProfileProposalIsExplicitlyDiscardedInCompose(): Unit = runBlocking<Unit> {
        assumeTrue(InstrumentationRegistry.getArguments().getString("uiMobile005Profile") == "discard")
        waitReady()
        signInIfNeeded()
        compose.onNodeWithTag("nav-Saved work").performClick()
        val rejected = active().store.dao.operations().lastOrNull {
            it.person == PERSON && it.kind == "update_person_details" && it.status == "attention" && it.lastError != "revision_conflict"
        }
        assertNotNull("expected the diagnosed rejected profile proposal", rejected)
        assertTrue("rejected proposal must retain its failure", requireNotNull(rejected).lastError.isNotEmpty())
        compose.onNodeWithText("Discard proposal").assertIsEnabled().performClick()
        compose.waitUntil(15_000) { active().store.dao.operation(rejected.id)?.status == "covered" }
        assertEquals("", active().store.dao.operation(rejected.id)?.lastError)
        screenshot("mobile005-ui-discarded-rejected-profile")
    }

    private suspend fun waitReady() {
        val application = compose.activity.application as FieldApplication
        try {
            // The API-37 emulator can spend more than 20 seconds recreating the test activity
            // after a forced process stop; NativeUiProof uses the same 60-second startup bound.
            compose.waitUntil(60_000) { !application.ready.isActive }
        } catch (error: Throwable) {
            val state = repository.ui.value
            throw AssertionError(
                "restore active=${application.ready.isActive} completed=${application.ready.isCompleted} " +
                    "cancelled=${application.ready.isCancelled} locked=${state.locked} busy=${state.busy} " +
                    "coverage=${state.coverage} message=${state.message}",
                error,
            )
        }
    }

    private suspend fun syncThroughVisibleControl(operation: String) {
        val before = requireNotNull(active().store.dao.operation(operation))
        if (repository.ui.value.busy) {
            compose.waitUntil(90_000) { !repository.ui.value.busy }
            return
        }
        if (repository.ui.value.paused) {
            compose.onNodeWithText("Resume sync").assertIsEnabled().performClick()
        } else {
            compose.waitUntil(15_000) { runCatching { compose.onNodeWithText("Sync now").assertIsEnabled() }.isSuccess }
            compose.onNodeWithText("Sync now").assertIsEnabled().performClick()
        }
        compose.waitUntil(30_000) {
            val after = active().store.dao.operation(operation)
            repository.ui.value.busy || (after?.let { it.status != before.status || it.attempts != before.attempts } ?: true)
        }
        compose.waitUntil(90_000) { !repository.ui.value.busy }
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

    private fun uniquePhone(contacts: String): String {
        val existing = JSONArray(contacts).let { values ->
            (0 until values.length()).map { index -> values.getJSONObject(index).getString("value").filter(Char::isDigit) }.toSet()
        }
        var suffix = (UUID.randomUUID().hashCode() and Int.MAX_VALUE) % 10_000
        fun candidateDigits(value: Int) = "555555%04d".format(value)
        while (candidateDigits(suffix) in existing || "1${candidateDigits(suffix)}" in existing) suffix = (suffix + 1) % 10_000
        return "555-555-%04d".format(suffix)
    }

    private companion object { const val PERSON = "20f1bb9b-d502-48da-ae66-7990ba2cfef7" }
}
