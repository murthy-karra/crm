package org.crm.field

import android.app.Application
import android.app.job.JobInfo
import android.app.job.JobParameters
import android.app.job.JobScheduler
import android.app.job.JobService
import android.content.ComponentName
import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import kotlinx.coroutines.*

class FieldApplication : Application() {
    lateinit var repository: FieldRepository
    lateinit var ready: Deferred<Unit>

    override fun onCreate() {
        super.onCreate()
        repository = FieldRepository(this)
        ready = repository.scope.async { repository.restore() }
        (getSystemService(CONNECTIVITY_SERVICE) as ConnectivityManager)
            .registerDefaultNetworkCallback(
                object : ConnectivityManager.NetworkCallback() {
                    override fun onAvailable(network: Network) {
                        repository.scope.launch {
                            ready.await()
                            repository.requestSync()
                        }
                    }
                }
            )
    }
}

class FieldSyncJob : JobService() {
    private var work: Job? = null

    override fun onStartJob(params: JobParameters): Boolean {
        val app = application as FieldApplication
        work =
            app.repository.scope.launch {
                try {
                    app.ready.await()
                    app.repository.sync()
                } finally {
                    jobFinished(params, false)
                }
            }
        return true
    }

    override fun onStopJob(params: JobParameters): Boolean {
        work?.cancel()
        return true
    }

    companion object {
        fun schedule(context: Context, delaySeconds: Long = 30) {
            (context.getSystemService(Context.JOB_SCHEDULER_SERVICE) as JobScheduler).schedule(
                JobInfo.Builder(1001, ComponentName(context, FieldSyncJob::class.java))
                    .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY)
                    .setMinimumLatency(delaySeconds.coerceIn(30, 3600) * 1000)
                    .build()
            )
        }
    }
}
