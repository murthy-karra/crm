package org.crm.field

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.util.UUID
import kotlinx.coroutines.*
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class RepositoryBoundaryTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext

    private fun transport(
        deny: () -> Boolean = { false },
        wait: CompletableDeferred<Unit>? = null,
    ) = Transport { _, _, path, _, _, body ->
        val binding = fixture("reconciliation_bootstrap")
        if (path == "/api/session") {
            HttpResult(
                200,
                json(
                    "user" to
                        json(
                            "id" to binding.getString("actor_user_id"),
                            "display_name" to "Synthetic agent",
                        ),
                    "organization" to
                        json(
                            "id" to binding.getString("organization_id"),
                            "workspace_mode" to "operational",
                        ),
                ),
                "synthetic=session",
                30,
            )
        } else if (path.endsWith("bootstrap")) {
            wait?.await()
            if (deny()) HttpResult(403, json("error" to "forbidden"), null, 30)
            else
                HttpResult(
                    200,
                    binding.put("installation_id", JSONObject(body!!).getString("installation_id")),
                    null,
                    30,
                )
        } else throw java.io.IOException("Offline synthetic transport")
    }

    private fun database(namespace: String): FieldDatabase {
        val vault = DeviceVault(context, namespace)
        val registry = vault.registry()
        val directory = vault.accountDirectory(registry.getString("account"))
        return FieldDatabase.open(context, directory, vault.databaseKey(directory))
    }

    @Test
    fun registryFailureDuringSignOutCannotReopenOnFreshRepository() =
        runBlocking<Unit> {
            val name = "lock-fault-${UUID.randomUUID()}"
            val repo = FieldRepository(context, namespace = name, transport = transport())
            repo.restore()
            repo.login("synthetic", "synthetic")
            assertFalse(repo.ui.value.locked)
            val db = database(name)
            db.openHelper.writableDatabase.execSQL(
                "CREATE TRIGGER reject_lock BEFORE INSERT ON metadata WHEN NEW.key='locked' AND NEW.value='true' BEGIN SELECT RAISE(ABORT, 'synthetic lock persistence failure'); END"
            )
            db.close()
            repo.lockLocal("Signed out")
            val vault = DeviceVault(context, name)
            assertTrue(
                "Independent marker must lock even while DB/registry remain old",
                vault.isLocked(),
            )
            assertFalse(
                "Fault deliberately leaves old registry unlocked",
                vault.registry().getBoolean("locked"),
            )
            val restored = FieldRepository(context, namespace = name, transport = transport())
            restored.restore()
            assertTrue(restored.ui.value.locked)
            assertTrue(restored.ui.value.people.isEmpty())
            repo.scope.cancel()
            restored.scope.cancel()
            vault.root.deleteRecursively()
        }

    @Test
    fun deniedCurrentAuthorityLocksRatherThanDisplayingPreviousCache() =
        runBlocking<Unit> {
            var deny = false
            val name = "authority-${UUID.randomUUID()}"
            val repo = FieldRepository(context, namespace = name, transport = transport({ deny }))
            repo.restore()
            repo.login("synthetic", "synthetic")
            assertFalse(repo.ui.value.locked)
            val db = database(name)
            db.data()
                .person(
                    PersonRow(
                        UUID.randomUUID().toString(),
                        "1",
                        json("display_name" to "Protected old cache").toString(),
                        "[]",
                        "[]",
                        "[]",
                        UUID.randomUUID().toString(),
                        "2026-09-13T00:00:00Z",
                    )
                )
            db.close()
            repo.refreshView()
            assertEquals(1, repo.ui.value.people.size)
            deny = true
            repo.sync(true)
            assertTrue(repo.ui.value.locked)
            assertTrue(repo.ui.value.people.isEmpty())
            assertTrue(DeviceVault(context, name).isLocked())
            repo.scope.cancel()
            DeviceVault(context, name).root.deleteRecursively()
        }

    @Test
    fun reclaimedGenerationStartsFreshWithoutDroppingActiveCacheOrDraft() =
        runBlocking<Unit> {
            val name = "reclaimed-${UUID.randomUUID()}"
            val basic = transport()
            var started = 0
            val remote = Transport { origin, method, path, cookie, binding, body ->
                when {
                    path.endsWith("/reconciliations") -> {
                        started++
                        HttpResult(
                            200,
                            fixture("reconciliation")
                                .put("selected_count", 0)
                                .put(
                                    "manifest",
                                    json(
                                        "items" to org.json.JSONArray(),
                                        "complete" to true,
                                        "next_cursor" to null,
                                    ),
                                ),
                            null,
                            30,
                        )
                    }
                    path.endsWith("/seal") ->
                        HttpResult(404, json("error" to "not_found"), null, 30)
                    else -> basic.request(origin, method, path, cookie, binding, body)
                }
            }
            val repo = FieldRepository(context, namespace = name, transport = remote)
            repo.restore()
            repo.login("synthetic", "synthetic")
            val db = database(name)
            val person = UUID.randomUUID().toString()
            db.data()
                .person(
                    PersonRow(
                        person,
                        "1",
                        json("id" to person, "display_name" to "Last complete Person").toString(),
                        "[]",
                        "[]",
                        "[]",
                        UUID.randomUUID().toString(),
                        "2026-09-13T00:00:00Z",
                    )
                )
            db.close()
            val draft =
                repo.saveDraft(
                    "preserve",
                    person,
                    "add_note",
                    json("person_id" to person, "body" to "Protected saved input"),
                )
            repo.sync(true)
            assertEquals(1, started)
            assertEquals(1, repo.ui.value.people.size)
            assertEquals(draft.payload, repo.draft(draft.id)!!.payload)
            repo.sync(true)
            assertEquals(2, started)
            assertEquals(1, repo.ui.value.people.size)
            val reopened = database(name)
            assertNull(reopened.data().meta("generation"))
            reopened.close()
            repo.lockLocal("Complete")
            repo.scope.cancel()
            DeviceVault(context, name).root.deleteRecursively()
        }

    @Test
    fun retryAfterSurvivesRepeatedForegroundRequests() =
        runBlocking<Unit> {
            val name = "backoff-${UUID.randomUUID()}"
            val clock = TestClock()
            val basic = transport()
            var deny = false
            var checks = 0
            val remote = Transport { origin, method, path, cookie, binding, body ->
                if (deny && path.endsWith("bootstrap")) {
                    checks++
                    HttpResult(429, json("error" to "mobile_capacity"), null, 120)
                } else basic.request(origin, method, path, cookie, binding, body)
            }
            val repo = FieldRepository(context, namespace = name, clock = clock, transport = remote)
            repo.restore()
            repo.login("synthetic", "synthetic")
            deny = true
            repo.sync(true)
            repo.sync(false)
            repo.sync(true)
            assertEquals(1, checks)
            clock.elapsedValue += 120_001
            repo.sync(false)
            assertEquals(2, checks)
            repo.lockLocal("Complete")
            repo.scope.cancel()
            DeviceVault(context, name).root.deleteRecursively()
        }

    @Test
    fun failingRetryCheckpointKeepsQueueAndDoesNotCrashSync() =
        runBlocking<Unit> {
            val name = "retry-storage-${UUID.randomUUID()}"
            val repo = FieldRepository(context, namespace = name, transport = transport())
            repo.restore()
            repo.login("synthetic", "synthetic")
            val db = database(name)
            val person = UUID.randomUUID().toString()
            db.data()
                .person(
                    PersonRow(
                        person,
                        "1",
                        json("id" to person, "display_name" to "Saved Person").toString(),
                        "[]",
                        "[]",
                        "[]",
                        UUID.randomUUID().toString(),
                        "2026-09-13T00:00:00Z",
                    )
                )
            val saved =
                repo.saveDraft(
                    "durable",
                    person,
                    "add_note",
                    json("person_id" to person, "body" to "Committed input"),
                )
            val operation = repo.submit(saved.id, saved.revision)
            db.openHelper.writableDatabase.execSQL(
                "CREATE TRIGGER reject_retry BEFORE INSERT ON metadata WHEN NEW.key='sync_failures' BEGIN SELECT RAISE(ABORT, 'synthetic checkpoint failure'); END"
            )
            repo.sync(true)
            assertTrue(repo.ui.value.locked)
            assertEquals(operation.envelope, db.data().operation(operation.id)!!.envelope)
            assertTrue(repo.ui.value.message.contains("storage needs attention"))
            db.openHelper.writableDatabase.execSQL("DROP TRIGGER reject_retry")
            db.close()
            repo.lockLocal("Complete")
            repo.scope.cancel()
            DeviceVault(context, name).root.deleteRecursively()
        }

    @Test
    fun lateLoginCannotUnlockAfterUserSignsOut() =
        runBlocking<Unit> {
            val gate = CompletableDeferred<Unit>()
            val name = "late-${UUID.randomUUID()}"
            val repo =
                FieldRepository(context, namespace = name, transport = transport(wait = gate))
            repo.restore()
            val login = async(Dispatchers.IO) { repo.login("synthetic", "synthetic") }
            withTimeout(5000) { while (!repo.ui.value.busy) delay(10) }
            repo.lockLocal("Signed out during login")
            gate.complete(Unit)
            login.await()
            assertTrue(repo.ui.value.locked)
            assertTrue(DeviceVault(context, name).isLocked())
            repo.scope.cancel()
            DeviceVault(context, name).root.deleteRecursively()
        }
}
