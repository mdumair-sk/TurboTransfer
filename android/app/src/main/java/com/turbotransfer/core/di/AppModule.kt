package com.turbotransfer.core.di

import android.content.Context
import com.turbotransfer.WifiHotspotManager
import com.turbotransfer.core.common.DefaultDispatcherProvider
import com.turbotransfer.core.common.DispatcherProvider
import com.turbotransfer.core.util.TransferLockManager
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
object AppModule {

    @Provides
    @Singleton
    fun provideDispatcherProvider(): DispatcherProvider {
        return DefaultDispatcherProvider()
    }

    @Provides
    @Singleton
    fun provideTransferLockManager(@ApplicationContext context: Context): TransferLockManager {
        return TransferLockManager(context)
    }

    @Provides
    @Singleton
    fun provideWifiHotspotManager(@ApplicationContext context: Context): WifiHotspotManager {
        return WifiHotspotManager(context)
    }
}
