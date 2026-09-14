package org.crm.field

import org.json.JSONObject

/** Actual bytes of a compact synthetic response, using the production lazy wire decoder. */
fun testHttpResult(status: Int, body: JSONObject, cookie: String?, retryAfter: Long) =
    HttpResult(status, body.toString().toByteArray(Charsets.UTF_8), cookie, retryAfter)
