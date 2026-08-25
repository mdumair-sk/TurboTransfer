package com.turbotransfer.domain.repository

interface SettingsRepository {
    fun getDeviceName(): String
    fun setDeviceName(name: String)
    fun is5GhzPreferred(): Boolean
    fun set5GhzPreferred(enabled: Boolean)
    fun isAutoWakeLockEnabled(): Boolean
    fun setAutoWakeLockEnabled(enabled: Boolean)
    fun getReceiveDestDir(): String
    fun setReceiveDestDir(dir: String)
}
