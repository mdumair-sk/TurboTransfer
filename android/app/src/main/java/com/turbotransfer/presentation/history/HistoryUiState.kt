package com.turbotransfer.presentation.history

import com.turbotransfer.domain.model.HistoryItem

data class HistoryUiState(
    val historyList: List<HistoryItem> = emptyList(),
    val showClearDialog: Boolean = false
)
