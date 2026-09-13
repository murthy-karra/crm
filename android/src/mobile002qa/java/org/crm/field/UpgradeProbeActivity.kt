package org.crm.field

import android.app.Activity
import android.os.Bundle
import android.util.Log
import java.security.MessageDigest
import org.json.JSONArray
import org.json.JSONObject

/** Debug-QA-only, read-only inspection hook for the preserved Mobile 001 upgrade fixture. */
class UpgradeProbeActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Thread {
                val vault = DeviceVault(this, BuildConfig.VAULT_NAMESPACE)
                val registry = vault.registry()
                val account = registry.getString("account")
                val db = FieldDatabase.open(this, vault.accountDirectory(account), vault.databaseKey(vault.accountDirectory(account)))
                val dao = db.data()
                val person = dao.person("10000000-0000-4000-8000-000000000051")!!
                val operation = dao.operations().single()
                val digest = MessageDigest.getInstance("SHA-256").digest(operation.envelope.toByteArray()).joinToString("") { "%02x".format(it) }
                val note = JSONArray(person.notes).getJSONObject(0)
                Log.i(
                    "Mobile002Probe",
                    "UPGRADE_PROBE person=${person.id} qualified=${person.noteRevisionsQualified} note_revision=${note.optString("revision", "<none>")} operation=${operation.id} digest=$digest draft=${dao.draft("legacy-draft")?.payload}",
                )
                db.close()
                runOnUiThread { finish() }
            }
            .start()
    }
}
