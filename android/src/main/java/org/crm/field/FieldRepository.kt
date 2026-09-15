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
import java.util.UUID
data class OrganizationSearchPerson(val id: String, val displayName: String, val stageName: String?, val assignedName: String?, val email: String?, val phone: String?) { companion object { fun parse(item: JSONObject): OrganizationSearchPerson { val id = uuid(item.getString("person_id")); val stage = item.optJSONObject("stage"); val assigned = item.optJSONObject("assigned_user"); require(item.getString("display_name").isNotBlank()); stage?.let { uuid(it.getString("id")); require(it.getString("name").isNotBlank()) }; assigned?.let { uuid(it.getString("id")); require(it.getString("display_name").isNotBlank()) }; return OrganizationSearchPerson(id, item.getString("display_name"), stage?.getString("name"), assigned?.getString("display_name"), item.stringOrNull("primary_email"), item.stringOrNull("primary_phone")) } } }
data class RequestedPin(val id: String, val label: String, val shortId: String)

class FieldUi(
    val locked: Boolean = true,
    val busy: Boolean = false,
    val message: String = "Sign in to prepare your field workspace.",
    val people: List<PersonCard> = emptyList(),
    val person: PersonRow? = null,
    val operations: List<OperationRow> = emptyList(),
    val drafts: List<DraftRow> = emptyList(),
    val contactDrafts: List<ContactDraftRow> = emptyList(),
    val stageDrafts: List<StageDraftRow> = emptyList(),
    val stageCatalog: List<StageCatalogRow> = emptyList(),
    val today: String? = null,
    val lastSync: String = "Never",
    val coverage: String = "No complete download",
    val paused: Boolean = false,
    val displayName: String = "",
    val pendingCount: Int = 0,
    val editsEnabled: Boolean = false,
    val contactLoggingEnabled: Boolean = false,
    val stageChangesEnabled: Boolean = false,
    val editContexts: List<EditContextRow> = emptyList(),
    val stageContexts: List<StageContextRow> = emptyList(),
    val profileDrafts: List<ProfileDraftRow> = emptyList(),
    val profileContexts: List<ProfileContextRow> = emptyList(),
    val profileEditingEnabled: Boolean = false,
    val metadataDrafts: List<MetadataDraftRow> = emptyList(),
    val metadataContexts: List<MetadataContextRow> = emptyList(),
    val metadataTags: List<MetadataTagRow> = emptyList(),
    val metadataFields: List<MetadataFieldRow> = emptyList(),
    val metadataOptions: List<MetadataOptionRow> = emptyList(),
    val metadataEditingEnabled: Boolean = false,
    val pinnedPeople: Set<String> = emptySet(), val organizationSearchResults: List<OrganizationSearchPerson> = emptyList(), val organizationSearchHasMore: Boolean = false, val organizationSearching: Boolean = false, val organizationSearchMessage: String = "", val requestedPins: List<RequestedPin> = emptyList(), val pinReviewRequired: Boolean = false, val peopleReasons: Map<String, List<String>> = emptyMap(),
)

class ActiveAccount(
    @Volatile var store: FieldStore,
    val api: FieldApi,
    val key: ByteArray,
    val id: String,
    val epoch: Long,
)

class FieldRepository(
    val context: Context,
    private val origin: String = BuildConfig.API_BASE,
    private val namespace: String = BuildConfig.VAULT_NAMESPACE,
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
    private var organizationSearchEpoch = 0L; private var organizationSearchResults: List<OrganizationSearchPerson> = emptyList(); private var organizationSearchHasMore = false; private var organizationSearching = false; private var organizationSearchMessage = ""

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
                organizationSearchEpoch++; organizationSearchResults = emptyList(); organizationSearchHasMore = false; organizationSearching = false; organizationSearchMessage = ""
                mutable.value =
                    FieldUi(message = message, pendingCount = registry.optInt("pending"))
                try {
                    if (registry.length() == 0) registry = vault.registry()
                    vault.setLocked(true)
                    if (old != null) {
                        val count =
                            old.store.dao.operations().count {
                                it.status !in listOf("accepted", "covered")
                            } +
                                old.store.dao.drafts().size +
                                old.store.dao.contactDrafts().count { it.operation.isEmpty() } +
                                old.store.dao.profileDrafts().count { it.operation.isEmpty() }
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
        baseline: JSONObject? = null,
        target: JSONObject? = null,
    ): DraftRow =
        withContext(Dispatchers.IO) {
            val account = active ?: throw AccessLocked()
            check(account)
            try {
                account.store.saveDraft(id, person, kind, payload, expectedRevision, baseline, target).also {
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

    suspend fun contactDraft(id: String): ContactDraftRow? =
        withContext(Dispatchers.IO) {
            val account = active ?: throw AccessLocked()
            check(account)
            account.store.dao.contactDraft(id)?.also {
                if (account.store.dao.person(it.person) == null) throw AccessLocked()
            }
        }

    suspend fun saveContactDraft(
        id: String,
        person: String,
        channel: String,
        outcome: String,
        occurredAt: String,
        resolvedOffset: String,
        revision: Long = 0,
    ): ContactDraftRow =
        withContext(Dispatchers.IO) {
            val account = active ?: throw AccessLocked()
            check(account)
            try {
                account.store
                    .saveContactDraft(
                        id,
                        person,
                        channel,
                        outcome,
                        occurredAt,
                        resolvedOffset,
                        revision,
                    )
                    .also { refreshView("Contact draft saved on this device") }
            } catch (error: Exception) {
                if (error is AccessLocked) throw error
                throw StorageFailure()
            }
        }

    suspend fun submitContactDraft(id: String, revision: Long): OperationRow =
        withContext(Dispatchers.IO) {
            val account = active ?: throw AccessLocked()
            check(account)
            if (!account.store.binding.supportsContactLogging())
                throw ApiFailure(409, "contact_logging_unsupported")
            account.store.submitContactDraft(id, revision).also {
                refreshView("Contact saved on this device. Waiting for server acceptance.")
                FieldSyncJob.schedule(context)
            }
        }

    suspend fun reviseFutureContact(operation: String): ContactDraftRow =
        withContext(Dispatchers.IO) {
            val account = active ?: throw AccessLocked()
            check(account)
            account.store.reviseFutureContact(operation).also {
                refreshView("Correct the reported time in a new contact draft")
            }
        }

    suspend fun contactLoggingSupported(): Boolean =
        withContext(Dispatchers.IO) { active?.store?.binding?.supportsContactLogging() == true }

    suspend fun stageDraft(id: String): StageDraftRow? = withContext(Dispatchers.IO) {
        val account = active ?: throw AccessLocked(); check(account)
        account.store.dao.stageDraft(id)?.also { if (account.store.dao.person(it.person) == null) throw AccessLocked() }
    }

    suspend fun saveStageDraft(id: String, person: String, stage: String, revision: Long = 0, baseline: JSONObject? = null): StageDraftRow = withContext(Dispatchers.IO) {
        val account = active ?: throw AccessLocked(); check(account)
        if (!account.store.binding.supportsStageChanges()) throw ApiFailure(409, "stage_changes_unsupported")
        try { account.store.saveStageDraft(id, person, stage, revision, baseline).also { refreshView("Stage proposal saved on this device") } }
        catch (error: Exception) { if (error is AccessLocked || error is ApiFailure) throw error; throw StorageFailure() }
    }

    suspend fun submitStageDraft(id: String, revision: Long): OperationRow = withContext(Dispatchers.IO) {
        val account = active ?: throw AccessLocked(); check(account)
        if (!account.store.binding.supportsStageChanges()) throw ApiFailure(409, "stage_changes_unsupported")
        account.store.submitStageDraft(id, revision).also { refreshView("Stage change saved on this device. Waiting for server acceptance."); FieldSyncJob.schedule(context) }
    }

    suspend fun stageChangesSupported(): Boolean = withContext(Dispatchers.IO) { active?.store?.binding?.supportsStageChanges() == true }

    suspend fun profileDraft(id: String): ProfileDraftRow? = withContext(Dispatchers.IO) {
        val account = active ?: throw AccessLocked(); check(account)
        account.store.dao.profileDraft(id)?.also { if (account.store.dao.person(it.person) == null) throw AccessLocked() }
    }

    suspend fun saveProfileDraft(id: String, person: String, proposal: JSONObject, revision: Long = 0, baseline: JSONObject? = null): ProfileDraftRow = withContext(Dispatchers.IO) {
        val account = active ?: throw AccessLocked(); check(account)
        if (!account.store.binding.supportsPersonDetails()) throw ApiFailure(409, "person_details_unsupported")
        try { account.store.saveProfileDraft(id, person, proposal, revision, baseline).also { refreshView("Profile draft saved on this device") } }
        catch (error: Exception) { if (error is AccessLocked || error is ApiFailure) throw error; throw StorageFailure() }
    }

    suspend fun submitProfileDraft(id: String, revision: Long): OperationRow = withContext(Dispatchers.IO) {
        val account = active ?: throw AccessLocked(); check(account)
        if (!account.store.binding.supportsPersonDetails()) throw ApiFailure(409, "person_details_unsupported")
        account.store.submitProfileDraft(id, revision).also { refreshView("Profile changes saved on this device. Waiting for server acceptance."); FieldSyncJob.schedule(context) }
    }

    suspend fun profileEditingSupported(): Boolean = withContext(Dispatchers.IO) {
        val account = active ?: return@withContext false
        check(account); account.store.binding.supportsPersonDetails() && account.store.dao.person(selected ?: return@withContext false)?.let(account.store::detailsQualified) == true
    }

    suspend fun metadataDraft(id: String): MetadataDraftRow? = withContext(Dispatchers.IO) {
        val account = active ?: throw AccessLocked(); check(account)
        account.store.dao.metadataDraft(id)?.also { if (account.store.dao.person(it.person) == null) throw AccessLocked() }
    }

    suspend fun saveMetadataDraft(id: String, person: String, proposal: JSONArray, revision: Long = 0, baseline: JSONObject? = null): MetadataDraftRow = withContext(Dispatchers.IO) {
        val account = active ?: throw AccessLocked(); check(account)
        if (!account.store.binding.supportsMetadata()) throw ApiFailure(409, "metadata_unsupported")
        try { account.store.saveMetadataDraft(id, person, proposal, revision, baseline).also { refreshView("Tag and field proposal saved on this device") } }
        catch (error: Exception) { if (error is AccessLocked || error is ApiFailure) throw error; throw StorageFailure() }
    }

    suspend fun submitMetadataDraft(id: String, revision: Long): OperationRow = withContext(Dispatchers.IO) {
        val account = active ?: throw AccessLocked(); check(account)
        if (!account.store.binding.supportsMetadata()) throw ApiFailure(409, "metadata_unsupported")
        account.store.submitMetadataDraft(id, revision).also { refreshView("Metadata update saved on this device. Waiting for server acceptance."); FieldSyncJob.schedule(context) }
    }

    suspend fun metadataEditingSupported(): Boolean = withContext(Dispatchers.IO) { active?.store?.binding?.supportsMetadata() == true }

    suspend fun reviseMetadataConflict(operation: String): MetadataDraftRow = withContext(Dispatchers.IO) {
        val account = active ?: throw AccessLocked(); check(account)
        account.store.reviseMetadataConflict(operation, UUID.randomUUID().toString()).also { refreshView("Review the refreshed metadata catalog and proposal") }
    }

    suspend fun discardMetadataConflict(operation: String) = withContext(Dispatchers.IO) {
        val account = active ?: throw AccessLocked(); check(account)
        account.store.discardMetadataConflict(operation); refreshView("Saved metadata proposal discarded; current values remain unchanged")
    }

    suspend fun reviseProfileConflict(operation: String): ProfileDraftRow = withContext(Dispatchers.IO) {
        val account = active ?: throw AccessLocked(); check(account)
        account.store.reviseProfileConflict(operation, java.util.UUID.randomUUID().toString()).also { refreshView("Review the new profile proposal") }
    }

    suspend fun discardProfileConflict(operation: String) = withContext(Dispatchers.IO) {
        val account = active ?: throw AccessLocked(); check(account)
        account.store.discardProfileConflict(operation); refreshView("Saved profile proposal discarded; current version remains unchanged")
    }

    suspend fun reviseStageConflict(operation: String): StageDraftRow = withContext(Dispatchers.IO) {
        val account = active ?: throw AccessLocked(); check(account)
        account.store.reviseStageConflict(operation, java.util.UUID.randomUUID().toString()).also { refreshView("Review the new stage proposal") }
    }

    suspend fun discardStageConflict(operation: String) = withContext(Dispatchers.IO) {
        val account = active ?: throw AccessLocked(); check(account)
        account.store.discardStageConflict(operation); refreshView("Saved stage proposal discarded; current stage remains unchanged")
    }

    suspend fun complete(person: String, task: JSONObject? = null, creation: String? = null) =
        withContext(Dispatchers.IO) {
            val account = active ?: throw AccessLocked()
            check(account)
            account.store.complete(person, task, creation)
            refreshView("Completion saved on this device")
            FieldSyncJob.schedule(context)
        }

    suspend fun editSupported(): Boolean =
        withContext(Dispatchers.IO) { active?.store?.binding?.supportsEdits() == true }

    suspend fun supersedeConflict(id: String): JSONObject =
        withContext(Dispatchers.IO) {
            val account = active ?: throw AccessLocked()
            check(account)
            account.store.supersedeConflict(id).also { refreshView("Prepare a revised saved edit") }
        }

    suspend fun discardConflict(id: String) =
        withContext(Dispatchers.IO) {
            val account = active ?: throw AccessLocked()
            check(account)
            account.store.discardConflict(id)
            refreshView("Saved proposal discarded; current version remains unchanged")
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
            account.store.setPinIntent(id, value)
            refreshView("Offline selection will update at the next complete sync")
            requestSync(true)
        }
    suspend fun retryPinRequests() = withContext(Dispatchers.IO) { val account = active ?: throw AccessLocked(); check(account); account.store.retryPinRequests(); refreshView("Retrying requested downloads"); requestSync(true) }
    suspend fun clearOrganizationSearch() = withContext(Dispatchers.IO) { organizationSearchEpoch++; organizationSearchResults = emptyList(); organizationSearchHasMore = false; organizationSearchMessage = ""; organizationSearching = false; refreshView() }
    suspend fun searchOrganization(term: String) = withContext(Dispatchers.IO) { val account = active ?: throw AccessLocked(); check(account); if (!account.store.binding.supportsPeopleSearch()) { organizationSearchMessage = "Organization search is unavailable. Update the app to use this feature."; refreshView(); return@withContext }; val trimmed = term.trim(); if (trimmed.isEmpty()) { clearOrganizationSearch(); return@withContext }; if (trimmed.codePointCount(0, trimmed.length) > 200 || trimmed.toByteArray().size > 800) { organizationSearchMessage = "Use 200 characters or fewer."; refreshView(); return@withContext }; val run = ++organizationSearchEpoch; organizationSearching = true; organizationSearchMessage = "Searching the Organization…"; organizationSearchResults = emptyList(); organizationSearchHasMore = false; refreshView(organizationSearchMessage); try { val response = account.api.searchPeople(account.store.binding, trimmed); check(account); if (run != organizationSearchEpoch) return@withContext; val items = response.getJSONArray("items"); organizationSearchResults = (0 until items.length()).map { OrganizationSearchPerson.parse(items.getJSONObject(it)) }; organizationSearchHasMore = response.getBoolean("has_more"); organizationSearchMessage = if (organizationSearchResults.isEmpty()) "No matches. Search uses name fragments or an exact email or phone." else "" } catch (error: Exception) { if (run == organizationSearchEpoch) organizationSearchMessage = error.localizedMessage ?: "Search failed." } finally { if (run == organizationSearchEpoch) organizationSearching = false; if (active === account && run == organizationSearchEpoch) refreshView(organizationSearchMessage) } }

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
            val authorizedStore = FieldStore(account.store.db, binding, clock)
            authorizedStore.authorize(account.api.cookie)
            // Capabilities are current authorization state. Retaining the pre-refresh Binding
            // would strand schema-5 caches or keep withdrawn editor capabilities enabled.
            synchronized(accountChanges) {
                if (active !== account || epoch != account.epoch) throw AccessLocked()
                account.store = authorizedStore
            }
            upload(account, manual)
            try {
                download(account)
            } catch (error: ApiFailure) {
                // Reclaimed/expired generation rows can return404. Operation404 has a different
                // recovery path.
                if (error.status == 404 && error.code != "not_found") throw ApiFailure(409, "generation_expired")
                throw error
            }
            account.store.dao.removeMeta("sync_retry_at")
            account.store.dao.removeMeta("sync_failures")
            account.store.dao.removeMeta("retry_mandatory")
            automaticAttempts = 0
            refreshView("Up to date. Complete cache available offline.")
            if ((account.store.dao.pinSetRevision()?.toLongOrNull() ?: 0L) > (account.store.dao.admittedPinRevision()?.toLongOrNull() ?: 0L)) schedulePinFollowup()
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
                        val pinReviewRequired = error is ApiFailure && error.code == "not_found"
                        if (pinReviewRequired) {
                            account.store.markPinFailure("not_found")
                            // An invalid requested set is durable review state. Do not
                            // turn it into an automatic retry loop; the user can cancel
                            // the selected record or explicitly retry the remaining set.
                            automaticAttempts = 3
                        }
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

    private fun schedulePinFollowup() { scope.launch { delay(100); this@FieldRepository.sync(manual = false) } }

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
                if (error is ApiFailure && error.code in setOf("revision_conflict", "catalog_revision_conflict") && row.kind in setOf("edit_note", "update_task", "change_person_stage", "update_person_details", "update_person_metadata")) {
                    // The conflict response is deliberately content-free.  Fetching comparison
                    // data is a separate authorized, identity-fenced request; a failure leaves
                    // the immutable proposal in attention instead of inventing a new baseline.
                    dao.operationState(row.id, "attention", row.attempts + 1, 0, error.code)
                    try {
                        val payload = JSONObject(row.envelope).getJSONObject("payload")
                        val current =
                            if (row.kind == "edit_note")
                                account.api.currentNote(account.store.binding, row.person, payload.getString("note_id"))
                            else if (row.kind == "update_task")
                                account.api.currentTask(account.store.binding, row.person, payload.getString("task_id"))
                            else if (row.kind == "change_person_stage") account.api.currentStage(account.store.binding, row.person)
                            else if (row.kind == "update_person_details") currentProfile(account, row)
                            else currentMetadata(account, row)
                        check(account)
                        if (row.kind == "change_person_stage") account.store.recordCurrentStage(row.id, current)
                        else if (row.kind == "update_person_details") account.store.recordCurrentProfile(row.id, current)
                        else if (row.kind == "update_person_metadata") account.store.recordCurrentMetadata(row.id, current)
                        else account.store.recordCurrent(row.id, current)
                    } catch (_: Exception) {
                        // The explicit conflict remains reviewable without a guessed current version.
                    }
                    continue
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
                if (row.kind == "log_contact_attempt")
                    dao.contactState(row.id, if (retry) "saved" else "attention", code)
                if (retry) throw error
            }
        }
        refreshView("Saved actions checked. Downloading complete records…", true)
    }

    /** Traversal is context/operation fenced by the caller; it never becomes reconciliation data. */
    internal suspend fun currentProfile(account: ActiveAccount, row: OperationRow): JSONObject {
        val path = "/api/mobile/v1/people/${uuid(row.person)}/details"
        var cursor = ""
        val visited = mutableSetOf<String>()
        var output: JSONObject? = null
        val all = JSONArray()
        var details: String? = null
        var firstName: String? = null
        var lastName: String? = null
        while (true) {
            if (!visited.add(cursor)) throw ProtocolFailure()
            val page =
                account.api.page(path, account.store.binding, cursor, maximumBytes = 524_288)
            if (
                page.getString("context_id") != account.store.binding.context ||
                    page.getString("person_id") != row.person ||
                    page.getJSONArray("items").length() > 100
            )
                throw ProtocolFailure()
            val pageDetails = revision(page.getString("details_revision"))
            val pagePersonRevision = revision(page.getString("person_revision"))
            val pageFirst = if (page.get("first_name") === JSONObject.NULL) null else page.get("first_name") as String
            val pageLast = if (page.get("last_name") === JSONObject.NULL) null else page.get("last_name") as String
            validateDetailContacts(page.getJSONArray("items"))
            if (details == null) {
                details = pageDetails
                firstName = pageFirst
                lastName = pageLast
                output = JSONObject(page.toString())
            } else if (pageDetails != details || pageFirst != firstName || pageLast != lastName)
                throw ProtocolFailure()
            requireNotNull(output).put("person_revision", pagePersonRevision)
            page.getJSONArray("items").objects().forEach { all.put(it) }
            val next = page.stringOrNull("next_cursor")
            if (
                page.getBoolean("complete") != (next == null) ||
                    next == cursor ||
                    (next != null && (next.isEmpty() || next.toByteArray().size > 2_048))
            ) throw ProtocolFailure()
            if (next == null) break
            cursor = next
        }
        validateDetailContacts(all)
        return requireNotNull(output).put("items", all).put("next_cursor", JSONObject.NULL).put("complete", true)
    }

    internal suspend fun currentMetadata(account: ActiveAccount, row: OperationRow): JSONObject {
        val response = account.api.call("GET", "/api/mobile/v1/people/${uuid(row.person)}/metadata", account.store.binding.context, maximumBytes = 131_072)
        if (response.getString("context_id") != account.store.binding.context || response.getString("person_id") != row.person || !response.getBoolean("complete")) throw ProtocolFailure()
        return response
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
            val pinStage = store.preparePinStaging()
            val created =
                account.api.call(
                    "POST",
                    "/api/mobile/v1/reconciliations",
                    store.binding.context,
                    json(
                            "protocol" to PROTOCOL,
                            "installation_id" to store.binding.installation,
                            "pinned_person_ids" to JSONArray(pinStage.second),
                            "include_stage_catalog" to store.binding.supportsStageChanges(),
                            "include_metadata" to store.binding.supportsMetadata(),
                        )
                        .toString(),
                )
            check(account)
            store.beginGeneration(created, pinStage.first)
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
        if (generation.has("stage_catalog")) {
            while (true) {
                val cursor = store.nextStageCatalogPage(id) ?: break
                val page = account.api.page("/api/mobile/v1/reconciliations/$id/stages", store.binding, cursor)
                check(account); store.stageCatalogPage(id, cursor, page)
            }
        }
        if (generation.has("metadata")) {
            for (section in listOf("tags", "fields", "options")) while (true) {
                val cursor = store.nextMetadataCatalogPage(id, section) ?: break
                val page = account.api.page("/api/mobile/v1/reconciliations/$id/metadata/catalog/$section", store.binding, cursor, 524_288)
                check(account); store.metadataCatalogPage(id, section, cursor, page)
            }
        }
        for ((index, item) in manifest.withIndex()) {
            check(account)
            if (
                dao.person(item.person)?.let {
                    it.revision == item.revision &&
                        it.noteRevisionsQualified &&
                        (!generation.has("stage_catalog") || it.stageRevisionsQualified) &&
                        // A schema-5 cache has no details baseline even when its broad revision
                        // is current. Only a server that advertises Mobile005 details must fetch
                        // the complete contact traversal to qualify that new baseline.
                        (!store.binding.supportsPersonDetails() || store.detailsQualified(it))
                        && (!store.binding.supportsMetadata() || store.metadataQualified(
                            it,
                            generation.optJSONObject("metadata")?.getString("catalog_revision"),
                            item.metadataRevision,
                        ))
                } == true
            ) continue
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
                if (generation.has("metadata")) {
                val cursor = store.nextPage(id, item.person, "metadata") ?: ""
                if (cursor.isNotEmpty()) throw ProtocolFailure()
                val page = account.api.page("/api/mobile/v1/reconciliations/$id/people/${item.person}/metadata", store.binding, "", 524_288)
                check(account); store.stagePage(id, item.person, "metadata", "", page)
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
                val people = dao.people()
                val visibleIds = people.map { it.id }.toSet()
                val ops =
                    dao.operations()
                        .filter { it.status != "covered" }
                        .map {
                            if (it.person in visibleIds) it
                            else it.copy(envelope = "", receipt = null)
                        }
                val drafts =
                    dao.drafts().map { if (it.person in visibleIds) it else it.copy(payload = "") }
                val contacts =
                    dao.contactDrafts().map {
                        if (it.person in visibleIds) it
                        else it.copy(occurredAt = "", resolvedOffset = "")
                    }
                val stages = dao.stageDrafts().map { if (it.person in visibleIds) it else it.copy(baselineStageName = "", proposedStageName = "") }
                val contexts =
                    dao.editContexts().map {
                        if (it.person in visibleIds) it else it.copy(baseline = "", current = "")
                    }
                val stageContexts = dao.stageContexts().map { if (it.person in visibleIds) it else it.copy(baseline = "", proposal = "", current = "") }
                val profiles = dao.profileDrafts().map { if (it.person in visibleIds) it else it.copy(baseline = "", proposal = "") }
                val profileContexts = dao.profileContexts().map { if (it.person in visibleIds) it else it.copy(baseline = "", proposal = "", current = "") }
                val metadataDrafts = dao.metadataDrafts().map { if (it.person in visibleIds) it else it.copy(baseline = "", proposal = "") }
                val metadataContexts = dao.metadataContexts().map { if (it.person in visibleIds) it else it.copy(baseline = "", proposal = "", current = "") }
                registry.put(
                    "pending",
                    ops.count { it.status != "accepted" } +
                        drafts.size +
                        contacts.count { it.operation.isEmpty() } +
                        profiles.count { it.operation.isEmpty() },
                )
                vault.saveRegistry(registry)
                val result =
                    FieldUi(
                        false,
                        busy,
                        message,
                        people,
                        selected?.let { dao.person(it) }?.let { it.copy(detailsRevisionsQualified = account.store.detailsQualified(it)) },
                        ops,
                        drafts,
                        contacts,
                        stages,
                        dao.stageCatalog(),
                        dao.meta("today"),
                        dao.meta("last_sync") ?: "Never",
                        dao.meta("coverage") ?: "No complete download",
                        dao.meta("paused") == "true",
                        dao.meta("display_name") ?: "Field agent",
                        registry.getInt("pending"),
                        account.store.binding.supportsEdits(),
                        account.store.binding.supportsContactLogging(),
                        account.store.binding.supportsStageChanges(),
                        contexts,
                        stageContexts,
                        profiles,
                        profileContexts,
                        account.store.binding.supportsPersonDetails(),
                        metadataDrafts,
                        metadataContexts,
                        dao.metadataTags(),
                        dao.metadataFields(),
                        dao.allMetadataOptions(),
                        account.store.binding.supportsMetadata(),
                        dao.pins().toSet(), organizationSearchResults, organizationSearchHasMore, organizationSearching, organizationSearchMessage,
                        account.store.requestedPinProjection(people, organizationSearchResults), account.store.pinReviewRequired(), account.store.activeReasons(),
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
