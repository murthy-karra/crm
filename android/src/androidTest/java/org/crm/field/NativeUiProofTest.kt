package org.crm.field

import android.view.WindowManager
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.util.UUID
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test

/** Staged proof leaves a small synthetic queue for external force-stop/relaunch/reboot checks. */
class NativeUiProofTest {
    @get:Rule(order = 0)
    val localNetwork =
        androidx.test.rule.GrantPermissionRule.grant("android.permission.ACCESS_LOCAL_NETWORK")
    @get:Rule(order = 1) val compose = createAndroidComposeRule<MainActivity>()
    private val repository
        get() = (compose.activity.application as FieldApplication).repository

    private fun screenshot(name: String) {
        // Test APK only: production keeps FLAG_SECURE, including the app switcher preview.
        compose.runOnUiThread {
            compose.activity.window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
        }
        compose.waitForIdle()
        val file = File(compose.activity.getExternalFilesDir(null), "$name.png")
        compose
            .onRoot()
            .captureToImage()
            .asAndroidBitmap()
            .compress(android.graphics.Bitmap.CompressFormat.PNG, 100, file.outputStream())
    }

    private fun signIn() {
        android.util.Log.i("CRMFieldTest", "ui_sign_in_started")
        compose.onNodeWithTag("email").performTextInput("agent@mobile.test")
        compose.onNodeWithTag("password").performTextInput("Mobile-demo-only-123!")
        compose.onNodeWithTag("sign-in").performClick()
        android.util.Log.i("CRMFieldTest", "ui_sign_in_submitted")
        compose.waitUntil(180_000) {
            !repository.ui.value.locked &&
                repository.ui.value.people.size == 100 &&
                !repository.ui.value.busy
        }
    }

    @Test
    fun nativeScreensOfflineQueueAndFailedAutosave() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("uiStage") == "prepare")
        compose.waitUntil(10_000) {
            !(compose.activity.application as FieldApplication).ready.isActive
        }
        compose.runOnUiThread {
            compose.activity.window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
        }
        if (repository.ui.value.locked) signIn()
        compose.waitUntil(180_000) {
            repository.ui.value.people.size == 100 && !repository.ui.value.busy
        }
        compose.onNodeWithText("Pause sync").performClick()
        compose.waitUntil { repository.ui.value.paused }
        screenshot("android-today")
        compose.onNodeWithTag("nav-People").performClick()
        compose.onNodeWithTag("people-search").performTextInput("020")
        val person =
            repository.ui.value.people.single {
                org.json.JSONObject(it.summary).optString("last_name") == "020"
            }
        compose.onNodeWithTag("person-${person.id}").performScrollTo().performClick()
        compose.waitUntil { repository.ui.value.person != null }
        screenshot("android-person")
        compose.onNodeWithText("Add note").performClick()
        compose
            .onNodeWithTag("composer-text")
            .performTextInput("Android UI synthetic ${UUID.randomUUID()}")
        compose.waitUntil(10_000) {
            compose.onAllNodesWithText("Save on device").fetchSemanticsNodes().isNotEmpty() &&
                !repository.ui.value.drafts.isEmpty()
        }
        compose.onNodeWithText("Save on device").assertIsEnabled().performClick()
        compose.waitUntil { repository.ui.value.operations.size == 1 }
        compose.onNodeWithText("Create task").performClick()
        compose
            .onNodeWithTag("composer-text")
            .performTextInput("Android UI follow up ${UUID.randomUUID()}")
        compose.waitUntil(10_000) { repository.ui.value.drafts.any { it.kind == "create_task" } }
        compose.onNodeWithText("Save on device").assertIsEnabled().performClick()
        compose.waitUntil { repository.ui.value.operations.size == 2 }
        compose.onNodeWithText("Tasks", useUnmergedTree = true).performClick()
        compose.onNodeWithText("Complete saved task").performScrollTo().performClick()
        compose.waitUntil { repository.ui.value.operations.size == 3 }

        // An actual encrypted SQLite failure must not turn unsaved text into a dismissible draft.
        val vault = DeviceVault(compose.activity)
        val registry = vault.registry()
        val dir = vault.accountDirectory(registry.getString("account"))
        val db = FieldDatabase.open(compose.activity, dir, vault.databaseKey(dir))
        db.openHelper.writableDatabase.execSQL(
            "CREATE TRIGGER reject_ui_draft BEFORE INSERT ON drafts BEGIN SELECT RAISE(ABORT, 'synthetic UI storage failure'); END"
        )
        try {
            compose.onNodeWithText("Add note").performScrollTo().performClick()
            compose.onNodeWithTag("composer-text").performTextInput("Uncommitted synthetic text")
            compose.waitUntil(10_000) {
                compose
                    .onAllNodesWithText("Not saved.", substring = true)
                    .fetchSemanticsNodes()
                    .isNotEmpty()
            }
            compose.onNodeWithText("Close draft").assertIsNotEnabled()
            androidx.test.uiautomator.UiDevice.getInstance(
                    InstrumentationRegistry.getInstrumentation()
                )
                .pressBack()
            compose.onNodeWithTag("composer-text").assertExists()
            screenshot("android-unsaved-storage-failure")
            compose.onNodeWithText("Discard unsaved edits").performClick()
            compose.onAllNodesWithText("Discard unsaved edits").onLast().performClick()
            compose.waitUntil {
                compose.onAllNodesWithTag("composer-text").fetchSemanticsNodes().isEmpty()
            }
        } finally {
            db.openHelper.writableDatabase.execSQL("DROP TRIGGER reject_ui_draft")
            db.close()
        }
        compose.onNodeWithTag("nav-Saved work").performClick()
        screenshot("android-pending-queue")
        assertEquals(3, repository.ui.value.operations.size)
        assertTrue(repository.ui.value.paused)
    }

    @Test
    fun nativeDateTimePickerDraftReopensWithoutProtocolInput() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("uiStage") == "date")
        compose.runOnUiThread {
            compose.activity.window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
        }
        compose.waitUntil(10_000) {
            !(compose.activity.application as FieldApplication).ready.isActive
        }
        compose.waitUntil(30_000) {
            !repository.ui.value.locked && repository.ui.value.people.size == 100
        }
        if (!repository.ui.value.paused) compose.onNodeWithText("Pause sync").performClick()
        compose.onNodeWithTag("nav-People").performClick()
        compose.onNodeWithTag("people-search").performTextInput("020")
        val person =
            repository.ui.value.people.single {
                org.json.JSONObject(it.summary).optString("last_name") == "020"
            }
        compose.onNodeWithTag("person-${person.id}").performScrollTo().performClick()
        compose.waitUntil { repository.ui.value.person != null }
        compose.onNodeWithText("Create task").performClick()
        compose.onNodeWithTag("composer-text").performTextReplacement("Date picker synthetic draft")
        compose.onNodeWithText("Choose due date and time").performScrollTo().performClick()
        val device =
            androidx.test.uiautomator.UiDevice.getInstance(
                InstrumentationRegistry.getInstrumentation()
            )
        assertTrue(
            device.wait(
                androidx.test.uiautomator.Until.hasObject(androidx.test.uiautomator.By.text("OK")),
                5000,
            )
        )
        device.findObject(androidx.test.uiautomator.By.text("OK")).click()
        assertTrue(
            device.wait(
                androidx.test.uiautomator.Until.hasObject(androidx.test.uiautomator.By.text("OK")),
                5000,
            )
        )
        device.findObject(androidx.test.uiautomator.By.text("OK")).click()
        compose.waitUntil(10_000) {
            repository.ui.value.drafts.any {
                it.kind == "create_task" && !org.json.JSONObject(it.payload).isNull("due_at")
            }
        }
        compose.waitUntil(10_000) {
            runCatching { compose.onNodeWithText("Close draft").assertIsEnabled() }.isSuccess
        }
        val expected = repository.ui.value.drafts.single { it.kind == "create_task" }.payload
        compose.onNodeWithText("Close draft").assertIsEnabled().performClick()
        compose.onNodeWithText("Create task").performClick()
        compose.onNodeWithTag("composer-text").assertTextContains("Date picker synthetic draft")
        compose.onNodeWithText("Clear due date").assertExists()
        assertEquals(
            expected,
            repository.ui.value.drafts.single { it.kind == "create_task" }.payload,
        )
        screenshot("android-date-picker-draft")
        compose.onNodeWithText("Close draft").performClick()
    }

    @Test
    fun pendingQueueSurvivesActualProcessRelaunch() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("uiStage") == "relaunch")
        compose.waitUntil(10_000) {
            !(compose.activity.application as FieldApplication).ready.isActive
        }
        compose.waitUntil { !repository.ui.value.locked }
        assertEquals(3, repository.ui.value.operations.size)
        assertEquals(100, repository.ui.value.people.size)
        assertTrue(repository.ui.value.paused)
        compose.onNodeWithTag("nav-Saved work").performClick()
        screenshot("android-relaunched-queue")
    }

    @Test
    fun rebootLocksProtectedQueueUntilOnlineReauthorizationThenSyncs() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("uiStage") == "reboot")
        compose.waitUntil(10_000) {
            !(compose.activity.application as FieldApplication).ready.isActive
        }
        assertTrue(repository.ui.value.locked)
        assertTrue(repository.ui.value.operations.isEmpty())
        assertTrue(repository.ui.value.people.isEmpty())
        screenshot("android-reboot-locked")
        signIn()
        assertEquals(3, repository.ui.value.operations.size)
        compose.onNodeWithText("Resume sync").performClick()
        compose.waitUntil(180_000) {
            !repository.ui.value.busy && repository.ui.value.operations.isEmpty()
        }
        compose.onNodeWithTag("nav-Saved work").performClick()
        screenshot("android-synced-queue")
    }
}
