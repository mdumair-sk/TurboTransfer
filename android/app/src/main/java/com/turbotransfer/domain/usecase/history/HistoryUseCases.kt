package com.turbotransfer.domain.usecase.history

import com.turbotransfer.domain.model.HistoryItem
import com.turbotransfer.domain.repository.HistoryRepository
import kotlinx.coroutines.flow.StateFlow
import javax.inject.Inject

class GetHistoryUseCase @Inject constructor(
    private val historyRepository: HistoryRepository
) {
    operator fun invoke(): StateFlow<List<HistoryItem>> {
        return historyRepository.historyFlow
    }
}

class AddHistoryRecordUseCase @Inject constructor(
    private val historyRepository: HistoryRepository
) {
    operator fun invoke(record: HistoryItem) {
        historyRepository.addTransferRecord(record)
    }
}

class DeleteHistoryRecordUseCase @Inject constructor(
    private val historyRepository: HistoryRepository
) {
    operator fun invoke(id: String) {
        historyRepository.deleteRecord(id)
    }
}

class ClearHistoryUseCase @Inject constructor(
    private val historyRepository: HistoryRepository
) {
    operator fun invoke() {
        historyRepository.clearHistory()
    }
}
