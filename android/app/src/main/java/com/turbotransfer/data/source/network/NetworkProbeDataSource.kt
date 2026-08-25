package com.turbotransfer.data.source.network

import com.turbotransfer.core.common.DispatcherProvider
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.withContext
import java.io.File
import java.net.Inet4Address
import java.net.InetSocketAddress
import java.net.NetworkInterface
import java.net.Socket
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class NetworkProbeDataSource @Inject constructor(
    private val dispatcherProvider: DispatcherProvider
) {
    suspend fun probeUsbTunnel(port: Int = 9876, timeoutMs: Int = 250): Boolean = withContext(dispatcherProvider.io) {
        try {
            Socket().use { socket ->
                socket.connect(InetSocketAddress("127.0.0.1", port), timeoutMs)
                true
            }
        } catch (_: Exception) {
            false
        }
    }

    suspend fun probeCandidateWifiReceivers(port: Int = 9876): String? = withContext(dispatcherProvider.io) {
        val candidateIps = mutableSetOf(
            "10.18.163.1",
            "10.18.163.2",
            "192.168.43.1",
            "192.168.43.2",
            "192.168.1.19",
            "10.78.112.46"
        )

        try {
            val arpLines = File("/proc/net/arp").readLines()
            for (line in arpLines.drop(1)) {
                val tokens = line.trim().split(Regex("\\s+"))
                if (tokens.isNotEmpty()) {
                    val ip = tokens[0]
                    if (ip.matches(Regex("\\d+\\.\\d+\\.\\d+\\.\\d+"))) {
                        candidateIps.add(ip)
                    }
                }
            }
        } catch (_: Exception) {}

        val probeDeferreds = candidateIps.map { targetIp ->
            async(dispatcherProvider.io) {
                try {
                    Socket().use { socket ->
                        socket.connect(InetSocketAddress(targetIp, port), 200)
                        targetIp
                    }
                } catch (_: Exception) {
                    null
                }
            }
        }

        probeDeferreds.awaitAll().filterNotNull().firstOrNull()
    }

    suspend fun getLocalIpAddresses(): List<String> = withContext(dispatcherProvider.io) {
        val foundIps = mutableListOf<String>()
        try {
            val interfaces = NetworkInterface.getNetworkInterfaces()
            while (interfaces.hasMoreElements()) {
                val iface = interfaces.nextElement()
                if (iface.isLoopback || !iface.isUp) continue
                val addresses = iface.inetAddresses
                while (addresses.hasMoreElements()) {
                    val addr = addresses.nextElement()
                    if (addr is Inet4Address && !addr.isLoopbackAddress) {
                        val host = addr.hostAddress
                        if (!host.isNullOrBlank() && !foundIps.contains(host)) {
                            foundIps.add(host)
                        }
                    }
                }
            }
        } catch (_: Exception) {}
        foundIps
    }
}
