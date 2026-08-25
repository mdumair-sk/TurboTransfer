package com.turbotransfer.data.repository

import com.turbotransfer.data.source.local.HistoryLocalDataSource
import com.turbotransfer.domain.model.HistoryItem
import com.turbotransfer.domain.repository.HistoryRepository
import kotlinx.coroutines.flow.StateFlow
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class HistoryRepositoryImpl @Inject constructor(
    private val localDataSource: HistoryLocalDataSource
) : HistoryRepository {

    override val historyFlow: StateFlow<List<HistoryItem>> = localDataSource.historyFlow

    override fun addTransferRecord(record: HistoryItem) {
        localDataSource.addTransferRecord(record)
    }

    override fun deleteRecord(id: String) {
        localDataSource.deleteRecord(id)
    }

    override fun clearHistory() {
        localDataSource.clearHistory()
    }
}
