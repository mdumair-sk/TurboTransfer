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
            "192.168.137.1"
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

        // 2. Add subnet candidates ONLY from local active Wi-Fi interfaces (wlan*, p2p*, ap*, softap*)
        val wifiIps = getLocalWifiIpAddresses()
        for (localIp in wifiIps) {
            val parts = localIp.split(".")
            if (parts.size == 4) {
                val prefix = "${parts[0]}.${parts[1]}.${parts[2]}"
                candidateIps.add("$prefix.1")
                candidateIps.add("$prefix.2")
                candidateIps.add("$prefix.19")
                candidateIps.add("$prefix.100")
                candidateIps.add("$prefix.101")
                candidateIps.add("$prefix.254")

                // Probe nearest 1..35 neighbors in local Wi-Fi subnet
                for (host in 1..35) {
                    val ip = "$prefix.$host"
                    if (ip != localIp) {
                        candidateIps.add(ip)
                    }
                }
            }
        }

        // 3. Scan ARP table entries associated with wlan interfaces
        try {
            val arpLines = File("/proc/net/arp").readLines()
            for (line in arpLines.drop(1)) {
                val tokens = line.trim().split(Regex("\\s+"))
                if (tokens.size >= 6) {
                    val ip = tokens[0]
                    val dev = tokens[5].lowercase()
                    if (ip.matches(Regex("\\d+\\.\\d+\\.\\d+\\.\\d+")) && (dev.startsWith("wlan") || dev.startsWith("p2p") || dev.startsWith("ap"))) {
                        candidateIps.add(ip)
                    }
                }
            }
        } catch (_: Exception) {}

        probeIpSet(candidateIps, port)
    }

    suspend fun probeCandidateUsbTetherReceivers(port: Int = 9876): String? = withContext(dispatcherProvider.io) {
        val candidateIps = mutableSetOf<String>()

        // 1. Add subnet candidates from USB tether / RNDIS interfaces (rndis*, usb*, ncm*)
        val tetherIps = getLocalUsbTetherIpAddresses()
        for (localIp in tetherIps) {
            val parts = localIp.split(".")
            if (parts.size == 4) {
                val prefix = "${parts[0]}.${parts[1]}.${parts[2]}"
                candidateIps.add("$prefix.1")
                candidateIps.add("$prefix.2")
                candidateIps.add("$prefix.30")

                for (host in 1..35) {
                    val ip = "$prefix.$host"
                    if (ip != localIp) {
                        candidateIps.add(ip)
                    }
                }
            }
        }

        // 2. Scan ARP table entries for resolved neighbors on rndis/usb devices
        try {
            val arpLines = File("/proc/net/arp").readLines()
            for (line in arpLines.drop(1)) {
                val tokens = line.trim().split(Regex("\\s+"))
                if (tokens.size >= 6) {
                    val ip = tokens[0]
                    val dev = tokens[5].lowercase()
                    if (ip.matches(Regex("\\d+\\.\\d+\\.\\d+\\.\\d+")) && (dev.startsWith("rndis") || dev.startsWith("usb") || dev.startsWith("ncm"))) {
                        candidateIps.add(ip)
                    }
                }
            }
        } catch (_: Exception) {}

        probeIpSet(candidateIps, port)
    }

    private suspend fun probeIpSet(candidateIps: Set<String>, port: Int): String? = withContext(dispatcherProvider.io) {
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

    suspend fun getLocalWifiIpAddresses(): List<String> = withContext(dispatcherProvider.io) {
        getFilteredIpAddresses { name ->
            val lower = name.lowercase()
            lower.startsWith("wlan") || lower.startsWith("p2p") || lower.startsWith("ap") || lower.startsWith("softap")
        }
    }

    suspend fun getLocalUsbTetherIpAddresses(): List<String> = withContext(dispatcherProvider.io) {
        getFilteredIpAddresses { name ->
            val lower = name.lowercase()
            lower.startsWith("rndis") || lower.startsWith("usb") || lower.startsWith("ncm")
        }
    }

    suspend fun getLocalIpAddresses(): List<String> = withContext(dispatcherProvider.io) {
        getFilteredIpAddresses { true }
    }

    private suspend fun getFilteredIpAddresses(filter: (String) -> Boolean): List<String> = withContext(dispatcherProvider.io) {
        try {
            NetworkInterface.getNetworkInterfaces()?.asSequence().orEmpty()
                .filter { !it.isLoopback && it.isUp && filter(it.name) }
                .flatMap { it.inetAddresses.asSequence() }
                .filterIsInstance<Inet4Address>()
                .mapNotNull { it.hostAddress }
                .filter { it.isNotBlank() && !it.startsWith("127.") }
                .distinct()
                .toList()
        } catch (_: Exception) {
            emptyList()
        }
    }
}
