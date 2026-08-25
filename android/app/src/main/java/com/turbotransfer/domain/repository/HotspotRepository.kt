package com.turbotransfer.domain.repository

import com.turbotransfer.core.common.Resource
import com.turbotransfer.domain.model.HotspotStateInfo
import kotlinx.coroutines.flow.StateFlow

interface HotspotRepository {
    val hotspotStateFlow: StateFlow<HotspotStateInfo>
    fun startHotspot(port: Int = 9876, onResult: (Resource<String>) -> Unit)
    fun stopHotspot()
    fun cleanup()
}
