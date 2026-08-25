package com.turbotransfer

import android.app.Application
import dagger.hilt.android.HiltAndroidApp

@HiltAndroidApp
class TurboTransferApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        try {
            System.loadLibrary("turbotransfer_core")
        } catch (e: UnsatisfiedLinkError) {
            e.printStackTrace()
        }
    }
}
