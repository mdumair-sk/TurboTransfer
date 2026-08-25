package com.turbotransfer.presentation.settings

data class SettingsUiState(
    val deviceName: String = "",
    val prefer5Ghz: Boolean = true,
    val autoWakeLock: Boolean = true,
    val usbLabel: String = "USB 2.0",
    val userMessage: String? = null
)
