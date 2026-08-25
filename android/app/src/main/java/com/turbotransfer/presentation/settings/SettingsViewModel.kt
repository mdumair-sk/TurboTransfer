package com.turbotransfer.presentation.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.turbotransfer.WifiDirectSpikeManager
import com.turbotransfer.domain.usecase.discovery.GetUsbSpeedLabelUseCase
import com.turbotransfer.domain.usecase.settings.GetSettingsUseCase
import com.turbotransfer.domain.usecase.settings.UpdateSettingsUseCase
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch
import javax.inject.Inject

@HiltViewModel
class SettingsViewModel @Inject constructor(
    private val getSettingsUseCase: GetSettingsUseCase,
    private val updateSettingsUseCase: UpdateSettingsUseCase,
    private val getUsbSpeedLabelUseCase: GetUsbSpeedLabelUseCase,
    private val spikeManager: WifiDirectSpikeManager
) : ViewModel() {

    private val _uiState = MutableStateFlow(SettingsUiState())
    val uiState: StateFlow<SettingsUiState> = _uiState.asStateFlow()

    init {
        _uiState.update {
            it.copy(
                deviceName = getSettingsUseCase.getDeviceName(),
                prefer5Ghz = getSettingsUseCase.is5GhzPreferred(),
                autoWakeLock = getSettingsUseCase.isAutoWakeLockEnabled(),
                usbLabel = getUsbSpeedLabelUseCase()
            )
        }

        viewModelScope.launch {
            spikeManager.state.collect { spikeState ->
                _uiState.update { it.copy(spikeState = spikeState) }
            }
        }
    }

    fun setDeviceName(name: String) {
        updateSettingsUseCase.setDeviceName(name)
        _uiState.update { it.copy(deviceName = name) }
    }

    fun setPrefer5Ghz(enabled: Boolean) {
        updateSettingsUseCase.set5GhzPreferred(enabled)
        _uiState.update { it.copy(prefer5Ghz = enabled) }
    }

    fun setAutoWakeLock(enabled: Boolean) {
        updateSettingsUseCase.setAutoWakeLockEnabled(enabled)
        _uiState.update { it.copy(autoWakeLock = enabled) }
    }

    fun toggleShowDiagnostics() {
        _uiState.update { it.copy(showDiagnostics = !it.showDiagnostics) }
    }

    fun createP2pGroup() {
        spikeManager.createP2pGroup { _, msg ->
            _uiState.update { it.copy(userMessage = msg) }
        }
    }

    fun removeP2pGroup() {
        spikeManager.removeP2pGroup { _, _ -> }
        spikeManager.stopLocalOnlyHotspot()
        _uiState.update { it.copy(userMessage = "Stopped P2P Group and Hotspot") }
    }

    fun clearUserMessage() {
        _uiState.update { it.copy(userMessage = null) }
    }
}
