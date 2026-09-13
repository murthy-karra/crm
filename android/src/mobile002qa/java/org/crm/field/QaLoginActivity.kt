package org.crm.field

import android.app.Activity
import android.os.Bundle
import android.util.Log
import kotlinx.coroutines.launch

/** QA-only bridge for live API cases when emulator text injection is unreliable. */
class QaLoginActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val email = intent.getStringExtra("email")
        val password = intent.getStringExtra("password")
        if (email.isNullOrBlank() || password.isNullOrBlank()) {
            Log.w("Mobile002Probe", "QA_LOGIN missing_credentials")
            finish()
            return
        }
        val app = application as FieldApplication
        app.repository.scope.launch {
            app.ready.await()
            app.repository.login(email, password)
            Log.i("Mobile002Probe", "QA_LOGIN completed")
            runOnUiThread { finish() }
        }
    }
}
