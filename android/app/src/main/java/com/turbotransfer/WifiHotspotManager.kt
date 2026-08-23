package com.turbotransfer

import android.annotation.SuppressLint
import android.content.Context
import android.net.wifi.WifiManager
import android.os.Build
import android.util.Log
import uniffi.turbotransfer_core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import org.json.JSONObject

/**
 * Observable UI and lifecycle state for Android Wi-Fi Direct / Local Hotspot Transport (§9).
 */
data class WifiHotspotState(
    val isActive: Boolean = false,
    val hotspotInfo: FfiWifiHotspotInfo? = null,
    val isListening: Boolean = false,
    val connectedClients: List<String> = emptyList(),
    val totalBytesReceived: Long = 0L,
    val statusMessage: String = "Idle"
)

/**
 * Production manager for Android's Local-Only Hotspot transport link (TRD §9, Approach 1B).
 *
 * Enforces 5 GHz band (802.11ac) by default on API 30+ with automatic 2.4 GHz fallback.
 */
class WifiHotspotManager(private val context: Context) {

    private val TAG = "WifiHotspotManager"
    private var reservation: WifiManager.LocalOnlyHotspotReservation? = null
    private var wifiLock: WifiManager.WifiLock? = null
    private var wakeLock: android.os.PowerManager.WakeLock? = null
    private val scope = CoroutineScope(Dispatchers.IO + SupervisorJob())

    private val _state = MutableStateFlow(WifiHotspotState())
    val state = _state.asStateFlow()

    /**
     * Starts the Local-Only Hotspot with 5 GHz band preferred. The protocol
     * receiver is owned by the Rust Receive mode; a second raw Socket listener
     * here would consume and discard transfer bytes.
     */
    @SuppressLint("MissingPermission")
    fun startHotspot(port: Int = 9876, onResult: (Boolean, String) -> Unit) {
        val wifiManager = context.applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
        if (wifiManager == null) {
            val err = "WifiManager is unavailable on this device"
            _state.value = _state.value.copy(statusMessage = err)
            onResult(false, err)
            return
        }

        // Acquire High-Performance / Low-Latency WifiLock and WakeLock
        try {
            val powerManager = context.applicationContext.getSystemService(Context.POWER_SERVICE) as? android.os.PowerManager
            if (wifiLock == null) {
                val lockType = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                    WifiManager.WIFI_MODE_FULL_LOW_LATENCY
                } else {
                    WifiManager.WIFI_MODE_FULL_HIGH_PERF
                }
                wifiLock = wifiManager.createWifiLock(lockType, "TurboTransfer:HotspotLock").apply {
                    setReferenceCounted(false)
                    acquire()
                }
                Log.i(TAG, "Acquired high-performance WifiLock (mode=$lockType)")
            }
            if (wakeLock == null && powerManager != null) {
                wakeLock = powerManager.newWakeLock(android.os.PowerManager.PARTIAL_WAKE_LOCK, "TurboTransfer:HotspotWakeLock").apply {
                    setReferenceCounted(false)
                    acquire(60 * 60 * 1000L)
                }
                Log.i(TAG, "Acquired partial WakeLock")
            }
        } catch (e: Exception) {
            Log.w(TAG, "Could not acquire WifiLock/WakeLock", e)
        }

        _state.value = _state.value.copy(statusMessage = "Starting 5GHz Hotspot...")

        try {
            val callback = object : WifiManager.LocalOnlyHotspotCallback() {
                override fun onStarted(res: WifiManager.LocalOnlyHotspotReservation?) {
                    super.onStarted(res)
                    reservation = res

                    val config = res?.wifiConfiguration
                    val softApConfig = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                        res?.softApConfiguration
                    } else null

                    val ssid = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                        softApConfig?.ssid ?: "AndroidShare"
                    } else {
                        config?.SSID?.removeSurrounding("\"") ?: "AndroidShare"
                    }

                    val passphrase = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                        softApConfig?.passphrase ?: ""
                    } else {
                        config?.preSharedKey?.removeSurrounding("\"") ?: ""
                    }

                    val bandInfo = try {
                        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R && softApConfig != null) {
                            val band = softApConfig.javaClass.getMethod("getBand").invoke(softApConfig) as? Int ?: 0
                            val is5g = (band and 2) != 0
                            val is2g = (band and 1) != 0
                            if (is5g && is2g) "Dual-Band (2.4/5GHz)" else if (is5g) "5 GHz" else "2.4 GHz"
                        } else {
                            "Auto Band"
                        }
                    } catch (e: Throwable) {
                        "Auto Band"
                    }

                    // Windows resolves the gateway after association. Do not
                    // publish a vendor-specific hotspot address.
                    val ip = ""
                    val info = FfiWifiHotspotInfo(
                        ssid = ssid,
                        passphrase = passphrase,
                        ip = ip,
                        port = port.toUShort(),
                        band = bandInfo
                    )

                    Log.i(TAG, "Local Hotspot Active: SSID='$ssid', Band=$bandInfo, Port=$port")
                    _state.value = _state.value.copy(
                        isActive = true,
                        hotspotInfo = info,
                        statusMessage = "Hotspot Active ($bandInfo)"
                    )

                    startControlServer(9875, info)
                    onResult(true, "Hotspot started on $bandInfo. Start Receive mode to accept files.")
                }

                override fun onStopped() {
                    super.onStopped()
                    Log.i(TAG, "Local Hotspot Stopped")
                    stopControlServer()
                    _state.value = _state.value.copy(
                        isActive = false,
                        hotspotInfo = null,
                        statusMessage = "Hotspot Stopped"
                    )
                }

                override fun onFailed(reason: Int) {
                    super.onFailed(reason)
                    val msg = "Failed to start Hotspot (code $reason)"
                    Log.e(TAG, msg)
                    _state.value = _state.value.copy(statusMessage = msg)
                    onResult(false, msg)
                }
            }

            var started = false
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                try {
                    val builderClass = Class.forName("android.net.wifi.SoftApConfiguration\$Builder")
                    val builder = builderClass.getDeclaredConstructor().newInstance()
                    val setBandMethod = builderClass.getMethod("setBand", Int::class.javaPrimitiveType)
                    setBandMethod.invoke(builder, 2) // 2 = BAND_5GHZ
                    val buildMethod = builderClass.getMethod("build")
                    val softApConfig = buildMethod.invoke(builder)

                    val methodWithConfig = wifiManager.javaClass.getMethod(
                        "startLocalOnlyHotspot",
                        Class.forName("android.net.wifi.SoftApConfiguration"),
                        java.util.concurrent.Executor::class.java,
                        WifiManager.LocalOnlyHotspotCallback::class.java
                    )
                    Log.i(TAG, "Starting Local-Only Hotspot with 5GHz preference...")
                    methodWithConfig.invoke(wifiManager, softApConfig, context.mainExecutor, callback)
                    started = true
                } catch (e: Throwable) {
                    Log.w(TAG, "5GHz SoftApConfiguration failed: ${e.message}, falling back to default", e)
                }
            }

            if (!started) {
                wifiManager.startLocalOnlyHotspot(callback, null)
            }
        } catch (e: Exception) {
            Log.e(TAG, "Exception starting Local Hotspot", e)
            onResult(false, "Exception: ${e.message}")
        }
    }

    private var controlServerSocket: ServerSocket? = null
    private var controlJob: Job? = null

    private fun startControlServer(port: Int = 9875, info: FfiWifiHotspotInfo) {
        stopControlServer()
        controlJob = scope.launch {
            try {
                // Credentials travel only over the ADB-forwarded loopback
                // control channel, never to devices joined to the hotspot.
                val server = ServerSocket(port, 1, InetAddress.getLoopbackAddress())
                controlServerSocket = server
                Log.i(TAG, "Hotspot Control Server listening on loopback:$port")
                val json = JSONObject()
                    .put("ssid", info.ssid)
                    .put("passphrase", info.passphrase)
                    .put("ip", info.ip)
                    .put("port", info.port.toInt())
                    .put("band", info.band)
                    .toString()

                while (isActive && !server.isClosed) {
                    val socket = server.accept()
                    launch {
                        try {
                            socket.use { s ->
                                s.tcpNoDelay = true
                                val out = s.getOutputStream()
                                out.write((json + "\n").toByteArray(Charsets.UTF_8))
                                out.flush()
                                Log.i(TAG, "Sent hotspot credentials to discovery client: ${socket.inetAddress.hostAddress}")
                            }
                        } catch (e: Exception) {
                            Log.w(TAG, "Error writing hotspot info to socket: ${e.message}")
                        }
                    }
                }
            } catch (e: Exception) {
                if (isActive) {
                    Log.w(TAG, "Hotspot Control Server closed/error: ${e.message}")
                }
            }
        }
    }

    private fun stopControlServer() {
        controlJob?.cancel()
        controlJob = null
        try {
            controlServerSocket?.close()
        } catch (e: Exception) {
            Log.e(TAG, "Error closing control server socket", e)
        }
        controlServerSocket = null
    }

    /**
     * Tears down the active Hotspot and closes all sockets.
     */
    fun stopHotspot() {
        stopControlServer()
        try {
            if (wifiLock?.isHeld == true) {
                wifiLock?.release()
            }
        } catch (e: Exception) {
            Log.w(TAG, "Error releasing WifiLock", e)
        }
        wifiLock = null

        try {
            if (wakeLock?.isHeld == true) {
                wakeLock?.release()
            }
        } catch (e: Exception) {
            Log.w(TAG, "Error releasing WakeLock", e)
        }
        wakeLock = null

        try {
            reservation?.close()
        } catch (e: Exception) {
            Log.e(TAG, "Error closing hotspot reservation", e)
        }
        reservation = null
        _state.value = _state.value.copy(
            isActive = false,
            hotspotInfo = null,
            statusMessage = "Hotspot Stopped"
        )
    }

    fun cleanup() {
        stopHotspot()
    }
}
