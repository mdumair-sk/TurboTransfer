package com.turbotransfer

import android.app.Application
import dagger.hilt.android.HiltAndroidApp

@HiltAndroidApp
class TurboTransferApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        try {
            System.loadLibrary("turbotransfer_core")
            uniffi.turbotransfer_core.initLogger()
            val appDataDir = filesDir.absolutePath
            uniffi.turbotransfer_core.setDataDirectory(appDataDir)
        } catch (e: UnsatisfiedLinkError) {
            e.printStackTrace()
        } catch (e: Throwable) {
            e.printStackTrace()
        }
    }
}
