package com.turbotransfer.presentation.main

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.turbotransfer.domain.model.TransferProgressInfo
import com.turbotransfer.domain.model.TransferSession
import com.turbotransfer.domain.model.TransferStatus
import com.turbotransfer.domain.usecase.hotspot.StartHotspotUseCase
import com.turbotransfer.domain.usecase.hotspot.StopHotspotUseCase
import com.turbotransfer.domain.usecase.transfer.EnterReceiveModeUseCase
import com.turbotransfer.domain.usecase.transfer.ObserveActiveTransferUseCase
import com.turbotransfer.domain.usecase.transfer.ObserveTransferProgressUseCase
import com.turbotransfer.domain.usecase.transfer.PollIncomingTransferUseCase
import com.turbotransfer.domain.usecase.transfer.StartTransferUseCase
import com.turbotransfer.domain.usecase.settings.GetSettingsUseCase
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch
import javax.inject.Inject

@HiltViewModel
class MainViewModel @Inject constructor(
    private val observeActiveTransferUseCase: ObserveActiveTransferUseCase,
    private val observeTransferProgressUseCase: ObserveTransferProgressUseCase,
    private val startTransferUseCase: StartTransferUseCase,
    private val enterReceiveModeUseCase: EnterReceiveModeUseCase,
    private val startHotspotUseCase: StartHotspotUseCase,
    private val stopHotspotUseCase: StopHotspotUseCase,
    private val pollIncomingTransferUseCase: PollIncomingTransferUseCase,
    private val getSettingsUseCase: GetSettingsUseCase
) : ViewModel() {

    private val _selectedTab = MutableStateFlow(0)
    val selectedTab: StateFlow<Int> = _selectedTab.asStateFlow()

    val activeSession: StateFlow<TransferSession?> = observeActiveTransferUseCase()

    private val _currentProgress = MutableStateFlow<TransferProgressInfo?>(null)
    val currentProgress: StateFlow<TransferProgressInfo?> = _currentProgress.asStateFlow()

    init {
        // Observe progress of active transfer
        viewModelScope.launch {
            activeSession.collectLatest { session ->
                if (session != null) {
                    observeTransferProgressUseCase(session.transferId).collect { progress ->
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
                if (activeSession.value == null) {
                    val saveDir = getSettingsUseCase.getReceiveDestDir()
                    val incoming = pollIncomingTransferUseCase(saveDir)
                    if (incoming != null) {
                        _selectedTab.value = 2 // Auto-switch to transfer dashboard
                    }
                }
                delay(400)
            }
        }
    }

    fun selectTab(index: Int) {
        _selectedTab.value = index
    }

    fun handleStartTransferBroadcast(filePath: String, address: String) {
        viewModelScope.launch {
            startTransferUseCase(filePath, address)
            _selectedTab.value = 2
        }
    }

    fun handleStartHotspotBroadcast() {
        startHotspotUseCase { }
    }

    fun handleStopHotspotBroadcast() {
        stopHotspotUseCase()
    }

    fun handleEnterReceiveBroadcast(destDir: String?) {
        viewModelScope.launch {
            val dir = destDir ?: getSettingsUseCase.getReceiveDestDir()
            enterReceiveModeUseCase(dir, null)
            _selectedTab.value = 1
        }
    }
}
