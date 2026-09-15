package org.crm.field

import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Rule
import org.junit.Test

/** UI projection proof independent of transient Organization search responses. */
class Mobile007UiProjectionTest {
    @get:Rule val compose = createComposeRule()

    @Test fun requestedIntentListShowsAfterSearchResultsAreGone() {
        val repository = FieldRepository(InstrumentationRegistry.getInstrumentation().targetContext)
        compose.setContent {
            MaterialTheme {
                PeopleScreen(
                    FieldUi(
                        locked = false,
                        requestedPins = listOf(RequestedPin("10000000-0000-4000-8000-000000000007", "Person 10000000", "10000000")),
                        pinReviewRequired = true,
                    ),
                    repository,
                ) {}
            }
        }
        compose.onNodeWithText("Requested offline People").assertIsDisplayed()
        compose.onNodeWithText("The requested set needs review. No individual Person was identified.").assertIsDisplayed()
        compose.onNodeWithText("Person 10000000").assertIsDisplayed()
        compose.onNodeWithText("Retry requested downloads").assertIsDisplayed()
        compose.onNodeWithText("Cancel").assertIsDisplayed()
    }

    @Test fun cachedPeopleExposeLastSealedReason() {
        val repository = FieldRepository(InstrumentationRegistry.getInstrumentation().targetContext)
        val id = "20000000-0000-4000-8000-000000000007"
        compose.setContent {
            MaterialTheme {
                PeopleScreen(
                    FieldUi(
                        locked = false,
                        people = listOf(PersonCard(id, "1", JSONObject(mapOf("display_name" to "Cached Person")).toString())),
                        peopleReasons = mapOf(id to listOf("assigned")),
                    ),
                    repository,
                ) {}
            }
        }
        compose.onNodeWithText("Last synced: assigned").assertIsDisplayed()
    }
}
