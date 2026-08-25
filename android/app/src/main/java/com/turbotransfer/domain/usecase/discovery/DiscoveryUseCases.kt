package com.turbotransfer.domain.usecase.discovery

import com.turbotransfer.domain.model.DiscoveredReceiverInfo
import com.turbotransfer.domain.repository.DiscoveryRepository
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

class ObserveReceiverDiscoveryUseCase @Inject constructor(
    private val discoveryRepository: DiscoveryRepository
) {
    operator fun invoke(): Flow<DiscoveredReceiverInfo?> {
        return discoveryRepository.observeReceiverDiscovery()
    }
}

class GetNetworkStatusUseCase @Inject constructor(
    private val discoveryRepository: DiscoveryRepository
) {
    suspend operator fun invoke(): Pair<Boolean, List<String>> {
        return discoveryRepository.getNetworkInterfacesAndUsb()
    }
}

class GetUsbSpeedLabelUseCase @Inject constructor(
    private val discoveryRepository: DiscoveryRepository
) {
    operator fun invoke(): String {
        return discoveryRepository.getUsbSpeedLabel()
    }
}
