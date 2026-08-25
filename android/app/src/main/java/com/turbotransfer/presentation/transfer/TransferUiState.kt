package com.turbotransfer.presentation.transfer

import com.turbotransfer.domain.model.HistoryItem
import com.turbotransfer.domain.model.SelectedFileInfo
import com.turbotransfer.domain.model.TransferProgressInfo
import com.turbotransfer.domain.model.TransferSession

data class TransferUiState(
    val activeSession: TransferSession? = null,
    val progress: TransferProgressInfo? = null,
    val lastCompletedItem: HistoryItem? = null,
    val transferQueue: List<SelectedFileInfo> = emptyList(),
    val currentQueueIndex: Int = 0,
    val peakTotalSpeed: Double = 0.0,
    val peakUsbSpeed: Double = 0.0,
    val peakWifiSpeed: Double = 0.0,
    val userMessage: String? = null
)
