package com.turbotransfer.domain.model

data class DiscoveredReceiverInfo(
    val address: String,
    val displayName: String,
    val transport: String,
    val isReady: Boolean,
    val isUsbAvailable: Boolean = false,
    val isWifiAvailable: Boolean = false
)
