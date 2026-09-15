package org.crm.field

import android.view.WindowManager
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
        val conflictDraft = repository.ui.value.metadataDrafts.single { it.operation == stale }
        val original = active().store.dao.operation(stale)!!
        val metadataDraft =
            if (original.status == "attention") {
                val metadataCard = "metadata-draft-${conflictDraft.id}"
                // Saved work can also contain a profile conflict with an identically labelled action.
                // The proof must exercise the metadata operation that the staged server conflict named.
                val review = compose.onNode(
                    hasClickAction() and
                        hasAnyAncestor(hasTestTag(metadataCard)) and
                        hasAnyDescendant(hasText("Review and replace")),
                    useUnmergedTree = true,
                )
                review.performScrollTo().assertIsDisplayed().assertIsEnabled().performClick()
                conflictDraft
            } else {
                // A test diagnostic can stop after the local atomic supersession but before the
                // editor gesture. Resume that exact replacement; never regenerate it or discard
                // the protected original proposal.
                assertEquals("superseded", original.status)
                val replacementDraft = repository.ui.value.metadataDrafts.single {
                    it.person == conflictDraft.person && it.operation.isEmpty()
                }
                val replacementCard = "metadata-draft-${replacementDraft.id}"
                // The card tag is nested inside its clickable semantics node. Query that action
                // in the unmerged tree so the test invokes the card, not an off-screen child.
                compose.onNodeWithTag("saved-work-list")
                    .performScrollToNode(hasTestTag(replacementCard))
                compose.onNode(
                    hasClickAction() and hasAnyDescendant(hasTestTag(replacementCard)),
                    useUnmergedTree = true,
                )
                    .performScrollTo().assertIsDisplayed().performClick()
                replacementDraft
            }
        val metadataCard = "metadata-draft-${metadataDraft.id}"
        delay(1_500)
        recordEditorDiagnostic("after_metadata_review_click", stale, metadataCard)
        // Retain the exact projection and dialog state before asserting individual controls;
        // this makes a failed native proof distinguish a rejected revision from a missing row.
        try {
            compose.waitUntil(20_000) {
                compose.onAllNodesWithText("Save and sync", useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty() ||
                    compose.onAllNodesWithText("The original metadata proposal remains protected; a replacement could not be prepared.", useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()
            }
        } catch (error: Throwable) {
            recordEditorDiagnostic("metadata_review_wait_failed", stale, metadataCard)
            throw error
        }
        val editorState = repository.ui.value
        File(compose.activity.filesDir, "mobile006-ui-evidence.txt").appendText(
            "before_editor stale=$stale dao_fields=${active().store.dao.metadataFields().size} ui_fields=${editorState.metadataFields.size} " +
                "drafts=${editorState.metadataDrafts.size} operation=${active().store.dao.operation(stale)?.status}\n",
        )
        compose.onAllNodes(isRoot(), useUnmergedTree = true).onFirst().printToLog("Mobile006UiProof")
        screenshot("mobile006-ui-catalog-conflict-editor-open")
        // OutlinedTextField carries its tag in the dialog's unmerged semantics tree.
        compose.waitUntil(20_000) { compose.onAllNodesWithTag("metadata-text-d9d979a3-5015-4cc9-9252-a58e1d109548", useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty() }
        screenshot("mobile006-ui-catalog-conflict-editor")
        compose.onNodeWithTag("metadata-text-d9d979a3-5015-4cc9-9252-a58e1d109548", useUnmergedTree = true)
            .performScrollTo().assertIsDisplayed()
            .performTextReplacement("Android UI replacement ${java.util.UUID.randomUUID()}")
        val existingMetadataOperations = active().store.dao.operations()
            .filter { it.kind == "update_person_metadata" }
            .map { it.id }
            .toSet()
        compose.onNode(
            hasClickAction() and hasAnyDescendant(hasText("Save and sync")),
            useUnmergedTree = true,
        ).assertIsDisplayed().assertIsEnabled().performClick()
        compose.waitUntil(20_000) {
            active().store.dao.operations().any {
                it.kind == "update_person_metadata" && it.id !in existingMetadataOperations &&
                    it.status in setOf("queued", "accepted", "covered")
            }
        }
        val replacement = active().store.dao.operations().single {
            it.kind == "update_person_metadata" && it.id !in existingMetadataOperations
        }
        screenshot("mobile006-ui-catalog-replacement-queued")
        if (repository.ui.value.paused) compose.onNodeWithText("Resume sync").assertIsEnabled().performClick()
        compose.waitUntil(90_000) {
            active().store.dao.operation(replacement.id)?.status in setOf("accepted", "covered")
        }
        val result = active().store.dao.operation(replacement.id)!!
        assertTrue("replacement status=${result.status} error=${result.lastError}", result.status in setOf("accepted", "covered"))
        screenshot("mobile006-ui-catalog-replacement-settled")
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
        // AlertDialog owns a second Compose root. UiAutomation captures the actual displayed
        // window stack and cannot fail on the root ambiguity that a Compose-only capture has.
        val bitmap = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        file.outputStream().use { bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
        bitmap.recycle()
    }

    private fun recordEditorDiagnostic(phase: String, stale: String, metadataCard: String) {
        val account = active()
        val ui = repository.ui.value
        val saveControls = compose.onAllNodesWithText("Save and sync", useUnmergedTree = true).fetchSemanticsNodes().size
        val protectedNotice = compose.onAllNodesWithText(
            "The original metadata proposal remains protected; a replacement could not be prepared.",
            useUnmergedTree = true,
        ).fetchSemanticsNodes().size
        val card = compose.onAllNodesWithTag(metadataCard, useUnmergedTree = true).fetchSemanticsNodes().size
        File(compose.activity.filesDir, "mobile006-ui-evidence.txt").appendText(
            "$phase stale=$stale metadata_card=$metadataCard card_nodes=$card dao_fields=${account.store.dao.metadataFields().size} " +
                "ui_fields=${ui.metadataFields.size} ui_drafts=${ui.metadataDrafts.size} metadata_contexts=${ui.metadataContexts.size} " +
                "operation=${account.store.dao.operation(stale)?.status} error=${account.store.dao.operation(stale)?.lastError} " +
                "save_controls=$saveControls protected_notice=$protectedNotice\n",
        )
        screenshot("mobile006-ui-$phase")
        // Dialogs add a second semantics root. Keep the app-root tree in the log while the
        // screenshot captures the composed dialog as well.
        compose.onAllNodes(isRoot(), useUnmergedTree = true).onFirst().printToLog("Mobile006UiProof-$phase")
    }
}
