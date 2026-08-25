package com.turbotransfer.domain.model

data class HotspotCredentials(
    val ssid: String,
    val passphrase: String,
    val ip: String,
    val port: Int,
    val band: String
)

data class HotspotStateInfo(
    val isActive: Boolean = false,
    val credentials: HotspotCredentials? = null,
    val isListening: Boolean = false,
    val connectedClients: List<String> = emptyList(),
    val totalBytesReceived: Long = 0L,
    val statusMessage: String = "Idle"
)
