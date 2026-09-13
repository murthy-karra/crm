package org.crm.field

import android.app.Activity
import android.os.Bundle
import android.util.Log
import java.security.MessageDigest
import org.json.JSONObject

/**
 * Read-only installed-store check after the historical Mobile 003 APK is replaced in-place.
 * It deliberately logs only synthetic IDs and hashes, never protected payload content.
 */
class Mobile004UpgradeProbeActivity : Activity() {
    override fun onCreate(state: Bundle?) {
        super.onCreate(state)
        Thread {
            try {
                val vault = DeviceVault(this, BuildConfig.VAULT_NAMESPACE)
                val registry = vault.registry()
                val directory = vault.accountDirectory(registry.getString("account"))
                val key = vault.databaseKey(directory)
                val database = FieldDatabase.open(this, directory, key)
                val dao = database.data()
                val queued = dao.operations().single { it.status == "queued" }
                val accepted = dao.operations().single { it.status == "accepted" }
                val receipt = requireNotNull(accepted.receipt)
                val draft = requireNotNull(dao.draft("legacy-draft"))
                val cached = requireNotNull(dao.person(PERSON))
                check(JSONObject(queued.envelope).getString("operation_id") == queued.id)
                check(!cached.stageRevisionsQualified)
                check(dao.stageDrafts().isEmpty())
                Log.i(
                    "Mobile004Upgrade",
                    "V5_PROBE key=${sha(key)} queued_id=${queued.id} queued_sha=${sha(queued.envelope.toByteArray())} " +
                        "receipt_id=${accepted.id} receipt_sha=${sha(receipt.toByteArray())} " +
                        "draft_id=${draft.id} draft_sha=${sha(draft.payload.toByteArray())} " +
                        "cache_id=${cached.id} cache_sha=${sha(cached.summary.toByteArray())} " +
                        "stage_qualified=${cached.stageRevisionsQualified}",
                )
                database.close()
            } catch (error: Exception) {
                Log.e("Mobile004Upgrade", "V5_PROBE_FAILED ${error.javaClass.simpleName}: ${error.message}", error)
            } finally {
                runOnUiThread { finish() }
            }
        }.start()
    }

    private fun sha(bytes: ByteArray): String =
        MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }

    private companion object {
        const val PERSON = "10000000-0000-4000-8000-000000000051"
    }
}
