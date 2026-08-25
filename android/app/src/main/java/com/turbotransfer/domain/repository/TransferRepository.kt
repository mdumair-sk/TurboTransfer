package com.turbotransfer.domain.repository

import com.turbotransfer.core.common.Resource
import com.turbotransfer.domain.model.TransferProgressInfo
import com.turbotransfer.domain.model.TransferSession
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.StateFlow

interface TransferRepository {
    val activeSessionFlow: StateFlow<TransferSession?>
    val isListeningFlow: StateFlow<Boolean>
    val receiveStatusFlow: StateFlow<String>
    val receiveDestDirFlow: StateFlow<String>

    fun setReceiveDestDir(path: String)
    suspend fun startTransfer(filePath: String, address: String?, fileName: String? = null): Resource<String>
    fun observeTransferProgress(transferId: String): Flow<TransferProgressInfo?>
    suspend fun enterReceiveMode(destDir: String, address: String?): Resource<String>
    suspend fun stopReceiveMode(): Boolean
    suspend fun pauseTransfer(transferId: String): Resource<Unit>
    suspend fun resumeTransfer(transferId: String): Resource<String>
    suspend fun cancelTransfer(transferId: String): Resource<Unit>
    fun setActiveSession(session: TransferSession?)
    fun clearActiveSession()
    suspend fun pollPendingIncomingTransfer(saveDir: String): TransferSession?
}
