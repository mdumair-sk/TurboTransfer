package com.turbotransfer.presentation.receive

import com.turbotransfer.domain.model.HotspotStateInfo
import com.turbotransfer.domain.model.TransferSession

data class ReceiveUiState(
    val isListening: Boolean = false,
    val statusText: String = "Idle",
    val destDir: String = "",
    val usbAvailable: Boolean = false,
    val detectedIps: List<String> = emptyList(),
    val hotspotState: HotspotStateInfo = HotspotStateInfo(),
    val showQrDialog: Boolean = false,
    val activeIncomingSession: TransferSession? = null,
    val userMessage: String? = null
) {
    val isDualChannelReady: Boolean
        get() = usbAvailable && (hotspotState.isActive || detectedIps.isNotEmpty())

    val primaryIp: String
        get() = if (hotspotState.isActive) "192.168.43.1" else detectedIps.firstOrNull() ?: "127.0.0.1"
}
