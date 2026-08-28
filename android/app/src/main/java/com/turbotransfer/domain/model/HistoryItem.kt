package com.turbotransfer.domain.model

data class HistoryItem(
    val id: String,
    val fileName: String,
    val fileSize: Long,
    val formattedSize: String,
    val filePath: String,
    val isOutgoing: Boolean, // true = Sent, false = Received
    val timestamp: Long,
    val formattedDate: String,
    val durationMs: Long,
    val avgSpeedMBps: Double,
    val peakSpeedMBps: Double,
    val usbSpeedMBps: Double = 0.0, // Average USB speed
    val wifiSpeedMBps: Double = 0.0, // Average Wi-Fi speed
    val peakUsbSpeedMBps: Double = 0.0, // Peak USB speed
    val peakWifiSpeedMBps: Double = 0.0, // Peak Wi-Fi speed
    val status: String // Completed, Failed, Cancelled
)
