package com.turbotransfer.data.source.rust

import android.util.Log
import com.turbotransfer.core.common.DispatcherProvider
import com.turbotransfer.domain.model.TransferProgressInfo
import com.turbotransfer.domain.model.TransferStatus
import kotlinx.coroutines.withContext
import uniffi.turbotransfer_core.*
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class RustCoreDataSource @Inject constructor(
    private val dispatcherProvider: DispatcherProvider
) {
    suspend fun startTransfer(
        filePath: String,
        fileName: String? = null,
        deviceId: String? = null,
        transportPref: FfiTransportPreference = FfiTransportPreference.AUTOMATIC,
        address: String? = null
    ): Result<String> = withContext(dispatcherProvider.io) {
        try {
            Log.i("TurboTransfer", "RustCoreDataSource: starting transfer for path=$filePath, address=$address")
            val handle = uniffi.turbotransfer_core.startTransfer(
                filePath = filePath,
                fileName = fileName,
                deviceId = deviceId,
                transportPref = transportPref,
                address = address?.ifBlank { null }
            )
            Log.i("TurboTransfer", "RustCoreDataSource: startTransfer SUCCESS transferId=${handle.transferId}")
            Result.success(handle.transferId)
        } catch (e: Exception) {
            Log.e("TurboTransfer", "RustCoreDataSource: startTransfer FAILED for path=$filePath, address=$address: ${e.message}", e)
            Result.failure(e)
        }
    }

    suspend fun enterReceiveMode(
        address: String?,
        destDir: String
    ): Result<String> = withContext(dispatcherProvider.io) {
        try {
            val status = uniffi.turbotransfer_core.enterReceiveMode(
                address = address?.ifBlank { null },
                destDir = destDir
            )
            Result.success(status)
        } catch (e: Exception) {
            if (e.message?.contains("already active") == true) {
                Result.success("Listening on ${address?.ifBlank { "0.0.0.0:9876" } ?: "0.0.0.0:9876"}")
            } else {
                Result.failure(e)
            }
        }
    }

    suspend fun stopReceiveMode(): Boolean = withContext(dispatcherProvider.io) {
        try {
            uniffi.turbotransfer_core.stopReceiveMode()
        } catch (e: Exception) {
            false
        }
    }

    suspend fun getProgress(transferId: String): TransferProgressInfo? = withContext(dispatcherProvider.io) {
        try {
            val ffiProgress = uniffi.turbotransfer_core.getProgress(transferId) ?: return@withContext null
            val status = when (ffiProgress.status) {
                FfiTransferStatus.IN_PROGRESS -> TransferStatus.IN_PROGRESS
                FfiTransferStatus.PAUSED -> TransferStatus.PAUSED
                FfiTransferStatus.COMPLETED -> TransferStatus.COMPLETED
                FfiTransferStatus.FAILED -> TransferStatus.FAILED
                FfiTransferStatus.CANCELLED -> TransferStatus.CANCELLED
            }
            val aggregateSpeedMBps = ffiProgress.aggregateThroughputBps / (1024.0 * 1024.0)
            val usbSpeedMBps = ffiProgress.usbThroughputBps / (1024.0 * 1024.0)
            val wifiSpeedMBps = ffiProgress.wifiThroughputBps / (1024.0 * 1024.0)

            TransferProgressInfo(
                transferId = ffiProgress.transferId,
                fileName = ffiProgress.fileName,
                bytesTransferred = ffiProgress.bytesTransferred.toLong(),
                fileSize = ffiProgress.fileSize.toLong(),
                percent = ffiProgress.percent,
                aggregateSpeedMBps = aggregateSpeedMBps,
                usbSpeedMBps = usbSpeedMBps,
                wifiSpeedMBps = wifiSpeedMBps,
                etaSeconds = ffiProgress.etaSeconds?.toLong(),
                status = status
            )
        } catch (e: Exception) {
            null
        }
    }

    suspend fun getTransfers(): List<FfiTransferSummary> = withContext(dispatcherProvider.io) {
        try {
            uniffi.turbotransfer_core.getTransfers()
        } catch (e: Exception) {
            emptyList()
        }
    }

    suspend fun cancelTransfer(transferId: String): Result<Unit> = withContext(dispatcherProvider.io) {
        try {
            uniffi.turbotransfer_core.cancelTransfer(transferId)
            Result.success(Unit)
        } catch (e: Exception) {
            Result.failure(e)
        }
    }

    suspend fun pauseTransfer(transferId: String): Result<Unit> = withContext(dispatcherProvider.io) {
        try {
            uniffi.turbotransfer_core.pauseTransfer(transferId)
            Result.success(Unit)
        } catch (e: Exception) {
            Result.failure(e)
        }
    }

    suspend fun resumeTransfer(
        transferId: String,
        transportPref: FfiTransportPreference = FfiTransportPreference.AUTOMATIC
    ): Result<String> = withContext(dispatcherProvider.io) {
        try {
            val handle = uniffi.turbotransfer_core.resumeTransfer(transferId, transportPref)
            Result.success(handle.transferId)
        } catch (e: Exception) {
            Result.failure(e)
        }
    }

    suspend fun getTransferBottleneckReport(transferId: String): FfiBottleneckReport? = withContext(dispatcherProvider.io) {
        try {
            uniffi.turbotransfer_core.getTransferBottleneckReport(transferId)
        } catch (e: Exception) {
            null
        }
    }

    suspend fun getTransferLogs(transferId: String, maxEvents: Long? = null): List<FfiTransferEvent> = withContext(dispatcherProvider.io) {
        try {
            uniffi.turbotransfer_core.getTransferLogs(transferId, maxEvents?.toUInt())
        } catch (e: Exception) {
            emptyList()
        }
    }

    suspend fun getTransferLogJson(transferId: String): String = withContext(dispatcherProvider.io) {
        try {
            uniffi.turbotransfer_core.getTransferLogJson(transferId)
        } catch (e: Exception) {
            "{}"
        }
    }

    suspend fun listTransferLogs(): List<FfiTransferLogSummary> = withContext(dispatcherProvider.io) {
        try {
            uniffi.turbotransfer_core.listTransferLogs()
        } catch (e: Exception) {
            emptyList()
        }
    }

    suspend fun exportTransferLogs(transferId: String, outputDir: String? = null): Result<String> = withContext(dispatcherProvider.io) {
        try {
            val path = uniffi.turbotransfer_core.exportTransferLogs(transferId, outputDir)
            Result.success(path)
        } catch (e: Exception) {
            Result.failure(e)
        }
    }
}
