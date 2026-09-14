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
            testHttpResult(
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
            if (deny()) testHttpResult(403, json("error" to "forbidden"), null, 30)
            else
                testHttpResult(
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
                        testHttpResult(
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
                        testHttpResult(404, json("error" to "not_found"), null, 30)
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
                    testHttpResult(429, json("error" to "mobile_capacity"), null, 120)
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

    @Test fun refreshedCapabilitiesReplaceTheActiveBindingAndProtectPendingProfileDrafts() = runBlocking<Unit> {
        val namespace = "profile-capabilities-${UUID.randomUUID()}"
        var supported = true
        val basic = transport()
        val remote = Transport { origin, method, path, cookie, binding, body ->
            val response = basic.request(origin, method, path, cookie, binding, body)
            if (path.endsWith("bootstrap") && supported) testHttpResult(response.status,
                response.body.put("capabilities", org.json.JSONArray(listOf("add_note", "create_task", "complete_task", "reconciliation", "update_person_details", "details_revisions"))), response.cookie, response.retryAfter)
            else response
        }
        val repo = FieldRepository(context, namespace = namespace, transport = remote)
        repo.login("synthetic", "synthetic")
        val db = database(namespace)
        db.data().insertProfileDraft(ProfileDraftRow(UUID.randomUUID().toString(), UUID.randomUUID().toString(), "{}", "{}", "1", 1))
        repo.refreshView()
        assertEquals(1, repo.ui.value.pendingCount)
        assertTrue(repo.ui.value.profileEditingEnabled)
        supported = false
        repo.sync(true)
        assertFalse(repo.ui.value.profileEditingEnabled)
        assertEquals(1, repo.ui.value.pendingCount)
        assertEquals(1, db.data().profileDrafts().size)
        db.close()
        repo.lockLocal("Complete")
        repo.scope.cancel()
        DeviceVault(context, namespace).root.deleteRecursively()
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

    @Test
    fun currentProfileRejectsOversizedAndCyclicPagesBeforeRecordingComparison() = runBlocking<Unit> {
        val person = UUID.randomUUID().toString()
        fun page(items: org.json.JSONArray, next: String?) = json(
            "context_id" to fixture("reconciliation_bootstrap").getString("context_id"),
            "person_id" to person,
            "person_revision" to "2",
            "details_revision" to "2",
            "first_name" to "Current",
            "last_name" to "Person",
            "items" to items,
            "next_cursor" to next,
            "complete" to (next == null),
        )
        suspend fun rejects(name: String, detail: (String) -> JSONObject) {
            val namespace = "profile-page-$name-${UUID.randomUUID()}"
            val remote = Transport { origin, method, path, cookie, binding, body ->
                if (path.contains("/details")) testHttpResult(200, detail(path.substringAfterLast("cursor=", "")), null, 30)
                else transport().request(origin, method, path, cookie, binding, body)
            }
            val repo = FieldRepository(context, namespace = namespace, transport = remote)
            repo.login("synthetic", "synthetic")
            val active = FieldRepository::class.java.getDeclaredField("active").apply { isAccessible = true }.get(repo) as ActiveAccount
            val row = OperationRow(UUID.randomUUID().toString(), person, "update_person_details", json("payload" to json()).toString(), 1)
            try {
                repo.currentProfile(active, row)
                fail("$name profile traversal was accepted")
            } catch (_: ProtocolFailure) {
                // The traversal was rejected before any FieldStore comparison write.
            }
            repo.lockLocal("Complete")
            repo.scope.cancel()
            DeviceVault(context, namespace).root.deleteRecursively()
        }
        rejects("oversized") { page(org.json.JSONArray().apply { repeat(101) { put(json("id" to UUID.randomUUID().toString())) } }, null) }
        rejects("cyclic") { page(org.json.JSONArray(), "again") }
        rejects("bytes") { page(org.json.JSONArray(), null).put("padding", "x".repeat(524_289)) }
    }
    @Test fun currentProfileAllowsBroadRevisionChangesButFencesDetailsAndNames() = runBlocking<Unit> {
        val person = UUID.randomUUID().toString()
        val namespace = "profile-fences-${UUID.randomUUID()}"
        var changed: Pair<String, Any>? = null
        val remote = Transport { origin, method, path, cookie, binding, body ->
            if (!path.contains("/details")) transport().request(origin, method, path, cookie, binding, body)
            else {
                val last = path.contains("cursor=")
                val page = json("context_id" to binding, "person_id" to person, "person_revision" to if (last) "9" else "2", "details_revision" to "2", "first_name" to "First", "last_name" to JSONObject.NULL,
                    "items" to org.json.JSONArray().put(json("id" to UUID.randomUUID().toString(), "kind" to "email", "value" to "current@example.test", "import_order" to JSONObject.NULL, "created_at" to "2026-09-14T00:00:00Z")), "next_cursor" to if (last) JSONObject.NULL else "next", "complete" to last)
                if (last) changed?.let { page.put(it.first, it.second) }
                testHttpResult(200, page, null, 30)
            }
        }
        val repo = FieldRepository(context, namespace = namespace, transport = remote)
        repo.login("synthetic", "synthetic")
        val active = FieldRepository::class.java.getDeclaredField("active").apply { isAccessible = true }.get(repo) as ActiveAccount
        val row = OperationRow(UUID.randomUUID().toString(), person, "update_person_details", "{}", 1)
        val result = repo.currentProfile(active, row)
        assertEquals("9", result.getString("person_revision"))
        assertEquals("2", result.getString("details_revision"))
        assertEquals(2, result.getJSONArray("items").length())
        for (mutation in listOf("details_revision" to "3", "first_name" to "Changed", "last_name" to "Changed", "person_revision" to "0", "person_revision" to "9223372036854775808")) {
            changed = mutation
            assertTrue("accepted changed page: $mutation", runCatching { repo.currentProfile(active, row) }.isFailure)
        }
        repo.lockLocal("Complete"); repo.scope.cancel(); DeviceVault(context, namespace).root.deleteRecursively()
    }

    @Test fun currentDetailsRejectsWhitespacePaddedActualHttpResponse() = runBlocking<Unit> {
        InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand("pm grant ${context.packageName} android.permission.ACCESS_LOCAL_NETWORK").close()
        val binding = fixtureBinding()
        for (size in listOf(524_288, 524_289, 600 * 1024)) {
            val compact = json("items" to org.json.JSONArray()).toString()
            val bytes = (compact + " ".repeat(size - compact.toByteArray().size)).toByteArray()
            assertTrue(JSONObject(String(bytes)).toString().toByteArray().size < 524_288)
            val server = java.net.ServerSocket(0, 1, java.net.InetAddress.getByName("127.0.0.1"))
            val writer = async(Dispatchers.IO) {
                server.accept().use { socket ->
                    socket.soTimeout = 10_000
                    val reader = socket.getInputStream().bufferedReader()
                    while (!reader.readLine().isNullOrEmpty()) { }
                    socket.getOutputStream().use { out ->
                        out.write("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nCache-Control: no-store\r\nContent-Length: ${bytes.size}\r\nConnection: close\r\n\r\n".toByteArray())
                        out.write(bytes); out.flush()
                    }
                }
            }
            try {
                val api = FieldApi("http://127.0.0.1:${server.localPort}")
                val response = runCatching { api.page("/api/mobile/v1/people/${UUID.randomUUID()}/details", binding, "", maximumBytes = 524_288) }
                if (size == 524_288) assertTrue("exact-limit response failed: ${response.exceptionOrNull()}", response.isSuccess)
                else assertTrue("padded response accepted: $size", response.exceptionOrNull() is ProtocolFailure)
                withTimeout(15_000) { writer.await() }
            } finally { server.close(); writer.cancel() }
        }
    }

    @Test fun malformedPreviouslyQualifiedCacheRefetchesAtSameRevisionWithoutLosingWork() = runBlocking<Unit> {
        val namespace = "profile-requalify-${UUID.randomUUID()}"
        val person = UUID.randomUUID().toString()
        val generation = UUID.randomUUID().toString()
        var summaries = 0
        val bootstrap = fixture("reconciliation_bootstrap").put("capabilities", org.json.JSONArray(listOf("add_note", "create_task", "complete_task", "reconciliation", "update_person_details", "details_revisions")))
        val summary = json("id" to person, "display_name" to "Saved Person", "first_name" to "Saved", "last_name" to "Person", "details_revision" to "7")
        val methods = org.json.JSONArray().put(json("id" to UUID.randomUUID().toString(), "kind" to "email", "value" to "saved@example.test", "import_order" to JSONObject.NULL, "created_at" to "2026-09-14T00:00:00Z"))
        val remote = Transport { origin, method, path, cookie, binding, body ->
            val result = when {
                path == "/api/session" -> null
                path.endsWith("bootstrap") -> bootstrap.put("installation_id", JSONObject(body!!).getString("installation_id"))
                path.endsWith("/reconciliations") -> json("context_id" to bootstrap.getString("context_id"), "generation_id" to generation, "evaluated_at" to "2026-09-14T00:00:00Z", "expires_at" to "2099-01-01T00:00:00Z", "selected_count" to 1, "complete" to true,
                    "manifest" to json("items" to org.json.JSONArray().put(json("person_id" to person, "revision" to "7")), "next_cursor" to null, "complete" to true))
                path.endsWith("/seal") -> json("context_id" to bootstrap.getString("context_id"), "generation_id" to generation, "evaluated_at" to "2026-09-14T00:00:00Z", "sealed_at" to "2026-09-14T00:01:00Z", "selected_count" to 1, "today" to json("sources" to json("status" to "complete"), "items" to org.json.JSONArray()))
                path.contains("/people/") -> {
                    val section = path.substringAfterLast('/')
                    if (section == "summary") summaries++
                    json("generation_id" to generation, "person_id" to person, "revision" to "7", "section" to section, "summary" to if (section == "summary") summary else JSONObject.NULL,
                        "items" to if (section == "summary") methods else org.json.JSONArray(), "next_cursor" to null, "complete" to true)
                }
                else -> error("Unexpected request $path")
            }
            if (result == null) transport().request(origin, method, path, cookie, binding, body) else testHttpResult(200, result, null, 30)
        }
        val repo = FieldRepository(context, namespace = namespace, transport = remote)
        repo.login("synthetic", "synthetic")
        val active = FieldRepository::class.java.getDeclaredField("active").apply { isAccessible = true }.get(repo) as ActiveAccount
        val dao = active.store.dao
        val invalid = org.json.JSONArray(methods.toString()).apply { getJSONObject(0).put("import_order", "1") }
        dao.person(PersonRow(person, "7", summary.toString(), invalid.toString(), "[]", "[]", "old", "saved", true, true, true))
        val draft = ProfileDraftRow(UUID.randomUUID().toString(), person, summary.toString(), json("last_name" to "Protected", "contact_operations" to org.json.JSONArray()).toString(), "7", 1)
        dao.insertProfileDraft(draft)
        val operation = OperationRow(UUID.randomUUID().toString(), person, "add_note", json("payload" to json("person_id" to person, "body" to "Protected note")).toString(), 1, retryAt = Long.MAX_VALUE)
        dao.operation(operation)
        repo.select(person)
        repo.refreshView()
        assertFalse(repo.ui.value.person!!.detailsRevisionsQualified)
        assertFalse(repo.profileEditingSupported())
        repo.sync(false)
        assertEquals("7", dao.person(person)!!.revision)
        assertEquals(1, summaries)
        assertTrue(dao.person(person)!!.detailsRevisionsQualified)
        assertTrue(repo.profileEditingSupported())
        assertEquals(draft, dao.profileDraft(draft.id))
        assertEquals(operation.envelope, dao.operation(operation.id)!!.envelope)
        repo.lockLocal("Complete"); repo.scope.cancel(); DeviceVault(context, namespace).root.deleteRecursively()
    }

}
