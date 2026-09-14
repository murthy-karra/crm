package org.crm.field

import android.app.Activity
import android.os.Bundle
import android.util.Log
import java.security.MessageDigest

/** Read-only post-install check: schema 6 must retain every schema-5 protected artifact and key. */
class Mobile005UpgradeProbeActivity : Activity() {
    override fun onCreate(state: Bundle?) { super.onCreate(state); Thread {
        try {
            val vault = DeviceVault(this, BuildConfig.VAULT_NAMESPACE); val registry = vault.registry()
            val directory = vault.accountDirectory(registry.getString("account")); val key = vault.databaseKey(directory)
            val db = FieldDatabase.open(this, directory, key); val dao = db.data()
            val queued = requireNotNull(dao.operation("30000000-0000-4000-8000-000000000051"))
            val accepted = requireNotNull(dao.operation("30000000-0000-4000-8000-000000000052"))
            val draft = requireNotNull(dao.draft("legacy-draft")); val person = requireNotNull(dao.person("10000000-0000-4000-8000-000000000051"))
            check(!person.detailsRevisionsQualified); check(dao.profileDrafts().isEmpty()); check(dao.profileContexts().isEmpty())
            Log.i("Mobile005Upgrade", "V6_PROBE key=${sha(key)} queued_id=${queued.id} queued_sha=${sha(queued.envelope.toByteArray())} receipt_id=${accepted.id} receipt_sha=${sha(requireNotNull(accepted.receipt).toByteArray())} draft_id=${draft.id} draft_sha=${sha(draft.payload.toByteArray())} details_qualified=${person.detailsRevisionsQualified}")
            db.close()
        } catch (e: Exception) { Log.e("Mobile005Upgrade", "V6_PROBE_FAILED ${e.javaClass.simpleName}: ${e.message}", e) } finally { runOnUiThread { finish() } }
    }.start() }
    private fun sha(v: ByteArray) = MessageDigest.getInstance("SHA-256").digest(v).joinToString("") { "%02x".format(it) }
}
