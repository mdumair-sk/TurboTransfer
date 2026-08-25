package com.turbotransfer.data.repository

import com.turbotransfer.UriUtils
import com.turbotransfer.core.common.DispatcherProvider
import com.turbotransfer.core.common.Resource
import com.turbotransfer.core.util.TransferLockManager
import com.turbotransfer.data.source.local.SettingsLocalDataSource
import com.turbotransfer.data.source.rust.RustCoreDataSource
import com.turbotransfer.domain.model.TransferProgressInfo
import com.turbotransfer.domain.model.TransferSession
import com.turbotransfer.domain.model.TransferStatus
import com.turbotransfer.domain.repository.TransferRepository
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
class TransferRepositoryImpl @Inject constructor(
    private val rustCoreDataSource: RustCoreDataSource,
    private val settingsLocalDataSource: SettingsLocalDataSource,
    private val transferLockManager: TransferLockManager,
    private val dispatcherProvider: DispatcherProvider
) : TransferRepository {

    private val _activeSessionFlow = MutableStateFlow<TransferSession?>(null)
    override val activeSessionFlow: StateFlow<TransferSession?> = _activeSessionFlow.asStateFlow()

    private val _isListeningFlow = MutableStateFlow(false)
    override val isListeningFlow: StateFlow<Boolean> = _isListeningFlow.asStateFlow()

    private val _receiveStatusFlow = MutableStateFlow("Idle")
    override val receiveStatusFlow: StateFlow<String> = _receiveStatusFlow.asStateFlow()

    private val _receiveDestDirFlow = MutableStateFlow(settingsLocalDataSource.getReceiveDestDir())
    override val receiveDestDirFlow: StateFlow<String> = _receiveDestDirFlow.asStateFlow()

    override fun setReceiveDestDir(path: String) {
        settingsLocalDataSource.setReceiveDestDir(path)
        _receiveDestDirFlow.value = path
    }

    override fun setActiveSession(session: TransferSession?) {
        _activeSessionFlow.value = session
    }

    override fun clearActiveSession() {
        _activeSessionFlow.value = null
    }

    override suspend fun startTransfer(filePath: String, address: String?, fileName: String?): Resource<String> {
        val result = rustCoreDataSource.startTransfer(
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
                Resource.Success(transferId)
            },
            onFailure = { error ->
                Resource.Error(error.message ?: "Failed to start transfer", error)
            }
        )
    }

    override fun observeTransferProgress(transferId: String): Flow<TransferProgressInfo?> = flow {
        transferLockManager.acquireLocks()
        try {
            while (true) {
                val progress = rustCoreDataSource.getProgress(transferId)
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
            transferLockManager.releaseLocks()
        }
    }.flowOn(dispatcherProvider.io)

    override suspend fun enterReceiveMode(destDir: String, address: String?): Resource<String> {
        transferLockManager.acquireLocks()
        val result = rustCoreDataSource.enterReceiveMode(address, destDir)
        return result.fold(
            onSuccess = { statusMsg ->
                _isListeningFlow.value = true
                _receiveStatusFlow.value = statusMsg
                Resource.Success(statusMsg)
            },
            onFailure = { error ->
                transferLockManager.releaseLocks()
                Resource.Error(error.message ?: "Failed to enter receive mode", error)
            }
        )
    }

    override suspend fun stopReceiveMode(): Boolean {
        val stopped = rustCoreDataSource.stopReceiveMode()
        transferLockManager.releaseLocks()
        _isListeningFlow.value = false
        _receiveStatusFlow.value = "Receive listener stopped"
        return stopped
    }

    override suspend fun pauseTransfer(transferId: String): Resource<Unit> {
        val result = rustCoreDataSource.pauseTransfer(transferId)
        return result.fold(
            onSuccess = { Resource.Success(Unit) },
            onFailure = { Resource.Error(it.message ?: "Failed to pause transfer", it) }
        )
    }

    override suspend fun resumeTransfer(transferId: String): Resource<String> {
        val result = rustCoreDataSource.resumeTransfer(transferId, FfiTransportPreference.AUTOMATIC)
        return result.fold(
            onSuccess = { Resource.Success(it) },
            onFailure = { Resource.Error(it.message ?: "Failed to resume transfer", it) }
        )
    }

    override suspend fun cancelTransfer(transferId: String): Resource<Unit> {
        val result = rustCoreDataSource.cancelTransfer(transferId)
        _activeSessionFlow.value = null
        return result.fold(
            onSuccess = { Resource.Success(Unit) },
            onFailure = { Resource.Error(it.message ?: "Failed to cancel transfer", it) }
        )
    }

    override suspend fun pollPendingIncomingTransfer(saveDir: String): TransferSession? {
        val transfers = rustCoreDataSource.getTransfers()
        val activeTransfer = transfers.firstOrNull {
            it.status == FfiTransferStatus.IN_PROGRESS && it.role == FfiTransferRole.RECEIVER
        } ?: transfers.firstOrNull {
            it.status == FfiTransferStatus.IN_PROGRESS
        }

        return if (activeTransfer != null) {
            val resolvedPath = File(saveDir, activeTransfer.fileName).absolutePath
            val isOut = (activeTransfer.role == FfiTransferRole.SENDER)
            TransferSession(
                transferId = activeTransfer.transferId,
                fileName = activeTransfer.fileName,
                fileSize = activeTransfer.fileSize.toLong(),
                formattedSize = UriUtils.formatFileSize(activeTransfer.fileSize.toLong()),
                filePath = resolvedPath,
                isOutgoing = isOut,
                startTimeMs = System.currentTimeMillis()
            )
        } else {
            null
        }
    }
}
