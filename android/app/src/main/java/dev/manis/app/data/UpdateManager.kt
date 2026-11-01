package dev.manis.app.data

import android.content.Context
import java.io.File
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

sealed interface UpdateState {
    data object Idle : UpdateState
    data object Checking : UpdateState
    data object UpToDate : UpdateState
    data class Available(val update: AvailableUpdate) : UpdateState
    data class Downloading(val update: AvailableUpdate, val progress: Int) : UpdateState
    data class Ready(val update: AvailableUpdate, val file: File) : UpdateState
    data class Failed(val message: String, val update: AvailableUpdate? = null) : UpdateState
}

/**
 * Application-scoped update state, so a download keeps running when Settings leaves composition
 * (for example after switching to another tab) and the install button is still there on return.
 */
internal object UpdateManager {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val mutableState = MutableStateFlow<UpdateState>(UpdateState.Idle)
    val state: StateFlow<UpdateState> = mutableState.asStateFlow()

    fun check(context: Context) {
        when (mutableState.value) {
            is UpdateState.Checking, is UpdateState.Downloading -> return
            else -> Unit
        }
        val appContext = context.applicationContext
        mutableState.value = UpdateState.Checking
        scope.launch {
            mutableState.value = UpdateChecker.check(appContext).fold(
                onSuccess = { update ->
                    when {
                        update == null -> UpdateState.UpToDate
                        else -> UpdateChecker.cachedApk(appContext, update.version)
                            ?.let { UpdateState.Ready(update, it) }
                            ?: UpdateState.Available(update)
                    }
                },
                onFailure = { UpdateState.Failed(it.message ?: "检查更新失败") },
            )
        }
    }

    fun download(context: Context, update: AvailableUpdate) {
        if (mutableState.value is UpdateState.Downloading) return
        val appContext = context.applicationContext
        mutableState.value = UpdateState.Downloading(update, 0)
        scope.launch {
            runCatching {
                UpdateChecker.download(appContext, update) { progress ->
                    mutableState.value = UpdateState.Downloading(update, progress)
                }
            }.onSuccess { file ->
                mutableState.value = UpdateState.Ready(update, file)
            }.onFailure { error ->
                mutableState.value = UpdateState.Failed(error.message ?: "下载失败", update)
            }
        }
    }
}
