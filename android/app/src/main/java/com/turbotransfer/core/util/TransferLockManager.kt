package com.turbotransfer.core.util

import android.content.Context
import android.net.wifi.WifiManager
import android.os.Build
import android.os.PowerManager
import android.util.Log

class TransferLockManager(private val context: Context) {

    private val tag = "TransferLockManager"
    private var wifiLock: WifiManager.WifiLock? = null
    private var wakeLock: PowerManager.WakeLock? = null

    private var lockRefCount = 0

    @Synchronized
    fun acquireLocks() {
        lockRefCount++
        if (lockRefCount > 1) {
            return
        }
        try {
            val wifiManager = context.applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
            val powerManager = context.applicationContext.getSystemService(Context.POWER_SERVICE) as? PowerManager

            if (wifiLock == null && wifiManager != null) {
                val lockType = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                    WifiManager.WIFI_MODE_FULL_LOW_LATENCY
                } else {
                    WifiManager.WIFI_MODE_FULL_HIGH_PERF
                }
                wifiLock = wifiManager.createWifiLock(lockType, "TurboTransfer:ActiveTransferWifiLock").apply {
                    setReferenceCounted(false)
                    acquire()
                }
                Log.i(tag, "Acquired ActiveTransfer WifiLock (mode=$lockType)")
            }

            if (wakeLock == null && powerManager != null) {
                wakeLock = powerManager.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "TurboTransfer:ActiveTransferWakeLock").apply {
                    setReferenceCounted(false)
                    acquire(60 * 60 * 1000L)
                }
                Log.i(tag, "Acquired ActiveTransfer WakeLock")
            }
        } catch (e: Exception) {
            Log.w(tag, "Failed to acquire transfer locks", e)
        }
    }

    @Synchronized
    fun releaseLocks() {
        if (lockRefCount > 0) {
            lockRefCount--
        }
        if (lockRefCount > 0) {
            return
        }
        try {
            if (wifiLock?.isHeld == true) {
                wifiLock?.release()
            }
        } catch (e: Exception) {
            Log.w(tag, "Error releasing transfer WifiLock", e)
        }
        wifiLock = null

        try {
            if (wakeLock?.isHeld == true) {
                wakeLock?.release()
            }
        } catch (e: Exception) {
            Log.w(tag, "Error releasing transfer WakeLock", e)
        }
        wakeLock = null
    }
}
