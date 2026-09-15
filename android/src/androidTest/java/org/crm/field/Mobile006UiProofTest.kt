package org.crm.field

import android.view.WindowManager
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test

/**
 * Native continuation of the staged API conflict proof. All review, replacement editing and
 * submission actions here are Compose gestures; only the temporary second actor catalog rename
 * is external to the primary device.
 */
class Mobile006UiProofTest {
    @get:Rule(order = 0) val localNetwork = androidx.test.rule.GrantPermissionRule.grant("android.permission.ACCESS_LOCAL_NETWORK")
    @get:Rule(order = 1) val compose = createAndroidComposeRule<MainActivity>()

    private val repository get() = (compose.activity.application as FieldApplication).repository

    @Test fun catalogConflictIsReviewedAndReplacedThroughNativeMetadataEditor() = runBlocking {
        assumeTrue(InstrumentationRegistry.getArguments().getString("uiMobile006Conflict") == "review")
        waitReady()
        val stage = JSONObject(File(compose.activity.filesDir, "mobile006-conflict-stage.json").readText())
        assertEquals("catalog_conflict_ready_for_ui_review", stage.getString("state"))
        val stale = stage.getString("stale")
        compose.onNodeWithTag("nav-Saved work").performClick()
        compose.waitUntil(20_000) { active().store.dao.metadataContext(stale)?.current?.isNotEmpty() == true }
        compose.onNodeWithText("Review and replace").assertIsEnabled().performClick()
        compose.waitUntil(20_000) { compose.onNodeWithTag("metadata-text-d9d979a3-5015-4cc9-9252-a58e1d109548").fetchSemanticsNode().config.isMergingSemanticsOfDescendants }
        screenshot("mobile006-ui-catalog-conflict-editor")
        compose.onNodeWithTag("metadata-text-d9d979a3-5015-4cc9-9252-a58e1d109548")
            .performTextReplacement("Android UI replacement ${java.util.UUID.randomUUID()}")
        compose.onNodeWithText("Save and sync").assertIsEnabled().performClick()
        compose.waitUntil(20_000) {
            active().store.dao.operations().any { it.kind == "update_person_metadata" && it.id != stale && it.status == "queued" }
        }
        val replacement = active().store.dao.operations().last { it.kind == "update_person_metadata" && it.id != stale }
        screenshot("mobile006-ui-catalog-replacement-queued")
        if (repository.ui.value.paused) compose.onNodeWithText("Resume sync").assertIsEnabled().performClick()
        repeat(90) {
            if (active().store.dao.operation(replacement.id)?.status in setOf("accepted", "covered")) return@repeat
            delay(1_000)
        }
        val result = active().store.dao.operation(replacement.id)!!
        assertTrue("replacement status=${result.status} error=${result.lastError}", result.status in setOf("accepted", "covered"))
        File(compose.activity.filesDir, "mobile006-ui-evidence.txt").appendText(
            "catalog_conflict=$stale replacement=${replacement.id} status=${result.status}\n",
        )
    }

    private suspend fun waitReady() {
        val app = compose.activity.application as FieldApplication
        compose.waitUntil(60_000) { !app.ready.isActive }
        compose.runOnUiThread { compose.activity.window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE) }
    }

    private fun active(): ActiveAccount = FieldRepository::class.java.getDeclaredField("active").apply { isAccessible = true }.get(repository) as ActiveAccount

    private fun screenshot(name: String) {
        compose.runOnUiThread { compose.activity.window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE) }
        compose.waitForIdle()
        val file = File(compose.activity.getExternalFilesDir(null), "$name.png")
        compose.onRoot().captureToImage().asAndroidBitmap().compress(android.graphics.Bitmap.CompressFormat.PNG, 100, file.outputStream())
    }
}
