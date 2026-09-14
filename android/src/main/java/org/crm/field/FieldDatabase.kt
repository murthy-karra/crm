package org.crm.field

import android.content.Context
import androidx.room.*
import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase
import java.io.File
import net.zetetic.database.sqlcipher.SupportOpenHelperFactory

@Entity(tableName = "metadata") data class MetaRow(@PrimaryKey val key: String, val value: String)

@Entity(tableName = "people")
data class PersonRow(
    @PrimaryKey val id: String,
    val revision: String,
    val summary: String,
    val contacts: String,
    val notes: String,
    val tasks: String,
    val generation: String,
    val evaluatedAt: String,
    @ColumnInfo(defaultValue = "0") val noteRevisionsQualified: Boolean = false,
    /** A v4 cache has no stage baseline even when its broad revision is unchanged. */
    @ColumnInfo(defaultValue = "0") val stageRevisionsQualified: Boolean = false,
    /** A v5 cache has no complete contact/details baseline even at the same broad revision. */
    @ColumnInfo(defaultValue = "0") val detailsRevisionsQualified: Boolean = false,
)

data class PersonCard(val id: String, val revision: String, val summary: String)

@Entity(tableName = "drafts")
data class DraftRow(
    @PrimaryKey val id: String,
    val person: String,
    val kind: String,
    val payload: String,
    val revision: Long,
    /** Immutable server record at the start of an edit; empty only for Mobile 001 drafts. */
    @ColumnInfo(defaultValue = "''") val baseline: String = "",
    /** Canonical target and expected record revision; never inferred from a later cache. */
    @ColumnInfo(defaultValue = "''") val target: String = "",
    @ColumnInfo(defaultValue = "'draft'") val state: String = "draft",
)

/**
 * Mobile 003 owns contact input separately from the legacy generic draft table.  That keeps
 * Mobile 001/002 JSON payloads and outbox envelopes immutable through the upgrade while a
 * contact draft can retain its locally resolved offset and an attention state.
 */
@Entity(tableName = "contact_drafts")
data class ContactDraftRow(
    @PrimaryKey val id: String,
    val person: String,
    val channel: String,
    val outcome: String,
    /** RFC3339 instant with the explicit offset selected by the agent. */
    val occurredAt: String,
    /** The offset is display metadata; occurredAt remains the wire source of truth. */
    val resolvedOffset: String,
    val revision: Long,
    /** Empty until this exact draft has published one immutable outbox operation. */
    val operation: String = "",
    val state: String = "draft",
    @ColumnInfo(defaultValue = "''") val lastError: String = "",
)

/** One local proposal.  `operation` is populated atomically with the immutable outbox row. */
@Entity(tableName = "stage_drafts")
data class StageDraftRow(
    @PrimaryKey val id: String,
    val person: String,
    val baselineStageId: String,
    val baselineStageName: String,
    val baselineRevision: String,
    val proposedStageId: String,
    val proposedStageName: String,
    val revision: Long,
    val operation: String = "",
    val state: String = "draft",
    @ColumnInfo(defaultValue = "''") val lastError: String = "",
)

/** Comparison material is deliberately separate from reconciled Person data. */
@Entity(tableName = "stage_context")
data class StageContextRow(
    @PrimaryKey val operation: String,
    val person: String,
    val baseline: String,
    val proposal: String,
    val current: String = "",
    val editorRevision: Long = 0,
)

@Entity(tableName = "stage_catalog")
data class StageCatalogRow(
    @PrimaryKey val id: String,
    val name: String,
    val position: Int,
    val generation: String,
    val revision: String,
)

@Entity(tableName = "stage_catalog_pages", primaryKeys = ["generation", "cursor"])
data class StageCatalogPageRow(val generation: String, val cursor: String, val body: String)

/** Protected comparison material.  It is not a reconciliation component or cache promotion. */
@Entity(tableName = "edit_context")
data class EditContextRow(
    @PrimaryKey val operation: String,
    val person: String,
    val kind: String,
    val target: String,
    val baseline: String,
    val current: String = "",
    val editorRevision: Long = 0,
)

/** A protected CAS draft for an aggregate name/contact proposal. */
@Entity(tableName = "profile_drafts")
data class ProfileDraftRow(
    @PrimaryKey val id: String,
    val person: String,
    val baseline: String,
    val proposal: String,
    val detailsRevision: String,
    val revision: Long,
    val operation: String = "",
    val state: String = "draft",
    @ColumnInfo(defaultValue = "''") val lastError: String = "",
)

/** A bounded current-profile traversal is editor comparison material, never cache data. */
@Entity(tableName = "profile_context")
data class ProfileContextRow(
    @PrimaryKey val operation: String,
    val person: String,
    val baseline: String,
    val proposal: String,
    val current: String = "",
    val editorRevision: Long = 0,
)

@Entity(tableName = "operations")
data class OperationRow(
    @PrimaryKey val id: String,
    val person: String,
    val kind: String,
    val envelope: String,
    val createdAt: Long,
    val status: String = "queued",
    val receipt: String? = null,
    val attempts: Int = 0,
    val retryAt: Long = 0,
    @ColumnInfo(defaultValue = "''") val lastError: String = "",
)

@Entity(tableName = "manifest", primaryKeys = ["generation", "person"])
data class ManifestRow(val generation: String, val person: String, val revision: String)

@Entity(tableName = "pages", primaryKeys = ["generation", "person", "section", "cursor"])
data class PageRow(
    val generation: String,
    val person: String,
    val section: String,
    val cursor: String,
    val body: String,
)

@Entity(tableName = "pins") data class PinRow(@PrimaryKey val person: String)

@Dao
interface FieldDao {
    @Query("SELECT value FROM metadata WHERE `key`=:key") fun meta(key: String): String?

    @Insert(onConflict = OnConflictStrategy.REPLACE) fun meta(row: MetaRow)

    @Query("DELETE FROM metadata WHERE `key`=:key") fun removeMeta(key: String)

    @Query("SELECT id,revision,summary FROM people ORDER BY id") fun people(): List<PersonCard>

    @Query("SELECT * FROM people WHERE id=:id") fun person(id: String): PersonRow?

    @Insert(onConflict = OnConflictStrategy.REPLACE) fun person(row: PersonRow)

    @Query("DELETE FROM people WHERE id=:id") fun removePerson(id: String)

    @Query("SELECT * FROM drafts WHERE id=:id") fun draft(id: String): DraftRow?

    @Query("SELECT * FROM drafts ORDER BY id") fun drafts(): List<DraftRow>

    @Insert(onConflict = OnConflictStrategy.REPLACE) fun draft(row: DraftRow)

    @Query("DELETE FROM drafts WHERE id=:id") fun removeDraft(id: String)

    @Query("SELECT * FROM contact_drafts WHERE id=:id") fun contactDraft(id: String): ContactDraftRow?

    @Query("SELECT * FROM contact_drafts ORDER BY id") fun contactDrafts(): List<ContactDraftRow>

    @Query("SELECT * FROM stage_drafts WHERE id=:id") fun stageDraft(id: String): StageDraftRow?

    @Query("SELECT * FROM stage_drafts ORDER BY id") fun stageDrafts(): List<StageDraftRow>

    @Insert(onConflict = OnConflictStrategy.ABORT) fun insertStageDraft(row: StageDraftRow)

    @Query("UPDATE stage_drafts SET proposedStageId=:stageId,proposedStageName=:stageName,revision=:revision,state='draft',lastError='' WHERE id=:id AND revision=:expectedRevision AND operation='' ")
    fun updateStageDraft(id: String, stageId: String, stageName: String, revision: Long, expectedRevision: Long): Int

    @Query("UPDATE stage_drafts SET operation=:operation,state='saved',lastError='' WHERE id=:id AND operation='' ")
    fun saveStageOperation(id: String, operation: String): Int

    @Query("UPDATE stage_drafts SET state=:state,lastError=:error WHERE operation=:operation")
    fun stageState(operation: String, state: String, error: String)

    @Query("DELETE FROM stage_drafts WHERE operation=:operation") fun removeStageOperation(operation: String)

    @Query("SELECT * FROM stage_context WHERE operation=:operation") fun stageContext(operation: String): StageContextRow?

    @Query("SELECT * FROM stage_context ORDER BY operation") fun stageContexts(): List<StageContextRow>

    @Insert(onConflict = OnConflictStrategy.ABORT) fun stageContext(row: StageContextRow)

    @Query("UPDATE stage_context SET current=:current,editorRevision=:editorRevision WHERE operation=:operation")
    fun currentStageContext(operation: String, current: String, editorRevision: Long)

    @Query("DELETE FROM stage_context WHERE operation=:operation") fun removeStageContext(operation: String)

    @Insert(onConflict = OnConflictStrategy.ABORT) fun insertContactDraft(row: ContactDraftRow)

    @Query(
        "UPDATE contact_drafts SET channel=:channel,outcome=:outcome,occurredAt=:occurredAt,resolvedOffset=:resolvedOffset,revision=:revision,state='draft',lastError='' WHERE id=:id AND revision=:expectedRevision AND operation=''"
    )
    fun updateContactDraft(
        id: String,
        channel: String,
        outcome: String,
        occurredAt: String,
        resolvedOffset: String,
        revision: Long,
        expectedRevision: Long,
    ): Int

    @Query(
        "UPDATE contact_drafts SET operation=:operation,state='saved',lastError='' WHERE id=:id AND operation=''"
    )
    fun saveContactOperation(id: String, operation: String): Int

    @Query("UPDATE contact_drafts SET state=:state,lastError=:error WHERE operation=:operation")
    fun contactState(operation: String, state: String, error: String)

    @Query("DELETE FROM contact_drafts WHERE operation=:operation") fun removeContactOperation(operation: String)

    @Query("SELECT * FROM edit_context WHERE operation=:operation") fun editContext(operation: String): EditContextRow?

    @Query("SELECT * FROM edit_context ORDER BY operation") fun editContexts(): List<EditContextRow>

    @Insert(onConflict = OnConflictStrategy.ABORT) fun editContext(row: EditContextRow)

    @Query("UPDATE edit_context SET current=:current,editorRevision=:editorRevision WHERE operation=:operation")
    fun currentEditContext(operation: String, current: String, editorRevision: Long)

    @Query("DELETE FROM edit_context WHERE operation=:operation") fun removeEditContext(operation: String)

    @Query("SELECT * FROM profile_drafts WHERE id=:id") fun profileDraft(id: String): ProfileDraftRow?
    @Query("SELECT * FROM profile_drafts ORDER BY id") fun profileDrafts(): List<ProfileDraftRow>
    @Insert(onConflict = OnConflictStrategy.ABORT) fun insertProfileDraft(row: ProfileDraftRow)
    @Query("UPDATE profile_drafts SET proposal=:proposal,revision=:revision,state='draft',lastError='' WHERE id=:id AND revision=:expectedRevision AND operation='' ")
    fun updateProfileDraft(id: String, proposal: String, revision: Long, expectedRevision: Long): Int
    @Query("UPDATE profile_drafts SET operation=:operation,state='saved',lastError='' WHERE id=:id AND operation='' ")
    fun saveProfileOperation(id: String, operation: String): Int
    @Query("UPDATE profile_drafts SET state=:state,lastError=:error WHERE operation=:operation") fun profileState(operation: String, state: String, error: String)
    @Query("DELETE FROM profile_drafts WHERE operation=:operation") fun removeProfileOperation(operation: String)
    @Query("SELECT * FROM profile_context WHERE operation=:operation") fun profileContext(operation: String): ProfileContextRow?
    @Query("SELECT * FROM profile_context ORDER BY operation") fun profileContexts(): List<ProfileContextRow>
    @Insert(onConflict = OnConflictStrategy.ABORT) fun profileContext(row: ProfileContextRow)
    @Query("UPDATE profile_context SET current=:current,editorRevision=:editorRevision WHERE operation=:operation") fun currentProfileContext(operation: String, current: String, editorRevision: Long)
    @Query("DELETE FROM profile_context WHERE operation=:operation") fun removeProfileContext(operation: String)

    @Insert(onConflict = OnConflictStrategy.ABORT) fun operation(row: OperationRow)

    @Query("SELECT * FROM operations ORDER BY createdAt,id") fun operations(): List<OperationRow>

    @Query("SELECT * FROM operations WHERE id=:id") fun operation(id: String): OperationRow?

    @Query(
        "UPDATE operations SET status=:status,attempts=:attempts,retryAt=:retryAt,lastError=:error WHERE id=:id"
    )
    fun operationState(id: String, status: String, attempts: Int, retryAt: Long, error: String)

    @Query("UPDATE operations SET status='superseded', retryAt=0, lastError='revision_conflict' WHERE id=:id AND status='attention'")
    fun supersede(id: String): Int

    @Query(
        "UPDATE operations SET status='accepted',receipt=:receipt,lastError='',retryAt=0 WHERE id=:id"
    )
    fun accept(id: String, receipt: String)

    @Query("UPDATE operations SET status='covered' WHERE id=:id AND status='accepted'")
    fun cover(id: String)

    @Query("UPDATE operations SET status='queued',retryAt=0 WHERE status='uploading'")
    fun recoverUploads()

    @Query("UPDATE operations SET retryAt=0 WHERE status='queued'") fun resetBackoff()

    @Query("SELECT * FROM manifest WHERE generation=:generation ORDER BY person")
    fun manifest(generation: String): List<ManifestRow>

    @Query("SELECT * FROM manifest WHERE generation=:generation AND person=:person")
    fun manifestPerson(generation: String, person: String): ManifestRow?

    @Insert(onConflict = OnConflictStrategy.REPLACE) fun manifest(rows: List<ManifestRow>)

    @Query(
        "SELECT * FROM pages WHERE generation=:generation AND person=:person AND section=:section"
    )
    fun pages(generation: String, person: String, section: String): List<PageRow>

    @Insert(onConflict = OnConflictStrategy.ABORT) fun page(row: PageRow)

    @Query("DELETE FROM pages") fun clearPages()

    @Query("DELETE FROM manifest") fun clearManifest()

    @Query("SELECT * FROM stage_catalog ORDER BY position,id") fun stageCatalog(): List<StageCatalogRow>

    @Query("SELECT * FROM stage_catalog WHERE id=:id") fun stage(id: String): StageCatalogRow?

    @Insert(onConflict = OnConflictStrategy.REPLACE) fun stageCatalog(rows: List<StageCatalogRow>)

    @Query("DELETE FROM stage_catalog") fun clearStageCatalog()

    @Query("SELECT * FROM stage_catalog_pages WHERE generation=:generation") fun stageCatalogPages(generation: String): List<StageCatalogPageRow>

    @Insert(onConflict = OnConflictStrategy.ABORT) fun stageCatalogPage(row: StageCatalogPageRow)

    @Query("DELETE FROM stage_catalog_pages") fun clearStageCatalogPages()

    @Query("SELECT person FROM pins ORDER BY person") fun pins(): List<String>

    @Insert(onConflict = OnConflictStrategy.REPLACE) fun pin(row: PinRow)

    @Query("DELETE FROM pins WHERE person=:id") fun unpin(id: String)
}

@Database(
    entities =
        [
            MetaRow::class,
            PersonRow::class,
            DraftRow::class,
            ContactDraftRow::class,
            StageDraftRow::class,
            StageContextRow::class,
            StageCatalogRow::class,
            StageCatalogPageRow::class,
            OperationRow::class,
            ManifestRow::class,
            PageRow::class,
            PinRow::class,
            EditContextRow::class,
            ProfileDraftRow::class,
            ProfileContextRow::class,
        ],
    version = 6,
    exportSchema = true,
)
abstract class FieldDatabase : RoomDatabase() {
    abstract fun data(): FieldDao

    companion object {
        val UPGRADE_1_2 =
            object : Migration(1, 2) {
                override fun migrate(db: SupportSQLiteDatabase) {
                    db.execSQL(
                        "ALTER TABLE operations ADD COLUMN lastError TEXT NOT NULL DEFAULT ''"
                    )
                }
            }

        /**
         * This upgrade deliberately only adds protected columns/tables.  It never rewrites
         * legacy JSON envelopes or drafts: their bytes, IDs, receipt-null semantics and
         * create→complete dependency remain the Mobile 001 record of truth.
         */
        val UPGRADE_2_3 =
            object : Migration(2, 3) {
                override fun migrate(db: SupportSQLiteDatabase) {
                    db.execSQL("ALTER TABLE people ADD COLUMN noteRevisionsQualified INTEGER NOT NULL DEFAULT 0")
                    db.execSQL("ALTER TABLE drafts ADD COLUMN baseline TEXT NOT NULL DEFAULT ''")
                    db.execSQL("ALTER TABLE drafts ADD COLUMN target TEXT NOT NULL DEFAULT ''")
                    db.execSQL("ALTER TABLE drafts ADD COLUMN state TEXT NOT NULL DEFAULT 'draft'")
                    db.execSQL("CREATE TABLE IF NOT EXISTS edit_context (operation TEXT NOT NULL, person TEXT NOT NULL, kind TEXT NOT NULL, target TEXT NOT NULL, baseline TEXT NOT NULL, current TEXT NOT NULL DEFAULT '', editorRevision INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(operation))")
                }
            }

        /**
         * This is deliberately a table-only upgrade.  Existing v3 rows remain precisely as
         * sealed: especially legacy draft payload strings and operation envelope bytes.
         */
        val UPGRADE_3_4 =
            object : Migration(3, 4) {
                override fun migrate(db: SupportSQLiteDatabase) {
                    db.execSQL(
                        "CREATE TABLE IF NOT EXISTS contact_drafts (id TEXT NOT NULL, person TEXT NOT NULL, channel TEXT NOT NULL, outcome TEXT NOT NULL, occurredAt TEXT NOT NULL, resolvedOffset TEXT NOT NULL, revision INTEGER NOT NULL, operation TEXT NOT NULL DEFAULT '', state TEXT NOT NULL DEFAULT 'draft', lastError TEXT NOT NULL DEFAULT '', PRIMARY KEY(id))"
                    )
                }
            }

        /**
         * Mobile 004 adds typed stage state only. It intentionally does not rewrite a v1-v4
         * draft, operation envelope, receipt, database key, or protected account directory.
         */
        val UPGRADE_4_5 =
            object : Migration(4, 5) {
                override fun migrate(db: SupportSQLiteDatabase) {
                    db.execSQL("ALTER TABLE people ADD COLUMN stageRevisionsQualified INTEGER NOT NULL DEFAULT 0")
                    db.execSQL("CREATE TABLE IF NOT EXISTS stage_drafts (id TEXT NOT NULL, person TEXT NOT NULL, baselineStageId TEXT NOT NULL, baselineStageName TEXT NOT NULL, baselineRevision TEXT NOT NULL, proposedStageId TEXT NOT NULL, proposedStageName TEXT NOT NULL, revision INTEGER NOT NULL, operation TEXT NOT NULL DEFAULT '', state TEXT NOT NULL DEFAULT 'draft', lastError TEXT NOT NULL DEFAULT '', PRIMARY KEY(id))")
                    db.execSQL("CREATE TABLE IF NOT EXISTS stage_context (operation TEXT NOT NULL, person TEXT NOT NULL, baseline TEXT NOT NULL, proposal TEXT NOT NULL, current TEXT NOT NULL DEFAULT '', editorRevision INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(operation))")
                    db.execSQL("CREATE TABLE IF NOT EXISTS stage_catalog (id TEXT NOT NULL, name TEXT NOT NULL, position INTEGER NOT NULL, generation TEXT NOT NULL, revision TEXT NOT NULL, PRIMARY KEY(id))")
                    db.execSQL("CREATE TABLE IF NOT EXISTS stage_catalog_pages (generation TEXT NOT NULL, cursor TEXT NOT NULL, body TEXT NOT NULL, PRIMARY KEY(generation,cursor))")
                }
            }

        /** Mobile005 preserves every schema-5 row and only adds the new protected profile state. */
        val UPGRADE_5_6 =
            object : Migration(5, 6) {
                override fun migrate(db: SupportSQLiteDatabase) {
                    db.execSQL("ALTER TABLE people ADD COLUMN detailsRevisionsQualified INTEGER NOT NULL DEFAULT 0")
                    db.execSQL("CREATE TABLE IF NOT EXISTS profile_drafts (id TEXT NOT NULL, person TEXT NOT NULL, baseline TEXT NOT NULL, proposal TEXT NOT NULL, detailsRevision TEXT NOT NULL, revision INTEGER NOT NULL, operation TEXT NOT NULL DEFAULT '', state TEXT NOT NULL DEFAULT 'draft', lastError TEXT NOT NULL DEFAULT '', PRIMARY KEY(id))")
                    db.execSQL("CREATE TABLE IF NOT EXISTS profile_context (operation TEXT NOT NULL, person TEXT NOT NULL, baseline TEXT NOT NULL, proposal TEXT NOT NULL, current TEXT NOT NULL DEFAULT '', editorRevision INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(operation))")
                }
            }

        fun open(context: Context, directory: File, key: ByteArray): FieldDatabase {
            System.loadLibrary("sqlcipher")
            val db =
                Room.databaseBuilder(
                        context,
                        FieldDatabase::class.java,
                        File(directory, "field.db").absolutePath,
                    )
                    .openHelperFactory(SupportOpenHelperFactory(key, null, true))
                    .setJournalMode(JournalMode.WRITE_AHEAD_LOGGING)
                    .addMigrations(UPGRADE_1_2, UPGRADE_2_3, UPGRADE_3_4, UPGRADE_4_5, UPGRADE_5_6)
                    .addCallback(
                        object : Callback() {
                            override fun onOpen(db: SupportSQLiteDatabase) {
                                db.execSQL("PRAGMA synchronous=FULL")
                                db.execSQL("PRAGMA temp_store=MEMORY")
                                db.query("PRAGMA cipher_version").use {
                                    check(
                                        it.moveToFirst() && it.getString(0).startsWith("4.19.0")
                                    ) {
                                        "Encrypted database unavailable"
                                    }
                                }
                                db.query("PRAGMA journal_mode").use {
                                    check(it.moveToFirst() && it.getString(0).equals("wal", true))
                                }
                                db.query("PRAGMA synchronous").use {
                                    check(it.moveToFirst() && it.getInt(0) == 2)
                                }
                                db.query("PRAGMA temp_store").use {
                                    check(it.moveToFirst() && it.getInt(0) == 2)
                                }
                            }
                        }
                    )
                    .build()
            try {
                db.openHelper.writableDatabase
            } catch (error: Exception) {
                db.close()
                throw error
            }
            return db
        }
    }
}
