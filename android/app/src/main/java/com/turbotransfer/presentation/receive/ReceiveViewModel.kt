package com.turbotransfer.presentation.receive

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.turbotransfer.core.common.Resource
import com.turbotransfer.data.repository.DiscoveryRepositoryImpl
import com.turbotransfer.data.repository.HotspotRepositoryImpl
import com.turbotransfer.data.repository.SettingsRepositoryImpl
import com.turbotransfer.data.repository.TransferRepositoryImpl
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch
import javax.inject.Inject

@HiltViewModel
class ReceiveViewModel @Inject constructor(
    private val transferRepository: TransferRepositoryImpl,
    private val hotspotRepository: HotspotRepositoryImpl,
    private val discoveryRepository: DiscoveryRepositoryImpl,
    private val settingsRepository: SettingsRepositoryImpl
) : ViewModel() {

    private val _uiState = MutableStateFlow(ReceiveUiState())
    val uiState: StateFlow<ReceiveUiState> = _uiState.asStateFlow()

    init {
        _uiState.update {
            it.copy(
                destDir = settingsRepository.getReceiveDestDir()
            )
        }

        // Observe receiver listening state & status from repository
        viewModelScope.launch {
            transferRepository.isListeningFlow.collect { isListening ->
                _uiState.update { it.copy(isListening = isListening) }
            }
        }
        viewModelScope.launch {
            transferRepository.receiveStatusFlow.collect { status ->
                _uiState.update { it.copy(statusText = status) }
            }
        }
        // Observe hotspot state
        viewModelScope.launch {
            hotspotRepository.hotspotStateFlow.collect { hotspotState ->
                _uiState.update { it.copy(hotspotState = hotspotState) }
            }
        }

        // Observe active incoming transfer
        viewModelScope.launch {
            transferRepository.activeSessionFlow.collect { session ->
                _uiState.update { it.copy(activeIncomingSession = if (session?.isOutgoing == false) session else null) }
            }
        }
        // Auto-start receive mode and hotspot on screen entry
        toggleReceiveMode("0.0.0.0:9876")


        // Network & USB probe loop
        viewModelScope.launch {
            var previousUsb = false
            while (true) {
                val (usb, ips) = discoveryRepository.getNetworkInterfacesAndUsb()
                val usbNewlyConnected = usb && !previousUsb
                previousUsb = usb
                _uiState.update { current ->
                    current.copy(
                        usbAvailable = usb,
                        detectedIps = ips
                    )
                }
                if (usbNewlyConnected && !_uiState.value.isListening) {
                    toggleReceiveMode("0.0.0.0:9876")
                }
                delay(1500)
            }
        }
    }

    fun setDestinationDir(path: String) {
        settingsRepository.setReceiveDestDir(path)
        _uiState.update { it.copy(destDir = path) }
    }

    fun setShowQrDialog(show: Boolean) {
        _uiState.update { it.copy(showQrDialog = show) }
    }

    fun toggleHotspot() {
        val active = _uiState.value.hotspotState.isActive
        if (!active) {
            hotspotRepository.startHotspot(9876) { res ->
                when (res) {
                    is Resource.Success -> _uiState.update { it.copy(userMessage = res.data) }
                    is Resource.Error -> _uiState.update { it.copy(userMessage = res.message) }
                    is Resource.Loading -> {}
                }
            }
        } else {
            hotspotRepository.stopHotspot()
        }
    }

    fun toggleReceiveMode(address: String = "0.0.0.0:9876") {
        viewModelScope.launch {
            if (!_uiState.value.isListening) {
                val dest = _uiState.value.destDir
                val res = transferRepository.enterReceiveMode(dest, address)
                when (res) {
                    is Resource.Success -> {
                        _uiState.update { it.copy(isListening = true, statusText = res.data) }
                        if (!_uiState.value.hotspotState.isActive) {
                            hotspotRepository.startHotspot(9876) { }
                        }
                    }
                    is Resource.Error -> {
                        _uiState.update { it.copy(isListening = false, statusText = "Error: ${res.message}", userMessage = res.message) }
                    }
                    is Resource.Loading -> {}
                }
            } else {
                transferRepository.stopReceiveMode()
                _uiState.update { it.copy(isListening = false, statusText = "Receive listener stopped") }
            }
        }
    }

    fun clearUserMessage() {
        _uiState.update { it.copy(userMessage = null) }
    }
}
