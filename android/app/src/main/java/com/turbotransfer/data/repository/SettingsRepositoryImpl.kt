package com.turbotransfer.data.repository

import com.turbotransfer.data.source.local.SettingsLocalDataSource
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
open class SettingsRepositoryImpl(
    private val localDataSource: SettingsLocalDataSource?,
    @Suppress("UNUSED_PARAMETER") dummy: Unit?
) {
    @Inject
    constructor(
        localDataSource: SettingsLocalDataSource
    ) : this(localDataSource, null)

    constructor() : this(null, null)

    open fun getDeviceName(): String = localDataSource?.getDeviceName() ?: "Android Device"

    open fun setDeviceName(name: String) {
        localDataSource?.setDeviceName(name)
    }

    open fun is5GhzPreferred(): Boolean = localDataSource?.is5GhzPreferred() ?: true

    open fun set5GhzPreferred(enabled: Boolean) {
        localDataSource?.set5GhzPreferred(enabled)
    }

    open fun isAutoWakeLockEnabled(): Boolean = localDataSource?.isAutoWakeLockEnabled() ?: true

    open fun setAutoWakeLockEnabled(enabled: Boolean) {
        localDataSource?.setAutoWakeLockEnabled(enabled)
    }

    open fun getReceiveDestDir(): String = localDataSource?.getReceiveDestDir() ?: "/sdcard/Download"

    open fun setReceiveDestDir(dir: String) {
        localDataSource?.setReceiveDestDir(dir)
    }
}
