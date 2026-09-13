package org.crm.field

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.test.platform.app.InstrumentationRegistry
import java.util.UUID
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
}
