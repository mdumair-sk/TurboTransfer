package com.turbotransfer

import com.turbotransfer.core.common.Resource
import com.turbotransfer.domain.model.HistoryItem
import com.turbotransfer.domain.repository.HistoryRepository
import com.turbotransfer.domain.repository.TransferRepository
import com.turbotransfer.domain.usecase.history.AddHistoryRecordUseCase
import com.turbotransfer.domain.usecase.history.GetHistoryUseCase
import com.turbotransfer.domain.usecase.transfer.StartTransferUseCase
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class CleanArchitectureUseCasesTest {

    private class FakeHistoryRepository : HistoryRepository {
        private val list = mutableListOf<HistoryItem>()
        private val _flow = MutableStateFlow<List<HistoryItem>>(emptyList())
        override val historyFlow: StateFlow<List<HistoryItem>> = _flow.asStateFlow()

        override fun addTransferRecord(record: HistoryItem) {
            list.add(0, record)
            _flow.value = list.toList()
        }

        override fun deleteRecord(id: String) {
            list.removeAll { it.id == id }
            _flow.value = list.toList()
        }

        override fun clearHistory() {
            list.clear()
            _flow.value = emptyList()
        }
    }

    private class FakeTransferRepository : TransferRepository {
        override val activeSessionFlow = MutableStateFlow<com.turbotransfer.domain.model.TransferSession?>(null)
        override val isListeningFlow = MutableStateFlow(false)
        override val receiveStatusFlow = MutableStateFlow("Idle")
        override val receiveDestDirFlow = MutableStateFlow("/sdcard/Download")

        override fun setReceiveDestDir(path: String) {
            receiveDestDirFlow.value = path
        }

        override suspend fun startTransfer(filePath: String, address: String?): Resource<String> {
            return Resource.Success("test-transfer-id-123")
        }

        override fun observeTransferProgress(transferId: String) = kotlinx.coroutines.flow.flowOf<com.turbotransfer.domain.model.TransferProgressInfo?>(null)
        override suspend fun enterReceiveMode(destDir: String, address: String?) = Resource.Success("Listening")
        override suspend fun stopReceiveMode() = true
        override suspend fun pauseTransfer(transferId: String) = Resource.Success(Unit)
        override suspend fun resumeTransfer(transferId: String) = Resource.Success("test-transfer-id-123")
        override suspend fun cancelTransfer(transferId: String) = Resource.Success(Unit)
        override fun setActiveSession(session: com.turbotransfer.domain.model.TransferSession?) {
            activeSessionFlow.value = session
        }
        override fun clearActiveSession() {
            activeSessionFlow.value = null
        }
        override suspend fun pollPendingIncomingTransfer(saveDir: String) = null
    }

    @Test
    fun testStartTransferUseCaseReturnsSuccessId() = runBlocking {
        val fakeRepo = FakeTransferRepository()
        val useCase = StartTransferUseCase(fakeRepo)

        val result = useCase("/sdcard/test.mp4", "127.0.0.1:9876")
        assertTrue(result is Resource.Success)
        assertEquals("test-transfer-id-123", (result as Resource.Success).data)
    }

    @Test
    fun testAddAndGetHistoryUseCase() = runBlocking {
        val fakeRepo = FakeHistoryRepository()
        val addUseCase = AddHistoryRecordUseCase(fakeRepo)
        val getUseCase = GetHistoryUseCase(fakeRepo)

        val item = HistoryItem(
            id = "tx-1",
            fileName = "sample.zip",
            fileSize = 1048576L,
            formattedSize = "1.00 MB",
            filePath = "/sdcard/sample.zip",
            isOutgoing = true,
            timestamp = System.currentTimeMillis(),
            formattedDate = "Today",
            durationMs = 1200L,
            avgSpeedMBps = 24.5,
            peakSpeedMBps = 32.0,
            usbSpeedMBps = 32.0,
            wifiSpeedMBps = 0.0,
            status = "Completed"
        )

        addUseCase(item)
        val historyList = getUseCase().value
        assertEquals(1, historyList.size)
        assertEquals("sample.zip", historyList[0].fileName)
    }
}
