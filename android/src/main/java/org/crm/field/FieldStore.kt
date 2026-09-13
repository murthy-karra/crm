package org.crm.field

import java.time.Instant
import java.util.UUID
import org.json.JSONArray
import org.json.JSONObject

/** Database transactions are the only boundary that publishes a local save or cache. */
class FieldStore(val db: FieldDatabase, val binding: Binding, private val clock: DeviceClock) {
    val dao = db.data()

    private fun <T> atomic(body: () -> T): T =
        db.runInTransaction(java.util.concurrent.Callable { body() })

    fun authorize(cookie: String) = atomic {
        val old = dao.meta("binding")?.let { JSONObject(it) }
        if (
            old != null &&
                old.getString("context_id") != binding.context &&
                (dao.operations().isNotEmpty() || dao.drafts().isNotEmpty())
        )
            throw AccessLocked()
        val bootstrap = JSONObject(binding.bootstrap)
        val duration =
            java.time.Duration.between(
                    Instant.parse(bootstrap.getString("server_time")),
                    Instant.parse(bootstrap.getString("offline_access_expires_at")),
                )
                .toMillis()
        require(duration in 1..604_800_000)
        dao.meta(MetaRow("binding", binding.bootstrap))
        dao.meta(
            MetaRow(
                "lease",
                json(
                        "boot" to clock.boot(),
                        "elapsed" to clock.elapsed(),
                        "duration" to duration,
                        "wall" to clock.wall(),
                    )
                    .toString(),
            )
        )
        dao.meta(MetaRow("cookie", cookie))
        dao.meta(MetaRow("locked", "false"))
    }

    fun requireAccess() {
        val value = dao.meta("lease")?.let { JSONObject(it) } ?: throw AccessLocked()
        val lease =
            LeaseClock(
                value.getInt("boot"),
                value.getLong("elapsed"),
                value.getLong("duration"),
                value.getLong("wall"),
            )
        if (
            dao.meta("locked") == "true" ||
                !lease.usable(clock.boot(), clock.elapsed(), clock.wall())
        ) {
            dao.meta(MetaRow("locked", "true"))
            throw AccessLocked()
        }
    }

    fun observeClock() {
        requireAccess()
        val value = JSONObject(dao.meta("lease")!!).put("wall", clock.wall())
        dao.meta(MetaRow("lease", value.toString()))
    }

    fun lock() = atomic {
        dao.meta(MetaRow("locked", "true"))
        dao.meta(MetaRow("cookie", ""))
    }

    fun saveDraft(
        id: String,
        person: String,
        kind: String,
        payload: JSONObject,
        expectedRevision: Long = 0,
    ): DraftRow = atomic {
        requireAccess()
        require(dao.person(person) != null)
        val current = dao.draft(id)
        require((current?.revision ?: 0) == expectedRevision) {
            "Draft changed; reload the committed revision"
        }
        val value = DraftRow(id, person, kind, payload.toString(), expectedRevision + 1)
        dao.draft(value)
        value
    }

    fun submitDraft(id: String, expectedRevision: Long): OperationRow = atomic {
        requireAccess()
        val draft = dao.draft(id) ?: throw StorageFailure()
        require(draft.revision == expectedRevision)
        require(dao.person(draft.person) != null) {
            "Person is outside the current authorized offline selection"
        }
        val payload = JSONObject(draft.payload)
        val text = payload.getString(if (draft.kind == "add_note") "body" else "title")
        require(draft.kind in setOf("add_note", "create_task"))
        val normalized = text.trim()
        require(
            normalized.isNotBlank() &&
                normalized.codePointCount(0, normalized.length) <=
                    if (draft.kind == "add_note") 10_000 else 500
        )
        require(normalized.none { it.isISOControl() && it !in "\n\r\t" })
        require(payload.getString("person_id") == draft.person)
        if (draft.kind == "create_task") {
            require(
                payload.getString("kind") in setOf("call", "email", "text", "follow_up", "other")
            )
            payload.stringOrNull("due_at")?.let { Instant.parse(it) }
        }
        val row = newOperation(draft.person, draft.kind, payload)
        dao.operation(row)
        dao.removeDraft(id)
        row
    }

    fun complete(
        person: String,
        task: JSONObject? = null,
        createOperation: String? = null,
    ): OperationRow = atomic {
        requireAccess()
        require(dao.person(person) != null)
        val target =
            if (createOperation != null) {
                val creation = dao.operation(createOperation) ?: throw ProtocolFailure()
                require(
                    creation.person == person &&
                        creation.kind == "create_task" &&
                        creation.status !in listOf("attention")
                )
                json("created_by_operation_id" to creation.id)
            } else {
                require(
                    task != null &&
                        task.getString("person_id") == person &&
                        task.getBoolean("can_manage")
                )
                json(
                    "task_id" to uuid(task.getString("id")),
                    "expected_revision" to revision(task.getString("revision")),
                )
            }
        newOperation(person, "complete_task", json("person_id" to person, "target" to target))
            .also { dao.operation(it) }
    }

    private fun newOperation(person: String, kind: String, payload: JSONObject): OperationRow {
        val id = UUID.randomUUID().toString()
        val envelope =
            json(
                    "context_id" to binding.context,
                    "operation_id" to id,
                    "kind" to kind,
                    "device_recorded_at" to Instant.ofEpochMilli(clock.wall()).toString(),
                    "payload" to payload,
                )
                .toString()
        require(envelope.toByteArray().size <= 131_072)
        return OperationRow(id, person, kind, envelope, clock.wall())
    }

    fun acknowledge(id: String, response: JSONObject) = atomic {
        requireAccess()
        val operation = dao.operation(id) ?: throw ProtocolFailure()
        if (
            response.getString("operation_id") != id ||
                response.getString("outcome") != "accepted" ||
                response.getString("resource_type") !=
                    if (operation.kind == "add_note") "note" else "task"
        )
            throw ProtocolFailure()
        uuid(response.getString("resource_id"))
        revision(response.getString("person_revision"))
        Instant.parse(response.getString("accepted_at"))
        response.getBoolean("changed")
        response.getBoolean("replayed")
        if (operation.kind == "add_note") require(response.isNull("committed_revision"))
        else revision(response.getString("committed_revision"))
        dao.accept(id, response.toString())
        val current = dao.person(operation.person)
        if (
            current != null &&
                revisionAtLeast(current.revision, response.getString("person_revision"))
        )
            dao.cover(id)
    }

    fun beginGeneration(response: JSONObject) = atomic {
        requireAccess()
        if (response.getString("context_id") != binding.context || !response.getBoolean("complete"))
            throw ProtocolFailure()
        uuid(response.getString("generation_id"))
        require(response.getInt("selected_count") in 0..25_000)
        dao.clearPages()
        dao.clearManifest()
        dao.meta(MetaRow("generation", response.toString()))
        appendManifest(response.getString("generation_id"), response.getJSONObject("manifest"))
    }

    fun appendManifest(generation: String, response: JSONObject) = atomic {
        requireAccess()
        require(JSONObject(dao.meta("generation")!!).getString("generation_id") == generation)
        val items = response.getJSONArray("items").objects()
        require(items.size <= 250)
        dao.manifest(
            items.map {
                ManifestRow(
                    generation,
                    uuid(it.getString("person_id")),
                    revision(it.getString("revision")),
                )
            }
        )
        require(dao.manifest(generation).size <= 25_000)
        val cursor = response.stringOrNull("next_cursor")
        if (response.getBoolean("complete") != (cursor == null)) throw ProtocolFailure()
        dao.meta(MetaRow("manifest_cursor", cursor ?: ""))
        dao.meta(MetaRow("manifest_complete", (cursor == null).toString()))
    }

    fun stagePage(
        generation: String,
        person: String,
        section: String,
        cursor: String,
        response: JSONObject,
    ) = atomic {
        requireAccess()
        val expected = dao.manifestPerson(generation, person) ?: throw ProtocolFailure()
        if (
            response.getString("generation_id") != generation ||
                response.getString("person_id") != person ||
                response.getString("section") != section ||
                response.getString("revision") != expected.revision
        )
            throw ProtocolFailure()
        require(
            response.getJSONArray("items").length() <= 100 &&
                response.toString().toByteArray().size <= 524_288
        )
        require(section in setOf("summary", "notes", "tasks"))
        if (section != "summary")
            response.getJSONArray("items").objects().forEach { item ->
                uuid(item.getString("id"))
                require(item.getString("person_id") == person)
                if (section == "tasks") revision(item.getString("revision"))
            }
        val next = response.stringOrNull("next_cursor")
        if (
            response.getBoolean("complete") != (next == null) ||
                next == cursor ||
                next != null && dao.pages(generation, person, section).any { it.cursor == next }
        )
            throw ProtocolFailure()
        dao.page(PageRow(generation, person, section, cursor, response.toString()))
    }

    fun nextPage(generation: String, person: String, section: String): String? {
        val pages = dao.pages(generation, person, section).associateBy { it.cursor }
        var cursor = ""
        val visited = mutableSetOf<String>()
        while (true) {
            if (!visited.add(cursor)) throw ProtocolFailure()
            val page = pages[cursor] ?: return cursor
            cursor = JSONObject(page.body).stringOrNull("next_cursor") ?: return null
        }
    }

    private fun component(
        generation: String,
        person: String,
        section: String,
    ): Pair<JSONObject?, JSONArray> {
        val pages = dao.pages(generation, person, section).associateBy { it.cursor }
        val items = JSONArray()
        var cursor = ""
        var summary: JSONObject? = null
        val visited = mutableSetOf<String>()
        while (true) {
            if (!visited.add(cursor)) throw ProtocolFailure()
            val response = JSONObject(pages[cursor]?.body ?: throw ProtocolFailure())
            if (!response.isNull("summary")) summary = response.getJSONObject("summary")
            response.getJSONArray("items").objects().forEach { items.put(it) }
            cursor = response.stringOrNull("next_cursor") ?: break
        }
        return summary to items
    }

    fun promote(seal: JSONObject) = atomic {
        requireAccess()
        val generation = JSONObject(dao.meta("generation") ?: throw ProtocolFailure())
        val id = generation.getString("generation_id")
        if (
            seal.getString("context_id") != binding.context ||
                seal.getString("generation_id") != id ||
                seal.getString("evaluated_at") != generation.getString("evaluated_at") ||
                dao.meta("manifest_complete") != "true"
        )
            throw ProtocolFailure()
        val manifest = dao.manifest(id)
        require(
            manifest.size == generation.getInt("selected_count") &&
                manifest.size == seal.getInt("selected_count")
        )
        val today = seal.getJSONObject("today")
        require(today.getJSONObject("sources").getString("status") == "complete")
        val pending = dao.operations().filter { it.status != "covered" }
        for (item in manifest) {
            val old = dao.person(item.person)
            if (old == null || old.revision != item.revision) {
                val summary = component(id, item.person, "summary")
                val notes = component(id, item.person, "notes").second
                val tasks = component(id, item.person, "tasks").second
                require(summary.first?.getString("id") == item.person)
                if (old == null || revisionAtLeast(item.revision, old.revision))
                    dao.person(
                        PersonRow(
                            item.person,
                            item.revision,
                            summary.first!!.toString(),
                            summary.second.toString(),
                            notes.toString(),
                            tasks.toString(),
                            id,
                            seal.getString("evaluated_at"),
                        )
                    )
            }
            val active = dao.person(item.person) ?: throw ProtocolFailure()
            pending
                .filter { it.person == item.person && it.status == "accepted" }
                .forEach { op ->
                    if (
                        revisionAtLeast(
                            active.revision,
                            JSONObject(op.receipt!!).getString("person_revision"),
                        )
                    )
                        dao.cover(op.id)
                }
        }
        val selected = manifest.map { it.person }.toSet()
        var removalConflicts = 0
        for (old in dao.people().filter { it.id !in selected }) {
            // Selection removal cannot discard pending work or establish that an accepted action
            // was covered.
            if (pending.any { it.person == old.id } || dao.drafts().any { it.person == old.id })
                removalConflicts++
            dao.removePerson(old.id)
        }
        dao.meta(MetaRow("today", today.toString()))
        dao.meta(MetaRow("last_sync", seal.getString("sealed_at")))
        dao.meta(
            MetaRow(
                "coverage",
                if (removalConflicts == 0) "Complete: ${manifest.size} People"
                else
                    "Complete selection; $removalConflicts protected saved-work conflicts; records unavailable",
            )
        )
        dao.removeMeta("generation")
        dao.clearPages()
        dao.clearManifest()
    }
}
