package com.turbotransfer.presentation.send

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.turbotransfer.core.common.Resource
import com.turbotransfer.domain.model.SelectedFileInfo
import com.turbotransfer.domain.usecase.discovery.ObserveReceiverDiscoveryUseCase
import com.turbotransfer.domain.usecase.hotspot.ObserveHotspotStateUseCase
import com.turbotransfer.domain.usecase.hotspot.StartHotspotUseCase
import com.turbotransfer.domain.usecase.hotspot.StopHotspotUseCase
import com.turbotransfer.domain.usecase.transfer.StartTransferUseCase
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch
import javax.inject.Inject

@HiltViewModel
class SendViewModel @Inject constructor(
    private val observeReceiverDiscoveryUseCase: ObserveReceiverDiscoveryUseCase,
    private val observeHotspotStateUseCase: ObserveHotspotStateUseCase,
    private val startHotspotUseCase: StartHotspotUseCase,
    private val stopHotspotUseCase: StopHotspotUseCase,
    private val startTransferUseCase: StartTransferUseCase
) : ViewModel() {

    private val _uiState = MutableStateFlow(SendUiState())
    val uiState: StateFlow<SendUiState> = _uiState.asStateFlow()

    init {
        // Observe receiver discovery
        viewModelScope.launch {
            observeReceiverDiscoveryUseCase().collect { receiver ->
                _uiState.update { current ->
                    val updatedAddress = if (receiver != null && (current.customAddress == "127.0.0.1:9876" || current.customAddress.isEmpty() || current.customAddress.contains("127.0.0.1"))) {
                        receiver.address
                    } else {
                        current.customAddress
                    }
                    current.copy(
                        discoveredReceiver = receiver,
                        customAddress = updatedAddress
                    )
                }
            }
        }

        // Observe hotspot state
        viewModelScope.launch {
            observeHotspotStateUseCase().collect { hotspotState ->
                _uiState.update { it.copy(hotspotState = hotspotState) }
            }
        }
    }

    fun addFilesToQueue(files: List<SelectedFileInfo>) {
        _uiState.update { current ->
            val updated = current.transferQueue.toMutableList()
            for (f in files) {
                if (updated.none { it.path == f.path }) {
                    updated.add(f)
                }
            }
            current.copy(transferQueue = updated)
        }
    }

    fun removeFileFromQueue(file: SelectedFileInfo) {
        _uiState.update { current ->
            val updated = current.transferQueue.toMutableList()
            updated.remove(file)
            current.copy(transferQueue = updated)
        }
    }

    fun clearQueue() {
        _uiState.update { it.copy(transferQueue = emptyList(), isQueueRunning = false, currentQueueIndex = 0) }
    }

    fun setCustomAddress(address: String) {
        _uiState.update { it.copy(customAddress = address) }
    }

    fun toggleCustomAddressField() {
        _uiState.update { it.copy(showCustomAddressField = !it.showCustomAddressField) }
    }

    fun setShowQrDialog(show: Boolean) {
        _uiState.update { it.copy(showQrDialog = show) }
    }

    fun toggleHotspot() {
        val active = _uiState.value.hotspotState.isActive
        if (!active) {
            startHotspotUseCase { res ->
                when (res) {
                    is Resource.Success -> _uiState.update { it.copy(userMessage = res.data) }
                    is Resource.Error -> _uiState.update { it.copy(userMessage = res.message) }
                    is Resource.Loading -> {}
                }
            }
        } else {
            stopHotspotUseCase()
        }
    }

    fun startBatchTransfer(targetAddress: String, onTransferStarted: () -> Unit) {
        val queue = _uiState.value.transferQueue
        if (queue.isEmpty()) {
            _uiState.update { it.copy(userMessage = "Please select at least one file to send") }
            return
        }

        _uiState.update { it.copy(isQueueRunning = true, currentQueueIndex = 0) }
        processQueueItem(0, targetAddress, onTransferStarted)
    }

    private fun processQueueItem(index: Int, targetAddress: String, onTransferStarted: () -> Unit) {
        val queue = _uiState.value.transferQueue
        if (index < queue.size) {
            val item = queue[index]
            viewModelScope.launch {
                val res = startTransferUseCase(item.path, targetAddress)
                when (res) {
                    is Resource.Success -> {
                        _uiState.update { it.copy(currentQueueIndex = index) }
                        onTransferStarted()
                    }
                    is Resource.Error -> {
                        _uiState.update { it.copy(userMessage = "Error sending ${item.displayName}: ${res.message}") }
                        if (index + 1 < queue.size) {
                            processQueueItem(index + 1, targetAddress, onTransferStarted)
                        } else {
                            _uiState.update { it.copy(isQueueRunning = false) }
                        }
                    }
                    is Resource.Loading -> {}
                }
            }
        } else {
            _uiState.update { it.copy(isQueueRunning = false) }
        }
    }

    fun clearUserMessage() {
        _uiState.update { it.copy(userMessage = null) }
    }
}
