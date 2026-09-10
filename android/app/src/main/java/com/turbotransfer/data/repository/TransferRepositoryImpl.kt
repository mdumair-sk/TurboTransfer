package com.turbotransfer.data.repository

import android.content.Context
import com.turbotransfer.UriUtils
import com.turbotransfer.core.common.DispatcherProvider
import com.turbotransfer.core.common.Resource
import com.turbotransfer.core.util.TransferLockManager
import com.turbotransfer.data.source.local.SettingsLocalDataSource
import com.turbotransfer.data.source.rust.RustCoreDataSource
import com.turbotransfer.domain.model.TransferProgressInfo
import com.turbotransfer.domain.model.TransferSession
import com.turbotransfer.domain.model.TransferStatus
import com.turbotransfer.service.TransferService
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import uniffi.turbotransfer_core.FfiTransferRole
import uniffi.turbotransfer_core.FfiTransferStatus
import uniffi.turbotransfer_core.FfiTransportPreference
import java.io.File
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
open class TransferRepositoryImpl(
    private val context: Context?,
    private val rustCoreDataSource: RustCoreDataSource?,
    private val settingsLocalDataSource: SettingsLocalDataSource?,
    private val transferLockManager: TransferLockManager?,
    private val dispatcherProvider: DispatcherProvider?,
    @Suppress("UNUSED_PARAMETER") dummy: Unit?
) {
    @Inject
    constructor(
        @ApplicationContext context: Context,
        rustCoreDataSource: RustCoreDataSource,
        settingsLocalDataSource: SettingsLocalDataSource,
        transferLockManager: TransferLockManager,
        dispatcherProvider: DispatcherProvider
    ) : this(context, rustCoreDataSource, settingsLocalDataSource, transferLockManager, dispatcherProvider, null)

    constructor() : this(null, null, null, null, null, null)

    private val _activeSessionFlow = MutableStateFlow<TransferSession?>(null)
    open val activeSessionFlow: StateFlow<TransferSession?> = _activeSessionFlow.asStateFlow()

    private val _isListeningFlow = MutableStateFlow(false)
    open val isListeningFlow: StateFlow<Boolean> = _isListeningFlow.asStateFlow()

    private val _receiveStatusFlow = MutableStateFlow("Idle")
    open val receiveStatusFlow: StateFlow<String> = _receiveStatusFlow.asStateFlow()

    private val _receiveDestDirFlow by lazy { MutableStateFlow(settingsLocalDataSource?.getReceiveDestDir() ?: "/sdcard/Download") }
    open val receiveDestDirFlow: StateFlow<String> get() = _receiveDestDirFlow.asStateFlow()

    open fun setReceiveDestDir(path: String) {
        settingsLocalDataSource?.setReceiveDestDir(path)
        _receiveDestDirFlow.value = path
    }

    open fun setActiveSession(session: TransferSession?) {
        _activeSessionFlow.value = session
        if (session != null && context != null) {
            TransferService.start(context, session.transferId)
        }
    }

    open fun clearActiveSession() {
        _activeSessionFlow.value = null
        if (context != null) {
            TransferService.stop(context)
        }
    }

    open suspend fun startTransfer(filePath: String, address: String?, fileName: String?): Resource<String> {
        // Ensure local receiver is stopped so port 9876 is released for outgoing USB/ADB tunnel
        rustCoreDataSource?.stopReceiveMode()
        _isListeningFlow.value = false

        val ds = rustCoreDataSource ?: return Resource.Error("RustCoreDataSource unavailable")
        val result = ds.startTransfer(
            filePath = filePath,
            fileName = fileName,
            deviceId = null,
            transportPref = FfiTransportPreference.AUTOMATIC,
            address = address?.ifBlank { null }
        )
        return result.fold(
            onSuccess = { transferId ->
                val name = fileName ?: File(filePath).name
                val size = if (File(filePath).exists()) File(filePath).length() else 0L
                val session = TransferSession(
                    transferId = transferId,
                    fileName = name,
                    fileSize = size,
                    formattedSize = UriUtils.formatFileSize(size),
                    filePath = filePath,
                    isOutgoing = true
                )
                _activeSessionFlow.value = session
                if (context != null) { TransferService.start(context, transferId) }
                Resource.Success(transferId)
            },
            onFailure = { error ->
                Resource.Error(error.message ?: "Failed to start transfer", error)
            }
        )
    }

    open fun observeTransferProgress(transferId: String): Flow<TransferProgressInfo?> = flow {
        transferLockManager?.acquireLocks()
        try {
            while (true) {
                val progress = rustCoreDataSource?.getProgress(transferId)
                emit(progress)

                if (progress == null ||
                    progress.status == TransferStatus.COMPLETED ||
                    progress.status == TransferStatus.FAILED ||
                    progress.status == TransferStatus.CANCELLED
                ) {
                    break
                }
                delay(250)
            }
        } finally {
            transferLockManager?.releaseLocks()
        }
    }.flowOn(dispatcherProvider?.io ?: kotlinx.coroutines.Dispatchers.IO)

    open suspend fun enterReceiveMode(destDir: String, address: String?): Resource<String> {
        transferLockManager?.acquireLocks()
        val ds = rustCoreDataSource ?: return Resource.Error("RustCoreDataSource unavailable")
        val result = ds.enterReceiveMode(address, destDir)
        return result.fold(
            onSuccess = { statusMsg ->
                _isListeningFlow.value = true
                _receiveStatusFlow.value = statusMsg
                Resource.Success(statusMsg)
            },
            onFailure = { error ->
                transferLockManager?.releaseLocks()
                Resource.Error(error.message ?: "Failed to enter receive mode", error)
            }
        )
    }

    open suspend fun stopReceiveMode(): Boolean {
        val stopped = rustCoreDataSource?.stopReceiveMode() ?: false
        transferLockManager?.releaseLocks()
        _isListeningFlow.value = false
        _receiveStatusFlow.value = "Receive listener stopped"
        return stopped
    }

    open suspend fun pauseTransfer(transferId: String): Resource<Unit> {
        val ds = rustCoreDataSource ?: return Resource.Error("RustCoreDataSource unavailable")
        val result = ds.pauseTransfer(transferId)
        return result.fold(
            onSuccess = { Resource.Success(Unit) },
            onFailure = { Resource.Error(it.message ?: "Failed to pause transfer", it) }
        )
    }

    open suspend fun resumeTransfer(transferId: String): Resource<String> {
        val ds = rustCoreDataSource ?: return Resource.Error("RustCoreDataSource unavailable")
        val result = ds.resumeTransfer(transferId, FfiTransportPreference.AUTOMATIC)
        return result.fold(
            onSuccess = { Resource.Success(it) },
            onFailure = { Resource.Error(it.message ?: "Failed to resume transfer", it) }
        )
    }

    open suspend fun cancelTransfer(transferId: String): Resource<Unit> {
        val ds = rustCoreDataSource ?: return Resource.Error("RustCoreDataSource unavailable")
        val result = ds.cancelTransfer(transferId)
        _activeSessionFlow.value = null
        return result.fold(
            onSuccess = { Resource.Success(Unit) },
            onFailure = { Resource.Error(it.message ?: "Failed to cancel transfer", it) }
        )
    }

    open suspend fun pollPendingIncomingTransfer(saveDir: String): TransferSession? {
        val ds = rustCoreDataSource ?: return null
        val transfers = ds.getTransfers()
        val activeTransfer = transfers.firstOrNull {
            it.status == FfiTransferStatus.IN_PROGRESS && it.role == FfiTransferRole.RECEIVER
        } ?: transfers.firstOrNull {
            it.status == FfiTransferStatus.IN_PROGRESS
        }

        return if (activeTransfer != null) {
            val current = _activeSessionFlow.value
            if (current == null || current.transferId != activeTransfer.transferId) {
                val resolvedPath = File(saveDir, activeTransfer.fileName).absolutePath
                val isOut = (activeTransfer.role == FfiTransferRole.SENDER)
                val session = TransferSession(
                    transferId = activeTransfer.transferId,
                    fileName = activeTransfer.fileName,
                    fileSize = activeTransfer.fileSize.toLong(),
                    formattedSize = UriUtils.formatFileSize(activeTransfer.fileSize.toLong()),
                    filePath = resolvedPath,
                    isOutgoing = isOut,
                    startTimeMs = System.currentTimeMillis()
                )
                _activeSessionFlow.value = session
                if (context != null) { TransferService.start(context, activeTransfer.transferId) }
                session
            } else {
                current
            }
        } else {
            null
        }
    }
}
