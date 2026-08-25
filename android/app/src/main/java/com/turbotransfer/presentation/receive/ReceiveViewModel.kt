package com.turbotransfer.presentation.receive

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.turbotransfer.core.common.Resource
import com.turbotransfer.domain.usecase.discovery.GetNetworkStatusUseCase
import com.turbotransfer.domain.usecase.discovery.GetUsbSpeedLabelUseCase
import com.turbotransfer.domain.usecase.hotspot.ObserveHotspotStateUseCase
import com.turbotransfer.domain.usecase.hotspot.StartHotspotUseCase
import com.turbotransfer.domain.usecase.hotspot.StopHotspotUseCase
import com.turbotransfer.domain.usecase.settings.GetSettingsUseCase
import com.turbotransfer.domain.usecase.settings.UpdateSettingsUseCase
import com.turbotransfer.domain.usecase.transfer.EnterReceiveModeUseCase
import com.turbotransfer.domain.usecase.transfer.ObserveActiveTransferUseCase
import com.turbotransfer.domain.usecase.transfer.StopReceiveModeUseCase
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch
import javax.inject.Inject

@HiltViewModel
class ReceiveViewModel @Inject constructor(
    private val enterReceiveModeUseCase: EnterReceiveModeUseCase,
    private val stopReceiveModeUseCase: StopReceiveModeUseCase,
    private val observeActiveTransferUseCase: ObserveActiveTransferUseCase,
    private val observeHotspotStateUseCase: ObserveHotspotStateUseCase,
    private val startHotspotUseCase: StartHotspotUseCase,
    private val stopHotspotUseCase: StopHotspotUseCase,
    private val getNetworkStatusUseCase: GetNetworkStatusUseCase,
    private val getUsbSpeedLabelUseCase: GetUsbSpeedLabelUseCase,
    private val getSettingsUseCase: GetSettingsUseCase,
    private val updateSettingsUseCase: UpdateSettingsUseCase
) : ViewModel() {

    private val _uiState = MutableStateFlow(ReceiveUiState())
    val uiState: StateFlow<ReceiveUiState> = _uiState.asStateFlow()

    init {
        _uiState.update {
            it.copy(
                destDir = getSettingsUseCase.getReceiveDestDir(),
                usbLabel = getUsbSpeedLabelUseCase()
            )
        }

        // Observe hotspot state
        viewModelScope.launch {
            observeHotspotStateUseCase().collect { hotspotState ->
                _uiState.update { it.copy(hotspotState = hotspotState) }
            }
        }

        // Observe active incoming transfer
        viewModelScope.launch {
            observeActiveTransferUseCase().collect { session ->
                _uiState.update { it.copy(activeIncomingSession = if (session?.isOutgoing == false) session else null) }
            }
        }

        // Network & USB probe loop
        viewModelScope.launch {
            while (true) {
                val (usb, ips) = getNetworkStatusUseCase()
                _uiState.update { current ->
                    val shouldAutoListen = usb && !current.isListening
                    current.copy(
                        usbAvailable = usb,
                        detectedIps = ips,
                        isListening = if (shouldAutoListen) true else current.isListening,
                        statusText = if (shouldAutoListen) "Listening on 0.0.0.0:9876" else current.statusText
                    )
                }
                delay(1500)
            }
        }
    }

    fun setDestinationDir(path: String) {
        updateSettingsUseCase.setReceiveDestDir(path)
        _uiState.update { it.copy(destDir = path) }
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

    fun toggleReceiveMode(address: String = "0.0.0.0:9876") {
        viewModelScope.launch {
            if (!_uiState.value.isListening) {
                val dest = _uiState.value.destDir
                val res = enterReceiveModeUseCase(dest, address)
                when (res) {
                    is Resource.Success -> {
                        _uiState.update { it.copy(isListening = true, statusText = res.data) }
                    }
                    is Resource.Error -> {
                        _uiState.update { it.copy(statusText = "Error: ${res.message}", userMessage = res.message) }
                    }
                    is Resource.Loading -> {}
                }
            } else {
                stopReceiveModeUseCase()
                _uiState.update { it.copy(isListening = false, statusText = "Receive listener stopped") }
            }
        }
    }

    fun clearUserMessage() {
        _uiState.update { it.copy(userMessage = null) }
    }
}
