package org.crm.field

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import java.util.UUID
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

class Mobile004StageFollowupUiTest {
    @get:Rule val compose = createComposeRule()

    @Test fun unresolvedStageChangeExposesAnEditableFollowupWithoutCreatingAnotherOperation() {
        val personId = UUID.randomUUID().toString()
        val stageId = UUID.randomUUID().toString()
        val pending = OperationRow(
            UUID.randomUUID().toString(), personId, "change_person_stage",
            json("payload" to json("person_id" to personId, "stage_id" to stageId, "expected_stage_revision" to "1")).toString(),
            1,
        )
        val person = PersonRow(
            personId, "1",
            json("id" to personId, "display_name" to "Synthetic Person", "stage" to json("id" to stageId, "name" to "Lead"), "stage_revision" to "1").toString(),
            "[]", "[]", "[]", "generation", "2026-09-13T00:00:00Z", stageRevisionsQualified = true,
        )
        val state = mutableStateOf(
            FieldUi(
                locked = false,
                person = person,
                operations = listOf(pending),
                stageChangesEnabled = true,
                stageCatalog = listOf(StageCatalogRow(stageId, "Lead", -32768, "generation", "1")),
            )
        )
        compose.setContent {
            MaterialTheme {
                PersonScreen(
                    state.value,
                    FieldRepository(InstrumentationRegistry.getInstrumentation().targetContext),
                    { _, _ -> },
                    {},
                )
            }
        }
        compose.onNodeWithText("Save follow-up stage").assertIsEnabled()
        compose.onNodeWithText("A follow-up can be saved on this device and will wait for the pending stage change.").assertExists()
        // The person card only opens a composer; it must not synthesize an outbox row.
        compose.runOnIdle { assertEquals(1, state.value.operations.size) }
    }

    @Test fun baselineStageFollowupIsExplicitlySavedWithoutASecondOutboxOperation() {
        runBlocking {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val namespace = "mobile004-followup-${UUID.randomUUID()}"
        val repository = FieldRepository(context, namespace = namespace, transport = syntheticTransport())
        repository.login("synthetic", "synthetic")
        val active = active(repository)
        val personId = UUID.randomUUID().toString()
        val stageA = UUID.randomUUID().toString()
        val stageB = UUID.randomUUID().toString()
        val summary = json(
            "id" to personId,
            "display_name" to "Synthetic Person",
            "stage" to json("id" to stageA, "name" to "Lead"),
            "stage_revision" to "1",
        ).toString()
        active.store.dao.person(
            PersonRow(personId, "1", summary, "[]", "[]", "[]", "generation", "2026-09-13T00:00:00Z", stageRevisionsQualified = true)
        )
        active.store.dao.stageCatalog(
            listOf(
                StageCatalogRow(stageA, "Lead", 1, "generation", "1"),
                StageCatalogRow(stageB, "Custom", 2, "generation", "1"),
            )
        )
        val predecessor = repository.saveStageDraft(UUID.randomUUID().toString(), personId, stageB)
        val operation = repository.submitStageDraft(predecessor.id, predecessor.revision)
        val originalEnvelope = active.store.dao.operation(operation.id)!!.envelope
        repository.select(personId)
        repeat(50) {
            if (repository.ui.value.person?.id == personId) return@repeat
            delay(100)
        }
        assertEquals(personId, repository.ui.value.person?.id)
        val followupId = UUID.randomUUID().toString()

        compose.setContent {
            MaterialTheme {
                StageComposer(repository, personId, followupId, {}, {})
            }
        }
        compose.waitUntil(5_000) {
            compose.onAllNodesWithTag("stage-save-followup").fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithTag("stage-save-followup").assertIsEnabled().performClick()
        compose.waitUntil(5_000) {
            runBlocking { repository.stageDraft(followupId) != null }
        }
        val followup = requireNotNull(repository.stageDraft(followupId))
        assertEquals(stageA, followup.baselineStageId)
        assertEquals("1", followup.baselineRevision)
        assertEquals(stageA, followup.proposedStageId)
        assertEquals("", followup.operation)
        assertEquals(1, active.store.dao.operations().size)
        assertEquals(originalEnvelope, active.store.dao.operation(operation.id)!!.envelope)
        compose.onNodeWithText("Follow-up draft saved on this device. Waiting for the previous stage change.").assertExists()
        }
    }

    private fun active(repository: FieldRepository): ActiveAccount =
        FieldRepository::class.java.getDeclaredField("active").apply { isAccessible = true }
            .get(repository) as ActiveAccount

    private fun syntheticTransport(): Transport = Transport { _, _, path, _, _, body ->
        val bootstrap = fixture("reconciliation_bootstrap").put(
            "capabilities",
            org.json.JSONArray(
                listOf(
                    "add_note", "create_task", "complete_task", "reconciliation",
                    "change_person_stage", "stage_revisions", "stage_catalog",
                )
            ),
        )
        when {
            path == "/api/session" ->
                HttpResult(
                    200,
                    json(
                        "user" to json("id" to bootstrap.getString("actor_user_id"), "display_name" to "Synthetic agent"),
                        "organization" to json("id" to bootstrap.getString("organization_id"), "workspace_mode" to "operational"),
                    ),
                    "synthetic=session",
                    30,
                )
            path.endsWith("bootstrap") ->
                HttpResult(
                    200,
                    bootstrap.put("installation_id", JSONObject(body!!).getString("installation_id")),
                    null,
                    30,
                )
            else -> throw java.io.IOException("Unexpected synthetic request: $path")
        }
    }
}
