package com.turbotransfer.presentation.transfer

import android.content.Context
import android.media.MediaScannerConnection
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.turbotransfer.UriUtils
import com.turbotransfer.core.common.Resource
import com.turbotransfer.domain.model.HistoryItem
import com.turbotransfer.domain.model.TransferProgressInfo
import com.turbotransfer.domain.model.TransferStatus
import com.turbotransfer.domain.usecase.history.AddHistoryRecordUseCase
import com.turbotransfer.domain.usecase.settings.GetSettingsUseCase
import com.turbotransfer.domain.usecase.transfer.CancelTransferUseCase
import com.turbotransfer.domain.usecase.transfer.ObserveActiveTransferUseCase
import com.turbotransfer.domain.usecase.transfer.ObserveTransferProgressUseCase
import com.turbotransfer.domain.usecase.transfer.PauseTransferUseCase
import com.turbotransfer.domain.usecase.transfer.ResumeTransferUseCase
import dagger.hilt.android.lifecycle.HiltViewModel
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch
import java.io.File
import javax.inject.Inject

@HiltViewModel
class TransferViewModel @Inject constructor(
    @ApplicationContext private val context: Context,
    private val observeActiveTransferUseCase: ObserveActiveTransferUseCase,
    private val observeTransferProgressUseCase: ObserveTransferProgressUseCase,
    private val pauseTransferUseCase: PauseTransferUseCase,
    private val resumeTransferUseCase: ResumeTransferUseCase,
    private val cancelTransferUseCase: CancelTransferUseCase,
    private val addHistoryRecordUseCase: AddHistoryRecordUseCase,
    private val getSettingsUseCase: GetSettingsUseCase
) : ViewModel() {

    private val _uiState = MutableStateFlow(TransferUiState())
    val uiState: StateFlow<TransferUiState> = _uiState.asStateFlow()

    private var speedSum = 0.0
    private var speedSampleCount = 0

    init {
        viewModelScope.launch {
            observeActiveTransferUseCase().collectLatest { session ->
                _uiState.update { it.copy(activeSession = session) }
                if (session != null) {
                    speedSum = 0.0
                    speedSampleCount = 0
                    _uiState.update {
                        it.copy(
                            peakTotalSpeed = 0.0,
                            peakUsbSpeed = 0.0,
                            peakWifiSpeed = 0.0
                        )
                    }

                    observeTransferProgressUseCase(session.transferId).collect { progress ->
                        handleProgressUpdate(progress, session)
                    }
                } else {
                    _uiState.update { it.copy(progress = null) }
                }
            }
        }
    }

    private fun handleProgressUpdate(progress: TransferProgressInfo?, session: com.turbotransfer.domain.model.TransferSession) {
        _uiState.update { it.copy(progress = progress) }
        if (progress == null) return

        val speed = progress.aggregateSpeedMBps
        val usbSpeed = progress.usbSpeedMBps
        val wifiSpeed = progress.wifiSpeedMBps

        _uiState.update { current ->
            current.copy(
                peakTotalSpeed = maxOf(current.peakTotalSpeed, speed),
                peakUsbSpeed = maxOf(current.peakUsbSpeed, usbSpeed),
                peakWifiSpeed = maxOf(current.peakWifiSpeed, wifiSpeed)
            )
        }

        if (speed > 0.1) {
            speedSum += speed
            speedSampleCount++
        }

        if (progress.status == TransferStatus.COMPLETED) {
            val durationMs = System.currentTimeMillis() - session.startTimeMs
            val avgSpeed = if (speedSampleCount > 0) speedSum / speedSampleCount else _uiState.value.peakTotalSpeed
            val saveDir = getSettingsUseCase.getReceiveDestDir()
            val finalPath = session.filePath.ifBlank { File(saveDir, progress.fileName).absolutePath }

            val item = HistoryItem(
                id = session.transferId,
                fileName = session.fileName.ifBlank { progress.fileName },
                fileSize = if (session.fileSize > 0) session.fileSize else progress.fileSize,
                formattedSize = UriUtils.formatFileSize(if (session.fileSize > 0) session.fileSize else progress.fileSize),
                filePath = finalPath,
                isOutgoing = session.isOutgoing,
                timestamp = System.currentTimeMillis(),
                formattedDate = "Just now",
                durationMs = durationMs,
                avgSpeedMBps = avgSpeed,
                peakSpeedMBps = _uiState.value.peakTotalSpeed,
                usbSpeedMBps = _uiState.value.peakUsbSpeed,
                wifiSpeedMBps = _uiState.value.peakWifiSpeed,
                status = "Completed"
            )

            addHistoryRecordUseCase(item)
            _uiState.update { it.copy(lastCompletedItem = item) }

            if (!session.isOutgoing && finalPath.isNotBlank()) {
                try {
                    MediaScannerConnection.scanFile(context, arrayOf(finalPath), null, null)
                } catch (_: Exception) {}
            }

            val verb = if (session.isOutgoing) "Sent" else "Received"
            _uiState.update { it.copy(userMessage = "$verb ${item.fileName} successfully!") }
        } else if (progress.status == TransferStatus.FAILED || progress.status == TransferStatus.CANCELLED) {
            val saveDir = getSettingsUseCase.getReceiveDestDir()
            val finalPath = session.filePath.ifBlank { File(saveDir, progress.fileName).absolutePath }
            val item = HistoryItem(
                id = session.transferId,
                fileName = session.fileName.ifBlank { progress.fileName },
                fileSize = if (session.fileSize > 0) session.fileSize else progress.fileSize,
                formattedSize = UriUtils.formatFileSize(if (session.fileSize > 0) session.fileSize else progress.fileSize),
                filePath = finalPath,
                isOutgoing = session.isOutgoing,
                timestamp = System.currentTimeMillis(),
                formattedDate = "Just now",
                durationMs = 0L,
                avgSpeedMBps = 0.0,
                peakSpeedMBps = 0.0,
                usbSpeedMBps = 0.0,
                wifiSpeedMBps = 0.0,
                status = if (progress.status == TransferStatus.FAILED) "Failed" else "Cancelled"
            )
            addHistoryRecordUseCase(item)
        }
    }

    fun pauseTransfer(transferId: String) {
        viewModelScope.launch {
            val res = pauseTransferUseCase(transferId)
            if (res is Resource.Error) {
                _uiState.update { it.copy(userMessage = "Error: ${res.message}") }
            }
        }
    }

    fun resumeTransfer(transferId: String) {
        viewModelScope.launch {
            val res = resumeTransferUseCase(transferId)
            if (res is Resource.Error) {
                _uiState.update { it.copy(userMessage = "Error: ${res.message}") }
            }
        }
    }

    fun cancelTransfer(transferId: String) {
        viewModelScope.launch {
            cancelTransferUseCase(transferId)
            _uiState.update { it.copy(progress = null, activeSession = null) }
        }
    }

    fun dismissCompleted() {
        _uiState.update { it.copy(lastCompletedItem = null) }
    }

    fun clearUserMessage() {
        _uiState.update { it.copy(userMessage = null) }
    }
}
