package com.turbotransfer.presentation.send

import com.turbotransfer.domain.model.DiscoveredReceiverInfo
import com.turbotransfer.domain.model.HotspotStateInfo
import com.turbotransfer.domain.model.SelectedFileInfo

data class SendUiState(
    val transferQueue: List<SelectedFileInfo> = emptyList(),
    val currentQueueIndex: Int = 0,
    val isQueueRunning: Boolean = false,
    val discoveredReceiver: DiscoveredReceiverInfo? = null,
    val customAddress: String = "127.0.0.1:9876",
    val showCustomAddressField: Boolean = false,
    val showQrDialog: Boolean = false,
    val hotspotState: HotspotStateInfo = HotspotStateInfo(),
    val userMessage: String? = null
)
