package com.turbotransfer.presentation.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.turbotransfer.domain.usecase.discovery.ObserveReceiverDiscoveryUseCase
import com.turbotransfer.domain.usecase.settings.GetSettingsUseCase
import com.turbotransfer.domain.usecase.settings.UpdateSettingsUseCase
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch
import javax.inject.Inject
import uniffi.turbotransfer_core.*

@HiltViewModel
class SettingsViewModel @Inject constructor(
    private val getSettingsUseCase: GetSettingsUseCase,
    private val updateSettingsUseCase: UpdateSettingsUseCase,
    private val observeReceiverDiscoveryUseCase: ObserveReceiverDiscoveryUseCase
) : ViewModel() {

    private val _uiState = MutableStateFlow(SettingsUiState())
    val uiState: StateFlow<SettingsUiState> = _uiState.asStateFlow()

    init {
        _uiState.update {
            it.copy(
                deviceName = getSettingsUseCase.getDeviceName(),
                prefer5Ghz = getSettingsUseCase.is5GhzPreferred(),
                autoWakeLock = getSettingsUseCase.isAutoWakeLockEnabled()
            )
        }
        loadSavedCalibration("")

        viewModelScope.launch {
            observeReceiverDiscoveryUseCase().collect { receiver ->
                if (receiver != null) {
                    _uiState.update { current ->
                        if (current.targetAddress.isBlank() || current.targetAddress == "127.0.0.1:9876" || !current.targetAddress.contains(",")) {
                            val combined = if (receiver.address != "127.0.0.1:9876" && !receiver.address.contains("127.0.0.1")) {
                                "127.0.0.1:9876, ${receiver.address}"
                            } else {
                                receiver.address
                            }
                            current.copy(targetAddress = combined)
                        } else {
                            current
                        }
                    }
                }
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

    fun setTargetAddress(address: String) {
        _uiState.update { it.copy(targetAddress = address) }
        loadSavedCalibration(address)
    }

    fun runBenchmark(targetAddress: String? = null) {
        val addr = targetAddress ?: _uiState.value.targetAddress
        viewModelScope.launch(Dispatchers.IO) {
            _uiState.update { it.copy(isBenchmarking = true) }
            try {
                val res = runBenchmark(
                    targetDeviceId = null,
                    address = addr.takeIf { it.isNotBlank() },
                    transportPref = FfiTransportPreference.COMBINED,
                    sizeMb = 250u
                )
                _uiState.update {
                    it.copy(
                        isBenchmarking = false,
                        benchmarkResult = res,
                        userMessage = "Benchmark complete: ${String.format("%.1f", res.avgSpeedMbps)} MB/s"
                    )
                }
            } catch (e: Exception) {
                _uiState.update {
                    it.copy(
                        isBenchmarking = false,
                        userMessage = "Benchmark error: ${e.message}"
                    )
                }
            }
        }
    }

    fun runCalibration(targetAddress: String? = null) {
        val addr = targetAddress ?: _uiState.value.targetAddress
        viewModelScope.launch(Dispatchers.IO) {
            _uiState.update { it.copy(isCalibrating = true, calibrationProgress = null) }
            try {
                val callback = object : FfiCalibrationProgressCallback {
                    override fun onProgress(update: FfiCalibrationProgressUpdate) {
                        _uiState.update { it.copy(calibrationProgress = update) }
                    }
                }
                val res = runCalibration(
                    targetDeviceId = null,
                    address = addr.takeIf { it.isNotBlank() },
                    callback = callback
                )
                val saved = getSavedCalibration(addr.takeIf { it.isNotBlank() } ?: "")
                _uiState.update {
                    it.copy(
                        isCalibrating = false,
                        calibrationResult = res,
                        savedCalibration = saved,
                        userMessage = "Calibration complete! Optimal speed: ${String.format("%.1f", res.bestSpeedMbps)} MB/s"
                    )
                }
            } catch (e: Exception) {
                _uiState.update {
                    it.copy(
                        isCalibrating = false,
                        userMessage = "Calibration error: ${e.message}"
                    )
                }
            }
        }
    }

    fun cancelCalibration() {
        val addr = _uiState.value.targetAddress.takeIf { it.isNotBlank() } ?: ""
        cancelCalibration(addr)
        _uiState.update { it.copy(isCalibrating = false, userMessage = "Calibration cancelled") }
    }

    fun loadSavedCalibration(targetKey: String) {
        try {
            val saved = getSavedCalibration(targetKey.takeIf { it.isNotBlank() } ?: "")
            _uiState.update { it.copy(savedCalibration = saved) }
        } catch (e: Exception) {
            // Ignore missing calibration
        }
    }

    fun clearSavedCalibration(targetKey: String? = null) {
        val key = (targetKey ?: _uiState.value.targetAddress).takeIf { it.isNotBlank() } ?: ""
        clearSavedCalibration(key)
        _uiState.update { it.copy(savedCalibration = null, userMessage = "Saved calibration cleared") }
    }

    fun clearUserMessage() {
        _uiState.update { it.copy(userMessage = null) }
    }
}
