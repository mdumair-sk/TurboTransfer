package com.turbotransfer.presentation.send

import com.turbotransfer.core.common.Resource
import com.turbotransfer.domain.model.*
import com.turbotransfer.domain.repository.DiscoveryRepository
import com.turbotransfer.domain.repository.HotspotRepository
import com.turbotransfer.domain.repository.TransferRepository
import com.turbotransfer.domain.usecase.discovery.ObserveReceiverDiscoveryUseCase
import com.turbotransfer.domain.usecase.hotspot.ObserveHotspotStateUseCase
import com.turbotransfer.domain.usecase.hotspot.StartHotspotUseCase
import com.turbotransfer.domain.usecase.hotspot.StopHotspotUseCase
import com.turbotransfer.domain.usecase.transfer.ObserveTransferProgressUseCase
import com.turbotransfer.domain.usecase.transfer.StartTransferUseCase
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class SendViewModelTest {

    private val testDispatcher = StandardTestDispatcher()

    private class FakeDiscoveryRepository : DiscoveryRepository {
        private val _flow = MutableStateFlow<DiscoveredReceiverInfo?>(null)
        override fun observeReceiverDiscovery(): Flow<DiscoveredReceiverInfo?> = _flow.asStateFlow()
        override suspend fun getNetworkInterfacesAndUsb(): Pair<Boolean, List<String>> = Pair(false, emptyList())
    }

    private class FakeHotspotRepository : HotspotRepository {
        private val _flow = MutableStateFlow(HotspotStateInfo())
        override val hotspotStateFlow: StateFlow<HotspotStateInfo> = _flow.asStateFlow()
        override fun startHotspot(port: Int, onResult: (Resource<String>) -> Unit) {
            onResult(Resource.Success("Hotspot started"))
        }
        override fun stopHotspot() {}
        override fun cleanup() {}
    }

    private class FakeTransferRepository : TransferRepository {
        override val activeSessionFlow = MutableStateFlow<TransferSession?>(null)
        override val isListeningFlow = MutableStateFlow(false)
        override val receiveStatusFlow = MutableStateFlow("Idle")
        override val receiveDestDirFlow = MutableStateFlow("/sdcard/Download")

        val progressFlowMap = mutableMapOf<String, MutableSharedFlow<TransferProgressInfo?>>()
        val startedTransfers = mutableListOf<String>()

        override fun setReceiveDestDir(path: String) {
            receiveDestDirFlow.value = path
        }

        override suspend fun startTransfer(filePath: String, address: String?, fileName: String?): Resource<String> {
            val transferId = "tx-${startedTransfers.size + 1}"
            startedTransfers.add(filePath)
            progressFlowMap.getOrPut(transferId) { MutableSharedFlow(replay = 1) }
            return Resource.Success(transferId)
        }

        override fun observeTransferProgress(transferId: String): Flow<TransferProgressInfo?> {
            return progressFlowMap.getOrPut(transferId) { MutableSharedFlow(replay = 1) }
        }

        override suspend fun enterReceiveMode(destDir: String, address: String?) = Resource.Success("Listening")
        override suspend fun stopReceiveMode() = true
        override suspend fun pauseTransfer(transferId: String) = Resource.Success(Unit)
        override suspend fun resumeTransfer(transferId: String) = Resource.Success(transferId)
        override suspend fun cancelTransfer(transferId: String) = Resource.Success(Unit)
        override fun setActiveSession(session: TransferSession?) {
            activeSessionFlow.value = session
        }
        override fun clearActiveSession() {
            activeSessionFlow.value = null
        }
        override suspend fun pollPendingIncomingTransfer(saveDir: String) = null
    }

    private lateinit var fakeDiscoveryRepo: FakeDiscoveryRepository
    private lateinit var fakeHotspotRepo: FakeHotspotRepository
    private lateinit var fakeTransferRepo: FakeTransferRepository
    private lateinit var viewModel: SendViewModel

    @Before
    fun setUp() {
        Dispatchers.setMain(testDispatcher)
        fakeDiscoveryRepo = FakeDiscoveryRepository()
        fakeHotspotRepo = FakeHotspotRepository()
        fakeTransferRepo = FakeTransferRepository()

        viewModel = SendViewModel(
            observeReceiverDiscoveryUseCase = ObserveReceiverDiscoveryUseCase(fakeDiscoveryRepo),
            observeHotspotStateUseCase = ObserveHotspotStateUseCase(fakeHotspotRepo),
            startHotspotUseCase = StartHotspotUseCase(fakeHotspotRepo),
            stopHotspotUseCase = StopHotspotUseCase(fakeHotspotRepo),
            startTransferUseCase = StartTransferUseCase(fakeTransferRepo),
            observeTransferProgressUseCase = ObserveTransferProgressUseCase(fakeTransferRepo)
        )
    }

    @After
    fun tearDown() {
        Dispatchers.resetMain()
    }

    @Test
    fun testSuccessfulTransferClearsQueueAutomatically() = runTest(testDispatcher) {
        val file = SelectedFileInfo(
            path = "/storage/emulated/0/video.mp4",
            displayName = "video.mp4",
            sizeBytes = 1048576L,
            formattedSize = "1.0 MB",
            category = FileCategory.VIDEO
        )

        viewModel.addFilesToQueue(listOf(file))
        assertEquals(1, viewModel.uiState.value.transferQueue.size)

        var navigationCallbackCalled = false
        viewModel.startBatchTransfer("192.168.1.50:9876") {
            navigationCallbackCalled = true
        }

        advanceUntilIdle()
        assertTrue(navigationCallbackCalled)
        assertEquals(1, fakeTransferRepo.startedTransfers.size)

        // Emit completed progress
        val progressFlow = fakeTransferRepo.progressFlowMap["tx-1"]!!
        progressFlow.emit(
            TransferProgressInfo(
                transferId = "tx-1",
                fileName = "video.mp4",
                bytesTransferred = 1048576L,
                fileSize = 1048576L,
                percent = 100.0,
                aggregateSpeedMBps = 25.0,
                usbSpeedMBps = 25.0,
                wifiSpeedMBps = 0.0,
                etaSeconds = 0L,
                status = TransferStatus.COMPLETED
            )
        )

        advanceUntilIdle()

        // Queue must be cleared automatically on success
        assertTrue(viewModel.uiState.value.transferQueue.isEmpty())
        assertFalse(viewModel.uiState.value.isQueueRunning)
    }

    @Test
    fun testFailedTransferRetainsQueue() = runTest(testDispatcher) {
        val file = SelectedFileInfo(
            path = "/storage/emulated/0/doc.pdf",
            displayName = "doc.pdf",
            sizeBytes = 500000L,
            formattedSize = "500 KB",
            category = FileCategory.DOCUMENT
        )

        viewModel.addFilesToQueue(listOf(file))
        assertEquals(1, viewModel.uiState.value.transferQueue.size)

        viewModel.startBatchTransfer("192.168.1.50:9876") {}
        advanceUntilIdle()

        val progressFlow = fakeTransferRepo.progressFlowMap["tx-1"]!!
        progressFlow.emit(
            TransferProgressInfo(
                transferId = "tx-1",
                fileName = "doc.pdf",
                bytesTransferred = 100000L,
                fileSize = 500000L,
                percent = 20.0,
                aggregateSpeedMBps = 10.0,
                usbSpeedMBps = 10.0,
                wifiSpeedMBps = 0.0,
                etaSeconds = 5L,
                status = TransferStatus.FAILED
            )
        )

        advanceUntilIdle()

        // Queue must be retained on failure
        assertEquals(1, viewModel.uiState.value.transferQueue.size)
        assertFalse(viewModel.uiState.value.isQueueRunning)
    }

    @Test
    fun testCancelledTransferRetainsQueue() = runTest(testDispatcher) {
        val file = SelectedFileInfo(
            path = "/storage/emulated/0/image.png",
            displayName = "image.png",
            sizeBytes = 200000L,
            formattedSize = "200 KB",
            category = FileCategory.IMAGE
        )

        viewModel.addFilesToQueue(listOf(file))
        assertEquals(1, viewModel.uiState.value.transferQueue.size)

        viewModel.startBatchTransfer("192.168.1.50:9876") {}
        advanceUntilIdle()

        val progressFlow = fakeTransferRepo.progressFlowMap["tx-1"]!!
        progressFlow.emit(
            TransferProgressInfo(
                transferId = "tx-1",
                fileName = "image.png",
                bytesTransferred = 50000L,
                fileSize = 200000L,
                percent = 25.0,
                aggregateSpeedMBps = 5.0,
                usbSpeedMBps = 5.0,
                wifiSpeedMBps = 0.0,
                etaSeconds = 10L,
                status = TransferStatus.CANCELLED
            )
        )

        advanceUntilIdle()

        // Queue must be retained on cancellation
        assertEquals(1, viewModel.uiState.value.transferQueue.size)
        assertFalse(viewModel.uiState.value.isQueueRunning)
    }

    @Test
    fun testBatchTransferSequentiallyProcessesAndClearsQueueOnSuccess() = runTest(testDispatcher) {
        val file1 = SelectedFileInfo(
            path = "/storage/emulated/0/file1.mp4",
            displayName = "file1.mp4",
            sizeBytes = 1000L,
            formattedSize = "1 KB",
            category = FileCategory.VIDEO
        )
        val file2 = SelectedFileInfo(
            path = "/storage/emulated/0/file2.mp4",
            displayName = "file2.mp4",
            sizeBytes = 2000L,
            formattedSize = "2 KB",
            category = FileCategory.VIDEO
        )

        viewModel.addFilesToQueue(listOf(file1, file2))
        assertEquals(2, viewModel.uiState.value.transferQueue.size)

        viewModel.startBatchTransfer("192.168.1.50:9876") {}
        advanceUntilIdle()

        assertEquals(1, fakeTransferRepo.startedTransfers.size)

        // Complete first file
        val progressFlow1 = fakeTransferRepo.progressFlowMap["tx-1"]!!
        progressFlow1.emit(
            TransferProgressInfo(
                transferId = "tx-1",
                fileName = "file1.mp4",
                bytesTransferred = 1000L,
                fileSize = 1000L,
                percent = 100.0,
                aggregateSpeedMBps = 20.0,
                usbSpeedMBps = 20.0,
                wifiSpeedMBps = 0.0,
                etaSeconds = 0L,
                status = TransferStatus.COMPLETED
            )
        )

        advanceUntilIdle()

        // After first completes, queue has 1 item and second transfer has started
        assertEquals(1, viewModel.uiState.value.transferQueue.size)
        assertEquals("file2.mp4", viewModel.uiState.value.transferQueue[0].displayName)
        assertEquals(2, fakeTransferRepo.startedTransfers.size)

        // Complete second file
        val progressFlow2 = fakeTransferRepo.progressFlowMap["tx-2"]!!
        progressFlow2.emit(
            TransferProgressInfo(
                transferId = "tx-2",
                fileName = "file2.mp4",
                bytesTransferred = 2000L,
                fileSize = 2000L,
                percent = 100.0,
                aggregateSpeedMBps = 20.0,
                usbSpeedMBps = 20.0,
                wifiSpeedMBps = 0.0,
                etaSeconds = 0L,
                status = TransferStatus.COMPLETED
            )
        )

        advanceUntilIdle()

        // Entire batch is complete, queue is now empty
        assertTrue(viewModel.uiState.value.transferQueue.isEmpty())
        assertFalse(viewModel.uiState.value.isQueueRunning)
    }
}
