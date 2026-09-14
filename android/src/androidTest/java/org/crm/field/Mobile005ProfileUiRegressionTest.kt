package org.crm.field

import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.test.platform.app.InstrumentationRegistry
import java.util.UUID
import org.junit.Rule
import org.junit.Test

class Mobile005ProfileUiRegressionTest {
    @get:Rule val compose = createComposeRule()

    @Test fun primaryRemovalWarningNamesTheNextServerOrderedEmailAndPhone() {
        val contacts = listOf(
            ProfileContact(UUID.randomUUID().toString(), "email", "first-email@example.test", removed = true),
            ProfileContact(UUID.randomUUID().toString(), "email", "second-email@example.test"),
            ProfileContact(UUID.randomUUID().toString(), "phone", "555-555-0100", removed = true),
            ProfileContact(UUID.randomUUID().toString(), "phone", "555-555-0101"),
        )
        compose.setContent {
            MaterialTheme {
                ProfileRemovalNotice(contacts, 0)
                ProfileRemovalNotice(contacts, 2)
            }
        }
        compose.onNodeWithTag("profile-primary-warning-email").assertExists()
        compose.onNodeWithText("Removing the primary email; second-email@example.test becomes primary.").assertExists()
        compose.onNodeWithTag("profile-primary-warning-phone").assertExists()
        compose.onNodeWithText("Removing the primary phone; 555-555-0101 becomes primary.").assertExists()
    }

    @Test fun savedWorkShowsOneTypedProfileCardWithoutGenericTaskCard() {
        val person = UUID.randomUUID().toString()
        val operation = OperationRow(UUID.randomUUID().toString(), person, "update_person_details", json("payload" to json()).toString(), 1)
        val draft = ProfileDraftRow(UUID.randomUUID().toString(), person, "{}", "{}", "1", 1, operation.id, "saved")
        compose.setContent {
            MaterialTheme {
                SavedWork(
                    FieldUi(locked = false, operations = listOf(operation), profileDrafts = listOf(draft)),
                    FieldRepository(InstrumentationRegistry.getInstrumentation().targetContext),
                    {}, { _, _ -> }, {}, {}, {}, {}, {}, {}, {}, {},
                )
            }
        }
        compose.onAllNodesWithText("Profile details").assertCountEquals(1)
        compose.onAllNodesWithText("Complete task").assertCountEquals(0)
    }
}
