package com.turbotransfer.domain.usecase.hotspot

import com.turbotransfer.core.common.Resource
import com.turbotransfer.domain.model.HotspotStateInfo
import com.turbotransfer.domain.repository.HotspotRepository
import kotlinx.coroutines.flow.StateFlow
import javax.inject.Inject

class ObserveHotspotStateUseCase @Inject constructor(
    private val hotspotRepository: HotspotRepository
) {
    operator fun invoke(): StateFlow<HotspotStateInfo> {
        return hotspotRepository.hotspotStateFlow
    }
}

class StartHotspotUseCase @Inject constructor(
    private val hotspotRepository: HotspotRepository
) {
    operator fun invoke(port: Int = 9876, onResult: (Resource<String>) -> Unit) {
        hotspotRepository.startHotspot(port, onResult)
    }
}

class StopHotspotUseCase @Inject constructor(
    private val hotspotRepository: HotspotRepository
) {
    operator fun invoke() {
        hotspotRepository.stopHotspot()
    }
}
