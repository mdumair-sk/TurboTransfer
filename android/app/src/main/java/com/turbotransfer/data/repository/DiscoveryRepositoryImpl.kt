package com.turbotransfer.data.repository

import com.turbotransfer.core.common.DispatcherProvider
import com.turbotransfer.data.source.network.NetworkProbeDataSource
import com.turbotransfer.domain.model.DiscoveredReceiverInfo
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
open class DiscoveryRepositoryImpl(
    private val networkProbeDataSource: NetworkProbeDataSource?,
    private val dispatcherProvider: DispatcherProvider?,
    @Suppress("UNUSED_PARAMETER") dummy: Unit?
) {
    @Inject
    constructor(
        networkProbeDataSource: NetworkProbeDataSource,
        dispatcherProvider: DispatcherProvider
    ) : this(networkProbeDataSource, dispatcherProvider, null)

    constructor() : this(null, null, null)

    open fun observeReceiverDiscovery(): Flow<DiscoveredReceiverInfo?> = flow {
        while (true) {
            val usbTunnelFound = networkProbeDataSource?.probeUsbTunnel() ?: false
            val usbTetherIp = networkProbeDataSource?.probeCandidateUsbTetherReceivers()
            val wifiReceiverIp = networkProbeDataSource?.probeCandidateWifiReceivers()

            val usbFound = usbTunnelFound || (usbTetherIp != null)
            val wifiFound = wifiReceiverIp != null
            val wifiAddr = if (wifiFound) "$wifiReceiverIp:9876" else ""

            val receiver = if (usbFound && wifiFound) {
                val usbAddr = if (usbTunnelFound) "127.0.0.1:9876" else "$usbTetherIp:9876#usb"
                DiscoveredReceiverInfo(
                    address = "$usbAddr,$wifiAddr",
                    displayName = "Windows PC / Desktop",
                    transport = "USB + 5 GHz Wi-Fi (Multipath Active)",
                    isReady = true,
                    isUsbAvailable = true,
                    isWifiAvailable = true
                )
            } else if (usbFound) {
                val addr = listOfNotNull(
                    if (usbTunnelFound) "127.0.0.1:9876" else null,
                    usbTetherIp?.let { "$it:9876#usb" }
                ).joinToString(",")
                val transportName = when {
                    usbTunnelFound && usbTetherIp != null -> "USB (ADB + High-Speed Tether)"
                    usbTunnelFound -> "USB (ADB Tunnel)"
                    else -> "USB (High-Speed Tether)"
                }
                DiscoveredReceiverInfo(
                    address = addr,
                    displayName = "Windows PC / Desktop",
                    transport = transportName,
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
    }.flowOn(dispatcherProvider?.io ?: kotlinx.coroutines.Dispatchers.IO)

    open suspend fun getNetworkInterfacesAndUsb(): Pair<Boolean, List<String>> {
        val usb = networkProbeDataSource?.probeUsbTunnel() ?: false
        val ips = networkProbeDataSource?.getLocalIpAddresses() ?: emptyList()
        return Pair(usb, ips)
    }
}
