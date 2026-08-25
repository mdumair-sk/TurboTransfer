package com.turbotransfer.data.repository

import com.turbotransfer.WifiHotspotManager
import com.turbotransfer.core.common.Resource
import com.turbotransfer.domain.model.HotspotCredentials
import com.turbotransfer.domain.model.HotspotStateInfo
import com.turbotransfer.domain.repository.HotspotRepository
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
class HotspotRepositoryImpl @Inject constructor(
    private val wifiHotspotManager: WifiHotspotManager
) : HotspotRepository {

    private val _hotspotStateFlow = MutableStateFlow(HotspotStateInfo())
    override val hotspotStateFlow: StateFlow<HotspotStateInfo> = _hotspotStateFlow.asStateFlow()

    init {
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

    override fun startHotspot(port: Int, onResult: (Resource<String>) -> Unit) {
        wifiHotspotManager.startHotspot(port) { success, msg ->
            if (success) {
                onResult(Resource.Success(msg))
            } else {
                onResult(Resource.Error(msg))
            }
        }
    }

    override fun stopHotspot() {
        wifiHotspotManager.stopHotspot()
    }

    override fun cleanup() {
        wifiHotspotManager.cleanup()
    }
}
