package com.turbotransfer.presentation.settings

import uniffi.turbotransfer_core.FfiBenchmarkResult
import uniffi.turbotransfer_core.FfiCalibrationProgressUpdate
import uniffi.turbotransfer_core.FfiCalibrationResult
import uniffi.turbotransfer_core.FfiSavedCalibrationConfig

data class SettingsUiState(
    val deviceName: String = "",
    val prefer5Ghz: Boolean = true,
    val autoWakeLock: Boolean = true,
    val userMessage: String? = null,
    val isBenchmarking: Boolean = false,
    val isCalibrating: Boolean = false,
    val benchmarkResult: FfiBenchmarkResult? = null,
    val calibrationResult: FfiCalibrationResult? = null,
    val calibrationProgress: FfiCalibrationProgressUpdate? = null,
    val savedCalibration: FfiSavedCalibrationConfig? = null,
    val targetAddress: String = ""
)
