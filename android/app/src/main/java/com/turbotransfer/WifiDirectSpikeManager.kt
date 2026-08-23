package com.turbotransfer

import android.annotation.SuppressLint
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.net.NetworkInfo
import android.net.wifi.p2p.WifiP2pGroup
import android.net.wifi.p2p.WifiP2pInfo
import android.net.wifi.p2p.WifiP2pManager
import android.os.Build
import android.os.Looper
import android.util.Log
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import java.io.BufferedReader
import java.io.InputStreamReader
import java.io.PrintWriter
import java.net.InetAddress
import java.net.NetworkInterface
import java.net.ServerSocket
import java.net.Socket

data class WifiDirectSpikeState(
    val isGroupOwner: Boolean = false,
    val ssid: String? = null,
    val passphrase: String? = null,
    val groupOwnerIp: String = "192.168.49.1",
    val echoPort: Int = 9876,
    val isServerRunning: Boolean = false,
    val echoPacketsReceived: Int = 0,
    val lastClientConnected: String? = null,
    val statusMessage: String = "Idle",
    val activeBand: String? = null,
    val throughputInfo: String? = null
)

class WifiDirectSpikeManager(private val context: Context) {

    private val TAG = "WifiDirectSpike"
    private var p2pManager: WifiP2pManager? = null
    private var channel: WifiP2pManager.Channel? = null
    private var receiver: BroadcastReceiver? = null

    private var hotspotReservation: android.net.wifi.WifiManager.LocalOnlyHotspotReservation? = null
    private var serverSocket: ServerSocket? = null
    private var serverJob: Job? = null
    private val scope = CoroutineScope(Dispatchers.IO + SupervisorJob())

    private val _state = MutableStateFlow(WifiDirectSpikeState())
    val state = _state.asStateFlow()

    init {
        p2pManager = context.getSystemService(Context.WIFI_P2P_SERVICE) as? WifiP2pManager
        channel = p2pManager?.initialize(context, Looper.getMainLooper(), null)
        registerReceiver()
    }

    private fun registerReceiver() {
        val intentFilter = IntentFilter().apply {
            addAction(WifiP2pManager.WIFI_P2P_STATE_CHANGED_ACTION)
            addAction(WifiP2pManager.WIFI_P2P_CONNECTION_CHANGED_ACTION)
            addAction(WifiP2pManager.WIFI_P2P_THIS_DEVICE_CHANGED_ACTION)
        }

        receiver = object : BroadcastReceiver() {
            @SuppressLint("MissingPermission")
            override fun onReceive(c: Context?, intent: Intent?) {
                when (intent?.action) {
                    WifiP2pManager.WIFI_P2P_STATE_CHANGED_ACTION -> {
                        val state = intent.getIntExtra(WifiP2pManager.EXTRA_WIFI_STATE, -1)
                        val isEnabled = state == WifiP2pManager.WIFI_P2P_STATE_ENABLED
                        Log.i(TAG, "P2P State changed: isEnabled=$isEnabled")
                    }
                    WifiP2pManager.WIFI_P2P_CONNECTION_CHANGED_ACTION -> {
                        @Suppress("DEPRECATION")
                        val networkInfo = intent.getParcelableExtra<NetworkInfo>(WifiP2pManager.EXTRA_NETWORK_INFO)
                        if (networkInfo?.isConnected == true) {
                            requestGroupDetails()
                        }
                    }
                }
            }
        }
        context.registerReceiver(receiver, intentFilter)
    }

    @SuppressLint("MissingPermission")
    fun createP2pGroup(onResult: (Boolean, String) -> Unit) {
        val manager = p2pManager
        val ch = channel
        if (manager == null || ch == null) {
            val msg = "WifiP2pManager is unavailable on this device"
            _state.value = _state.value.copy(statusMessage = msg)
            onResult(false, msg)
            return
        }

        _state.value = _state.value.copy(statusMessage = "Creating P2P Group Owner...")
        Log.i(TAG, "Calling createGroup()...")

        val actionListener = object : WifiP2pManager.ActionListener {
            override fun onSuccess() {
                Log.i(TAG, "P2P createGroup() successful! Requesting group info...")
                _state.value = _state.value.copy(
                    isGroupOwner = true,
                    statusMessage = "P2P Group created. Fetching SSID & passphrase..."
                )
                startEchoServer(9876)
                requestGroupDetails()
                onResult(true, "P2P Group created successfully")
            }

            override fun onFailure(reason: Int) {
                val reasonStr = when (reason) {
                    WifiP2pManager.P2P_UNSUPPORTED -> "P2P unsupported"
                    WifiP2pManager.ERROR -> "Internal error"
                    WifiP2pManager.BUSY -> "Framework busy (try removing existing group first)"
                    else -> "Unknown code $reason"
                }
                Log.e(TAG, "createGroup() failed: $reasonStr")
                _state.value = _state.value.copy(
                    isGroupOwner = false,
                    statusMessage = "Failed to create group: $reasonStr"
                )
                onResult(false, "createGroup failed: $reasonStr")
            }
        }

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            try {
                val config = android.net.wifi.p2p.WifiP2pConfig.Builder()
                    .setNetworkName("DIRECT-TurboTransfer")
                    .setPassphrase("turbo12345")
                    .setGroupOperatingBand(android.net.wifi.p2p.WifiP2pConfig.GROUP_OWNER_BAND_2GHZ)
                    .build()
                manager.createGroup(ch, config, actionListener)
            } catch (e: Exception) {
                Log.w(TAG, "createGroup with config failed, falling back to default createGroup", e)
                manager.createGroup(ch, actionListener)
            }
        } else {
            manager.createGroup(ch, actionListener)
        }
    }

    @SuppressLint("MissingPermission")
    fun requestGroupDetails() {
        val manager = p2pManager ?: return
        val ch = channel ?: return

        manager.requestGroupInfo(ch) { group: WifiP2pGroup? ->
            if (group != null) {
                val ssid = group.networkName
                val passphrase = group.passphrase
                val isOwner = group.isGroupOwner
                val goIp = getLocalIpAddressForInterface(group.`interface`) ?: "192.168.49.1"

                Log.i(TAG, "Group Info: SSID='$ssid', Passphrase='$passphrase', isOwner=$isOwner, IP=$goIp")
                _state.value = _state.value.copy(
                    isGroupOwner = isOwner,
                    ssid = ssid,
                    passphrase = passphrase,
                    groupOwnerIp = goIp,
                    statusMessage = "Group Active: SSID='$ssid'"
                )
            } else {
                Log.w(TAG, "requestGroupInfo returned null group")
            }
        }
    }

    private fun getLocalIpAddressForInterface(interfaceName: String?): String? {
        try {
            val interfaces = NetworkInterface.getNetworkInterfaces()
            while (interfaces.hasMoreElements()) {
                val iface = interfaces.nextElement()
                if (interfaceName != null && iface.name != interfaceName && !iface.name.contains("p2p")) {
                    continue
                }
                val addresses = iface.inetAddresses
                while (addresses.hasMoreElements()) {
                    val addr = addresses.nextElement()
                    if (!addr.isLoopbackAddress && addr.hostAddress?.contains(":") == false) {
                        return addr.hostAddress
                    }
                }
            }
        } catch (e: Exception) {
            Log.e(TAG, "Error looking up interface IP", e)
        }
        return null
    }

    fun startEchoServer(port: Int = 9876) {
        stopEchoServer()
        serverJob = scope.launch {
            try {
                val server = ServerSocket(port)
                serverSocket = server
                _state.value = _state.value.copy(isServerRunning = true, echoPort = port)
                Log.i(TAG, "Echo server listening on 0.0.0.0:$port")

                while (isActive && !server.isClosed) {
                    val socket = server.accept()
                    val clientIp = socket.inetAddress.hostAddress
                    Log.i(TAG, "Accepted TCP connection from Windows client: $clientIp")
                    _state.value = _state.value.copy(lastClientConnected = clientIp)

                    launch {
                        handleClientConnection(socket)
                    }
                }
            } catch (e: Exception) {
                if (isActive) {
                    Log.e(TAG, "Echo server exception", e)
                    _state.value = _state.value.copy(isServerRunning = false)
                }
            }
        }
    }

    private fun handleClientConnection(socket: Socket) {
        try {
            socket.use { s ->
                s.receiveBufferSize = 4 * 1024 * 1024
                s.sendBufferSize = 4 * 1024 * 1024
                s.tcpNoDelay = true

                val inputStream = s.getInputStream()
                val outputStream = s.getOutputStream()
                val reader = BufferedReader(InputStreamReader(inputStream))
                val writer = PrintWriter(outputStream, true)

                var line: String?
                while (reader.readLine().also { line = it } != null) {
                    val received = line?.trim() ?: ""
                    Log.i(TAG, "Received message from ${s.inetAddress.hostAddress}: '$received'")

                    if (received.startsWith("STREAM_TEST:")) {
                        val expectedBytes = received.substringAfter("STREAM_TEST:").toLongOrNull() ?: 0L
                        Log.i(TAG, "Starting upload throughput test for $expectedBytes bytes...")
                        writer.println("READY")
                        writer.flush()

                        val buffer = ByteArray(256 * 1024)
                        var totalRead = 0L
                        val startTime = System.nanoTime()

                        while (totalRead < expectedBytes) {
                            val toRead = Math.min(buffer.size.toLong(), expectedBytes - totalRead).toInt()
                            val read = inputStream.read(buffer, 0, toRead)
                            if (read == -1) break
                            totalRead += read
                        }

                        val elapsedNanos = System.nanoTime() - startTime
                        val elapsedSec = elapsedNanos / 1_000_000_000.0
                        val mbTransferred = totalRead / (1024.0 * 1024.0)
                        val speedMBps = if (elapsedSec > 0) mbTransferred / elapsedSec else 0.0
                        val speedMbps = if (elapsedSec > 0) (totalRead * 8.0 / 1_000_000.0) / elapsedSec else 0.0

                        val summary = String.format("Upload: %.2f MB/s (%.1f Mbps) [%.1f MB in %.2fs]", speedMBps, speedMbps, mbTransferred, elapsedSec)
                        Log.i(TAG, "Upload result: $summary")

                        _state.value = _state.value.copy(
                            throughputInfo = summary,
                            statusMessage = summary
                        )

                        writer.println("RESULT:$speedMBps:$speedMbps:$elapsedSec")
                        writer.flush()
                    } else if (received.startsWith("STREAM_DOWNLOAD:")) {
                        val expectedBytes = received.substringAfter("STREAM_DOWNLOAD:").toLongOrNull() ?: 0L
                        Log.i(TAG, "Starting download throughput test for $expectedBytes bytes...")
                        writer.println("READY")
                        writer.flush()

                        val buffer = ByteArray(256 * 1024)
                        java.util.Arrays.fill(buffer, 0x55.toByte())
                        var totalSent = 0L
                        val startTime = System.nanoTime()

                        while (totalSent < expectedBytes) {
                            val toSend = Math.min(buffer.size.toLong(), expectedBytes - totalSent).toInt()
                            outputStream.write(buffer, 0, toSend)
                            totalSent += toSend
                        }
                        outputStream.flush()

                        val elapsedNanos = System.nanoTime() - startTime
                        val elapsedSec = elapsedNanos / 1_000_000_000.0
                        val mbTransferred = totalSent / (1024.0 * 1024.0)
                        val speedMBps = if (elapsedSec > 0) mbTransferred / elapsedSec else 0.0
                        val speedMbps = if (elapsedSec > 0) (totalSent * 8.0 / 1_000_000.0) / elapsedSec else 0.0

                        val summary = String.format("Download: %.2f MB/s (%.1f Mbps) [%.1f MB in %.2fs]", speedMBps, speedMbps, mbTransferred, elapsedSec)
                        Log.i(TAG, "Download result: $summary")

                        _state.value = _state.value.copy(
                            throughputInfo = summary,
                            statusMessage = summary
                        )
                    } else {
                        _state.value = _state.value.copy(
                            echoPacketsReceived = _state.value.echoPacketsReceived + 1,
                            statusMessage = "Received '$received' from ${s.inetAddress.hostAddress}"
                        )
                        val reply = if (received.startsWith("TURBO_PING") || received.startsWith("PING")) {
                            "TURBO_PONG\n"
                        } else {
                            "ECHO: $received\n"
                        }
                        writer.print(reply)
                        writer.flush()
                    }
                }
            }
        } catch (e: Exception) {
            Log.e(TAG, "Error handling client TCP connection", e)
        }
    }

    fun stopEchoServer() {
        serverJob?.cancel()
        serverJob = null
        try {
            serverSocket?.close()
        } catch (e: Exception) {
            Log.e(TAG, "Error closing server socket", e)
        }
        serverSocket = null
        _state.value = _state.value.copy(isServerRunning = false)
    }

    @SuppressLint("MissingPermission")
    fun removeP2pGroup(onResult: (Boolean, String) -> Unit) {
        stopEchoServer()
        val manager = p2pManager
        val ch = channel
        if (manager == null || ch == null) {
            onResult(false, "WifiP2pManager unavailable")
            return
        }

        manager.removeGroup(ch, object : WifiP2pManager.ActionListener {
            override fun onSuccess() {
                Log.i(TAG, "P2P removeGroup() successful")
                _state.value = _state.value.copy(
                    isGroupOwner = false,
                    ssid = null,
                    passphrase = null,
                    activeBand = null,
                    throughputInfo = null,
                    statusMessage = "P2P Group removed"
                )
                onResult(true, "P2P Group removed")
            }

            override fun onFailure(reason: Int) {
                Log.w(TAG, "removeGroup() failed: code $reason")
                _state.value = _state.value.copy(statusMessage = "removeGroup failed: code $reason")
                onResult(false, "removeGroup failed: code $reason")
            }
        })
    }

    @SuppressLint("MissingPermission")
    fun startLocalOnlyHotspot(onResult: (Boolean, String) -> Unit) {
        val wifiManager = context.applicationContext.getSystemService(Context.WIFI_SERVICE) as? android.net.wifi.WifiManager
        if (wifiManager == null) {
            onResult(false, "WifiManager unavailable")
            return
        }

        _state.value = _state.value.copy(statusMessage = "Starting 5GHz Local-Only Hotspot...")

        try {
            val callback = object : android.net.wifi.WifiManager.LocalOnlyHotspotCallback() {
            override fun onStarted(reservation: android.net.wifi.WifiManager.LocalOnlyHotspotReservation?) {
                super.onStarted(reservation)
                hotspotReservation = reservation
                val config = reservation?.wifiConfiguration
                val softApConfig = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                    reservation?.softApConfiguration
                } else null

                    val ssid = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                        softApConfig?.ssid
                    } else {
                        config?.SSID?.removeSurrounding("\"")
                    }

                    val passphrase = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                        softApConfig?.passphrase
                    } else {
                        config?.preSharedKey?.removeSurrounding("\"")
                    }

                    val bandInfo = try {
                        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R && softApConfig != null) {
                            val band = softApConfig.javaClass.getMethod("getBand").invoke(softApConfig) as? Int ?: 0
                            val is5g = (band and 2) != 0
                            val is2g = (band and 1) != 0
                            val name = if (is5g && is2g) "Dual-Band (2.4/5GHz)" else if (is5g) "5 GHz" else if (is2g) "2.4 GHz" else "Auto ($band)"
                            name
                        } else {
                            "Auto Band"
                        }
                    } catch (e: Throwable) {
                        "Auto Band"
                    }

                    val ip = "192.168.43.1"

                    Log.i(TAG, "LocalOnlyHotspot Started: SSID='$ssid', Passphrase='$passphrase', Band=$bandInfo, IP=$ip")
                    _state.value = _state.value.copy(
                        isGroupOwner = true,
                        ssid = ssid,
                        passphrase = passphrase,
                        groupOwnerIp = ip,
                        activeBand = bandInfo,
                        statusMessage = "Hotspot Active ($bandInfo)"
                    )
                    startEchoServer(9876)
                    onResult(true, "Local-Only Hotspot started ($bandInfo)!")
                }

                override fun onStopped() {
                    super.onStopped()
                    Log.i(TAG, "LocalOnlyHotspot Stopped")
                    _state.value = _state.value.copy(
                        isGroupOwner = false,
                        ssid = null,
                        passphrase = null,
                        activeBand = null,
                        statusMessage = "Hotspot Stopped"
                    )
                }

                override fun onFailed(reason: Int) {
                    super.onFailed(reason)
                    val reasonStr = "Error code $reason"
                    Log.e(TAG, "LocalOnlyHotspot Failed: $reasonStr")
                    _state.value = _state.value.copy(statusMessage = "Hotspot Failed: $reasonStr")
                    onResult(false, "Hotspot Failed: $reasonStr")
                }
            }

            var started = false
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                try {
                    val builderClass = Class.forName("android.net.wifi.SoftApConfiguration\$Builder")
                    val builder = builderClass.getDeclaredConstructor().newInstance()
                    val setBandMethod = builderClass.getMethod("setBand", Int::class.javaPrimitiveType)
                    setBandMethod.invoke(builder, 2) // 2 = SoftApConfiguration.BAND_5GHZ
                    val buildMethod = builderClass.getMethod("build")
                    val softApConfig = buildMethod.invoke(builder)

                    val methodWithConfig = wifiManager.javaClass.getMethod(
                        "startLocalOnlyHotspot",
                        Class.forName("android.net.wifi.SoftApConfiguration"),
                        java.util.concurrent.Executor::class.java,
                        android.net.wifi.WifiManager.LocalOnlyHotspotCallback::class.java
                    )
                    Log.i(TAG, "Invoking startLocalOnlyHotspot with 5GHz SoftApConfiguration...")
                    methodWithConfig.invoke(wifiManager, softApConfig, context.mainExecutor, callback)
                    started = true
                } catch (e: Throwable) {
                    Log.w(TAG, "5GHz SoftApConfiguration reflection failed (${e.message}), falling back to default", e)
                }
            }

            if (!started) {
                wifiManager.startLocalOnlyHotspot(callback, null)
            }
        } catch (e: Exception) {
            Log.e(TAG, "Exception starting LocalOnlyHotspot", e)
            onResult(false, "Exception: ${e.message}")
        }
    }

    fun stopLocalOnlyHotspot() {
        stopEchoServer()
        try {
            hotspotReservation?.close()
        } catch (e: Exception) {
            Log.e(TAG, "Error closing hotspot", e)
        }
        hotspotReservation = null
        _state.value = _state.value.copy(isGroupOwner = false, ssid = null, passphrase = null, statusMessage = "Hotspot Stopped")
    }

    fun cleanup() {
        stopEchoServer()
        stopLocalOnlyHotspot()
        try {
            receiver?.let { context.unregisterReceiver(it) }
        } catch (e: Exception) {
            Log.e(TAG, "Error unregistering receiver", e)
        }
    }
}
