package com.turbotransfer.domain.repository

import com.turbotransfer.domain.model.DiscoveredReceiverInfo
import kotlinx.coroutines.flow.Flow

interface DiscoveryRepository {
    fun observeReceiverDiscovery(): Flow<DiscoveredReceiverInfo?>
    suspend fun getNetworkInterfacesAndUsb(): Pair<Boolean, List<String>>
}
