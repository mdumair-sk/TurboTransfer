package com.turbotransfer.data.repository

import com.turbotransfer.core.common.DispatcherProvider
import com.turbotransfer.data.source.network.NetworkProbeDataSource
import com.turbotransfer.domain.model.DiscoveredReceiverInfo
import com.turbotransfer.domain.repository.DiscoveryRepository
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class DiscoveryRepositoryImpl @Inject constructor(
    private val networkProbeDataSource: NetworkProbeDataSource,
    private val dispatcherProvider: DispatcherProvider
) : DiscoveryRepository {

    override fun observeReceiverDiscovery(): Flow<DiscoveredReceiverInfo?> = flow {
        while (true) {
            val usbFound = networkProbeDataSource.probeUsbTunnel()
            val wifiReceiverIp = networkProbeDataSource.probeCandidateWifiReceivers()
            val wifiFound = wifiReceiverIp != null
            val wifiAddr = if (wifiFound) "$wifiReceiverIp:9876" else ""

            val receiver = if (usbFound && wifiFound) {
                DiscoveredReceiverInfo(
                    address = "127.0.0.1:9876,$wifiAddr",
                    displayName = "Windows PC / Desktop",
                    transport = "USB + 5 GHz Wi-Fi (Multipath Active)",
                    isReady = true,
                    isUsbAvailable = true,
                    isWifiAvailable = true
                )
            } else if (usbFound) {
                DiscoveredReceiverInfo(
                    address = "127.0.0.1:9876",
                    displayName = "Windows PC / Desktop",
                    transport = "USB (ADB Tunnel)",
                    isReady = true,
                    isUsbAvailable = true,
                    isWifiAvailable = false
                )
            } else if (wifiFound) {
                DiscoveredReceiverInfo(
                    address = wifiAddr,
                    displayName = "Windows PC / Desktop",
                    transport = "5 GHz Wi-Fi Direct / LAN",
                    isReady = true,
                    isUsbAvailable = false,
                    isWifiAvailable = true
                )
            } else {
                null
            }

            emit(receiver)
            delay(1500)
        }
    }.flowOn(dispatcherProvider.io)

    override suspend fun getNetworkInterfacesAndUsb(): Pair<Boolean, List<String>> {
        val usb = networkProbeDataSource.probeUsbTunnel()
        val ips = networkProbeDataSource.getLocalIpAddresses()
        return Pair(usb, ips)
    }
}
