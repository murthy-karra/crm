package org.crm.field

import android.app.Activity
import android.os.Bundle
import android.util.Log
import kotlinx.coroutines.launch

/** QA-only inspection trigger; normal repository code remains the only sync path. */
class Mobile004ProbeActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val app = application as FieldApplication
        app.repository.scope.launch {
            app.ready.await()
            app.repository.sync(manual = true)
            Log.i("Mobile004Probe", "MOBILE004_SYNC pending=${app.repository.ui.value.pendingCount} stages=${app.repository.ui.value.stageCatalog.size}")
            runOnUiThread { finish() }
        }
    }
}
