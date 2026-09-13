package org.crm.field

import java.net.HttpURLConnection
import java.net.URI
import java.net.URL
import java.net.URLEncoder
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject

class HttpResult(val status: Int, val body: JSONObject, val cookie: String?, val retryAfter: Long)

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
                val response = if (bytes.isEmpty()) JSONObject() else JSONObject(String(bytes))
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
                    response,
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
    ): JSONObject {
        val response = transport.request(origin, method, path, cookie, context, body)
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

    suspend fun operation(binding: Binding, row: OperationRow) =
        call("POST", "/api/mobile/v1/operations", binding.context, row.envelope)

    suspend fun receipt(binding: Binding, id: String) =
        call("GET", "/api/mobile/v1/operations/${uuid(id)}", binding.context)

    suspend fun page(path: String, binding: Binding, cursor: String) =
        call(
            "GET",
            path + if (cursor.isEmpty()) "" else "?cursor=" + URLEncoder.encode(cursor, "UTF-8"),
            binding.context,
        )
}
