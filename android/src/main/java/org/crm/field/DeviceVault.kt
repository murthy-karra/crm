package org.crm.field

import android.content.Context
import android.os.SystemClock
import android.provider.Settings
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.AtomicFile
import java.io.File
import java.security.KeyStore
import java.security.MessageDigest
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import org.json.JSONObject

class AndroidClock(private val context: Context) : DeviceClock {
    override fun boot() =
        Settings.Global.getInt(context.contentResolver, Settings.Global.BOOT_COUNT, -1)

    override fun elapsed() = SystemClock.elapsedRealtime()

    override fun wall() = System.currentTimeMillis()
}

/** All keys and account registry files remain outside Android's backup domains. */
class DeviceVault(context: Context, private val namespace: String = "field") {
    val root = File(context.noBackupFilesDir, namespace).apply { mkdirs() }
    private val alias = "crm.$namespace.device-wrap.v1"

    private fun wrappingKey(): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(alias, null) as? SecretKey)?.let {
            return it
        }
        // Existing protected data with a lost key must never be overwritten.
        if (
            root.listFiles()?.any {
                it.isFile || it.isDirectory && it.listFiles()?.isNotEmpty() == true
            } == true
        )
            throw AccessLocked()
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
            .apply {
                init(
                    KeyGenParameterSpec.Builder(
                            alias,
                            KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
                        )
                        .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                        .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                        .setKeySize(256)
                        .setUnlockedDeviceRequired(true)
                        .build()
                )
            }
            .generateKey()
    }

    private fun encrypt(value: ByteArray): ByteArray {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, wrappingKey())
        cipher.updateAAD(namespace.toByteArray())
        return cipher.iv + cipher.doFinal(value)
    }

    private fun decrypt(value: ByteArray): ByteArray {
        require(value.size >= 28)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(
            Cipher.DECRYPT_MODE,
            wrappingKey(),
            GCMParameterSpec(128, value.copyOfRange(0, 12)),
        )
        cipher.updateAAD(namespace.toByteArray())
        return cipher.doFinal(value.copyOfRange(12, value.size))
    }

    fun read(file: File): ByteArray = decrypt(AtomicFile(file).readFully())

    fun write(file: File, bytes: ByteArray) {
        val encrypted = encrypt(bytes)
        val atomic = AtomicFile(file)
        val stream = atomic.startWrite()
        try {
            stream.write(encrypted)
            atomic.finishWrite(stream)
        } catch (error: Exception) {
            atomic.failWrite(stream)
            throw error
        }
    }

    fun registry(): JSONObject {
        val file = File(root, "registry.bin")
        if (file.exists()) return JSONObject(String(read(file)))
        return json(
                "installation" to java.util.UUID.randomUUID().toString(),
                "locked" to true,
                "pending" to 0,
            )
            .also { saveRegistry(it) }
    }

    // Preallocated one-byte state permits a lock write without allocating a new file on full
    // storage.
    // A missing, corrupt, or unreadable marker is always locked; registry state alone never
    // unlocks.
    fun isLocked(): Boolean =
        try {
            File(root, "lock.state").readBytes().let { it.size != 1 || it[0] != 1.toByte() }
        } catch (_: Exception) {
            true
        }

    fun setLocked(locked: Boolean) {
        val file = File(root, "lock.state")
        java.io.RandomAccessFile(file, "rw").use { stream ->
            stream.seek(0)
            stream.write(if (locked) 0 else 1)
            stream.setLength(1)
            stream.fd.sync()
        }
        check(isLocked() == locked)
    }

    fun saveRegistry(value: JSONObject) =
        write(File(root, "registry.bin"), value.toString().toByteArray())

    fun accountId(origin: String, actor: String, org: String): String =
        MessageDigest.getInstance("SHA-256")
            .digest("$origin|$actor|$org".toByteArray())
            .joinToString("") { "%02x".format(it) }

    fun accountDirectory(id: String) =
        File(root, id).apply {
            require(id.matches(Regex("[a-f0-9]{64}")))
            mkdirs()
        }

    fun databaseKey(directory: File): ByteArray {
        val wrapped = File(directory, "database.key")
        if (wrapped.exists()) return read(wrapped).also { require(it.size == 32) }
        if (File(directory, "field.db").exists()) throw AccessLocked()
        return ByteArray(32).also {
            SecureRandom().nextBytes(it)
            write(wrapped, it)
        }
    }
}
