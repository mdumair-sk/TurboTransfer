package com.turbotransfer.data.repository

import com.turbotransfer.WifiHotspotManager
import com.turbotransfer.core.common.Resource
import com.turbotransfer.domain.model.HotspotCredentials
import com.turbotransfer.domain.model.HotspotStateInfo
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
open class HotspotRepositoryImpl(
    private val wifiHotspotManager: WifiHotspotManager?,
    @Suppress("UNUSED_PARAMETER") dummy: Unit?
) {
    @Inject
    constructor(
        wifiHotspotManager: WifiHotspotManager
    ) : this(wifiHotspotManager, null)

    constructor() : this(null, null)

    private val _hotspotStateFlow = MutableStateFlow(HotspotStateInfo())
    open val hotspotStateFlow: StateFlow<HotspotStateInfo> = _hotspotStateFlow.asStateFlow()

    init {
        @Suppress("SENSELESS_COMPARISON")
        if (wifiHotspotManager != null) {
            wifiHotspotManager.state.onEach { state ->
            val creds = state.hotspotInfo?.let {
                HotspotCredentials(
                    ssid = it.ssid,
                    passphrase = it.passphrase,
                    ip = it.ip,
                    port = it.port.toInt(),
                    band = it.band
                )
            }
            _hotspotStateFlow.value = HotspotStateInfo(
                isActive = state.isActive,
                credentials = creds,
                isListening = state.isListening,
                connectedClients = state.connectedClients,
                totalBytesReceived = state.totalBytesReceived,
                statusMessage = state.statusMessage
            )
            }.launchIn(CoroutineScope(Dispatchers.Default))
        }
    }

    open fun startHotspot(port: Int, onResult: (Resource<String>) -> Unit) {
        val manager = wifiHotspotManager ?: run {
            onResult(Resource.Error("WifiHotspotManager unavailable"))
            return
        }
        manager.startHotspot(port) { success, msg ->
            if (success) {
                onResult(Resource.Success(msg))
            } else {
                onResult(Resource.Error(msg))
            }
        }
    }

    open fun stopHotspot() {
        wifiHotspotManager?.stopHotspot()
    }

    open fun cleanup() {
        wifiHotspotManager?.cleanup()
    }
}
