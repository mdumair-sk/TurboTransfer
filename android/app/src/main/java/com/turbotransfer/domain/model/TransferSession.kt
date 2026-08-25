package com.turbotransfer.domain.model

data class TransferSession(
    val transferId: String,
    val fileName: String,
    val fileSize: Long,
    val formattedSize: String,
    val filePath: String,
    val isOutgoing: Boolean, // true = Sending, false = Receiving
    val startTimeMs: Long = System.currentTimeMillis()
)
