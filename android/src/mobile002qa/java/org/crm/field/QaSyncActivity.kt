package org.crm.field

import android.app.Activity
import android.os.Bundle
import android.util.Log
import kotlinx.coroutines.launch

/** QA-only trigger and inspection of the normal repository sync path. */
class QaSyncActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val app = application as FieldApplication
        app.repository.scope.launch {
            app.ready.await()
            app.repository.pause(false)
            app.repository.sync(manual = true)
            val vault = DeviceVault(this@QaSyncActivity, BuildConfig.VAULT_NAMESPACE)
            val account = vault.registry().getString("account")
            val dir = vault.accountDirectory(account)
            val db = FieldDatabase.open(this@QaSyncActivity, dir, vault.databaseKey(dir))
            db.data().operations().forEach { row ->
                Log.i("Mobile002Probe", "QA_SYNC operation=${row.id} kind=${row.kind} state=${row.status} receipt=${row.receipt}")
            }
            Log.i("Mobile002Probe", "QA_SYNC drafts=${db.data().drafts().size}")
            db.close()
            runOnUiThread { finish() }
        }
    }
}
