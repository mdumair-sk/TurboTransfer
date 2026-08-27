package com.turbotransfer.data.source.network

import android.content.Context
import android.net.wifi.WifiManager
import com.turbotransfer.core.common.DispatcherProvider
import dagger.hilt.android.qualifiers.ApplicationContext
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
    @ApplicationContext private val context: Context,
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
            "192.168.137.1",
            "192.168.1.1",
            "192.168.0.1",
            "10.0.2.2"
        )

        // 1. Check Wi-Fi DHCP gateway / server
        try {
            val wifiManager = context.applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
            val dhcp = wifiManager?.dhcpInfo
            if (dhcp != null) {
                if (dhcp.gateway != 0) {
                    val gw = intToIp(dhcp.gateway)
                    if (gw.isNotBlank()) candidateIps.add(gw)
                }
                if (dhcp.serverAddress != 0) {
                    val srv = intToIp(dhcp.serverAddress)
                    if (srv.isNotBlank()) candidateIps.add(srv)
                }
            }
        } catch (_: Exception) {}

        // 2. Add subnet candidates from local active interfaces
        val localIps = getLocalIpAddresses()
        for (localIp in localIps) {
            val parts = localIp.split(".")
            if (parts.size == 4) {
                val prefix = "${parts[0]}.${parts[1]}.${parts[2]}"
                candidateIps.add("$prefix.1")
                candidateIps.add("$prefix.2")
                candidateIps.add("$prefix.19")
                candidateIps.add("$prefix.100")
                candidateIps.add("$prefix.101")
                candidateIps.add("$prefix.254")

                // Probe nearest 1..35 neighbors in local subnet
                for (host in 1..35) {
                    val ip = "$prefix.$host"
                    if (ip != localIp) {
                        candidateIps.add(ip)
                    }
                }
            }
        }

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
                        socket.connect(InetSocketAddress(targetIp, port), 150)
                        targetIp
                    }
                } catch (_: Exception) {
                    null
                }
            }
        }

        probeDeferreds.awaitAll().filterNotNull().firstOrNull()
    }

    private fun intToIp(i: Int): String {
        return "${i and 0xFF}.${i shr 8 and 0xFF}.${i shr 16 and 0xFF}.${i shr 24 and 0xFF}"
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
