package org.crm.field

import android.app.Activity
import android.os.Bundle
import android.util.Log
import java.security.MessageDigest

/** Read-only QA probe for the populated Mobile 002 schema-3 → Mobile 003 schema-4 install. */
class Mobile003UpgradeProbeActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Thread {
                val vault = DeviceVault(this, BuildConfig.VAULT_NAMESPACE)
                val account = vault.registry().getString("account")
                val directory = vault.accountDirectory(account)
                val db = FieldDatabase.open(this, directory, vault.databaseKey(directory))
                val dao = db.data()
                val person = dao.person("10000000-0000-4000-8000-000000000051")!!
                val operations = dao.operations()
                val digest =
                    operations.joinToString(",") { row ->
                        val hash =
                            MessageDigest.getInstance("SHA-256")
                                .digest(row.envelope.toByteArray())
                                .joinToString("") { "%02x".format(it) }
                        "${row.kind}:${row.id}:$hash:${row.status}"
                    }
                Log.i(
                    "Mobile003Upgrade",
                    "V4_PROBE person=${person.id} revision=${person.revision} qualified=${person.noteRevisionsQualified} " +
                        "operations=$digest draft=${dao.draft("legacy-draft")?.payload} " +
                        "contacts=${dao.contactDrafts().size}",
                )
                db.close()
                runOnUiThread { finish() }
            }
            .start()
    }
}
