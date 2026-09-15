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
        val current = JSONObject(requireNotNull(active().store.dao.metadataContext(stale)).current)
        // The current-record read is intentionally insufficient after a catalog rename. Drive a
        // normal visible reconciliation before opening the replacement editor.
        if (!metadataCurrentIsSealed(stage.getString("person"), current)) {
            compose.waitUntil(30_000) { !repository.ui.value.busy }
            compose.onNodeWithText("Sync now").assertIsEnabled().performClick()
            compose.waitUntil(120_000) { metadataCurrentIsSealed(stage.getString("person"), current) }
        }
        compose.onNodeWithTag("nav-Saved work").performClick()
        compose.waitUntil(20_000) { active().store.dao.metadataContext(stale)?.current?.isNotEmpty() == true }
        screenshot("mobile006-ui-catalog-conflict")
        compose.onNodeWithText("Review and replace").assertIsEnabled().performClick()
        // Retain the exact projection and dialog state before asserting individual controls;
        // this makes a failed native proof distinguish a rejected revision from a missing row.
        compose.waitUntil(20_000) {
            compose.onAllNodesWithText("Save and sync", useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty() ||
                compose.onAllNodesWithText("The original metadata proposal remains protected; a replacement could not be prepared.", useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()
        }
        val editorState = repository.ui.value
        File(compose.activity.filesDir, "mobile006-ui-evidence.txt").appendText(
            "before_editor stale=$stale dao_fields=${active().store.dao.metadataFields().size} ui_fields=${editorState.metadataFields.size} " +
                "drafts=${editorState.metadataDrafts.size} operation=${active().store.dao.operation(stale)?.status}\n",
        )
        compose.onRoot(useUnmergedTree = true).printToLog("Mobile006UiProof")
        screenshot("mobile006-ui-catalog-conflict-editor-open")
        // OutlinedTextField carries its tag in the dialog's unmerged semantics tree.
        compose.waitUntil(20_000) { compose.onAllNodesWithTag("metadata-text-d9d979a3-5015-4cc9-9252-a58e1d109548", useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty() }
        screenshot("mobile006-ui-catalog-conflict-editor")
        compose.onNodeWithTag("metadata-text-d9d979a3-5015-4cc9-9252-a58e1d109548", useUnmergedTree = true)
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

    private fun metadataCurrentIsSealed(person: String, current: JSONObject): Boolean {
        val cached = active().store.dao.person(person) ?: return false
        return active().store.metadataQualified(cached, current.getString("catalog_revision"), current.getString("metadata_revision"))
    }

    private fun screenshot(name: String) {
        compose.runOnUiThread { compose.activity.window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE) }
        compose.waitForIdle()
        val file = File(compose.activity.getExternalFilesDir(null), "$name.png")
        compose.onRoot().captureToImage().asAndroidBitmap().compress(android.graphics.Bitmap.CompressFormat.PNG, 100, file.outputStream())
    }
}
