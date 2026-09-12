package com.turbotransfer.presentation.main

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.turbotransfer.domain.model.TransferProgressInfo
import com.turbotransfer.domain.model.TransferSession
import com.turbotransfer.domain.model.TransferStatus
import com.turbotransfer.data.repository.HotspotRepositoryImpl
import com.turbotransfer.data.repository.SettingsRepositoryImpl
import com.turbotransfer.data.repository.TransferRepositoryImpl
import dagger.hilt.android.lifecycle.HiltViewModel
import android.util.Log
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch
import uniffi.turbotransfer_core.*
import javax.inject.Inject

@HiltViewModel
class MainViewModel @Inject constructor(
    private val transferRepository: TransferRepositoryImpl,
    private val hotspotRepository: HotspotRepositoryImpl,
    private val settingsRepository: SettingsRepositoryImpl
) : ViewModel() {

    private val _selectedTab = MutableStateFlow(0)
    val selectedTab: StateFlow<Int> = _selectedTab.asStateFlow()

    val activeSession: StateFlow<TransferSession?> = transferRepository.activeSessionFlow

    private val _currentProgress = MutableStateFlow<TransferProgressInfo?>(null)
    val currentProgress: StateFlow<TransferProgressInfo?> = _currentProgress.asStateFlow()

    init {
        // Observe progress of active transfer
        viewModelScope.launch {
            activeSession.collectLatest { session ->
                if (session != null) {
                    transferRepository.observeTransferProgress(session.transferId).collect { progress ->
                        _currentProgress.value = progress
                    }
                } else {
                    _currentProgress.value = null
                }
            }
        }

        // Background detector for incoming external transfers
        viewModelScope.launch {
            while (true) {
                val currentStatus = _currentProgress.value?.status
                val isIdleOrDone = activeSession.value == null ||
                        currentStatus == TransferStatus.COMPLETED ||
                        currentStatus == TransferStatus.FAILED ||
                        currentStatus == TransferStatus.CANCELLED

                if (isIdleOrDone) {
                    val prevSessionId = activeSession.value?.transferId
                    val saveDir = settingsRepository.getReceiveDestDir()
                    val incoming = transferRepository.pollPendingIncomingTransfer(saveDir)
                    if (incoming != null && incoming.transferId != prevSessionId) {
                        _selectedTab.value = 2 // Auto-switch to transfer dashboard
                    }
                }
                delay(250)
            }
        }
    }

    fun selectTab(index: Int) {
        if (_selectedTab.value == 1 && index != 1) {
            viewModelScope.launch {
                transferRepository.stopReceiveMode()
            }
        }
        _selectedTab.value = index
    }

    fun handleStartTransferBroadcast(filePath: String, address: String) {
        viewModelScope.launch {
            transferRepository.startTransfer(filePath, address, null)
            _selectedTab.value = 2
        }
    }

    fun handleStartHotspotBroadcast() {
        viewModelScope.launch {
            transferRepository.stopReceiveMode()
            hotspotRepository.startHotspot(9876) { }
            _selectedTab.value = 0
        }
    }

    fun handleStopHotspotBroadcast() {
        hotspotRepository.stopHotspot()
    }

    fun handleEnterReceiveBroadcast(destDir: String?) {
        viewModelScope.launch {
            val dir = destDir ?: settingsRepository.getReceiveDestDir()
            transferRepository.enterReceiveMode(dir, null)
            hotspotRepository.startHotspot(9876) { }
            _selectedTab.value = 1
        }
    }

    fun handleStopReceiveBroadcast() {
        viewModelScope.launch {
            transferRepository.stopReceiveMode()
        }
    }

    fun handleRunBenchmarkBroadcast(address: String, sizeMb: UInt) {
        viewModelScope.launch(Dispatchers.IO) {
            _selectedTab.value = 2
            try {
                runBenchmark(
                    targetDeviceId = null,
                    address = address,
                    transportPref = FfiTransportPreference.COMBINED,
                    sizeMb = sizeMb
                )
            } catch (e: Exception) {
                Log.e("TurboTransfer", "Benchmark error: ${e.message}", e)
            }
        }
    }
}
