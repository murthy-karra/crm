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
)

data class PersonCard(val id: String, val revision: String, val summary: String)

@Entity(tableName = "drafts")
data class DraftRow(
    @PrimaryKey val id: String,
    val person: String,
    val kind: String,
    val payload: String,
    val revision: Long,
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

    @Insert(onConflict = OnConflictStrategy.ABORT) fun operation(row: OperationRow)

    @Query("SELECT * FROM operations ORDER BY createdAt,id") fun operations(): List<OperationRow>

    @Query("SELECT * FROM operations WHERE id=:id") fun operation(id: String): OperationRow?

    @Query(
        "UPDATE operations SET status=:status,attempts=:attempts,retryAt=:retryAt,lastError=:error WHERE id=:id"
    )
    fun operationState(id: String, status: String, attempts: Int, retryAt: Long, error: String)

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
            OperationRow::class,
            ManifestRow::class,
            PageRow::class,
            PinRow::class,
        ],
    version = 2,
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
                    .addMigrations(UPGRADE_1_2)
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
