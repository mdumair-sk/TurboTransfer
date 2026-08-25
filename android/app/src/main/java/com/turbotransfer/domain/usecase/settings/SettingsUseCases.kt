package com.turbotransfer.domain.usecase.settings

import com.turbotransfer.domain.repository.SettingsRepository
import javax.inject.Inject

class GetSettingsUseCase @Inject constructor(
    private val settingsRepository: SettingsRepository
) {
    fun getDeviceName(): String = settingsRepository.getDeviceName()
    fun is5GhzPreferred(): Boolean = settingsRepository.is5GhzPreferred()
    fun isAutoWakeLockEnabled(): Boolean = settingsRepository.isAutoWakeLockEnabled()
    fun getReceiveDestDir(): String = settingsRepository.getReceiveDestDir()
}

class UpdateSettingsUseCase @Inject constructor(
    private val settingsRepository: SettingsRepository
) {
    fun setDeviceName(name: String) = settingsRepository.setDeviceName(name)
    fun set5GhzPreferred(enabled: Boolean) = settingsRepository.set5GhzPreferred(enabled)
    fun setAutoWakeLockEnabled(enabled: Boolean) = settingsRepository.setAutoWakeLockEnabled(enabled)
    fun setReceiveDestDir(dir: String) = settingsRepository.setReceiveDestDir(dir)
}
