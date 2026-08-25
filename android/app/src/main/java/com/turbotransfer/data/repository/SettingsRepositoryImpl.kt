package com.turbotransfer.data.repository

import com.turbotransfer.data.source.local.SettingsLocalDataSource
import com.turbotransfer.domain.repository.SettingsRepository
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class SettingsRepositoryImpl @Inject constructor(
    private val localDataSource: SettingsLocalDataSource
) : SettingsRepository {

    override fun getDeviceName(): String = localDataSource.getDeviceName()

    override fun setDeviceName(name: String) {
        localDataSource.setDeviceName(name)
    }

    override fun is5GhzPreferred(): Boolean = localDataSource.is5GhzPreferred()

    override fun set5GhzPreferred(enabled: Boolean) {
        localDataSource.set5GhzPreferred(enabled)
    }

    override fun isAutoWakeLockEnabled(): Boolean = localDataSource.isAutoWakeLockEnabled()

    override fun setAutoWakeLockEnabled(enabled: Boolean) {
        localDataSource.setAutoWakeLockEnabled(enabled)
    }

    override fun getReceiveDestDir(): String = localDataSource.getReceiveDestDir()

    override fun setReceiveDestDir(dir: String) {
        localDataSource.setReceiveDestDir(dir)
    }
}
