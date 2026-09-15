package org.crm.field

import java.time.Instant
import java.time.LocalDateTime
import java.time.OffsetDateTime
import java.time.ZoneId
import java.util.UUID
import org.json.JSONArray
import org.json.JSONObject

const val PROTOCOL = "mobile-v1"

fun json(vararg pairs: Pair<String, Any?>) =
    JSONObject().apply { pairs.forEach { put(it.first, it.second ?: JSONObject.NULL) } }

fun JSONObject.stringOrNull(key: String): String? = if (isNull(key)) null else getString(key)

fun JSONArray.objects(): List<JSONObject> = (0 until length()).map { getJSONObject(it) }

fun uuid(value: String): String =
    UUID.fromString(value).toString().also { require(it == value.lowercase()) }

/** JSON numbers must be integral and exactly in range; org.json numeric getters coerce strings. */
fun strictJsonInt(value: Any, minimum: Int = Int.MIN_VALUE, maximum: Int = Int.MAX_VALUE): Int {
    require(value is Number)
    val number = java.math.BigDecimal(value.toString()).intValueExact()
    require(number in minimum..maximum)
    return number
}

fun detailImportOrder(item: JSONObject): Int? {
    require(item.has("import_order"))
    return if (item.isNull("import_order")) null else strictJsonInt(item.get("import_order"))
}

fun validateContactItems(contacts: JSONArray) {
    val ids = mutableSetOf<String>()
    contacts.objects().forEach { item ->
        require(ids.add(uuid(item.get("id") as String)))
        require(item.get("kind") in setOf("email", "phone"))
        require((item.get("value") as String).isNotEmpty())
    }
}

/** Full server metadata qualifies a baseline; legacy value lengths remain readable/frozen. */
fun validateDetailContacts(contacts: JSONArray) {
    validateContactItems(contacts)
    contacts.objects().forEach { item ->
        detailImportOrder(item)
        Instant.parse(item.get("created_at") as String)
    }
}

/** Canonical protocol revision: a positive signed 64-bit integer, rendered without leading zeroes. */
fun revision(value: String): String =
    value.also {
        require(it.matches(Regex("[1-9][0-9]*")))
        require(it.toLongOrNull() != null)
    }

fun revisionAtLeast(left: String, right: String) =
    revision(left).toLong() >= revision(right).toLong()

/**
 * Resolve a wall-clock choice without silently choosing a daylight-saving offset. An empty list
 * is a nonexistent local time; two entries require the person using the app to choose explicitly.
 */
fun resolveReportedLocal(local: LocalDateTime, zone: ZoneId): List<OffsetDateTime> =
    zone.rules.getValidOffsets(local).map { OffsetDateTime.of(local, it) }

/** Pure clock boundary: no wall-clock extension, reboot or backward-time recovery. */
data class LeaseClock(val boot: Int, val elapsed: Long, val duration: Long, val lastWall: Long) {
    fun usable(currentBoot: Int, currentElapsed: Long, currentWall: Long): Boolean =
        boot >= 0 &&
            currentBoot == boot &&
            currentElapsed >= elapsed &&
            duration in 1..604_800_000 &&
            currentElapsed - elapsed < duration &&
            currentWall >= lastWall - 2_000
}

interface DeviceClock {
    fun boot(): Int

    fun elapsed(): Long

    fun wall(): Long
}

class AccessLocked : Exception("Online authorization required; saved work remains protected")

class StorageFailure : Exception("Device save failed; keep the draft open and free storage")

class ProtocolFailure : Exception("Response did not match this device's current authorized context")

class ApiFailure(val status: Int, val code: String, val retryAfterSeconds: Long = 30) :
    Exception(code)

class Binding(
    val actor: String,
    val org: String,
    val context: String,
    val installation: String,
    val bootstrap: String,
) {
    fun supportsEdits(): Boolean {
        val capabilities = JSONObject(bootstrap).getJSONArray("capabilities")
        return listOf("edit_note", "update_task", "note_revisions").all { wanted ->
            (0 until capabilities.length()).any { capabilities.getString(it) == wanted }
        }
    }

    fun supportsContactLogging(): Boolean {
        val capabilities = JSONObject(bootstrap).getJSONArray("capabilities")
        return (0 until capabilities.length()).any {
            capabilities.getString(it) == "log_contact_attempt"
        }
    }

    fun supportsStageChanges(): Boolean {
        val capabilities = JSONObject(bootstrap).getJSONArray("capabilities")
        return listOf("change_person_stage", "stage_revisions", "stage_catalog").all { wanted ->
            (0 until capabilities.length()).any { capabilities.getString(it) == wanted }
        }
    }

    fun supportsPersonDetails(): Boolean {
        val capabilities = JSONObject(bootstrap).getJSONArray("capabilities")
        return listOf("update_person_details", "details_revisions").all { wanted ->
            (0 until capabilities.length()).any { capabilities.getString(it) == wanted }
        }
    }

    /** Metadata is deliberately an all-or-nothing feature. Older bootstrap responses remain
     * readable, but can never make an omitted tag/value section look like an empty baseline. */
    fun supportsMetadata(): Boolean {
        val capabilities = JSONObject(bootstrap).getJSONArray("capabilities")
        return listOf("update_person_metadata", "metadata_revisions", "metadata_catalog").all { wanted ->
            (0 until capabilities.length()).any { capabilities.getString(it) == wanted }
        }
    }
    companion object {
        fun parse(value: JSONObject, actor: String, org: String, installation: String): Binding {
            if (value.getString("protocol") != PROTOCOL)
                throw ApiFailure(409, "protocol_unsupported")
            if (
                uuid(value.getString("actor_user_id")) != actor ||
                    uuid(value.getString("organization_id")) != org ||
                    uuid(value.getString("installation_id")) != installation
            )
                throw ProtocolFailure()
            // All three names form one edit feature.  Their absence is valid for an old server;
            // the repository keeps already-saved edit input rather than degrading it.
            revision(value.getString("workspace_revision"))
            val capabilities = value.getJSONArray("capabilities")
            if (
                !listOf("add_note", "create_task", "complete_task", "reconciliation").all { item ->
                    (0 until capabilities.length()).any { capabilities.getString(it) == item }
                }
            )
                throw ProtocolFailure()
            Instant.parse(value.getString("server_time"))
            Instant.parse(value.getString("offline_access_expires_at"))
            return Binding(
                actor,
                org,
                uuid(value.getString("context_id")),
                installation,
                value.toString(),
            )
        }
    }
}

fun errorMessage(code: String): String =
    when (code) {
        "revision_conflict" ->
            "Changed elsewhere. Review the current version before preparing a new saved edit."
        "operation_payload_mismatch" ->
            "Saved operation could not be matched. Original input is preserved for review."
        "not_found" ->
            "This item is unavailable. The saved action is preserved; it will not be recreated."
        "forbidden" -> "This action is no longer permitted. Its input is preserved."
        "contact_time_in_future" ->
            "The reported contact time is later than the server clock. Correct the time to create a new saved contact; the original is preserved."
        "invalid_stage" -> "That stage is no longer available. Choose a stage from the current catalog in a new saved proposal."
        "invalid_person_details" -> "Those profile changes could not be accepted. The saved proposal remains available for review."
        "catalog_revision_conflict" ->
            "The tag or field catalog changed. Review the current catalog before preparing a new saved edit."
        "invalid_metadata" ->
            "A selected tag, field, or choice is no longer available. The saved proposal remains available for review."
        "dependency_pending" -> "Waiting for the original task creation."
        "generation_changed",
        "generation_expired" ->
            "Download changed or expired. The previous complete cache is still available."
        "protocol_unsupported" -> "App update required. Saved work remains protected."
        "over_limit" -> "Download is incomplete because a supported limit was reached."
        "mobile_capacity" -> "The server is busy with downloads. Retry after the pause."
        "mobile_unavailable" -> "Mobile sync is unavailable. Saved operation IDs are preserved."
        "storage" -> "Device storage could not be committed. Nothing was marked saved."
        else -> "Sync is paused. Saved work remains on this device; retry when connected."
    }
