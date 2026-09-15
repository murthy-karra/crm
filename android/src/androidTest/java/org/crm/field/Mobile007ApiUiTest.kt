package org.crm.field

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.GrantPermissionRule
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.BeforeClass
import org.junit.Rule
import org.junit.Test
import kotlinx.coroutines.runBlocking
import android.util.Log
import java.io.File
import java.util.UUID
import java.util.concurrent.atomic.AtomicInteger

/** Fixture-gated proof of the real Organization search -> save -> restart flow. */
class Mobile007ApiUiTest {
    @get:Rule(order = 0)
    val localNetwork = GrantPermissionRule.grant("android.permission.ACCESS_LOCAL_NETWORK")
    @get:Rule(order = 1) val compose = createAndroidComposeRule<MainActivity>()
    private val repository
        get() = (compose.activity.application as FieldApplication).repository

    companion object {
        /**
         * Recovery is deliberately opt-in and only accepts an empty synthetic vault.
         * It lets QA reuse an already admitted server context after an emulator harness
         * cleared the account directory; production startup has no equivalent fallback.
         */
        @BeforeClass
        @JvmStatic
        fun recoverKnownSyntheticInstallation() {
            val installation = InstrumentationRegistry.getArguments().getString("mobile007RecoveryInstallation") ?: return
            UUID.fromString(installation)
            require(BuildConfig.API_BASE == "http://10.0.2.2:3101") { "Mobile007 recovery is only valid for the demo API fixture" }
            val context = InstrumentationRegistry.getInstrumentation().targetContext
            val vault = DeviceVault(context, BuildConfig.VAULT_NAMESPACE)
            val registryFile = File(vault.root, "registry.bin")
            val registry = vault.registry()
            if (registry.has("account")) return
            require(registry.optInt("pending", 0) == 0) { "Mobile007 recovery refuses a vault with pending work" }
            val allowed = vault.root.listFiles().orEmpty().all { it.name in setOf("registry.bin", "lock.state", "registry.bin.mobile007-pre-recovery") }
            require(allowed) { "Mobile007 recovery only accepts an empty synthetic vault" }
            if (registryFile.exists()) {
                val backup = File(vault.root, "registry.bin.mobile007-pre-recovery")
                if (!backup.exists()) registryFile.copyTo(backup)
            }
            registry.put("installation", installation).put("locked", true)
            registry.remove("account"); registry.remove("actor"); registry.remove("org")
            vault.saveRegistry(registry)
            vault.setLocked(true)
        }
    }

    private fun signInAndDownload() {
        compose.waitUntil(60_000) {
            !(compose.activity.application as FieldApplication).ready.isActive
        }
        if (InstrumentationRegistry.getArguments().getString("mobile007RecoveryInstallation") != null) {
            // @BeforeClass may run while Application.onCreate is still restoring.
            // Re-read the opt-in registry after the app is ready so login uses the
            // selected existing context rather than a cached pre-recovery UUID.
            runBlocking { repository.restore() }
        }
        if (repository.ui.value.locked) {
            compose.onNodeWithTag("email").performTextInput("agent@mobile.test")
            compose.onNodeWithTag("password").performTextInput("Mobile-demo-only-123!")
            compose.onNodeWithTag("sign-in").performClick()
        }
        val setupPolls = AtomicInteger(0)
        var authorizationError: String? = null
        compose.waitUntil(180_000) {
            val state = repository.ui.value
            val poll = setupPolls.incrementAndGet()
            if (poll == 1 || poll % 300 == 0) {
                Log.i(
                    "CRMFieldTest",
                    "mobile007_setup state=${when { state.locked -> "locked"; state.busy -> "busy"; else -> "ready" }} people=${state.people.size} paused=${state.paused} coverage=${state.coverage.take(40)}",
                )
            }
            if (state.locked && state.message.startsWith("Could not authorize")) {
                authorizationError = "authorization_failed"
                return@waitUntil true
            }
            !state.locked && state.people.size >= 100 && !state.busy
        }
        assertTrue(
            "Mobile007 setup stopped before authorization; state=authorization_failed",
            authorizationError == null,
        )
        // A prior isolated run may have left the optional Remote Prospect admitted.
        // Remove only that requested pin through the same repository command, then
        // let the normal coordinator restore the original 100-record baseline.
        if (repository.ui.value.people.size > 100) {
            val remote = repository.ui.value.people.firstOrNull {
                org.json.JSONObject(it.summary).optString("display_name") == "Remote Prospect"
            }
            if (remote != null) {
                runBlocking {
                    repository.pin(remote.id, false)
                    repository.pause(false)
                }
                compose.waitUntil(180_000) {
                    !repository.ui.value.busy && repository.ui.value.people.size == 100
                }
                runBlocking { repository.pause(true) }
            }
        }
        assertEquals(100, repository.ui.value.people.size)
    }

    @Test
    fun realOrganizationSearchSavePersistsRequestedIntentAfterClearAndRestart() {
        assumeTrue(
            InstrumentationRegistry.getArguments().getString("uiMobile007Requested") == "true"
        )
        signInAndDownload()
        if (!repository.ui.value.paused) compose.onNodeWithText("Pause sync").performClick()
        compose.waitUntil(10_000) { repository.ui.value.paused }
        compose.onNodeWithTag("nav-People").performClick()

        fun searchOrganization(term: String) {
            compose.onNodeWithTag("organization-people-search").performTextInput(term)
            compose.onNodeWithText("Search").performClick()
            compose.waitUntil(30_000) {
                !repository.ui.value.organizationSearching &&
                    repository.ui.value.organizationSearchResults.isNotEmpty()
            }
        }

        // The same-name fixture must remain distinguishable by exact email and
        // normalized phone before any result is saved. These are UI assertions
        // against the real organization search cards, not repository-only checks.
        searchOrganization("Discovery Twin")
        compose.onNodeWithTag("organization-person-ab000007-0000-4000-8000-000000000001").assertIsDisplayed()
        compose.onNodeWithTag("organization-person-ab000007-0000-4000-8000-000000000002").assertIsDisplayed()
        Log.i("CRMFieldTest", "mobile007_discovery same_name_rows=2")
        compose.onNodeWithText("Clear").performClick()
        searchOrganization("+12025550101")
        compose.onNodeWithTag("organization-person-ab000007-0000-4000-8000-000000000001").assertIsDisplayed()
        Log.i("CRMFieldTest", "mobile007_discovery exact_phone_rows=1")
        compose.onNodeWithText("Clear").performClick()
        searchOrganization("twin.two@synthetic.test")
        compose.onNodeWithTag("organization-person-ab000007-0000-4000-8000-000000000002").assertIsDisplayed()
        Log.i("CRMFieldTest", "mobile007_discovery exact_email_rows=1")
        compose.onNodeWithText("Clear").performClick()

        compose.onNodeWithTag("organization-people-search").performTextInput("Remote Prospect")
        compose.onNodeWithText("Search").performClick()
        compose.waitUntil(30_000) {
            !repository.ui.value.organizationSearching &&
                repository.ui.value.organizationSearchResults.any { it.displayName == "Remote Prospect" }
        }
        compose.onAllNodesWithText("Remote Prospect").onLast().assertIsDisplayed()
        compose.onNodeWithText("Save offline").performClick()
        compose.waitUntil(10_000) {
            repository.ui.value.pinnedPeople.contains(
                repository.ui.value.organizationSearchResults.single { it.displayName == "Remote Prospect" }.id
            )
        }

        compose.onNodeWithText("Clear").performClick()
        compose.onNodeWithText("Requested offline People").performScrollTo().assertIsDisplayed()
        assertEquals(1, repository.ui.value.requestedPins.size)

        compose.activityRule.scenario.recreate()
        compose.waitUntil(30_000) {
            !repository.ui.value.locked && !repository.ui.value.busy
        }
        compose.onNodeWithTag("nav-People").performClick()
        compose.onNodeWithText("Requested offline People").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Cancel").assertIsDisplayed()
        assertTrue(repository.ui.value.requestedPins.any { it.label.startsWith("Person ") })

        compose.onNodeWithText("Retry requested downloads").performClick()
        compose.onNodeWithText("Resume sync").performClick()
        compose.waitUntil(180_000) {
            !repository.ui.value.busy && repository.ui.value.people.size == 101
        }
        Log.i("CRMFieldTest", "mobile007_remote sealed_cached_people=101")
        compose.onNodeWithText("Pause sync").performClick()
        compose.waitUntil(10_000) { repository.ui.value.paused }
        compose.onNodeWithTag("nav-People").performClick()
        compose.onNodeWithTag("people-search").performTextInput("Remote Prospect")
        val remoteId = repository.ui.value.people.single {
            org.json.JSONObject(it.summary).optString("display_name") == "Remote Prospect"
        }.id
        compose.onNodeWithTag("person-$remoteId").performScrollTo().assertIsDisplayed()
        compose.onNodeWithTag("person-$remoteId").assertTextContains("Last synced: pinned", substring = true)
        compose.onNodeWithTag("person-$remoteId").performClick()
        compose.waitUntil(20_000) { repository.ui.value.person?.id == remoteId }
        compose.onNodeWithText("Remote Prospect").assertIsDisplayed()

        // Use the existing offline editor on the newly pinned Person. The
        // resulting encrypted draft must survive a process/activity restart.
        compose.onNodeWithText("Add note").performClick()
        // Replace any prior synthetic draft so repeated preserved-store runs
        // remain idempotent without deleting the user's encrypted work.
        compose.onNodeWithTag("composer-text").performTextReplacement("Mobile007 Android offline note")
        compose.waitUntil(15_000) {
            repository.ui.value.drafts.any {
                it.person == remoteId &&
                    it.kind == "add_note" &&
                    org.json.JSONObject(it.payload).optString("body") == "Mobile007 Android offline note"
            }
        }
        assertTrue(
            "offline note draft did not settle before close",
            repository.ui.value.drafts.any {
                it.person == remoteId &&
                    it.kind == "add_note" &&
                    org.json.JSONObject(it.payload).optString("body") == "Mobile007 Android offline note"
            },
        )
        compose.waitUntil(10_000) {
            runCatching { compose.onNodeWithText("Close draft").assertIsEnabled() }.isSuccess
        }
        compose.onNodeWithText("Close draft").assertIsEnabled().performClick()
        compose.onNodeWithTag("nav-Saved work").performClick()
        compose.onNodeWithText("Note draft").assertIsDisplayed()
        assertTrue(
            repository.ui.value.drafts.any {
                it.person == remoteId &&
                    it.kind == "add_note" &&
                    org.json.JSONObject(it.payload).optString("body") == "Mobile007 Android offline note"
            }
        )
        compose.activityRule.scenario.recreate()
        compose.waitUntil(30_000) {
            !repository.ui.value.locked &&
                repository.ui.value.people.size == 101 &&
                repository.ui.value.drafts.any {
                    it.person == remoteId &&
                        it.kind == "add_note" &&
                        org.json.JSONObject(it.payload).optString("body") == "Mobile007 Android offline note"
                }
        }
        compose.onNodeWithTag("nav-Saved work").performClick()
        compose.onNodeWithText("Note draft").assertIsDisplayed()
        Log.i("CRMFieldTest", "mobile007_offline_note persisted_after_recreate=true")
    }

    @Test
    fun realOfflineDraftSurvivesColdProcessRestart() {
        assumeTrue(
            InstrumentationRegistry.getArguments().getString("uiMobile007ColdStart") == "true"
        )
        compose.waitUntil(60_000) {
            !(compose.activity.application as FieldApplication).ready.isActive
        }
        compose.waitUntil(30_000) {
            !repository.ui.value.locked &&
                repository.ui.value.people.size == 101 &&
                repository.ui.value.paused &&
                repository.ui.value.drafts.any {
                    it.kind == "add_note" &&
                        org.json.JSONObject(it.payload).optString("body") == "Mobile007 Android offline note"
                }
        }
        compose.onNodeWithTag("nav-Saved work").performClick()
        compose.onNodeWithText("Note draft").assertIsDisplayed()
        Log.i("CRMFieldTest", "mobile007_cold_start restored_people=101 paused=true offline_note=true")
    }
}
