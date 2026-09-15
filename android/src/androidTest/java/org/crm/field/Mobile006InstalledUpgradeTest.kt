package org.crm.field

import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

/** Checks the actual schema-6 installed package after `adb install -r` of Mobile006. */
class Mobile006InstalledUpgradeTest {
    @Test fun schemaSixProtectedStoreUpgradesInPlaceWithoutChangingOldBytesOrKey() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        assertEquals(BuildConfig.APPLICATION_ID, context.packageName)
        val vault = DeviceVault(context, BuildConfig.VAULT_NAMESPACE)
        val before = JSONObject(File(context.filesDir, "mobile006-upgrade-before.json").readText())
        val directory = vault.accountDirectory(vault.registry().getString("account")); val key = vault.databaseKey(directory)
        val db = FieldDatabase.open(context, directory, key); val dao = db.data()
        val version = db.openHelper.readableDatabase.query("PRAGMA user_version").use { it.moveToFirst(); it.getInt(0) }
        assertEquals(8, version); assertEquals(before.getInt("key_bytes"), key.size)
        val operation = requireNotNull(dao.operation("30000000-0000-4000-8000-000000000096"))
        assertEquals(before.getString("envelope"), operation.envelope)
        assertEquals(before.getString("draft"), requireNotNull(dao.draft("schema-six-draft")).payload)
        val person = requireNotNull(dao.person("10000000-0000-4000-8000-000000000096"))
        assertFalse(person.metadataRevisionsQualified); assertEquals("", person.metadata)
        assertTrue(dao.metadataDrafts().isEmpty()); assertTrue(dao.metadataContexts().isEmpty())
        db.close()
    }
}
