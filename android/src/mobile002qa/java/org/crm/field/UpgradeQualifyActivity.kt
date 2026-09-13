package org.crm.field

import android.app.Activity
import android.os.Bundle
import android.util.Log
import org.json.JSONArray

/** QA fixture's deterministic equivalent of a fully staged same-Person note-page refetch. */
class UpgradeQualifyActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Thread {
                val vault = DeviceVault(this, BuildConfig.VAULT_NAMESPACE)
                val account = vault.registry().getString("account")
                val directory = vault.accountDirectory(account)
                val db = FieldDatabase.open(this, directory, vault.databaseKey(directory))
                val dao = db.data()
                db.runInTransaction {
                    val old = dao.person("10000000-0000-4000-8000-000000000051")!!
                    check(!old.noteRevisionsQualified && old.revision == "1")
                    val notes = JSONArray(old.notes)
                    notes.getJSONObject(0).put("revision", "1")
                    // One transaction replaces only this qualified component representation at
                    // the same Person revision; drafts and immutable envelopes are untouched.
                    dao.person(old.copy(notes = notes.toString(), noteRevisionsQualified = true))
                }
                val updated = dao.person("10000000-0000-4000-8000-000000000051")!!
                Log.i("Mobile002Probe", "UPGRADE_QUALIFIED person=${updated.id} person_revision=${updated.revision} qualified=${updated.noteRevisionsQualified} note_revision=${JSONArray(updated.notes).getJSONObject(0).getString("revision")} operation=${dao.operations().single().id} draft=${dao.draft("legacy-draft") != null}")
                db.close()
                runOnUiThread { finish() }
            }
            .start()
    }
}
