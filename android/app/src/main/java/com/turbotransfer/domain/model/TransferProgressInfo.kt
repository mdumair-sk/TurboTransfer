package com.turbotransfer.domain.model

enum class TransferStatus {
    IDLE,
    IN_PROGRESS,
    PAUSED,
    COMPLETED,
    FAILED,
    CANCELLED
}

data class TransferProgressInfo(
    val transferId: String,
    val fileName: String,
    val bytesTransferred: Long,
    val fileSize: Long,
    val percent: Double,
    val aggregateSpeedMBps: Double,
    val usbSpeedMBps: Double,
    val wifiSpeedMBps: Double,
    val etaSeconds: Long?,
    val status: TransferStatus
)
