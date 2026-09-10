package com.turbotransfer.data.repository

import com.turbotransfer.data.source.local.HistoryLocalDataSource
import com.turbotransfer.domain.model.HistoryItem
import kotlinx.coroutines.flow.StateFlow
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
open class HistoryRepositoryImpl(
    private val localDataSource: HistoryLocalDataSource?,
    @Suppress("UNUSED_PARAMETER") dummy: Unit?
) {
    @Inject
    constructor(
        localDataSource: HistoryLocalDataSource
    ) : this(localDataSource, null)

    constructor() : this(null, null)

    open val historyFlow: StateFlow<List<HistoryItem>> by lazy { localDataSource?.historyFlow ?: kotlinx.coroutines.flow.MutableStateFlow(emptyList()) }

    open fun addTransferRecord(record: HistoryItem) {
        localDataSource?.addTransferRecord(record)
    }

    open fun deleteRecord(id: String) {
        localDataSource?.deleteRecord(id)
    }

    open fun clearHistory() {
        localDataSource?.clearHistory()
    }
}
