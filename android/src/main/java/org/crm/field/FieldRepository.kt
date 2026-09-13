package org.crm.field

import android.app.KeyguardManager
import android.content.Context
import kotlin.math.min
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import org.json.JSONArray
import org.json.JSONObject

class FieldUi(
    val locked: Boolean = true,
    val busy: Boolean = false,
    val message: String = "Sign in to prepare your field workspace.",
    val people: List<PersonCard> = emptyList(),
    val person: PersonRow? = null,
    val operations: List<OperationRow> = emptyList(),
    val drafts: List<DraftRow> = emptyList(),
    val today: String? = null,
    val lastSync: String = "Never",
    val coverage: String = "No complete download",
    val paused: Boolean = false,
    val displayName: String = "",
    val pendingCount: Int = 0,
)

class ActiveAccount(
    val store: FieldStore,
    val api: FieldApi,
    val key: ByteArray,
    val id: String,
    val epoch: Long,
)

class FieldRepository(
    val context: Context,
    private val origin: String = BuildConfig.API_BASE,
    private val namespace: String = "field",
    private val clock: DeviceClock = AndroidClock(context),
    private val transport: Transport = HttpTransport(),
) {
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val mutable = MutableStateFlow(FieldUi())
    val ui = mutable.asStateFlow()
    private val vault = DeviceVault(context, namespace)
    private var registry = JSONObject()
    @Volatile private var active: ActiveAccount? = null
    @Volatile private var epoch = 0L
    private val syncing = Mutex()
    private val accountChanges = Any()
    private var selected: String? = null
    private var syncJob: Job? = null
    private var automaticAttempts = 0

    suspend fun restore() =
        withContext(Dispatchers.IO) {
            try {
                registry = vault.registry()
                if (
                    vault.isLocked() ||
                        registry.optBoolean("locked", true) ||
                        !registry.has("account")
                ) {
                    mutable.value = FieldUi(pendingCount = registry.optInt("pending"))
                    return@withContext
                }
                val id = registry.getString("account")
                val directory = vault.accountDirectory(id)
                val key = vault.databaseKey(directory)
                val db = FieldDatabase.open(context, directory, key)
                val stored = JSONObject(db.data().meta("binding") ?: throw AccessLocked())
                val binding =
                    Binding.parse(
                        stored,
                        registry.getString("actor"),
                        registry.getString("org"),
                        registry.getString("installation"),
                    )
                val store = FieldStore(db, binding, clock)
                try {
                    store.requireAccess()
                    store.observeClock()
                } catch (error: Exception) {
                    db.close()
                    key.fill(0)
                    throw error
                }
                db.data().recoverUploads()
                active =
                    ActiveAccount(
                        store,
                        FieldApi(origin, db.data().meta("cookie") ?: "", transport),
                        key,
                        id,
                        ++epoch,
                    )
                refreshView("Ready for field work")
            } catch (_: Exception) {
                mutable.value =
                    FieldUi(
                        message =
                            "Online authorization is required. Existing data and pending work remain protected.",
                        pendingCount = registry.optInt("pending"),
                    )
            }
        }

    private fun check(account: ActiveAccount) {
        if (
            active !== account ||
                epoch != account.epoch ||
                (context.getSystemService(Context.KEYGUARD_SERVICE) as KeyguardManager)
                    .isDeviceLocked
        )
            throw AccessLocked()
        account.store.requireAccess()
    }

    suspend fun login(email: String, password: String) =
        withContext(Dispatchers.IO) {
            lockLocal("Signing in…", signingOut = false)
            val ticket = epoch
            mutable.value =
                FieldUi(
                    busy = true,
                    message = "Verifying your workspace…",
                    pendingCount = registry.optInt("pending"),
                )
            var opened: FieldDatabase? = null
            var key: ByteArray? = null
            try {
                if (registry.length() == 0) registry = vault.registry()
                val api = FieldApi(origin, transport = transport)
                val session =
                    api.call(
                        "POST",
                        "/api/session",
                        body = json("email" to email.trim(), "password" to password).toString(),
                    )
                val actor = uuid(session.getJSONObject("user").getString("id"))
                val org = session.getJSONObject("organization")
                if (org.getString("workspace_mode") != "operational")
                    throw ApiFailure(403, "workspace_in_migration_review")
                val installation = registry.getString("installation")
                val binding =
                    Binding.parse(
                        api.bootstrap(installation),
                        actor,
                        uuid(org.getString("id")),
                        installation,
                    )
                if (ticket != epoch) throw AccessLocked()
                val id = vault.accountId(origin, actor, binding.org)
                val directory = vault.accountDirectory(id)
                key = vault.databaseKey(directory)
                opened = FieldDatabase.open(context, directory, key)
                val store = FieldStore(opened, binding, clock)
                store.authorize(api.cookie)
                store.dao.recoverUploads()
                store.dao.meta(
                    MetaRow("display_name", session.getJSONObject("user").getString("display_name"))
                )
                synchronized(accountChanges) {
                    if (ticket != epoch) throw AccessLocked()
                    registry
                        .put("actor", actor)
                        .put("org", binding.org)
                        .put("account", id)
                        .put("locked", false)
                    vault.saveRegistry(registry)
                    vault.setLocked(false)
                    active = ActiveAccount(store, api, key, id, ticket)
                    opened = null
                    key = null
                }
                refreshView("Authorized. Preparing your offline workspace…")
                automaticAttempts = 0
            } catch (error: Exception) {
                android.util.Log.w(
                    "CRMField",
                    "authorization_failed category=${if (error is ApiFailure) "http_${error.status}" else error.javaClass.simpleName}",
                )
                opened?.close()
                key?.fill(0)
                if (ticket == epoch)
                    mutable.value =
                        FieldUi(
                            message =
                                if (error is ApiFailure && error.status == 401)
                                    "Sign-in failed. Check your email and password."
                                else if (
                                    error is ApiFailure &&
                                        error.code == "workspace_in_migration_review"
                                )
                                    "This workspace is held for migration review. Saved work is protected."
                                else
                                    "Could not authorize this workspace. Protected saved work was kept.",
                            pendingCount = registry.optInt("pending"),
                        )
            }
        }

    suspend fun lockLocal(message: String, signingOut: Boolean = true) =
        withContext(Dispatchers.IO) {
            synchronized(accountChanges) {
                val old = active
                active = null
                epoch++
                syncJob?.cancel()
                selected = null
                mutable.value =
                    FieldUi(message = message, pendingCount = registry.optInt("pending"))
                try {
                    if (registry.length() == 0) registry = vault.registry()
                    vault.setLocked(true)
                    if (old != null) {
                        val count =
                            old.store.dao.operations().count {
                                it.status !in listOf("accepted", "covered")
                            } + old.store.dao.drafts().size
                        registry.put("pending", count)
                        if (signingOut) old.store.lock()
                    }
                    registry.put("locked", true)
                    vault.saveRegistry(registry)
                } catch (_: Exception) {
                    mutable.value =
                        FieldUi(
                            message =
                                "Account locked in this session. Device storage needs attention before sign-out can be confirmed."
                        )
                }
                // Cancelling an in-flight request prevents its result from being applied; closing
                // waits for its short DB work.
                if (old != null) {
                    old.store.db.close()
                    old.key.fill(0)
                }
            }
        }

    suspend fun foreground() {
        val account = active ?: return
        try {
            check(account)
            account.store.observeClock()
            refreshView()
            requestSync()
        } catch (_: Exception) {
            lockLocal(
                "Offline access expired or the clock changed. Sign in online; saved work remains protected.",
                false,
            )
        }
    }

    fun select(id: String?) {
        selected = id
        scope.launch { refreshView() }
    }

    suspend fun draft(id: String): DraftRow? =
        withContext(Dispatchers.IO) {
            val account = active ?: throw AccessLocked()
            check(account)
            account.store.dao.draft(id)?.also {
                if (account.store.dao.person(it.person) == null) throw AccessLocked()
            }
        }

    suspend fun saveDraft(
        id: String,
        person: String,
        kind: String,
        payload: JSONObject,
        expectedRevision: Long = 0,
    ): DraftRow =
        withContext(Dispatchers.IO) {
            val account = active ?: throw AccessLocked()
            check(account)
            try {
                account.store.saveDraft(id, person, kind, payload, expectedRevision).also {
                    refreshView("Draft saved on this device")
                }
            } catch (error: Exception) {
                if (error is AccessLocked) throw error
                throw StorageFailure()
            }
        }

    suspend fun submit(id: String, revision: Long): OperationRow =
        withContext(Dispatchers.IO) {
            val account = active ?: throw AccessLocked()
            check(account)
            val result = account.store.submitDraft(id, revision)
            refreshView("Saved on this device. Waiting for server acceptance.")
            FieldSyncJob.schedule(context)
            result
        }

    suspend fun complete(person: String, task: JSONObject? = null, creation: String? = null) =
        withContext(Dispatchers.IO) {
            val account = active ?: throw AccessLocked()
            check(account)
            account.store.complete(person, task, creation)
            refreshView("Completion saved on this device")
            FieldSyncJob.schedule(context)
        }

    suspend fun pause(value: Boolean) =
        withContext(Dispatchers.IO) {
            val account = active ?: return@withContext
            check(account)
            account.store.dao.meta(MetaRow("paused", value.toString()))
            if (value) syncJob?.cancel()
            refreshView(
                if (value) "Sync paused. You can keep saving on this device." else "Sync resumed"
            )
            if (!value) requestSync(true)
        }

    suspend fun pin(id: String, value: Boolean) =
        withContext(Dispatchers.IO) {
            val account = active ?: throw AccessLocked()
            check(account)
            uuid(id)
            if (value) account.store.dao.pin(PinRow(id)) else account.store.dao.unpin(id)
            refreshView("Offline selection will update at the next complete sync")
            requestSync(true)
        }

    fun requestSync(manual: Boolean = false) {
        if (active == null || syncJob?.isActive == true) return
        if (manual) automaticAttempts = 0
        syncJob = scope.launch { sync(manual) }
    }

    suspend fun sync(manual: Boolean = false) = syncing.withLock {
        val account = active ?: return@withLock
        try {
            check(account)
            if (account.store.dao.meta("paused") == "true") return@withLock
            if (!manual && automaticAttempts >= 3) return@withLock
            val retryAt = account.store.dao.meta("sync_retry_at")?.toLongOrNull() ?: 0
            if (
                clock.elapsed() < retryAt &&
                    (!manual || account.store.dao.meta("retry_mandatory") == "true")
            ) {
                refreshView(
                    "Sync will retry in ${(retryAt - clock.elapsed() + 999) / 1000} seconds. Saved work remains on this device."
                )
                return@withLock
            }
            refreshView("Checking authorization…", true)
            val binding =
                Binding.parse(
                    account.api.bootstrap(account.store.binding.installation),
                    account.store.binding.actor,
                    account.store.binding.org,
                    account.store.binding.installation,
                )
            check(account)
            if (binding.context != account.store.binding.context) throw ProtocolFailure()
            FieldStore(account.store.db, binding, clock).authorize(account.api.cookie)
            upload(account, manual)
            try {
                download(account)
            } catch (error: ApiFailure) {
                // Reclaimed/expired generation rows can return404. Operation404 has a different
                // recovery path.
                if (error.status == 404) throw ApiFailure(409, "generation_expired")
                throw error
            }
            account.store.dao.removeMeta("sync_retry_at")
            account.store.dao.removeMeta("sync_failures")
            account.store.dao.removeMeta("retry_mandatory")
            automaticAttempts = 0
            refreshView("Up to date. Complete cache available offline.")
        } catch (cancel: CancellationException) {
            throw cancel
        } catch (error: Exception) {
            if (active !== account) return@withLock
            when {
                error is AccessLocked ||
                    error is ApiFailure &&
                        (error.status in setOf(401, 403) ||
                            error.code == "workspace_in_migration_review") ->
                    lockLocal(
                        "Online authorization is required. Saved work remains protected.",
                        false,
                    )
                error is ProtocolFailure ||
                    error is ApiFailure && error.code == "protocol_unsupported" ->
                    lockLocal(
                        "App/context verification required. Saved work remains protected.",
                        false,
                    )
                else -> {
                    try {
                        if (
                            error is ApiFailure &&
                                error.code in listOf("generation_changed", "generation_expired")
                        ) {
                            account.store.dao.removeMeta("generation")
                            automaticAttempts++
                        }
                        val failures =
                            (account.store.dao.meta("sync_failures")?.toIntOrNull() ?: 0) + 1
                        val seconds =
                            if (error is ApiFailure) error.retryAfterSeconds
                            else min(3600L, 5L shl min(failures, 9))
                        account.store.dao.meta(MetaRow("sync_failures", failures.toString()))
                        account.store.dao.meta(
                            MetaRow("sync_retry_at", (clock.elapsed() + seconds * 1000).toString())
                        )
                        account.store.dao.meta(
                            MetaRow(
                                "retry_mandatory",
                                (error is ApiFailure && error.status == 429).toString(),
                            )
                        )
                        refreshView(
                            if (error is ApiFailure) errorMessage(error.code)
                            else
                                "Sync interrupted. Your last complete download and saved work are intact."
                        )
                        if (automaticAttempts < 3)
                            FieldSyncJob.schedule(
                                context,
                                if (error is ApiFailure) error.retryAfterSeconds else 30,
                            )
                    } catch (_: Exception) {
                        if (active === account)
                            mutable.value =
                                FieldUi(
                                    message =
                                        "Device storage needs attention. Existing saved work remains protected; no new save was confirmed.",
                                    pendingCount = registry.optInt("pending"),
                                )
                    }
                }
            }
        } finally {
            if (active === account && mutable.value.busy) refreshView(mutable.value.message)
        }
    }

    private suspend fun upload(account: ActiveAccount, manual: Boolean) {
        val dao = account.store.dao
        for (row in dao.operations().filter { it.status in listOf("queued", "uploading") }) {
            check(account)
            if (!manual && row.retryAt > clock.elapsed()) continue
            val payload = JSONObject(row.envelope).getJSONObject("payload")
            val dependency =
                payload.optJSONObject("target")?.stringOrNull("created_by_operation_id")
            if (dependency != null) {
                val create = dao.operation(dependency)
                if (create == null || create.status == "attention") {
                    dao.operationState(row.id, "attention", row.attempts, 0, "dependency_pending")
                    continue
                }
                if (create.status !in listOf("accepted", "covered")) continue
            }
            dao.operationState(row.id, "uploading", row.attempts + 1, 0, "")
            try {
                val receipt = account.api.operation(account.store.binding, row)
                check(account)
                account.store.acknowledge(row.id, receipt)
            } catch (cancel: CancellationException) {
                throw cancel
            } catch (error: Exception) {
                check(account)
                if (
                    error is ApiFailure &&
                        (error.status == 401 ||
                            error.code == "workspace_in_migration_review" ||
                            error.code == "protocol_unsupported")
                )
                    throw error
                if (error is ApiFailure && error.status == 403) {
                    // A task-specific denial is attention; a current authority denial locks all
                    // cached data.
                    val authorized =
                        Binding.parse(
                            account.api.bootstrap(account.store.binding.installation),
                            account.store.binding.actor,
                            account.store.binding.org,
                            account.store.binding.installation,
                        )
                    check(account)
                    if (authorized.context != account.store.binding.context) throw ProtocolFailure()
                }
                val retry =
                    error !is ApiFailure ||
                        error.status >= 500 ||
                        error.status == 429 ||
                        error.code == "dependency_pending"
                val code = if (error is ApiFailure) error.code else "unavailable"
                val delay =
                    if (error is ApiFailure && error.status == 429) error.retryAfterSeconds
                    else min(3600L, 5L shl min(row.attempts, 9))
                dao.operationState(
                    row.id,
                    if (retry) "queued" else "attention",
                    row.attempts + 1,
                    clock.elapsed() + delay * 1000,
                    code,
                )
                if (retry) throw error
            }
        }
        refreshView("Saved actions checked. Downloading complete records…", true)
    }

    private suspend fun download(account: ActiveAccount) {
        val store = account.store
        val dao = store.dao
        var generation = dao.meta("generation")?.let { JSONObject(it) }
        if (
            generation != null &&
                java.time.Instant.parse(generation.getString("expires_at"))
                    .isBefore(java.time.Instant.now())
        ) {
            dao.removeMeta("generation")
            generation = null
        }
        if (generation == null) {
            val created =
                account.api.call(
                    "POST",
                    "/api/mobile/v1/reconciliations",
                    store.binding.context,
                    json(
                            "protocol" to PROTOCOL,
                            "installation_id" to store.binding.installation,
                            "pinned_person_ids" to JSONArray(dao.pins()),
                        )
                        .toString(),
                )
            check(account)
            store.beginGeneration(created)
            generation = created
        }
        val id = uuid(generation.getString("generation_id"))
        while (dao.meta("manifest_complete") != "true") {
            val cursor = dao.meta("manifest_cursor") ?: throw ProtocolFailure()
            val page =
                account.api.page(
                    "/api/mobile/v1/reconciliations/$id/manifest",
                    store.binding,
                    cursor,
                )
            check(account)
            store.appendManifest(id, page)
        }
        val manifest = dao.manifest(id)
        for ((index, item) in manifest.withIndex()) {
            check(account)
            if (dao.person(item.person)?.revision == item.revision) continue
            if (vault.root.usableSpace < 16 * 1024 * 1024) throw StorageFailure()
            for (section in listOf("summary", "notes", "tasks")) {
                while (true) {
                    val cursor = store.nextPage(id, item.person, section) ?: break
                    val page =
                        account.api.page(
                            "/api/mobile/v1/reconciliations/$id/people/${item.person}/$section",
                            store.binding,
                            cursor,
                        )
                    check(account)
                    store.stagePage(id, item.person, section, cursor, page)
                }
            }
            if (index % 10 == 0)
                refreshView(
                    "Downloading ${index + 1}/${manifest.size} People. Active cache unchanged until completion.",
                    true,
                )
        }
        val seal =
            account.api.call(
                "POST",
                "/api/mobile/v1/reconciliations/$id/seal",
                store.binding.context,
                "{}",
            )
        check(account)
        store.promote(seal)
    }

    suspend fun refreshView(message: String = mutable.value.message, busy: Boolean = false) =
        withContext(Dispatchers.IO) {
            val account = active ?: return@withContext
            try {
                check(account)
                val dao = account.store.dao
                val visibleIds = dao.people().map { it.id }.toSet()
                val ops =
                    dao.operations()
                        .filter { it.status != "covered" }
                        .map {
                            if (it.person in visibleIds) it
                            else it.copy(envelope = "", receipt = null)
                        }
                val drafts =
                    dao.drafts().map { if (it.person in visibleIds) it else it.copy(payload = "") }
                registry.put("pending", ops.count { it.status != "accepted" } + drafts.size)
                vault.saveRegistry(registry)
                val result =
                    FieldUi(
                        false,
                        busy,
                        message,
                        dao.people(),
                        selected?.let { dao.person(it) },
                        ops,
                        drafts,
                        dao.meta("today"),
                        dao.meta("last_sync") ?: "Never",
                        dao.meta("coverage") ?: "No complete download",
                        dao.meta("paused") == "true",
                        dao.meta("display_name") ?: "Field agent",
                        registry.getInt("pending"),
                    )
                if (active === account) mutable.value = result
            } catch (_: Exception) {
                if (active === account)
                    mutable.value =
                        FieldUi(
                            message =
                                "Data is locked or device storage needs attention. Saved work was preserved.",
                            pendingCount = registry.optInt("pending"),
                        )
            }
        }
}
