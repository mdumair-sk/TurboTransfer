package com.turbotransfer.presentation.settings

import androidx.lifecycle.ViewModel
import com.turbotransfer.domain.usecase.discovery.GetUsbSpeedLabelUseCase
import com.turbotransfer.domain.usecase.settings.GetSettingsUseCase
import com.turbotransfer.domain.usecase.settings.UpdateSettingsUseCase
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.*
import javax.inject.Inject

@HiltViewModel
class SettingsViewModel @Inject constructor(
    private val getSettingsUseCase: GetSettingsUseCase,
    private val updateSettingsUseCase: UpdateSettingsUseCase,
    private val getUsbSpeedLabelUseCase: GetUsbSpeedLabelUseCase
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

    fun clearUserMessage() {
        _uiState.update { it.copy(userMessage = null) }
    }
}
