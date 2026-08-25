package com.turbotransfer.presentation.settings

import com.turbotransfer.WifiDirectSpikeState

data class SettingsUiState(
    val deviceName: String = "",
    val prefer5Ghz: Boolean = true,
    val autoWakeLock: Boolean = true,
    val showDiagnostics: Boolean = false,
    val usbLabel: String = "USB 2.0",
    val spikeState: WifiDirectSpikeState = WifiDirectSpikeState(),
    val userMessage: String? = null
)
