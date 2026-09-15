package org.crm.field

import java.net.HttpURLConnection
import java.net.URI
import java.net.URL
import java.net.URLEncoder
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject

/** Preserve actual response bytes until the caller has enforced its route's byte budget. */
class HttpResult(val status: Int, bytes: ByteArray, val cookie: String?, val retryAfter: Long) {
    private val wire = bytes.copyOf()
    val wireByteCount: Int get() = wire.size
    val body: JSONObject by lazy { if (wire.isEmpty()) JSONObject() else JSONObject(String(wire, Charsets.UTF_8)) }
}

fun interface Transport {
    suspend fun request(
        origin: String,
        method: String,
        path: String,
        cookie: String,
        context: String?,
        body: String?,
    ): HttpResult
}

class HttpTransport : Transport {
    override suspend fun request(
        origin: String,
        method: String,
        path: String,
        cookie: String,
        context: String?,
        body: String?,
    ): HttpResult =
        withContext(Dispatchers.IO) {
            val uri = URI(origin)
            require(
                uri.rawUserInfo == null &&
                    uri.rawQuery == null &&
                    uri.rawFragment == null &&
                    uri.path.isNullOrEmpty()
            )
            require(
                uri.scheme == "https" ||
                    BuildConfig.DEBUG &&
                        uri.scheme == "http" &&
                        uri.host in setOf("10.0.2.2", "127.0.0.1", "localhost")
            )
            require(path.startsWith("/api/") && !path.contains("\r") && !path.contains("\n"))
            val connection = URL(origin + path).openConnection() as HttpURLConnection
            try {
                connection.instanceFollowRedirects = false
                connection.useCaches = false
                connection.connectTimeout = 20_000
                connection.readTimeout = 20_000
                connection.requestMethod = method
                connection.setRequestProperty("Accept", "application/json")
                connection.setRequestProperty("Cache-Control", "no-store")
                if (cookie.isNotEmpty()) connection.setRequestProperty("Cookie", cookie)
                if (context != null) connection.setRequestProperty("X-Mobile-Context", context)
                if (body != null) {
                    connection.doOutput = true
                    connection.setRequestProperty("Content-Type", "application/json")
                    connection.outputStream.use { it.write(body.toByteArray()) }
                }
                val status = connection.responseCode
                val stream =
                    if (status in 200..299) connection.inputStream else connection.errorStream
                val bytes =
                    stream?.use { input ->
                        val out = java.io.ByteArrayOutputStream()
                        val buffer = ByteArray(8192)
                        while (true) {
                            val count = input.read(buffer)
                            if (count < 0) break
                            out.write(buffer, 0, count)
                            if (out.size() > 1_048_576) throw ProtocolFailure()
                        }
                        out.toByteArray()
                    } ?: ByteArray(0)
                val updated =
                    connection.headerFields.entries
                        .firstOrNull { it.key.equals("Set-Cookie", true) }
                        ?.value
                        ?.firstOrNull()
                        ?.substringBefore(';')
                if (
                    path.startsWith("/api/mobile/") &&
                        connection.getHeaderField("Cache-Control")?.contains("no-store") != true
                )
                    throw ProtocolFailure()
                HttpResult(
                    status,
                    bytes,
                    updated,
                    connection.getHeaderField("Retry-After")?.toLongOrNull()?.coerceIn(1, 3600)
                        ?: 30,
                )
            } finally {
                connection.disconnect()
            }
        }
}

class FieldApi(
    val origin: String,
    var cookie: String = "",
    val transport: Transport = HttpTransport(),
) {
    suspend fun call(
        method: String,
        path: String,
        context: String? = null,
        body: String? = null,
        maximumBytes: Int = 1_048_576,
    ): JSONObject {
        val response = transport.request(origin, method, path, cookie, context, body)
        if (response.wireByteCount > maximumBytes) throw ProtocolFailure()
        if (response.status !in 200..299)
            throw ApiFailure(
                response.status,
                response.body.optString("error", "unavailable"),
                response.retryAfter,
            )
        response.cookie?.let { cookie = it }
        return response.body
    }

    suspend fun bootstrap(installation: String) =
        call(
            "POST",
            "/api/mobile/v1/bootstrap",
            body = json("protocol" to PROTOCOL, "installation_id" to installation).toString(),
        )
    suspend fun searchPeople(binding: Binding, term: String): JSONObject { val trimmed = term.trim(); require(trimmed.isNotEmpty() && trimmed.codePointCount(0, trimmed.length) <= 200 && trimmed.toByteArray().size <= 800); val result = call("POST", "/api/mobile/v1/people/search", binding.context, json("term" to trimmed).toString(), maximumBytes = 131_072); if (result.optString("context_id") != binding.context) throw ProtocolFailure(); val items = result.optJSONArray("items") ?: throw ProtocolFailure(); if (items.length() > 25 || !result.has("has_more")) throw ProtocolFailure(); return result }

    suspend fun operation(binding: Binding, row: OperationRow) =
        call("POST", "/api/mobile/v1/operations", binding.context, row.envelope)

    suspend fun receipt(binding: Binding, id: String) =
        call("GET", "/api/mobile/v1/operations/${uuid(id)}", binding.context)

    suspend fun currentNote(binding: Binding, person: String, note: String) =
        call("GET", "/api/mobile/v1/people/${uuid(person)}/notes/${uuid(note)}", binding.context)

    suspend fun currentTask(binding: Binding, person: String, task: String) =
        call("GET", "/api/mobile/v1/people/${uuid(person)}/tasks/${uuid(task)}", binding.context)

    suspend fun currentStage(binding: Binding, person: String) =
        call("GET", "/api/mobile/v1/people/${uuid(person)}/stage", binding.context)

    suspend fun page(path: String, binding: Binding, cursor: String, maximumBytes: Int = 1_048_576) =
        call(
            "GET",
            path + if (cursor.isEmpty()) "" else "?cursor=" + URLEncoder.encode(cursor, "UTF-8"),
            binding.context,
            maximumBytes = maximumBytes,
        )
}
