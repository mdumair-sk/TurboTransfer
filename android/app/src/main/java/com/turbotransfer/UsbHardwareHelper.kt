package com.turbotransfer

import android.content.Context
import android.util.Log
import java.io.File

object UsbHardwareHelper {

    private const val TAG = "UsbHardwareHelper"

    /**
     * Determines the physical USB link speed/generation if connected.
     * Defaults accurately to USB 2.0 (High Speed, 480 Mbps) unless the kernel
     * sysfs explicitly reports SuperSpeed (USB+).
     */
    fun getUsbSpeedLabel(context: Context): String {
        // 1. Check UDC current speed in sysfs (modern Android kernels)
        val udcDirs = arrayOf(
            "/sys/class/udc",
            "/sys/class/android_usb",
            "/sys/devices/virtual/android_usb/android_usb0",
            "/sys/devices/platform/soc"
        )

        for (dirPath in udcDirs) {
            val dir = File(dirPath)
            if (dir.exists() && dir.isDirectory) {
                val currentSpeed = File(dir, "current_speed")
                if (currentSpeed.exists() && currentSpeed.canRead()) {
                    val raw = currentSpeed.readText().trim().lowercase()
                    val parsed = parseSpeedString(raw)
                    if (parsed != null) return parsed
                }

                val directSpeed = File(dir, "speed")
                if (directSpeed.exists() && directSpeed.canRead()) {
                    val raw = directSpeed.readText().trim().lowercase()
                    val parsed = parseSpeedString(raw)
                    if (parsed != null) return parsed
                }

                // Check subdirectories (e.g. /sys/class/udc/a600000.dwc3/current_speed)
                dir.listFiles()?.forEach { sub ->
                    if (sub.isDirectory) {
                        val subSpeed = File(sub, "current_speed")
                        if (subSpeed.exists() && subSpeed.canRead()) {
                            val raw = subSpeed.readText().trim().lowercase()
                            val parsed = parseSpeedString(raw)
                            if (parsed != null) return parsed
                        }
                    }
                }
            }
        }

        // Accurate conservative default for Android USB Type-C: USB 2.0 (High Speed)
        return "USB 2.0 (High Speed)"
    }

    private fun parseSpeedString(raw: String): String? {
        return when {
            raw.contains("super-speed-plus") || raw.contains("ss+") || raw.contains("gen2") -> "USB 3.1+ (10 Gbps)"
            raw.contains("super-speed") || raw.contains("super") || raw.contains("5000") || raw.contains("ss") -> "USB 3.0 SuperSpeed (5 Gbps)"
            raw.contains("high-speed") || raw.contains("high") || raw.contains("480") || raw.contains("hs") -> "USB 2.0 (High Speed)"
            raw.contains("full-speed") || raw.contains("full") || raw.contains("12") -> "USB 1.1 (12 Mbps)"
            else -> null
        }
    }
}
