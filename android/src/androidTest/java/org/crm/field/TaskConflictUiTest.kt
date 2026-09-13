package org.crm.field

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import java.util.UUID
import org.junit.Rule
import org.junit.Test

class TaskConflictUiTest {
    @get:Rule val compose = createComposeRule()

    @Test
    fun oldConflictedRevisionCannotBeSubmittedAgainUntilRefresh() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val personId = UUID.randomUUID().toString()
        val taskId = UUID.randomUUID().toString()
        val task =
            json(
                "id" to taskId,
                "person_id" to personId,
                "revision" to "1",
                "title" to "Changed task",
                "kind" to "follow_up",
                "due_at" to null,
                "completed_at" to null,
                "can_manage" to true,
            )
        val person =
            PersonRow(
                personId,
                "1",
                json("id" to personId, "display_name" to "Synthetic Person").toString(),
                "[]",
                "[]",
                org.json.JSONArray().put(task).toString(),
                UUID.randomUUID().toString(),
                "2026-09-13T00:00:00Z",
            )
        val op =
            OperationRow(
                UUID.randomUUID().toString(),
                personId,
                "complete_task",
                json(
                        "payload" to
                            json(
                                "person_id" to personId,
                                "target" to json("task_id" to taskId, "expected_revision" to "1"),
                            )
                    )
                    .toString(),
                1,
                status = "attention",
                lastError = "revision_conflict",
            )
        val row = mutableStateOf(person)
        compose.setContent {
            MaterialTheme {
                PersonScreen(
                    FieldUi(locked = false, person = row.value, operations = listOf(op)),
                    FieldRepository(context),
                    {},
                    {},
                )
            }
        }
        compose.onNodeWithText("Tasks", useUnmergedTree = true).performClick()
        compose
            .onNodeWithText("Refresh changed task before retry")
            .performScrollTo()
            .assertIsNotEnabled()
        compose.runOnIdle {
            row.value =
                person.copy(
                    revision = "2",
                    tasks = org.json.JSONArray().put(task.put("revision", "2")).toString(),
                )
        }
        compose.onNodeWithText("Complete task").performScrollTo().assertIsEnabled()
    }
}
