package com.turbotransfer.core.di

import com.turbotransfer.data.repository.DiscoveryRepositoryImpl
import com.turbotransfer.data.repository.HistoryRepositoryImpl
import com.turbotransfer.data.repository.HotspotRepositoryImpl
import com.turbotransfer.data.repository.SettingsRepositoryImpl
import com.turbotransfer.data.repository.TransferRepositoryImpl
import com.turbotransfer.domain.repository.DiscoveryRepository
import com.turbotransfer.domain.repository.HistoryRepository
import com.turbotransfer.domain.repository.HotspotRepository
import com.turbotransfer.domain.repository.SettingsRepository
import com.turbotransfer.domain.repository.TransferRepository
import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
abstract class RepositoryModule {

    @Binds
    @Singleton
    abstract fun bindTransferRepository(
        impl: TransferRepositoryImpl
    ): TransferRepository

    @Binds
    @Singleton
    abstract fun bindHistoryRepository(
        impl: HistoryRepositoryImpl
    ): HistoryRepository

    @Binds
    @Singleton
    abstract fun bindHotspotRepository(
        impl: HotspotRepositoryImpl
    ): HotspotRepository

    @Binds
    @Singleton
    abstract fun bindDiscoveryRepository(
        impl: DiscoveryRepositoryImpl
    ): DiscoveryRepository

    @Binds
    @Singleton
    abstract fun bindSettingsRepository(
        impl: SettingsRepositoryImpl
    ): SettingsRepository
}
