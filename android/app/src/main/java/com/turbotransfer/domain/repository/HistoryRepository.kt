package com.turbotransfer.domain.repository

import com.turbotransfer.domain.model.HistoryItem
import kotlinx.coroutines.flow.StateFlow

interface HistoryRepository {
    val historyFlow: StateFlow<List<HistoryItem>>
    fun addTransferRecord(record: HistoryItem)
    fun deleteRecord(id: String)
    fun clearHistory()
}
