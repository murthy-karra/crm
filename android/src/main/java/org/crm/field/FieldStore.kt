package org.crm.field

import java.time.Instant
import java.time.OffsetDateTime
import java.util.UUID
import org.json.JSONArray
import org.json.JSONObject

/** Database transactions are the only boundary that publishes a local save or cache. */
class FieldStore(val db: FieldDatabase, val binding: Binding, private val clock: DeviceClock) {
    val dao = db.data()

    /**
     * An expired lease can be discovered inside a Room transaction.  Persist its locked marker
     * after that transaction rolls back, so a failed local write cannot leave the encrypted
     * account apparently usable on the next process start.
     */
    private class ExpiredLease : AccessLocked()

    private fun <T> atomic(body: () -> T): T =
        try {
            db.runInTransaction(java.util.concurrent.Callable { body() })
        } catch (error: ExpiredLease) {
            db.runInTransaction(java.util.concurrent.Callable { dao.meta(MetaRow("locked", "true")) })
            throw error
        }

    fun authorize(cookie: String) = atomic {
        val old = dao.meta("binding")?.let { JSONObject(it) }
        if (
            old != null &&
                old.getString("context_id") != binding.context &&
                (dao.operations().isNotEmpty() || dao.drafts().isNotEmpty() || dao.contactDrafts().isNotEmpty() || dao.stageDrafts().isNotEmpty() || dao.profileDrafts().isNotEmpty())
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
        if (dao.meta("locked") == "true") throw AccessLocked()
        if (!lease.usable(clock.boot(), clock.elapsed(), clock.wall())) {
            dao.meta(MetaRow("locked", "true"))
            throw ExpiredLease()
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
        baseline: JSONObject? = null,
        target: JSONObject? = null,
    ): DraftRow = atomic {
        requireAccess()
        require(dao.person(person) != null)
        val current = dao.draft(id)
        require((current?.revision ?: 0) == expectedRevision) {
            "Draft changed; reload the committed revision"
        }
        require(kind in setOf("add_note", "create_task", "edit_note", "update_task"))
        val value =
            DraftRow(
                id,
                person,
                kind,
                payload.toString(),
                expectedRevision + 1,
                // An existing edit must not be re-based by a late screen/cache response.
                current?.baseline ?: baseline?.toString().orEmpty(),
                current?.target ?: target?.toString().orEmpty(),
                current?.state ?: "draft",
            )
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
        val text = payload.getString(if (draft.kind in setOf("add_note", "edit_note")) "body" else "title")
        require(draft.kind in setOf("add_note", "create_task", "edit_note", "update_task"))
        val normalized = text.trim()
        require(
            normalized.isNotBlank() &&
                normalized.codePointCount(0, normalized.length) <=
                    if (draft.kind in setOf("add_note", "edit_note")) 10_000 else 500
        )
        require(normalized.none { it.isISOControl() && it !in "\n\r\t" })
        require(payload.getString("person_id") == draft.person)
        if (draft.kind in setOf("create_task", "update_task")) {
            require(
                payload.getString("kind") in setOf("call", "email", "text", "follow_up", "other")
            )
            payload.stringOrNull("due_at")?.let { Instant.parse(it) }
        }
        if (draft.kind == "edit_note") {
            require(uuid(payload.getString("note_id")).isNotEmpty())
            revision(payload.getString("expected_revision"))
        }
        if (draft.kind == "update_task") {
            require(uuid(payload.getString("task_id")).isNotEmpty())
            revision(payload.getString("expected_revision"))
            require(payload.has("due_at"))
        }
        val targetKey = targetKey(draft.kind, payload)
        if (draft.kind in setOf("edit_note", "update_task")) {
            require(draft.baseline.isNotEmpty() && draft.target.isNotEmpty())
            val frozenTarget = JSONObject(draft.target)
            require(frozenTarget.getString("person_id") == draft.person)
            require(frozenTarget.getString("resource_id") == targetKey!!.substringAfter(':'))
            require(frozenTarget.getString("expected_revision") == payload.getString("expected_revision"))
        }
        // Submitted work is immutable.  A later typing session remains a draft and never
        // replaces its operation ID or gets silently coalesced into it.
        // Creation commands have no mutable resource target.  They are independent immutable
        // actions, so a queued add-note or create-task must never block another such action just
        // because both have a null target key.  Targeted mutations remain serialized per resource.
        require(
            targetKey == null ||
                dao.operations().none {
                    it.status !in setOf("covered", "superseded") &&
                        targetKey(it.kind, JSONObject(it.envelope).getJSONObject("payload")) == targetKey
                }
        ) { "Saved draft — waiting for the previous change" }
        val row = newOperation(draft.person, draft.kind, payload)
        dao.operation(row)
        if (draft.kind in setOf("edit_note", "update_task"))
            dao.editContext(
                EditContextRow(row.id, draft.person, draft.kind, draft.target, draft.baseline)
            )
        dao.removeDraft(id)
        row
    }

    fun saveContactDraft(
        id: String,
        person: String,
        channel: String,
        outcome: String,
        occurredAt: String,
        resolvedOffset: String,
        expectedRevision: Long = 0,
    ): ContactDraftRow = atomic {
        requireAccess()
        uuid(id)
        require(dao.person(person) != null)
        validateContact(channel, outcome, occurredAt, resolvedOffset)
        val current = dao.contactDraft(id)
        require((current?.revision ?: 0) == expectedRevision)
        require(current == null || current.person == person)
        require(current?.operation.orEmpty().isEmpty()) { "Saved contact is immutable" }
        val next = expectedRevision + 1
        if (current == null)
            dao.insertContactDraft(
                ContactDraftRow(id, person, channel, outcome, occurredAt, resolvedOffset, next)
            )
        else
            require(
                dao.updateContactDraft(
                    id,
                    channel,
                    outcome,
                    occurredAt,
                    resolvedOffset,
                    next,
                    expectedRevision,
                ) == 1
            ) { "Contact draft changed; reload the committed revision" }
        dao.contactDraft(id)!!
    }

    /**
     * The draft ID is the local compare-and-set boundary. Repeated taps see the first sealed
     * operation and return it; different draft IDs for the same Person intentionally remain
     * different manual contacts.
     */
    fun submitContactDraft(id: String, expectedRevision: Long): OperationRow = atomic {
        requireAccess()
        val draft = dao.contactDraft(id) ?: throw StorageFailure()
        require(draft.revision == expectedRevision)
        require(dao.person(draft.person) != null) {
            "Person is outside the current authorized offline selection"
        }
        if (draft.operation.isNotEmpty()) return@atomic dao.operation(draft.operation) ?: throw ProtocolFailure()
        validateContact(draft.channel, draft.outcome, draft.occurredAt, draft.resolvedOffset)
        val payload =
            json(
                "person_id" to draft.person,
                "channel" to draft.channel,
                "outcome" to draft.outcome,
                "occurred_at" to draft.occurredAt,
            )
        val row = newOperation(draft.person, "log_contact_attempt", payload)
        dao.operation(row)
        require(dao.saveContactOperation(id, row.id) == 1)
        row
    }

    /** Saves an editable stage proposal against the complete, promoted catalog only. */
    fun saveStageDraft(
        id: String,
        person: String,
        proposedStageId: String,
        expectedRevision: Long = 0,
        baseline: JSONObject? = null,
    ): StageDraftRow = atomic {
        requireAccess()
        uuid(id)
        uuid(proposedStageId)
        val cached = dao.person(person) ?: throw AccessLocked()
        require(cached.stageRevisionsQualified) { "Refresh this Person before changing stage" }
        val current = dao.stageDraft(id)
        require((current?.revision ?: 0) == expectedRevision) { "Stage draft changed; reload it" }
        require(current == null || current.person == person)
        require(current?.operation.orEmpty().isEmpty()) { "Saved stage proposal is immutable" }
        val catalog = dao.stage(proposedStageId) ?: throw IllegalArgumentException("Stage is not in the complete catalog")
        val original = current?.let {
            json("id" to it.baselineStageId, "name" to it.baselineStageName, "revision" to it.baselineRevision)
        } ?: baseline ?: run {
            val summary = JSONObject(cached.summary)
            val stage = summary.getJSONObject("stage")
            json("id" to uuid(stage.getString("id")), "name" to stage.getString("name"), "revision" to revision(summary.getString("stage_revision")))
        }
        val next = expectedRevision + 1
        if (current == null) {
            dao.insertStageDraft(
                StageDraftRow(
                    id, person, uuid(original.getString("id")), original.getString("name"),
                    revision(original.getString("revision")), catalog.id, catalog.name, next,
                )
            )
        } else {
            require(dao.updateStageDraft(id, catalog.id, catalog.name, next, expectedRevision) == 1)
        }
        dao.stageDraft(id)!!
    }

    /** A profile baseline is frozen at first save. Later typing may only CAS its proposal. */
    fun saveProfileDraft(
        id: String,
        person: String,
        proposal: JSONObject,
        expectedRevision: Long = 0,
        baseline: JSONObject? = null,
    ): ProfileDraftRow = atomic {
        requireAccess(); uuid(id)
        val cached = dao.person(person) ?: throw AccessLocked()
        require(detailsQualified(cached)) { "Refresh this Person before editing details" }
        val current = dao.profileDraft(id)
        require((current?.revision ?: 0) == expectedRevision) { "Profile draft changed; reload it" }
        require(current == null || current.person == person)
        require(current?.operation.orEmpty().isEmpty()) { "Saved profile operation is immutable" }
        val source = current?.baseline?.let(::JSONObject) ?: baseline ?: profileBaseline(cached)
        val details = revision(source.getString("details_revision"))
        validateProfileProposal(person, details, proposal, source)
        val next = expectedRevision + 1
        if (current == null)
            dao.insertProfileDraft(ProfileDraftRow(id, person, source.toString(), proposal.toString(), details, next))
        else
            require(dao.updateProfileDraft(id, proposal.toString(), next, expectedRevision) == 1)
        dao.profileDraft(id)!!
    }

    /** Repeated submission returns the same immutable operation; a second draft waits. */
    fun submitProfileDraft(id: String, expectedRevision: Long): OperationRow = atomic {
        requireAccess()
        val draft = dao.profileDraft(id) ?: throw StorageFailure()
        require(draft.revision == expectedRevision)
        require(dao.person(draft.person)?.let(::detailsQualified) == true)
        if (draft.operation.isNotEmpty()) return@atomic dao.operation(draft.operation) ?: throw ProtocolFailure()
        val baseline = JSONObject(draft.baseline)
        val proposal = JSONObject(draft.proposal)
        validateProfileProposal(draft.person, draft.detailsRevision, proposal, baseline)
        require(dao.operations().none { it.person == draft.person && it.kind == "update_person_details" && it.status !in setOf("covered", "superseded") }) {
            "Saved profile proposal — waiting for the previous profile change"
        }
        val payload = JSONObject(proposal.toString())
        payload.put("person_id", draft.person)
        payload.put("expected_details_revision", draft.detailsRevision)
        val row = newOperation(draft.person, "update_person_details", payload)
        dao.operation(row)
        require(dao.saveProfileOperation(id, row.id) == 1)
        dao.profileContext(ProfileContextRow(row.id, draft.person, draft.baseline, payload.toString()))
        row
    }

    /**
     * A metadata baseline is usable only when both the complete Person section and the exact
     * catalog revision were sealed together. JSON is retained as server-shaped comparison
     * material so decimal/date strings are never parsed through a locale or floating point.
     */
    fun metadataQualified(
        person: PersonRow,
        expectedCatalogRevision: String? = dao.meta("metadata_catalog_revision"),
        expectedMetadataRevision: String? = null,
    ): Boolean = person.metadataRevisionsQualified &&
        runCatching {
            val baseline = JSONObject(person.metadata)
            val metadataRevision = baseline.getString("metadata_revision")
            val catalogRevision = baseline.getString("catalog_revision")
            revision(metadataRevision)
            revision(catalogRevision)
            if (expectedCatalogRevision != null) require(catalogRevision == expectedCatalogRevision)
            if (expectedMetadataRevision != null) require(metadataRevision == expectedMetadataRevision)
            validateMetadataBaseline(baseline)
        }.isSuccess

    fun saveMetadataDraft(
        id: String,
        person: String,
        proposal: JSONArray,
        expectedRevision: Long = 0,
        baseline: JSONObject? = null,
    ): MetadataDraftRow = atomic {
        requireAccess(); uuid(id)
        val cached = dao.person(person) ?: throw AccessLocked()
        require(metadataQualified(cached)) { "Refresh this Person before editing tags or fields" }
        val current = dao.metadataDraft(id)
        require((current?.revision ?: 0) == expectedRevision) { "Metadata draft changed; reload it" }
        require(current == null || current.person == person)
        require(current?.operation.orEmpty().isEmpty()) { "Saved metadata proposal is immutable" }
        val original = current?.let { JSONObject(it.baseline) } ?: baseline ?: JSONObject(cached.metadata)
        validateMetadataBaseline(original)
        validateMetadataProposal(person, proposal, original)
        val next = expectedRevision + 1
        if (current == null)
            dao.insertMetadataDraft(
                MetadataDraftRow(
                    id, person, original.toString(), proposal.toString(),
                    revision(original.getString("metadata_revision")),
                    revision(original.getString("catalog_revision")), next,
                )
            )
        else
            require(dao.updateMetadataDraft(id, proposal.toString(), next, expectedRevision) == 1)
        dao.metadataDraft(id)!!
    }

    /** Submit is one atomic CAS: draft state, immutable envelope, outbox and overlay context. */
    fun submitMetadataDraft(id: String, expectedRevision: Long): OperationRow = atomic {
        requireAccess()
        val draft = dao.metadataDraft(id) ?: throw StorageFailure()
        require(draft.revision == expectedRevision)
        require(dao.person(draft.person)?.let(::metadataQualified) == true)
        if (draft.operation.isNotEmpty()) return@atomic dao.operation(draft.operation) ?: throw ProtocolFailure()
        val baseline = JSONObject(draft.baseline)
        val actions = JSONArray(draft.proposal)
        validateMetadataProposal(draft.person, actions, baseline)
        require(
            dao.operations().none {
                it.person == draft.person && it.kind == "update_person_metadata" &&
                    it.status !in setOf("covered", "superseded")
            }
        ) { "Saved proposal — waiting for the previous metadata update" }
        val payload = json(
            "person_id" to draft.person,
            "expected_metadata_revision" to draft.metadataRevision,
            "expected_catalog_revision" to draft.catalogRevision,
            "actions" to actions,
        )
        val row = newOperation(draft.person, "update_person_metadata", payload)
        dao.operation(row)
        require(dao.saveMetadataOperation(id, row.id) == 1)
        dao.metadataContext(MetadataContextRow(row.id, draft.person, draft.baseline, draft.proposal))
        row
    }

    private fun validateMetadataBaseline(value: JSONObject) {
        require(value.getBoolean("complete"))
        revision(value.getString("metadata_revision")); revision(value.getString("catalog_revision"))
        val tags = value.getJSONArray("tags").objects(); val fields = value.getJSONArray("values").objects()
        require(tags.size <= 100 && fields.size <= 100)
        val ids = mutableSetOf<String>()
        tags.forEach { ids.add(uuid(it.getString("id"))) }
        fields.forEach { require(ids.add(uuid(it.getString("field_id")))) }
    }

    /** Syntax/identity checks mirror the contract only; availability and permissions stay server owned. */
    private fun validateMetadataProposal(person: String, actions: JSONArray, baseline: JSONObject) {
        validateMetadataBaseline(baseline); require(actions.length() in 1..50)
        val seenTags = mutableSetOf<String>(); val seenFields = mutableSetOf<String>()
        val knownTags = dao.metadataTags().map { it.id }.toSet()
        val knownFields = dao.metadataFields().associateBy { it.id }
        actions.objects().forEach { action ->
            when (action.getString("kind")) {
                "add_tag", "remove_tag" -> {
                    require(action.length() == 2); val id = uuid(action.getString("tag_id"))
                    require(seenTags.add(id)); require(id in knownTags) { "Deleted tag is no longer available" }
                }
                "clear_field" -> {
                    require(action.length() == 2); val id = uuid(action.getString("field_id"))
                    require(seenFields.add(id)); require(knownFields.containsKey(id))
                }
                "set_field" -> {
                    require(action.length() == 3); val id = uuid(action.getString("field_id"))
                    require(seenFields.add(id)); val field = knownFields[id] ?: throw IllegalArgumentException("Unknown field")
                    require(field.archivedAt == null) { "Archived fields can only be cleared" }
                    validateMetadataValue(field, action.getJSONObject("value"))
                }
                else -> throw IllegalArgumentException("Unknown metadata action")
            }
        }
        require(person.isNotEmpty())
    }

    private fun validateMetadataValue(field: MetadataFieldRow, value: JSONObject) {
        require(value.length() == 1)
        when (field.fieldType) {
            "text" -> { val text = value.getString("text"); require(text.isNotEmpty() && text.codePointCount(0, text.length) <= 500) }
            "number" -> {
                val number = value.getString("number")
                // Exact NUMERIC(19,4) grammar; keep the supplied canonical string intact.
                require(number.matches(Regex("-?(?:0|[1-9][0-9]{0,14})(?:\\.[0-9]{1,4})?")))
            }
            "date" -> {
                val date = java.time.LocalDate.parse(value.getString("date"))
                require(date >= java.time.LocalDate.parse("1900-01-01") && date <= java.time.LocalDate.parse("2200-12-31"))
            }
            "choice" -> {
                val option = uuid(value.getString("option_id"))
                require(dao.metadataOptions(field.id).any { it.id == option && it.archivedAt == null })
            }
            else -> throw IllegalArgumentException("Unknown field type")
        }
    }

    fun recordCurrentMetadata(operationId: String, response: JSONObject) = atomic {
        requireAccess()
        val operation = dao.operation(operationId) ?: throw ProtocolFailure()
        val context = dao.metadataContext(operationId) ?: throw ProtocolFailure()
        require(operation.kind == "update_person_metadata" && operation.status == "attention")
        require(response.getString("context_id") == binding.context && response.getString("person_id") == operation.person)
        validateMetadataBaseline(response)
        dao.currentMetadataContext(operationId, response.toString(), context.editorRevision + 1)
    }

    fun reviseMetadataConflict(operationId: String, draftId: String): MetadataDraftRow = atomic {
        requireAccess(); val op = dao.operation(operationId) ?: throw ProtocolFailure()
        val context = dao.metadataContext(operationId) ?: throw ProtocolFailure()
        require(op.kind == "update_person_metadata" && op.status == "attention" &&
            op.lastError in setOf("revision_conflict", "catalog_revision_conflict") && context.current.isNotEmpty())
        val current = JSONObject(context.current)
        // A conflict comparison is not a catalog refresh.  Never let a current-record response
        // attach a newer token to editor labels/options from an older sealed catalog.  The next
        // reconciliation must qualify this Person against that exact current baseline first.
        val installedCatalog = dao.meta("metadata_catalog_revision") ?: throw ProtocolFailure()
        require(current.getString("catalog_revision") == installedCatalog) {
            "Refresh the metadata catalog before preparing a replacement"
        }
        val cached = dao.person(op.person) ?: throw AccessLocked()
        require(metadataQualified(cached, installedCatalog, current.getString("metadata_revision"))) {
            "Refresh this Person against the current metadata catalog before preparing a replacement"
        }
        // The matching revision marker is meaningful only when the installed sealed catalog can
        // actually describe every retained current value.  This prevents a damaged/partial
        // catalog table from opening an editor with no controls for an otherwise valid baseline.
        val tags = dao.metadataTags().filter { it.revision == installedCatalog }.map { it.id }.toSet()
        require(current.getJSONArray("tags").objects().all { it.getString("id") in tags }) {
            "Refresh the complete metadata catalog before preparing a replacement"
        }
        val fields = dao.metadataFields().filter { it.revision == installedCatalog }.associateBy { it.id }
        val options = dao.allMetadataOptions().filter { it.revision == installedCatalog }.map { it.id }.toSet()
        require(current.getJSONArray("values").objects().all { value ->
            val field = fields[value.getString("field_id")] ?: return@all false
            field.fieldType != "choice" || value.getJSONObject("value").getString("option_id") in options
        }) { "Refresh the complete metadata catalog before preparing a replacement" }
        require(dao.supersede(operationId) == 1)
        val row = MetadataDraftRow(draftId, op.person, current.toString(), context.proposal,
            revision(current.getString("metadata_revision")), revision(current.getString("catalog_revision")), 1)
        dao.insertMetadataDraft(row); row
    }

    fun discardMetadataConflict(operationId: String) = atomic {
        requireAccess(); val op = dao.operation(operationId) ?: throw ProtocolFailure()
        require(op.kind == "update_person_metadata" && op.status in setOf("attention", "superseded"))
        dao.removeMetadataContext(operationId); dao.removeMetadataOperation(operationId)
        dao.operationState(operationId, "covered", op.attempts, 0, "")
    }

    private fun profileBaseline(person: PersonRow): JSONObject {
        val summary = JSONObject(person.summary)
        val details = revision(summary.getString("details_revision"))
        val contacts = JSONArray(person.contacts)
        validateDetailContacts(contacts)
        return json(
            "first_name" to summary.stringOrNull("first_name"),
            "last_name" to summary.stringOrNull("last_name"),
            "details_revision" to details,
            "contacts" to contacts,
        )
    }

    /** Client checks only declared syntax/identity. Normalization and uniqueness stay server-owned. */
    private fun validateProfileProposal(person: String, details: String, proposal: JSONObject, baseline: JSONObject) {
        validateDetailContacts(baseline.getJSONArray("contacts"))
        revision(details); require(proposal.length() in 1..3)
        require(proposal.has("contact_operations")); require(proposal.getJSONArray("contact_operations").length() <= 50)
        proposal.keys().forEach { require(it in setOf("first_name", "last_name", "contact_operations")) }
        listOf("first_name", "last_name").forEach { key ->
            if (proposal.has(key) && !proposal.isNull(key)) {
                val value = proposal.getString(key).trim()
                require(value.codePointCount(0, value.length) <= 200 && value.none { it.isISOControl() })
            }
        }
        val existing = baseline.getJSONArray("contacts").objects().associateBy { it.getString("id") }
        val changed = mutableSetOf<String>()
        proposal.getJSONArray("contact_operations").objects().forEach { op ->
            when (op.getString("op")) {
                "add" -> { require(op.length() == 3); require(op.getString("kind") in setOf("email", "phone")); detailValue(op.getString("value")) }
                "edit" -> { require(op.length() == 3); val key = uuid(op.getString("id")); require(existing.containsKey(key) && changed.add(key)); detailValue(op.getString("value")) }
                "remove" -> { require(op.length() == 2); val key = uuid(op.getString("id")); require(existing.containsKey(key) && changed.add(key)) }
                else -> throw IllegalArgumentException("Unknown contact operation")
            }
        }
        require(proposal.has("first_name") || proposal.has("last_name") || proposal.getJSONArray("contact_operations").length() > 0)
        require(person.isNotEmpty())
    }

    private fun detailValue(raw: String) {
        val value = raw.trim()
        require(value.isNotEmpty() && value.toByteArray().size <= 1024 && value.none { it.isISOControl() })
    }

    /** Recheck persisted flags: a pre-fix cache may have qualified coercible metadata. */
    fun detailsQualified(person: PersonRow): Boolean = person.detailsRevisionsQualified &&
        runCatching { profileBaseline(person) }.isSuccess

    /**
     * Page bytes remain verbatim reconciliation evidence. Once the complete contact traversal
     * is sealed, the local read projection follows the server's declared visible order rather
     * than an incidental UUID/page order. Old unqualified caches may omit these fields and
     * deliberately retain their stored order until a modern traversal replaces them.
     */
    private fun orderedDetailContacts(contacts: JSONArray): JSONArray {
        val values = contacts.objects()
        if (runCatching { validateDetailContacts(contacts) }.isFailure) return JSONArray(values)
        return JSONArray(
            values.sortedWith(
                compareBy<JSONObject> { item ->
                    detailImportOrder(item)?.toLong() ?: Long.MAX_VALUE
                }.thenBy { item -> Instant.parse(item.getString("created_at")) }
                    .thenBy { item -> item.getString("id") },
            ),
        )
    }

    /** The proposal/outbox/context appear together or none do. Repeated submit returns its ID. */
    fun submitStageDraft(id: String, expectedRevision: Long): OperationRow = atomic {
        requireAccess()
        val draft = dao.stageDraft(id) ?: throw StorageFailure()
        require(draft.revision == expectedRevision)
        require(dao.person(draft.person)?.stageRevisionsQualified == true)
        if (draft.operation.isNotEmpty()) return@atomic dao.operation(draft.operation) ?: throw ProtocolFailure()
        revision(draft.baselineRevision)
        uuid(draft.baselineStageId); uuid(draft.proposedStageId)
        require(dao.stage(draft.proposedStageId)?.name == draft.proposedStageName) { "Stage catalog changed; refresh before saving" }
        require(
            dao.operations().none {
                it.person == draft.person && it.kind == "change_person_stage" && it.status !in setOf("covered", "superseded")
            }
        ) { "Saved proposal — waiting for the previous stage change" }
        val payload = json("person_id" to draft.person, "stage_id" to draft.proposedStageId, "expected_stage_revision" to draft.baselineRevision)
        val row = newOperation(draft.person, "change_person_stage", payload)
        dao.operation(row)
        require(dao.saveStageOperation(id, row.id) == 1)
        dao.stageContext(
            StageContextRow(
                row.id, draft.person,
                json("id" to draft.baselineStageId, "name" to draft.baselineStageName, "revision" to draft.baselineRevision).toString(),
                json("id" to draft.proposedStageId, "name" to draft.proposedStageName).toString(),
            )
        )
        row
    }

    fun recordCurrentStage(operationId: String, response: JSONObject) = atomic {
        requireAccess()
        val operation = dao.operation(operationId) ?: throw ProtocolFailure()
        val context = dao.stageContext(operationId) ?: throw ProtocolFailure()
        require(operation.kind == "change_person_stage")
        require(response.getString("context_id") == binding.context && response.getString("person_id") == operation.person)
        revision(response.getString("person_revision")); val stageRevision = revision(response.getString("stage_revision"))
        val stage = response.getJSONObject("stage")
        uuid(stage.getString("id")); require(stage.getString("name").isNotBlank())
        dao.currentStageContext(operationId, json("id" to stage.getString("id"), "name" to stage.getString("name"), "revision" to stageRevision, "person_revision" to response.getString("person_revision")).toString(), context.editorRevision + 1)
    }

    /** Supersede evidence first, then create a separate proposal bound to the newly fetched revision. */
    fun reviseStageConflict(operationId: String, newDraftId: String): StageDraftRow = atomic {
        requireAccess()
        val operation = dao.operation(operationId) ?: throw ProtocolFailure()
        val context = dao.stageContext(operationId) ?: throw ProtocolFailure()
        require(operation.kind == "change_person_stage" && operation.status == "attention" && operation.lastError == "revision_conflict")
        require(context.current.isNotEmpty())
        require(dao.supersede(operationId) == 1)
        val current = JSONObject(context.current)
        val proposal = JSONObject(context.proposal)
        val chosen = dao.stage(proposal.getString("id")) ?: throw IllegalArgumentException("Proposed stage no longer exists")
        val row = StageDraftRow(newDraftId, operation.person, current.getString("id"), current.getString("name"), revision(current.getString("revision")), chosen.id, chosen.name, 1)
        dao.insertStageDraft(row)
        row
    }

    fun discardStageConflict(operationId: String) = atomic {
        requireAccess()
        val operation = dao.operation(operationId) ?: throw ProtocolFailure()
        require(operation.kind == "change_person_stage" && operation.status in setOf("attention", "superseded"))
        dao.removeStageContext(operationId)
        dao.operationState(operationId, "covered", operation.attempts, 0, "")
    }

    /**
     * A definitive future-time rejection is repairable, but its attempted operation is evidence.
     * Clone only the local input into a new draft ID; never change the uncertain original envelope
     * or ask a retry to reinterpret it with a new timestamp.
     */
    fun reviseFutureContact(operationId: String): ContactDraftRow = atomic {
        requireAccess()
        val operation = dao.operation(operationId) ?: throw ProtocolFailure()
        require(
            operation.kind == "log_contact_attempt" &&
                operation.status == "attention" &&
                operation.lastError == "contact_time_in_future"
        )
        val source = dao.contactDrafts().singleOrNull { it.operation == operationId } ?: throw ProtocolFailure()
        val replacement =
            ContactDraftRow(
                UUID.randomUUID().toString(),
                source.person,
                source.channel,
                source.outcome,
                source.occurredAt,
                source.resolvedOffset,
                1,
            )
        dao.insertContactDraft(replacement)
        replacement
    }

    private fun validateContact(
        channel: String,
        outcome: String,
        occurredAt: String,
        resolvedOffset: String,
    ) {
        require(channel in setOf("call", "text", "email", "other"))
        require(outcome in setOf("reached", "no_answer", "left_message", "sent", "busy", "wrong_number"))
        val parsed = OffsetDateTime.parse(occurredAt)
        require(parsed.year in 1..9999)
        require(parsed.offset.id == resolvedOffset)
        // The backend normalizes accepted values to PostgreSQL microseconds. The Android
        // envelope intentionally keeps the agent's selected RFC3339 offset and precision.
        require(Instant.parse(parsed.toInstant().toString()).epochSecond == parsed.toInstant().epochSecond)
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
        val payload = json("person_id" to person, "target" to target)
        val key = targetKey("complete_task", payload)
        require(
            dao.operations().none {
                it.status !in setOf("covered", "superseded") && targetKey(it.kind, JSONObject(it.envelope).getJSONObject("payload")) == key
            }
        ) { "Saved draft — waiting for the previous change" }
        newOperation(person, "complete_task", payload)
            .also { dao.operation(it) }
    }

    private fun targetKey(kind: String, payload: JSONObject): String? =
        when (kind) {
            "edit_note" -> "note:${uuid(payload.getString("note_id"))}"
            "update_task" -> "task:${uuid(payload.getString("task_id"))}"
            "complete_task" -> payload.optJSONObject("target")?.stringOrNull("task_id")?.let { "task:${uuid(it)}" }
            "change_person_stage" -> "person_stage:${uuid(payload.getString("person_id"))}"
            "update_person_details" -> "person_details:${uuid(payload.getString("person_id"))}"
            "update_person_metadata" -> "person_metadata:${uuid(payload.getString("person_id"))}"
            else -> null
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
        val expectedResource =
            when (operation.kind) {
                "add_note", "edit_note" -> "note"
                "create_task", "update_task", "complete_task" -> "task"
                "log_contact_attempt" -> "contact_attempt"
                "change_person_stage" -> "person_stage"
                "update_person_details" -> "person_details"
                "update_person_metadata" -> "person_metadata"
                else -> throw ProtocolFailure()
            }
        if (
            response.getString("operation_id") != id ||
                response.getString("outcome") != "accepted" ||
                response.getString("resource_type") != expectedResource
        )
            throw ProtocolFailure()
        uuid(response.getString("resource_id"))
        revision(response.getString("person_revision"))
        Instant.parse(response.getString("accepted_at"))
        response.getBoolean("changed")
        response.getBoolean("replayed")
        when (operation.kind) {
            "add_note" -> require(response.isNull("committed_revision"))
            "edit_note", "create_task", "update_task", "complete_task" ->
                revision(response.getString("committed_revision"))
            "log_contact_attempt" -> {
                // A contact fact is intentionally append-only and does not fabricate a Person
                // revision. Contact receipts are strict so an old task/note shape cannot hide a
                // protocol regression or incorrectly settle this independent outbox record.
                require(response.length() == 9)
                require(response.isNull("committed_revision"))
                require(response.getBoolean("changed"))
            }
            "change_person_stage" -> {
                require(response.length() == 9)
                require(response.getString("resource_id") == operation.person)
                revision(response.getString("committed_revision"))
            }
            "update_person_details" -> {
                require(response.length() == 10)
                require(response.getString("resource_id") == operation.person)
                revision(response.getString("committed_revision"))
                val payload = JSONObject(operation.envelope).getJSONObject("payload")
                val adds = payload.getJSONArray("contact_operations").objects().mapIndexedNotNull { index, op -> if (op.getString("op") == "add") index else null }.toSet()
                val mapping = response.getJSONArray("added_contact_ids").objects()
                val operationCount = payload.getJSONArray("contact_operations").length()
                val ordinals = mapping.map { strictJsonInt(it.get("ordinal"), 0, operationCount - 1) }
                val ids = mapping.map { entry -> require(entry.length() == 2); uuid(entry.get("id") as String) }
                require(mapping.size == adds.size && ordinals.toSet() == adds)
                require(ids.toSet().size == ids.size)
                if (!response.getBoolean("changed")) require(mapping.isEmpty())
            }
            "update_person_metadata" -> {
                require(response.length() == 9)
                require(response.getString("resource_id") == operation.person)
                revision(response.getString("committed_revision"))
            }
            else -> throw ProtocolFailure()
        }
        dao.accept(id, response.toString())
        if (operation.kind == "log_contact_attempt") {
            dao.contactState(id, "accepted", "")
            // A reconciliation begun before this append-only receipt may carry an equal Person
            // revision and an old Today evaluation. It cannot establish coverage. Drop only its
            // staging rows; the active complete cache and the durable receipt stay intact, and
            // the caller immediately starts a new generation/seal.
            dao.removeMeta("generation")
            dao.removeMeta("manifest_cursor")
            dao.removeMeta("manifest_complete")
            dao.clearPages()
            dao.clearManifest()
        }
        if (operation.kind == "change_person_stage") {
            dao.stageState(id, "accepted", "")
            // A receipt is authoritative but does not make a partial cache or Today fresh.
            dao.removeMeta("generation"); dao.removeMeta("manifest_cursor"); dao.removeMeta("manifest_complete")
            dao.clearPages(); dao.clearManifest(); dao.clearStageCatalogPages()
        }
        if (operation.kind == "update_person_details") {
            dao.profileState(id, "accepted", "")
            dao.removeMeta("generation"); dao.removeMeta("manifest_cursor"); dao.removeMeta("manifest_complete")
            dao.clearPages(); dao.clearManifest()
        }
        if (operation.kind == "update_person_metadata") {
            dao.metadataState(id, "accepted", "")
            // Only a causally later sealed metadata representation may retire this overlay.
            dao.removeMeta("generation"); dao.removeMeta("manifest_cursor"); dao.removeMeta("manifest_complete")
            dao.clearPages(); dao.clearManifest(); dao.clearMetadataCatalogPages()
        }
        val current = dao.person(operation.person)
        if (
            operation.kind !in setOf("log_contact_attempt", "change_person_stage", "update_person_details", "update_person_metadata") &&
            current != null &&
                revisionAtLeast(current.revision, response.getString("person_revision"))
        )
            dao.cover(id)
    }

    /** A current-record read is protected comparison context only: never a person cache seal. */
    fun recordCurrent(operationId: String, response: JSONObject) = atomic {
        requireAccess()
        val operation = dao.operation(operationId) ?: throw ProtocolFailure()
        val context = dao.editContext(operationId) ?: throw ProtocolFailure()
        val envelope = JSONObject(operation.envelope)
        require(response.getString("context_id") == binding.context)
        require(response.getString("person_id") == operation.person)
        revision(response.getString("person_revision"))
        val record = response.getJSONObject(if (operation.kind == "edit_note") "note" else "task")
        val expectedId = JSONObject(context.target).getString("resource_id")
        require(uuid(record.getString("id")) == expectedId)
        require(record.getString("person_id") == operation.person)
        revision(record.getString("revision"))
        // The immutable operation still owns the original target/baseline.  Only a response
        // associated with that exact operation may supply comparison data.
        require(envelope.getString("operation_id") == operationId)
        dao.currentEditContext(operationId, record.toString(), context.editorRevision + 1)
    }

    fun recordCurrentProfile(operationId: String, response: JSONObject) = atomic {
        requireAccess()
        val operation = dao.operation(operationId) ?: throw ProtocolFailure()
        val context = dao.profileContext(operationId) ?: throw ProtocolFailure()
        require(operation.kind == "update_person_details" && operation.status == "attention")
        require(response.getString("context_id") == binding.context && response.getString("person_id") == operation.person)
        revision(response.getString("person_revision")); revision(response.getString("details_revision"))
        validateDetailContacts(response.getJSONArray("items")); require(response.getBoolean("complete")); require(response.isNull("next_cursor"))
        val current = json("first_name" to if (response.isNull("first_name")) null else response.getString("first_name"), "last_name" to if (response.isNull("last_name")) null else response.getString("last_name"), "details_revision" to response.getString("details_revision"), "contacts" to orderedDetailContacts(response.getJSONArray("items")))
        dao.currentProfileContext(operationId, current.toString(), context.editorRevision + 1)
    }

    fun reviseProfileConflict(operationId: String, draftId: String): ProfileDraftRow = atomic {
        requireAccess()
        val op = dao.operation(operationId) ?: throw ProtocolFailure()
        val context = dao.profileContext(operationId) ?: throw ProtocolFailure()
        require(op.kind == "update_person_details" && op.status == "attention" && op.lastError == "revision_conflict" && context.current.isNotEmpty())
        require(dao.supersede(operationId) == 1)
        val current = JSONObject(context.current)
        val payload = JSONObject(context.proposal)
        payload.remove("person_id"); payload.remove("expected_details_revision")
        val row = ProfileDraftRow(draftId, op.person, current.toString(), payload.toString(), revision(current.getString("details_revision")), 1)
        dao.insertProfileDraft(row); row
    }

    fun discardProfileConflict(operationId: String) = atomic {
        requireAccess(); val op = dao.operation(operationId) ?: throw ProtocolFailure()
        require(op.kind == "update_person_details" && op.status in setOf("attention", "superseded"))
        // Keep the immutable operation as covered audit evidence, but remove the linked
        // proposal and comparison together so no unusable orphan draft remains in Saved work.
        dao.removeProfileContext(operationId)
        dao.removeProfileOperation(operationId)
        dao.operationState(operationId, "covered", op.attempts, 0, "")
    }

    fun supersedeConflict(operationId: String): JSONObject = atomic {
        requireAccess()
        val operation = dao.operation(operationId) ?: throw ProtocolFailure()
        val context = dao.editContext(operationId) ?: throw ProtocolFailure()
        require(operation.status == "attention" && operation.lastError == "revision_conflict")
        require(context.current.isNotEmpty())
        require(dao.supersede(operationId) == 1)
        JSONObject(context.current)
    }

    fun discardConflict(operationId: String) = atomic {
        requireAccess()
        val operation = dao.operation(operationId) ?: throw ProtocolFailure()
        require(operation.status in setOf("attention", "superseded"))
        // Keep no retained content after an explicit choice of the current version.
        dao.removeEditContext(operationId)
        dao.operationState(operationId, "covered", operation.attempts, 0, "")
    }

    fun beginGeneration(response: JSONObject) = atomic {
        requireAccess()
        if (response.getString("context_id") != binding.context || !response.getBoolean("complete"))
            throw ProtocolFailure()
        uuid(response.getString("generation_id"))
        require(response.getInt("selected_count") in 0..25_000)
        dao.clearPages()
        dao.clearManifest()
        dao.clearStageCatalogPages()
        dao.clearMetadataCatalogPages()
        if (response.has("stage_catalog")) {
            val catalog = response.getJSONObject("stage_catalog")
            revision(catalog.getString("revision"))
            require(catalog.getString("stages_url").startsWith("/api/mobile/v1/reconciliations/${response.getString("generation_id")}/stages"))
            dao.meta(MetaRow("stage_catalog_cursor", ""))
            dao.meta(MetaRow("stage_catalog_complete", "false"))
        } else {
            dao.removeMeta("stage_catalog_cursor")
            dao.removeMeta("stage_catalog_complete")
        }
        if (response.has("metadata")) {
            val catalog = response.getJSONObject("metadata")
            require(catalog.getString("representation") == "metadata-v1")
            revision(catalog.getString("catalog_revision"))
            listOf("tags", "fields", "options").forEach { section ->
                require(catalog.getString("catalog_url").startsWith("/api/mobile/v1/reconciliations/${response.getString("generation_id")}/metadata/catalog"))
                dao.meta(MetaRow("metadata_catalog_${section}_cursor", ""))
                dao.meta(MetaRow("metadata_catalog_${section}_complete", "false"))
            }
        } else listOf("tags", "fields", "options").forEach { section ->
            dao.removeMeta("metadata_catalog_${section}_cursor"); dao.removeMeta("metadata_catalog_${section}_complete")
        }
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
                    if (it.has("metadata_revision")) revision(it.getString("metadata_revision")) else "",
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
        require(section in setOf("summary", "notes", "tasks", "metadata"))
        val items = if (section == "metadata") emptyList() else response.getJSONArray("items").objects()
        require(items.size <= 100 && response.toString().toByteArray().size <= 524_288)
        if (section == "metadata") {
            require(response.getBoolean("complete") && response.isNull("next_cursor"))
            require(response.has("tags") && response.has("values"))
            require(expected.metadataRevision.isNotEmpty() && response.getString("metadata_revision") == expected.metadataRevision)
            val values = json(
                "metadata_revision" to response.getString("metadata_revision"),
                "catalog_revision" to response.getString("catalog_revision"),
                "tags" to response.getJSONArray("tags"), "values" to response.getJSONArray("values"),
                "complete" to true,
            )
            validateMetadataBaseline(values)
        }
        if (section == "summary") validateContactItems(response.getJSONArray("items"))
        if (section != "summary" && section != "metadata")
            response.getJSONArray("items").objects().forEach { item ->
                uuid(item.getString("id"))
                require(item.getString("person_id") == person)
                // Older complete caches did not expose per-item revisions. They remain
                // readable but cannot qualify an edit baseline until a modern traversal.
                if ((section == "tasks" || section == "notes") && item.has("revision")) revision(item.getString("revision"))
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

    fun stageCatalogPage(generation: String, cursor: String, response: JSONObject) = atomic {
        requireAccess()
        val active = JSONObject(dao.meta("generation") ?: throw ProtocolFailure())
        val catalog = active.optJSONObject("stage_catalog") ?: throw ProtocolFailure()
        require(response.getString("generation_id") == generation && response.getString("revision") == catalog.getString("revision"))
        val items = response.getJSONArray("items").objects()
        require(items.size <= 100 && response.toString().toByteArray().size <= 524_288)
        items.forEach { item -> uuid(item.getString("id")); require(item.getString("name").isNotBlank()) }
        val next = response.stringOrNull("next_cursor")
        require(response.getBoolean("complete") == (next == null) && next != cursor)
        require(dao.stageCatalogPages(generation).none { it.cursor == cursor })
        dao.stageCatalogPage(StageCatalogPageRow(generation, cursor, response.toString()))
        dao.meta(MetaRow("stage_catalog_cursor", next ?: ""))
        dao.meta(MetaRow("stage_catalog_complete", (next == null).toString()))
    }

    fun nextStageCatalogPage(generation: String): String? {
        val pages = dao.stageCatalogPages(generation).associateBy { it.cursor }
        var cursor = ""
        val visited = mutableSetOf<String>()
        while (true) {
            if (!visited.add(cursor)) throw ProtocolFailure()
            val page = pages[cursor] ?: return cursor
            cursor = JSONObject(page.body).stringOrNull("next_cursor") ?: return null
        }
    }

    fun metadataCatalogPage(generation: String, section: String, cursor: String, response: JSONObject) = atomic {
        requireAccess(); require(section in setOf("tags", "fields", "options"))
        val active = JSONObject(dao.meta("generation") ?: throw ProtocolFailure())
        val catalog = active.optJSONObject("metadata") ?: throw ProtocolFailure()
        require(response.getString("generation_id") == generation && response.getString("section") == section &&
            response.getString("revision") == catalog.getString("catalog_revision"))
        val items = response.getJSONArray("items").objects()
        require(items.size <= 100 && response.toString().toByteArray().size <= 524_288)
        items.forEach { item ->
            uuid(item.getString("id"))
            when (section) {
                "tags" -> require(item.getString("name").isNotBlank())
                "fields" -> { require(item.getString("field_type") in setOf("text", "number", "date", "choice")); item.getInt("position") }
                else -> { uuid(item.getString("field_id")); item.getInt("position") }
            }
        }
        val next = response.stringOrNull("next_cursor")
        require(response.getBoolean("complete") == (next == null) && next != cursor)
        require(dao.metadataCatalogPages(generation, section).none { it.cursor == cursor })
        dao.metadataCatalogPage(MetadataCatalogPageRow(generation, section, cursor, response.toString()))
        dao.meta(MetaRow("metadata_catalog_${section}_cursor", next ?: ""))
        dao.meta(MetaRow("metadata_catalog_${section}_complete", (next == null).toString()))
    }

    fun nextMetadataCatalogPage(generation: String, section: String): String? {
        val pages = dao.metadataCatalogPages(generation, section).associateBy { it.cursor }
        var cursor = ""; val visited = mutableSetOf<String>()
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
        val catalog = generation.optJSONObject("stage_catalog")
        if (catalog != null) {
            require(dao.meta("stage_catalog_complete") == "true")
            // The sealed server generation has already revalidated this pinned revision.  The
            // terminal wire shape intentionally does not echo it; the staged generation is the
            // authoritative local binding.
        }
        val metadataCatalog = generation.optJSONObject("metadata")
        if (metadataCatalog != null)
            listOf("tags", "fields", "options").forEach { section ->
                require(dao.meta("metadata_catalog_${section}_complete") == "true")
            }
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
            val needsQualification = old != null &&
                (!old.noteRevisionsQualified || !old.stageRevisionsQualified || !detailsQualified(old) ||
                    (metadataCatalog != null && !metadataQualified(
                        old,
                        metadataCatalog.getString("catalog_revision"),
                        item.metadataRevision,
                    )))
            if (old == null || old.revision != item.revision || needsQualification) {
                val summary = component(id, item.person, "summary")
                val notes = component(id, item.person, "notes").second
                val tasks = component(id, item.person, "tasks").second
                val metadata = if (metadataCatalog != null) JSONObject(dao.pages(id, item.person, "metadata").singleOrNull()?.body ?: throw ProtocolFailure()) else null
                require(summary.first?.getString("id") == item.person)
                val modernSummary = summary.first!!
                val detailsQualified =
                    modernSummary.has("details_revision") &&
                        runCatching { revision(modernSummary.getString("details_revision")); validateDetailContacts(summary.second) }.isSuccess
                val notesQualified = notes.objects().all { it.has("revision") }
                val tasksQualified = tasks.objects().all { it.has("revision") }
                val metadataQualified = metadata?.let { page ->
                    val candidate = json("metadata_revision" to page.getString("metadata_revision"), "catalog_revision" to page.getString("catalog_revision"),
                        "tags" to page.getJSONArray("tags"), "values" to page.getJSONArray("values"), "complete" to page.getBoolean("complete"))
                    candidate.getString("metadata_revision") == item.metadataRevision &&
                        candidate.getString("catalog_revision") == metadataCatalog?.getString("catalog_revision") &&
                        runCatching { validateMetadataBaseline(candidate) }.isSuccess
                } ?: false
                if (catalog != null && modernSummary.has("stage_revision")) revision(modernSummary.getString("stage_revision"))
                if (old == null || revisionAtLeast(item.revision, old.revision))
                    dao.person(
                        PersonRow(
                            item.person,
                            item.revision,
                            summary.first!!.toString(),
                            orderedDetailContacts(summary.second).toString(),
                            notes.toString(),
                            tasks.toString(),
                            id,
                            seal.getString("evaluated_at"),
                            notesQualified,
                            catalog != null && modernSummary.has("stage_revision"),
                            detailsQualified,
                            metadataQualified,
                            metadata?.let { page -> json("metadata_revision" to page.getString("metadata_revision"), "catalog_revision" to page.getString("catalog_revision"), "tags" to page.getJSONArray("tags"), "values" to page.getJSONArray("values"), "complete" to page.getBoolean("complete")).toString() } ?: "",
                        )
                    )
            }
            val active = dao.person(item.person) ?: throw ProtocolFailure()
            pending
                .filter { it.person == item.person && it.status == "accepted" }
                .forEach { op ->
                    val receipt = JSONObject(op.receipt!!)
                    val covered =
                        if (op.kind == "change_person_stage") {
                            val currentStageRevision = JSONObject(active.summary).stringOrNull("stage_revision")
                            currentStageRevision != null && active.stageRevisionsQualified &&
                                revisionAtLeast(currentStageRevision, receipt.getString("committed_revision"))
                        } else if (op.kind == "update_person_details") {
                            val details = JSONObject(active.summary).stringOrNull("details_revision")
                            details != null && detailsQualified(active) && revisionAtLeast(details, receipt.getString("committed_revision"))
                        } else if (op.kind == "update_person_metadata") {
                            val current = JSONObject(active.metadata)
                            active.metadataRevisionsQualified &&
                                revisionAtLeast(current.getString("metadata_revision"), receipt.getString("committed_revision"))
                        } else revisionAtLeast(active.revision, receipt.getString("person_revision"))
                    if (covered)
                        dao.cover(op.id)
                }
        }
        if (catalog != null) {
            val staged = dao.stageCatalogPages(id)
                .flatMap { JSONObject(it.body).getJSONArray("items").objects() }
                .map { StageCatalogRow(uuid(it.getString("id")), it.getString("name"), it.getInt("position"), id, catalog.getString("revision")) }
            require(staged.map { it.id }.distinct().size == staged.size)
            dao.clearStageCatalog()
            dao.stageCatalog(staged)
            dao.meta(MetaRow("stage_catalog_revision", catalog.getString("revision")))
        }
        if (metadataCatalog != null) {
            fun rows(section: String) = dao.metadataCatalogPages(id, section)
                .flatMap { JSONObject(it.body).getJSONArray("items").objects() }
            val tags = rows("tags").map { MetadataTagRow(uuid(it.getString("id")), it.getString("name"), id, metadataCatalog.getString("catalog_revision")) }
            val fields = rows("fields").map { MetadataFieldRow(uuid(it.getString("id")), it.getString("label"), it.getString("field_type"), it.getInt("position"), if (it.isNull("archived_at")) null else it.getString("archived_at"), id, metadataCatalog.getString("catalog_revision")) }
            val options = rows("options").map { MetadataOptionRow(uuid(it.getString("id")), uuid(it.getString("field_id")), it.getString("label"), it.getInt("position"), if (it.isNull("archived_at")) null else it.getString("archived_at"), id, metadataCatalog.getString("catalog_revision")) }
            require(tags.map { it.id }.distinct().size == tags.size && fields.map { it.id }.distinct().size == fields.size && options.map { it.id }.distinct().size == options.size)
            dao.clearMetadataTags(); dao.clearMetadataFields(); dao.clearMetadataOptions()
            dao.metadataTags(tags); dao.metadataFields(fields); dao.metadataOptions(options)
            dao.meta(MetaRow("metadata_catalog_revision", metadataCatalog.getString("catalog_revision")))
        }
        // A contact receipt is never covered merely because its Person revision happened to be
        // unchanged. Reaching this point proves a fresh complete seal (including Today) was
        // committed after the receipt. Drop only its bounded local display record then.
        dao.contactDrafts()
            .filter { it.operation.isNotEmpty() && dao.operation(it.operation)?.status == "covered" }
            .forEach { dao.removeContactOperation(it.operation) }
        dao.stageDrafts()
            .filter { it.operation.isNotEmpty() && dao.operation(it.operation)?.status == "covered" }
            .forEach { dao.removeStageOperation(it.operation) }
        dao.profileDrafts().filter { it.operation.isNotEmpty() && dao.operation(it.operation)?.status == "covered" }
            .forEach { dao.removeProfileOperation(it.operation) }
        dao.metadataDrafts().filter { it.operation.isNotEmpty() && dao.operation(it.operation)?.status == "covered" }
            .forEach { dao.removeMetadataOperation(it.operation) }
        val selected = manifest.map { it.person }.toSet()
        var removalConflicts = 0
        for (old in dao.people().filter { it.id !in selected }) {
            // Selection removal cannot discard pending work or establish that an accepted action
            // was covered.
            if (
                pending.any { it.person == old.id } ||
                    dao.drafts().any { it.person == old.id } ||
                    dao.contactDrafts().any { it.person == old.id } ||
                    dao.profileDrafts().any { it.person == old.id } ||
                    dao.profileContexts().any { it.person == old.id }
            )
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
        // These are staging-only cursors. Keeping them after promotion is harmless to the
        // current downloader (which keys from generation), but it makes an accepted contact
        // receipt appear to retain pre-receipt reconciliation state after the fresh seal.
        dao.removeMeta("manifest_cursor")
        dao.removeMeta("manifest_complete")
        dao.removeMeta("stage_catalog_cursor")
        dao.removeMeta("stage_catalog_complete")
        listOf("tags", "fields", "options").forEach { section ->
            dao.removeMeta("metadata_catalog_${section}_cursor"); dao.removeMeta("metadata_catalog_${section}_complete")
        }
        dao.clearPages()
        dao.clearManifest()
        dao.clearStageCatalogPages()
        dao.clearMetadataCatalogPages()
    }
}
