package com.turbotransfer.domain.usecase.transfer

import com.turbotransfer.core.common.Resource
import com.turbotransfer.domain.model.TransferProgressInfo
import com.turbotransfer.domain.model.TransferSession
import com.turbotransfer.domain.repository.TransferRepository
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.StateFlow
import javax.inject.Inject

class ObserveActiveTransferUseCase @Inject constructor(
    private val transferRepository: TransferRepository
) {
    operator fun invoke(): StateFlow<TransferSession?> {
        return transferRepository.activeSessionFlow
    }
}

class ObserveTransferProgressUseCase @Inject constructor(
    private val transferRepository: TransferRepository
) {
    operator fun invoke(transferId: String): Flow<TransferProgressInfo?> {
        return transferRepository.observeTransferProgress(transferId)
    }
}

class EnterReceiveModeUseCase @Inject constructor(
    private val transferRepository: TransferRepository
) {
    suspend operator fun invoke(destDir: String, address: String?): Resource<String> {
        return transferRepository.enterReceiveMode(destDir, address)
    }
}

class StopReceiveModeUseCase @Inject constructor(
    private val transferRepository: TransferRepository
) {
    suspend operator fun invoke(): Boolean {
        return transferRepository.stopReceiveMode()
    }
}

class PollIncomingTransferUseCase @Inject constructor(
    private val transferRepository: TransferRepository
) {
    suspend operator fun invoke(saveDir: String): TransferSession? {
        return transferRepository.pollPendingIncomingTransfer(saveDir)
    }
}
