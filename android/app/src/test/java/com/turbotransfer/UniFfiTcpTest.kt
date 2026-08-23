package com.turbotransfer

import org.junit.Test
import org.junit.Assert.*
import uniffi.turbotransfer_core.FfiTransferStatus
import uniffi.turbotransfer_core.FfiTransportPreference

class UniFfiTcpTest {

    @Test
    fun testFfiTransportEnumsAreAvailableToTheJvm() {
        assertEquals(FfiTransportPreference.AUTOMATIC, FfiTransportPreference.valueOf("AUTOMATIC"))
        assertEquals(FfiTransportPreference.USB_ONLY, FfiTransportPreference.valueOf("USB_ONLY"))
    }

    @Test
    fun testFfiStatusEnumsAreAvailableToTheJvm() {
        assertEquals(FfiTransferStatus.IN_PROGRESS, FfiTransferStatus.valueOf("IN_PROGRESS"))
        assertEquals(FfiTransferStatus.CANCELLED, FfiTransferStatus.valueOf("CANCELLED"))
    }
}
