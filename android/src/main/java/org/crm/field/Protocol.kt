package org.crm.field

import java.math.BigInteger
import java.time.Instant
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

fun revision(value: String): String = value.also { require(it.matches(Regex("[1-9][0-9]*"))) }

fun revisionAtLeast(left: String, right: String) =
    BigInteger(revision(left)) >= BigInteger(revision(right))

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
            "Task changed elsewhere. Review the latest task before choosing a new completion."
        "operation_payload_mismatch" ->
            "Saved operation could not be matched. Original input is preserved for review."
        "not_found" ->
            "This item is unavailable. The saved action is preserved; it will not be recreated."
        "forbidden" -> "This action is no longer permitted. Its input is preserved."
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
