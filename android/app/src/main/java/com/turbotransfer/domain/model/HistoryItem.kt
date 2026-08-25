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
    val usbSpeedMBps: Double,
    val wifiSpeedMBps: Double,
    val status: String // Completed, Failed, Cancelled
)
