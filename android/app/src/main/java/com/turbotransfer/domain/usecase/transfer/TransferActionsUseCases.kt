package com.turbotransfer.domain.usecase.transfer

import com.turbotransfer.core.common.Resource
import com.turbotransfer.domain.repository.TransferRepository
import javax.inject.Inject

class StartTransferUseCase @Inject constructor(
    private val transferRepository: TransferRepository
) {
    suspend operator fun invoke(filePath: String, address: String?, fileName: String? = null): Resource<String> {
        return transferRepository.startTransfer(filePath, address, fileName)
    }
}

class PauseTransferUseCase @Inject constructor(
    private val transferRepository: TransferRepository
) {
    suspend operator fun invoke(transferId: String): Resource<Unit> {
        return transferRepository.pauseTransfer(transferId)
    }
}

class ResumeTransferUseCase @Inject constructor(
    private val transferRepository: TransferRepository
) {
    suspend operator fun invoke(transferId: String): Resource<String> {
        return transferRepository.resumeTransfer(transferId)
    }
}

class CancelTransferUseCase @Inject constructor(
    private val transferRepository: TransferRepository
) {
    suspend operator fun invoke(transferId: String): Resource<Unit> {
        return transferRepository.cancelTransfer(transferId)
    }
}
