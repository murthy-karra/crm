package org.crm.field

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.IOException
import java.util.UUID
import kotlinx.coroutines.cancel
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Opt-in synthetic service test; never silently targets a release/customer API. */
@RunWith(AndroidJUnit4::class)
class LiveApiTest {
    @get:org.junit.Rule
    val localNetwork =
        androidx.test.rule.GrantPermissionRule.grant("android.permission.ACCESS_LOCAL_NETWORK")

    @Test
    fun realApiCompleteDownloadHundredDurableOperationsAndLostResponseReplay() = runBlocking {
        assumeTrue(InstrumentationRegistry.getArguments().getString("runLive") == "true")
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val real = HttpTransport()
        var loseNext = false
        val sent = mutableListOf<String>()
        val transport = Transport { origin, method, path, cookie, binding, body ->
            val result = real.request(origin, method, path, cookie, binding, body)
            if (method == "POST" && path == "/api/mobile/v1/operations") {
                sent.add(body!!)
                if (loseNext && result.status in 200..299) {
                    loseNext = false
                    throw IOException("Synthetic response loss after commit")
                }
            }
            result
        }
        val namespace = "live-api-proof"
        val repository =
            FieldRepository(context, "http://10.0.2.2:3101", namespace, transport = transport)
        try {
            repository.restore()
            repository.login("agent@mobile.test", "Mobile-demo-only-123!")
            assertFalse(
                "Authorized native repository: ${repository.ui.value.message}",
                repository.ui.value.locked,
            )
            repeat(3) { if (repository.ui.value.people.size < 100) repository.sync(true) }
            assertEquals(
                "Complete live selection: ${repository.ui.value.message}",
                100,
                repository.ui.value.people.size,
            )
            val person =
                repository.ui.value.people.single {
                    JSONObject(it.summary).optString("last_name") == "020"
                }
            var noteCount = 0
            var taskCount = 0
            for (card in repository.ui.value.people) {
                repository.select(card.id)
                repository.refreshView()
                noteCount += org.json.JSONArray(repository.ui.value.person!!.notes).length()
                taskCount += org.json.JSONArray(repository.ui.value.person!!.tasks).length()
            }
            assertTrue("At least 1,000 notes downloaded", noteCount >= 1000)
            assertTrue("At least 1,000 tasks downloaded", taskCount >= 1000)
            val label = "Android synthetic ${UUID.randomUUID()}"
            if (InstrumentationRegistry.getArguments().getString("apiStage") != "finish") {
                val operations =
                    (0 until 100).map { index ->
                        val draft =
                            repository.saveDraft(
                                "live-$index",
                                person.id,
                                "add_note",
                                json("person_id" to person.id, "body" to "$label $index"),
                            )
                        repository.submit(draft.id, draft.revision)
                    }
                assertEquals(100, repository.ui.value.operations.size)
                loseNext = true
                repository.sync(true)
                assertEquals(100, repository.ui.value.operations.size)
                assertEquals(
                    "queued",
                    repository.ui.value.operations
                        .first { it.id == JSONObject(sent[0]).getString("operation_id") }
                        .status,
                )
                repository.sync(true)
                assertEquals(
                    "All receipt overlays covered: ${repository.ui.value.message}",
                    0,
                    repository.ui.value.operations.size,
                )
                assertEquals(101, sent.size)
                assertEquals("Exactly the original bytes/UUID must be replayed", sent[0], sent[1])
                assertEquals(
                    100,
                    sent.map { JSONObject(it).getString("operation_id") }.toSet().size,
                )
                repository.select(person.id)
                repository.refreshView()
                val downloaded = org.json.JSONArray(repository.ui.value.person!!.notes).objects()
                assertEquals(
                    "One effect per operation after actual lost response",
                    100,
                    downloaded.count { it.getString("body").startsWith(label) },
                )
                assertTrue(operations.all { operation -> sent.any { it == operation.envelope } })

                if (InstrumentationRegistry.getArguments().getString("apiStage") == "notes") {
                    java.io
                        .File(context.getExternalFilesDir(null), "live-notes-evidence.json")
                        .writeText(
                            json(
                                    "selected_people" to 100,
                                    "downloaded_notes" to noteCount,
                                    "downloaded_tasks" to taskCount,
                                    "queued_notes" to 100,
                                    "note_posts" to 101,
                                    "unique_note_operations" to 100,
                                    "duplicate_effects" to 0,
                                )
                                .toString(2)
                        )
                    return@runBlocking
                }
            }
            val taskDraft =
                repository.saveDraft(
                    "dependency",
                    person.id,
                    "create_task",
                    json(
                        "person_id" to person.id,
                        "title" to "$label task",
                        "kind" to "follow_up",
                        "due_at" to null,
                        "assignee_user_id" to null,
                    ),
                )
            val creation = repository.submit(taskDraft.id, taskDraft.revision)
            repository.complete(person.id, creation = creation.id)
            repeat(2) { repository.sync(true) }
            repository.select(person.id)
            repository.refreshView()
            val task =
                org.json.JSONArray(repository.ui.value.person!!.tasks).objects().single {
                    it.getString("title") == "$label task"
                }
            assertFalse(task.isNull("completed_at"))
            assertEquals(0, repository.ui.value.operations.size)

            repeat(3) {
                val prior = repository.ui.value.lastSync
                repository.sync(true)
                assertTrue(
                    "Repeated fresh sync must remain available: ${repository.ui.value.message}",
                    repository.ui.value.message.startsWith("Up to date"),
                )
                assertNotEquals(prior, repository.ui.value.lastSync)
            }
            val protected =
                repository.saveDraft(
                    "protected",
                    person.id,
                    "add_note",
                    json("person_id" to person.id, "body" to "$label protected draft"),
                )
            repository.lockLocal("Signed out")
            assertTrue(repository.ui.value.locked)
            assertTrue(repository.ui.value.people.isEmpty())
            assertTrue(repository.ui.value.drafts.isEmpty())
            repository.login("second@mobile.test", "Mobile-demo-only-123!")
            assertFalse(repository.ui.value.locked)
            assertTrue(repository.ui.value.drafts.isEmpty())
            assertTrue(repository.ui.value.operations.isEmpty())
            repository.login("agent@mobile.test", "Mobile-demo-only-123!")
            assertFalse(repository.ui.value.locked)
            assertEquals(protected.payload, repository.draft("protected")!!.payload)
            // Safe test evidence contains counts/timings only, never credentials, cookies, or note
            // bodies.
            val evidence =
                json(
                    "selected_people" to 100,
                    "downloaded_notes" to noteCount,
                    "downloaded_tasks" to taskCount,
                    "queued_notes" to 100,
                    "note_posts" to 101,
                    "unique_note_operations" to 100,
                    "duplicate_effects" to 0,
                    "dependency_completion" to true,
                    "actor_isolation" to true,
                )
            java.io
                .File(context.getExternalFilesDir(null), "live-api-evidence.json")
                .writeText(evidence.toString(2))
        } finally {
            repository.lockLocal("Synthetic verification complete")
            repository.scope.cancel()
        }
    }
}
