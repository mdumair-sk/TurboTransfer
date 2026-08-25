package com.turbotransfer

import android.Manifest
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.*
import androidx.compose.animation.core.*
import androidx.compose.foundation.*
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.turbotransfer_core.*
import java.io.File

data class DiscoveredReceiver(
    val address: String,
    val displayName: String,
    val transport: String,
    val isReady: Boolean,
    val isUsbAvailable: Boolean = false,
    val isWifiAvailable: Boolean = false
)

data class ActiveTransferSessionState(
    val transferId: String,
    val fileName: String,
    val fileSize: Long,
    val formattedSize: String,
    val filePath: String,
    val isOutgoing: Boolean, // true = Sending, false = Receiving
    val startTimeMs: Long = System.currentTimeMillis()
)

class MainActivity : ComponentActivity() {

    companion object {
        val activeTransferIdFlow = kotlinx.coroutines.flow.MutableStateFlow<String?>(null)
        val selectedTabFlow = kotlinx.coroutines.flow.MutableStateFlow<Int>(0)
        val activeSessionFlow = kotlinx.coroutines.flow.MutableStateFlow<ActiveTransferSessionState?>(null)
        val lastCompletedItemFlow = kotlinx.coroutines.flow.MutableStateFlow<TransferHistoryItem?>(null)
        val isListeningFlow = kotlinx.coroutines.flow.MutableStateFlow<Boolean>(false)
        val receiveStatusFlow = kotlinx.coroutines.flow.MutableStateFlow<String>("Idle")
        val receiveDestDirFlow = kotlinx.coroutines.flow.MutableStateFlow<String>(
            Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS).absolutePath
        )
    }

    private lateinit var spikeManager: WifiDirectSpikeManager
    private lateinit var hotspotManager: WifiHotspotManager

    private val transferReceiver = object : android.content.BroadcastReceiver() {
        override fun onReceive(context: Context?, intent: android.content.Intent?) {
            when (intent?.action) {
                "com.turbotransfer.START_TRANSFER" -> {
                    val path = intent.getStringExtra("file_path") ?: return
                    val address = intent.getStringExtra("address") ?: "127.0.0.1:9876"
                    android.util.Log.d("TurboTransfer", "Broadcast received: START_TRANSFER path=$path, address=$address")
                    kotlinx.coroutines.CoroutineScope(Dispatchers.IO).launch {
                        try {
                            val handle = startTransfer(
                                filePath = path,
                                deviceId = null,
                                transportPref = FfiTransportPreference.AUTOMATIC,
                                address = address.ifBlank { null }
                            )
                            val file = File(path)
                            activeSessionFlow.value = ActiveTransferSessionState(
                                transferId = handle.transferId,
                                fileName = file.name,
                                fileSize = file.length(),
                                formattedSize = UriUtils.formatFileSize(file.length()),
                                filePath = path,
                                isOutgoing = true
                            )
                            activeTransferIdFlow.value = handle.transferId
                            selectedTabFlow.value = 2 // Switch to Transfer Dashboard
                        } catch (e: Exception) {
                            android.util.Log.e("TurboTransfer", "Failed to start transfer via broadcast", e)
                        }
                    }
                }
                "com.turbotransfer.START_HOTSPOT" -> {
                    hotspotManager.startHotspot { _, msg ->
                        android.util.Log.i("TurboTransfer", "Auto-started 5GHz hotspot: $msg")
                    }
                }
                "com.turbotransfer.STOP_HOTSPOT" -> {
                    hotspotManager.stopHotspot()
                }
                "com.turbotransfer.ENTER_RECEIVE" -> {
                    val dest = intent.getStringExtra("dest_dir") ?: receiveDestDirFlow.value
                    kotlinx.coroutines.CoroutineScope(Dispatchers.IO).launch {
                        try {
                            val statusMsg = enterReceiveMode(null, dest)
                            isListeningFlow.value = true
                            receiveStatusFlow.value = statusMsg
                            selectedTabFlow.value = 1 // Switch to Receive tab
                        } catch (e: Exception) {
                            if (e.message?.contains("already active") == true) {
                                isListeningFlow.value = true
                                receiveStatusFlow.value = "Listening on 0.0.0.0:9876"
                                selectedTabFlow.value = 1
                            } else {
                                android.util.Log.e("TurboTransfer", "Failed to enter receive mode via broadcast", e)
                            }
                        }
                    }
                }
            }
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        try {
            System.loadLibrary("turbotransfer_core")
        } catch (e: UnsatisfiedLinkError) {
            e.printStackTrace()
        }

        TransferHistoryManager.init(this)
        hotspotManager = WifiHotspotManager(this)
        spikeManager = WifiDirectSpikeManager(this)

        val filter = android.content.IntentFilter().apply {
            addAction("com.turbotransfer.START_TRANSFER")
            addAction("com.turbotransfer.START_HOTSPOT")
            addAction("com.turbotransfer.STOP_HOTSPOT")
            addAction("com.turbotransfer.ENTER_RECEIVE")
        }
        ContextCompat.registerReceiver(
            this,
            transferReceiver,
            filter,
            ContextCompat.RECEIVER_EXPORTED
        )

        setContent {
            TurboTransferTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.background
                ) {
                    TurboTransferApp(
                        spikeManager = spikeManager,
                        hotspotManager = hotspotManager
                    )
                }
            }
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        try {
            unregisterReceiver(transferReceiver)
        } catch (_: Exception) {}
        hotspotManager.cleanup()
        TransferLockManager.releaseLocks()
    }
}

object TransferLockManager {
    private var wifiLock: android.net.wifi.WifiManager.WifiLock? = null
    private var wakeLock: android.os.PowerManager.WakeLock? = null

    @Synchronized
    fun acquireLocks(context: Context) {
        try {
            val wifiManager = context.applicationContext.getSystemService(Context.WIFI_SERVICE) as? android.net.wifi.WifiManager
            val powerManager = context.applicationContext.getSystemService(Context.POWER_SERVICE) as? android.os.PowerManager

            if (wifiLock == null && wifiManager != null) {
                val lockType = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                    android.net.wifi.WifiManager.WIFI_MODE_FULL_LOW_LATENCY
                } else {
                    android.net.wifi.WifiManager.WIFI_MODE_FULL_HIGH_PERF
                }
                wifiLock = wifiManager.createWifiLock(lockType, "TurboTransfer:ActiveTransferWifiLock").apply {
                    setReferenceCounted(false)
                    acquire()
                }
                android.util.Log.i("TurboTransfer", "Acquired ActiveTransfer WifiLock (mode=$lockType)")
            }

            if (wakeLock == null && powerManager != null) {
                wakeLock = powerManager.newWakeLock(android.os.PowerManager.PARTIAL_WAKE_LOCK, "TurboTransfer:ActiveTransferWakeLock").apply {
                    setReferenceCounted(false)
                    acquire(60 * 60 * 1000L)
                }
                android.util.Log.i("TurboTransfer", "Acquired ActiveTransfer WakeLock")
            }
        } catch (e: Exception) {
            android.util.Log.w("TurboTransfer", "Failed to acquire transfer locks", e)
        }
    }

    @Synchronized
    fun releaseLocks() {
        try {
            if (wifiLock?.isHeld == true) {
                wifiLock?.release()
            }
        } catch (e: Exception) {
            android.util.Log.w("TurboTransfer", "Error releasing transfer WifiLock", e)
        }
        wifiLock = null

        try {
            if (wakeLock?.isHeld == true) {
                wakeLock?.release()
            }
        } catch (e: Exception) {
            android.util.Log.w("TurboTransfer", "Error releasing transfer WakeLock", e)
        }
        wakeLock = null
    }
}

// -------------------------------------------------------------------------------------------------
// MAIN APPLICATION COMPOSABLE WITH BOTTOM NAVIGATION
// -------------------------------------------------------------------------------------------------

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TurboTransferApp(
    spikeManager: WifiDirectSpikeManager,
    hotspotManager: WifiHotspotManager
) {
    val activeTransferId by MainActivity.activeTransferIdFlow.collectAsState()
    val activeSession by MainActivity.activeSessionFlow.collectAsState()
    val lastCompletedItem by MainActivity.lastCompletedItemFlow.collectAsState()
    val tabFromFlow by MainActivity.selectedTabFlow.collectAsState()
    var selectedTab by remember { mutableIntStateOf(0) }

    LaunchedEffect(tabFromFlow) {
        selectedTab = tabFromFlow
    }

    var currentProgress by remember { mutableStateOf<FfiTransferProgress?>(null) }
    val receiveStatus by MainActivity.receiveStatusFlow.collectAsState()
    val isListening by MainActivity.isListeningFlow.collectAsState()

    // Multi-File Transfer Queue
    val transferQueue = remember { mutableStateListOf<SelectedFileInfo>() }
    var currentQueueIndex by remember { mutableIntStateOf(0) }
    var isQueueRunning by remember { mutableStateOf(false) }
    var targetAddressForQueue by remember { mutableStateOf("127.0.0.1:9876") }

    // Metrics tracking for completion summary
    var peakTotalSpeed by remember { mutableDoubleStateOf(0.0) }
    var peakUsbSpeed by remember { mutableDoubleStateOf(0.0) }
    var peakWifiSpeed by remember { mutableDoubleStateOf(0.0) }
    var speedSum by remember { mutableDoubleStateOf(0.0) }
    var speedSampleCount by remember { mutableIntStateOf(0) }

    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    val hotspotState by hotspotManager.state.collectAsState()

    // Request permissions needed for Wi-Fi Direct and Storage
    val permissionsLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.RequestMultiplePermissions()
    ) { permissions ->
        val allGranted = permissions.values.all { it }
        if (!allGranted) {
            Toast.makeText(context, "Permissions needed for full functionality", Toast.LENGTH_SHORT).show()
        }
    }

    LaunchedEffect(Unit) {
        val permissions = mutableListOf(
            Manifest.permission.ACCESS_FINE_LOCATION,
            Manifest.permission.ACCESS_COARSE_LOCATION,
            Manifest.permission.WAKE_LOCK
        )
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            permissions.add(Manifest.permission.NEARBY_WIFI_DEVICES)
            permissions.add(Manifest.permission.READ_MEDIA_IMAGES)
            permissions.add(Manifest.permission.READ_MEDIA_VIDEO)
            permissions.add(Manifest.permission.READ_MEDIA_AUDIO)
        }
        val needed = permissions.filter {
            ContextCompat.checkSelfPermission(context, it) != PackageManager.PERMISSION_GRANTED
        }
        if (needed.isNotEmpty()) {
            permissionsLauncher.launch(needed.toTypedArray())
        }
    }

    // Function to run sequential queue transfers (one file at a time)
    fun processNextInQueue() {
        if (currentQueueIndex < transferQueue.size) {
            val fileInfo = transferQueue[currentQueueIndex]
            scope.launch(Dispatchers.IO) {
                try {
                    peakTotalSpeed = 0.0
                    peakUsbSpeed = 0.0
                    peakWifiSpeed = 0.0
                    speedSum = 0.0
                    speedSampleCount = 0

                    val handle = startTransfer(
                        filePath = fileInfo.path,
                        deviceId = null,
                        transportPref = FfiTransportPreference.AUTOMATIC,
                        address = targetAddressForQueue.ifBlank { null }
                    )
                    withContext(Dispatchers.Main) {
                        MainActivity.activeSessionFlow.value = ActiveTransferSessionState(
                            transferId = handle.transferId,
                            fileName = fileInfo.displayName,
                            fileSize = fileInfo.sizeBytes,
                            formattedSize = fileInfo.formattedSize,
                            filePath = fileInfo.path,
                            isOutgoing = true
                        )
                        MainActivity.activeTransferIdFlow.value = handle.transferId
                        MainActivity.selectedTabFlow.value = 2 // Switch to Transfer Dashboard
                    }
                } catch (e: Exception) {
                    android.util.Log.e("TurboTransfer", "Failed to start queue item: ${fileInfo.displayName}", e)
                    withContext(Dispatchers.Main) {
                        Toast.makeText(context, "Error: ${e.message}", Toast.LENGTH_SHORT).show()
                        currentQueueIndex++
                        if (currentQueueIndex < transferQueue.size) {
                            processNextInQueue()
                        } else {
                            isQueueRunning = false
                        }
                    }
                }
            }
        } else {
            isQueueRunning = false
        }
    }

    // Background detector for incoming or external active transfers
    LaunchedEffect(Unit) {
        while (true) {
            try {
                if (MainActivity.activeTransferIdFlow.value == null) {
                    val transfers = withContext(Dispatchers.IO) {
                        try {
                            getTransfers()
                        } catch (e: Exception) {
                            emptyList()
                        }
                    }
                    val activeTransfer = transfers.firstOrNull {
                        it.status == FfiTransferStatus.IN_PROGRESS && it.role == FfiTransferRole.RECEIVER
                    } ?: transfers.firstOrNull {
                        it.status == FfiTransferStatus.IN_PROGRESS
                    }

                    if (activeTransfer != null) {
                        val saveDir = MainActivity.receiveDestDirFlow.value
                        val resolvedPath = File(saveDir, activeTransfer.fileName).absolutePath
                        val isOut = (activeTransfer.role == FfiTransferRole.SENDER)

                        peakTotalSpeed = 0.0
                        peakUsbSpeed = 0.0
                        peakWifiSpeed = 0.0
                        speedSum = 0.0
                        speedSampleCount = 0

                        MainActivity.activeSessionFlow.value = ActiveTransferSessionState(
                            transferId = activeTransfer.transferId,
                            fileName = activeTransfer.fileName,
                            fileSize = activeTransfer.fileSize.toLong(),
                            formattedSize = UriUtils.formatFileSize(activeTransfer.fileSize.toLong()),
                            filePath = resolvedPath,
                            isOutgoing = isOut,
                            startTimeMs = System.currentTimeMillis()
                        )
                        MainActivity.activeTransferIdFlow.value = activeTransfer.transferId
                        MainActivity.selectedTabFlow.value = 2 // Auto-switch to Transfer Dashboard!
                    }
                }
            } catch (e: Exception) {
                android.util.Log.w("TurboTransfer", "Transfer polling error", e)
            }
            delay(400)
        }
    }

    // Polling loop for active transfer progress
    LaunchedEffect(activeTransferId) {
        val transferId = activeTransferId ?: return@LaunchedEffect
        TransferLockManager.acquireLocks(context)
        try {
            while (true) {
                try {
                    val p = withContext(Dispatchers.IO) {
                        getProgress(transferId)
                    }
                    currentProgress = p

                    if (p != null) {
                        val currentSpeedMBps = p.aggregateThroughputBps / (1024.0 * 1024.0)
                        val currentUsbMBps = p.usbThroughputBps / (1024.0 * 1024.0)
                        val currentWifiMBps = p.wifiThroughputBps / (1024.0 * 1024.0)

                        if (currentSpeedMBps > peakTotalSpeed) peakTotalSpeed = currentSpeedMBps
                        if (currentUsbMBps > peakUsbSpeed) peakUsbSpeed = currentUsbMBps
                        if (currentWifiMBps > peakWifiSpeed) peakWifiSpeed = currentWifiMBps

                        if (currentSpeedMBps > 0.1) {
                            speedSum += currentSpeedMBps
                            speedSampleCount++
                        }

                        // Handle Completion
                        if (p.status == FfiTransferStatus.COMPLETED) {
                            val durationMs = activeSession?.let { System.currentTimeMillis() - it.startTimeMs } ?: 1000L
                            val avgSpeed = if (speedSampleCount > 0) speedSum / speedSampleCount else peakTotalSpeed
                            val session = activeSession
                            val saveDir = MainActivity.receiveDestDirFlow.value
                            val finalFilePath = session?.filePath?.ifBlank { File(saveDir, p.fileName).absolutePath } ?: File(saveDir, p.fileName).absolutePath
                            val isOutgoing = session?.isOutgoing ?: false

                            val item = TransferHistoryItem(
                                id = transferId,
                                fileName = session?.fileName ?: p.fileName,
                                fileSize = session?.fileSize ?: p.fileSize.toLong(),
                                formattedSize = UriUtils.formatFileSize(session?.fileSize ?: p.fileSize.toLong()),
                                filePath = finalFilePath,
                                isOutgoing = isOutgoing,
                                timestamp = System.currentTimeMillis(),
                                formattedDate = "Just now",
                                durationMs = durationMs,
                                avgSpeedMBps = avgSpeed,
                                peakSpeedMBps = peakTotalSpeed,
                                usbSpeedMBps = peakUsbSpeed,
                                wifiSpeedMBps = peakWifiSpeed,
                                status = "Completed"
                            )

                            TransferHistoryManager.addTransferRecord(
                                id = item.id,
                                fileName = item.fileName,
                                fileSize = item.fileSize,
                                filePath = item.filePath,
                                isOutgoing = item.isOutgoing,
                                durationMs = item.durationMs,
                                avgSpeedMBps = item.avgSpeedMBps,
                                peakSpeedMBps = item.peakSpeedMBps,
                                usbSpeedMBps = item.usbSpeedMBps,
                                wifiSpeedMBps = item.wifiSpeedMBps,
                                status = "Completed"
                            )

                            // Index in Android MediaStore / Downloads
                            if (!isOutgoing && finalFilePath.isNotBlank()) {
                                try {
                                    android.media.MediaScannerConnection.scanFile(
                                        context,
                                        arrayOf(finalFilePath),
                                        null
                                    ) { path, uri ->
                                        android.util.Log.i("TurboTransfer", "Media scanned: $path -> $uri")
                                    }
                                } catch (e: Exception) {
                                    android.util.Log.w("TurboTransfer", "Media scan failed", e)
                                }
                            }

                            MainActivity.lastCompletedItemFlow.value = item
                            withContext(Dispatchers.Main) {
                                val verb = if (isOutgoing) "Sent" else "Received"
                                Toast.makeText(context, "$verb ${item.fileName} successfully!", Toast.LENGTH_SHORT).show()
                            }

                            // Advance queue if multi-file transfer is active
                            if (isQueueRunning) {
                                currentQueueIndex++
                                if (currentQueueIndex < transferQueue.size) {
                                    delay(500)
                                    processNextInQueue()
                                } else {
                                    isQueueRunning = false
                                }
                            }
                            break
                        } else if (p.status == FfiTransferStatus.FAILED || p.status == FfiTransferStatus.CANCELLED) {
                            val session = activeSession
                            val saveDir = MainActivity.receiveDestDirFlow.value
                            val finalFilePath = session?.filePath?.ifBlank { File(saveDir, p.fileName).absolutePath } ?: File(saveDir, p.fileName).absolutePath
                            val isOutgoing = session?.isOutgoing ?: false

                            TransferHistoryManager.addTransferRecord(
                                id = transferId,
                                fileName = session?.fileName ?: p.fileName,
                                fileSize = session?.fileSize ?: p.fileSize.toLong(),
                                filePath = finalFilePath,
                                isOutgoing = isOutgoing,
                                durationMs = 0L,
                                avgSpeedMBps = 0.0,
                                peakSpeedMBps = 0.0,
                                usbSpeedMBps = 0.0,
                                wifiSpeedMBps = 0.0,
                                status = if (p.status == FfiTransferStatus.FAILED) "Failed" else "Cancelled"
                            )
                            if (isQueueRunning) {
                                isQueueRunning = false
                            }
                            break
                        }
                    }
                } catch (e: Exception) {
                    android.util.Log.e("TurboTransfer", "Failed to read transfer progress", e)
                    break
                }
                delay(250)
            }
        } finally {
            TransferLockManager.releaseLocks()
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp)
                    ) {
                        Surface(
                            shape = CircleShape,
                            color = MaterialTheme.colorScheme.primary,
                            modifier = Modifier.size(32.dp)
                        ) {
                            Box(contentAlignment = Alignment.Center) {
                                Icon(
                                    imageVector = Icons.Default.Bolt,
                                    contentDescription = null,
                                    tint = MaterialTheme.colorScheme.onPrimary,
                                    modifier = Modifier.size(20.dp)
                                )
                            }
                        }
                        Column {
                            Text(
                                "TurboTransfer",
                                fontWeight = FontWeight.Bold,
                                fontSize = 18.sp
                            )
                            Text(
                                "High-Speed Dual-Channel Link",
                                fontSize = 11.sp,
                                color = MaterialTheme.colorScheme.onSurfaceVariant
                            )
                        }
                    }
                },
                actions = {
                    if (activeTransferId != null && currentProgress != null && currentProgress?.status == FfiTransferStatus.IN_PROGRESS) {
                        val mbps = currentProgress!!.aggregateThroughputBps / (1024.0 * 1024.0)
                        Surface(
                            color = Color(0xFF2E7D32).copy(alpha = 0.15f),
                            shape = RoundedCornerShape(16.dp),
                            border = BorderStroke(1.dp, Color(0xFF2E7D32)),
                            modifier = Modifier
                                .padding(end = 12.dp)
                                .clickable { selectedTab = 2 }
                        ) {
                            Row(
                                modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp),
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(4.dp)
                            ) {
                                Box(
                                    modifier = Modifier
                                        .size(8.dp)
                                        .background(Color(0xFF2E7D32), CircleShape)
                                )
                                Text(
                                    String.format("%.1f MB/s", mbps),
                                    fontSize = 12.sp,
                                    fontWeight = FontWeight.Bold,
                                    color = Color(0xFF2E7D32)
                                )
                            }
                        }
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.surface,
                    titleContentColor = MaterialTheme.colorScheme.onSurface
                )
            )
        },
        bottomBar = {
            NavigationBar(
                containerColor = MaterialTheme.colorScheme.surface,
                tonalElevation = 8.dp
            ) {
                NavigationBarItem(
                    selected = selectedTab == 0,
                    onClick = { selectedTab = 0 },
                    icon = { Icon(Icons.Default.Send, contentDescription = "Send") },
                    label = { Text("Send") }
                )
                NavigationBarItem(
                    selected = selectedTab == 1,
                    onClick = { selectedTab = 1 },
                    icon = { Icon(Icons.Default.Download, contentDescription = "Receive") },
                    label = { Text("Receive") }
                )
                NavigationBarItem(
                    selected = selectedTab == 2,
                    onClick = { selectedTab = 2 },
                    icon = {
                        BadgedBox(
                            badge = {
                                if (activeTransferId != null && currentProgress?.status == FfiTransferStatus.IN_PROGRESS) {
                                    Badge { Text("●") }
                                }
                            }
                        ) {
                            Icon(Icons.Default.Speed, contentDescription = "Transfer")
                        }
                    },
                    label = { Text("Transfer") }
                )
                NavigationBarItem(
                    selected = selectedTab == 3,
                    onClick = { selectedTab = 3 },
                    icon = { Icon(Icons.Default.History, contentDescription = "History") },
                    label = { Text("History") }
                )
                NavigationBarItem(
                    selected = selectedTab == 4,
                    onClick = { selectedTab = 4 },
                    icon = { Icon(Icons.Default.Settings, contentDescription = "Settings") },
                    label = { Text("Settings") }
                )
            }
        }
    ) { innerPadding ->
        Box(
            modifier = Modifier
                .fillMaxSize()
                .padding(innerPadding)
        ) {
            when (selectedTab) {
                0 -> SendScreen(
                    hotspotManager = hotspotManager,
                    hotspotState = hotspotState,
                    transferQueue = transferQueue,
                    onStartBatchTransfer = { address ->
                        targetAddressForQueue = address
                        currentQueueIndex = 0
                        isQueueRunning = true
                        processNextInQueue()
                    }
                )
                1 -> ReceiveScreen(
                    isListening = isListening,
                    statusText = receiveStatus,
                    hotspotManager = hotspotManager,
                    hotspotState = hotspotState,
                    onToggleReceive = { destDir, address ->
                        scope.launch(Dispatchers.IO) {
                            try {
                                if (!isListening) {
                                    TransferLockManager.acquireLocks(context)
                                    val statusMsg = try {
                                        enterReceiveMode(
                                            address = address.ifBlank { null },
                                            destDir = destDir
                                        )
                                    } catch (e: Exception) {
                                        if (e.message?.contains("already active") == true) {
                                            "Listening on ${address.ifBlank { "0.0.0.0:9876" }}"
                                        } else {
                                            throw e
                                        }
                                    }
                                    withContext(Dispatchers.Main) {
                                        MainActivity.receiveStatusFlow.value = statusMsg
                                        MainActivity.isListeningFlow.value = true
                                    }
                                } else {
                                    stopReceiveMode()
                                    TransferLockManager.releaseLocks()
                                    withContext(Dispatchers.Main) {
                                        MainActivity.receiveStatusFlow.value = "Receive listener stopped"
                                        MainActivity.isListeningFlow.value = false
                                    }
                                }
                            } catch (e: Exception) {
                                withContext(Dispatchers.Main) {
                                    MainActivity.receiveStatusFlow.value = "Error: ${e.message}"
                                }
                            }
                        }
                    }
                )
                2 -> TransferScreen(
                    activeSession = activeSession,
                    progress = currentProgress,
                    transferQueue = transferQueue,
                    currentQueueIndex = currentQueueIndex,
                    lastCompletedItem = lastCompletedItem,
                    onPause = { id ->
                        scope.launch(Dispatchers.IO) {
                            try {
                                pauseTransfer(id)
                                withContext(Dispatchers.Main) {
                                    Toast.makeText(context, "Paused", Toast.LENGTH_SHORT).show()
                                }
                            } catch (e: Exception) {
                                withContext(Dispatchers.Main) {
                                    Toast.makeText(context, "Error: ${e.message}", Toast.LENGTH_SHORT).show()
                                }
                            }
                        }
                    },
                    onResume = { id ->
                        scope.launch(Dispatchers.IO) {
                            try {
                                resumeTransfer(id, FfiTransportPreference.AUTOMATIC)
                                withContext(Dispatchers.Main) {
                                    Toast.makeText(context, "Resumed", Toast.LENGTH_SHORT).show()
                                }
                            } catch (e: Exception) {
                                withContext(Dispatchers.Main) {
                                    Toast.makeText(context, "Error: ${e.message}", Toast.LENGTH_SHORT).show()
                                }
                            }
                        }
                    },
                    onCancel = { id ->
                        scope.launch(Dispatchers.IO) {
                            try {
                                cancelTransfer(id)
                                withContext(Dispatchers.Main) {
                                    MainActivity.activeTransferIdFlow.value = null
                                    MainActivity.activeSessionFlow.value = null
                                    currentProgress = null
                                    isQueueRunning = false
                                    Toast.makeText(context, "Cancelled", Toast.LENGTH_SHORT).show()
                                }
                            } catch (e: Exception) {
                                withContext(Dispatchers.Main) {
                                    Toast.makeText(context, "Error: ${e.message}", Toast.LENGTH_SHORT).show()
                                }
                            }
                        }
                    },
                    onDismissCompleted = {
                        MainActivity.lastCompletedItemFlow.value = null
                    }
                )
                3 -> HistoryScreen()
                4 -> SettingsScreen(
                    spikeManager = spikeManager,
                    hotspotManager = hotspotManager,
                    hotspotState = hotspotState
                )
            }
        }
    }
}

// -------------------------------------------------------------------------------------------------
// TAB 0: SEND SCREEN (Categories, Folder & Multi-File Cart, Radar & Receiver Discovery)
// -------------------------------------------------------------------------------------------------

@Composable
fun SendScreen(
    hotspotManager: WifiHotspotManager,
    hotspotState: WifiHotspotState,
    transferQueue: MutableList<SelectedFileInfo>,
    onStartBatchTransfer: (address: String) -> Unit
) {
    var customAddress by remember { mutableStateOf("127.0.0.1:9876") }
    var showCustomAddressField by remember { mutableStateOf(false) }
    var showQrDialog by remember { mutableStateOf(false) }
    var discoveredReceiver by remember { mutableStateOf<DiscoveredReceiver?>(null) }

    val context = LocalContext.current

    // Media & File Launchers
    val singleDocLauncher = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        uri?.let {
            UriUtils.resolveSelectedFile(context, it)?.let { info ->
                if (transferQueue.none { existing -> existing.path == info.path }) {
                    transferQueue.add(info)
                }
            }
        }
    }

    val multiDocLauncher = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        val items = UriUtils.resolveSelectedUris(context, uris)
        for (item in items) {
            if (transferQueue.none { existing -> existing.path == item.path }) {
                transferQueue.add(item)
            }
        }
    }

    val folderLauncher = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { treeUri ->
        treeUri?.let {
            val items = UriUtils.resolveDirectoryUri(context, it)
            if (items.isNotEmpty()) {
                for (item in items) {
                    if (transferQueue.none { existing -> existing.path == item.path }) {
                        transferQueue.add(item)
                    }
                }
                Toast.makeText(context, "Added ${items.size} files from folder", Toast.LENGTH_SHORT).show()
            } else {
                Toast.makeText(context, "No readable files found in folder", Toast.LENGTH_SHORT).show()
            }
        }
    }

    // Receiver Discovery Probe (polls every 1.5s)
    LaunchedEffect(customAddress, hotspotState.isActive) {
        while (true) {
            val receiver = withContext(Dispatchers.IO) {
                var usbFound = false
                var wifiFound = false
                var wifiAddr = ""

                // 1. Probe USB ADB Tunnel (127.0.0.1:9876)
                try {
                    java.net.Socket().use { socket ->
                        socket.connect(java.net.InetSocketAddress("127.0.0.1", 9876), 250)
                        usbFound = true
                    }
                } catch (_: Exception) {}

                // 2. Probe candidate IPs & ARP table
                val candidateIps = mutableSetOf(
                    "10.18.163.1",
                    "10.18.163.2",
                    "192.168.43.1",
                    "192.168.43.2",
                    "192.168.1.19",
                    "10.78.112.46"
                )

                try {
                    val arpLines = java.io.File("/proc/net/arp").readLines()
                    for (line in arpLines.drop(1)) {
                        val tokens = line.trim().split(Regex("\\s+"))
                        if (tokens.isNotEmpty()) {
                            val ip = tokens[0]
                            if (ip.matches(Regex("\\d+\\.\\d+\\.\\d+\\.\\d+"))) {
                                candidateIps.add(ip)
                            }
                        }
                    }
                } catch (_: Exception) {}

                val probeDeferreds = candidateIps.map { targetIp ->
                    async(Dispatchers.IO) {
                        try {
                            java.net.Socket().use { socket ->
                                socket.connect(java.net.InetSocketAddress(targetIp, 9876), 200)
                                targetIp
                            }
                        } catch (_: Exception) {
                            null
                        }
                    }
                }
                val successfulProbe = probeDeferreds.awaitAll().filterNotNull().firstOrNull()
                if (successfulProbe != null) {
                    wifiFound = true
                    wifiAddr = "$successfulProbe:9876"
                }

                val usbLabel = UsbHardwareHelper.getUsbSpeedLabel(context)

                if (usbFound && wifiFound) {
                    DiscoveredReceiver(
                        address = "127.0.0.1:9876,$wifiAddr",
                        displayName = "Windows PC / Desktop",
                        transport = "$usbLabel + 5 GHz Wi-Fi (Multipath Active)",
                        isReady = true,
                        isUsbAvailable = true,
                        isWifiAvailable = true
                    )
                } else if (usbFound) {
                    DiscoveredReceiver(
                        address = "127.0.0.1:9876",
                        displayName = "Windows PC / Desktop",
                        transport = "$usbLabel (ADB Tunnel)",
                        isReady = true,
                        isUsbAvailable = true,
                        isWifiAvailable = false
                    )
                } else if (wifiFound) {
                    DiscoveredReceiver(
                        address = wifiAddr,
                        displayName = "Windows PC / Desktop",
                        transport = "5 GHz Wi-Fi Direct / LAN",
                        isReady = true,
                        isUsbAvailable = false,
                        isWifiAvailable = true
                    )
                } else {
                    null
                }
            }

            discoveredReceiver = receiver
            if (receiver != null && (customAddress == "127.0.0.1:9876" || customAddress.isEmpty() || customAddress.contains("127.0.0.1"))) {
                customAddress = receiver.address
            }
            delay(1500)
        }
    }

    LazyColumn(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
        contentPadding = PaddingValues(vertical = 16.dp)
    ) {
        // 1. Quick Media Categories Header
        item {
            Text(
                "Select Content to Send",
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Bold
            )
        }

        item {
            LazyRow(
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                contentPadding = PaddingValues(horizontal = 2.dp)
            ) {
                item {
                    CategoryChip(
                        icon = Icons.Default.Image,
                        label = "Photos",
                        onClick = { multiDocLauncher.launch(arrayOf("image/*")) }
                    )
                }
                item {
                    CategoryChip(
                        icon = Icons.Default.Videocam,
                        label = "Videos",
                        onClick = { multiDocLauncher.launch(arrayOf("video/*")) }
                    )
                }
                item {
                    CategoryChip(
                        icon = Icons.Default.Audiotrack,
                        label = "Audio",
                        onClick = { multiDocLauncher.launch(arrayOf("audio/*")) }
                    )
                }
                item {
                    CategoryChip(
                        icon = Icons.Default.InsertDriveFile,
                        label = "Files",
                        onClick = { multiDocLauncher.launch(arrayOf("*/*")) }
                    )
                }
                item {
                    CategoryChip(
                        icon = Icons.Default.Android,
                        label = "Apps / APKs",
                        onClick = { multiDocLauncher.launch(arrayOf("application/vnd.android.package-archive", "*/*")) }
                    )
                }
                item {
                    CategoryChip(
                        icon = Icons.Default.Folder,
                        label = "Folder",
                        onClick = { folderLauncher.launch(null) }
                    )
                }
            }
        }

        // 2. Selection Cart Card
        if (transferQueue.isNotEmpty()) {
            val totalBytes = transferQueue.sumOf { it.sizeBytes }
            item {
                Card(
                    modifier = Modifier.fillMaxWidth(),
                    colors = CardDefaults.cardColors(
                        containerColor = MaterialTheme.colorScheme.primaryContainer.copy(alpha = 0.5f)
                    ),
                    shape = RoundedCornerShape(16.dp),
                    border = BorderStroke(1.dp, MaterialTheme.colorScheme.primary.copy(alpha = 0.3f))
                ) {
                    Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Text(
                                "${transferQueue.size} items selected (${UriUtils.formatFileSize(totalBytes)})",
                                fontWeight = FontWeight.Bold,
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onPrimaryContainer
                            )
                            TextButton(
                                onClick = { transferQueue.clear() },
                                contentPadding = PaddingValues(horizontal = 8.dp)
                            ) {
                                Text("Clear All", color = MaterialTheme.colorScheme.error, fontSize = 13.sp)
                            }
                        }

                        LazyRow(
                            horizontalArrangement = Arrangement.spacedBy(8.dp)
                        ) {
                            items(transferQueue) { fileInfo ->
                                SelectedFileItemChip(
                                    fileInfo = fileInfo,
                                    onRemove = { transferQueue.remove(fileInfo) }
                                )
                            }
                        }
                    }
                }
            }
        } else {
            item {
                Card(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable { multiDocLauncher.launch(arrayOf("*/*")) },
                    shape = RoundedCornerShape(16.dp),
                    colors = CardDefaults.cardColors(
                        containerColor = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.4f)
                    ),
                    border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant)
                ) {
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(24.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(8.dp)
                    ) {
                        Surface(
                            shape = CircleShape,
                            color = MaterialTheme.colorScheme.primary.copy(alpha = 0.1f),
                            modifier = Modifier.size(56.dp)
                        ) {
                            Box(contentAlignment = Alignment.Center) {
                                Icon(
                                    Icons.Default.CloudUpload,
                                    contentDescription = null,
                                    tint = MaterialTheme.colorScheme.primary,
                                    modifier = Modifier.size(32.dp)
                                )
                            }
                        }
                        Text(
                            "Tap to Select Files or Folders",
                            fontWeight = FontWeight.Bold,
                            style = MaterialTheme.typography.titleMedium,
                            color = MaterialTheme.colorScheme.onSurface
                        )
                        Text(
                            "Select multiple photos, 4K videos, documents, or entire directories",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            textAlign = TextAlign.Center
                        )
                    }
                }
            }
        }

        // 3. 5 GHz Wi-Fi Hotspot Quick Link Card
        item {
            Card(
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(16.dp),
                colors = CardDefaults.cardColors(
                    containerColor = if (hotspotState.isActive) Color(0xFF1B382B) else MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.4f)
                ),
                border = BorderStroke(
                    1.dp,
                    if (hotspotState.isActive) Color(0xFF4CAF50) else MaterialTheme.colorScheme.outlineVariant
                )
            ) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(16.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.SpaceBetween
                ) {
                    Column(modifier = Modifier.weight(1f)) {
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(6.dp)
                        ) {
                            Icon(
                                Icons.Default.WifiTethering,
                                contentDescription = null,
                                tint = if (hotspotState.isActive) Color(0xFF81C784) else MaterialTheme.colorScheme.primary,
                                modifier = Modifier.size(20.dp)
                            )
                            Text(
                                if (hotspotState.isActive) "5 GHz Hotspot (Active)" else "5 GHz Direct Hotspot",
                                fontWeight = FontWeight.Bold,
                                style = MaterialTheme.typography.bodyLarge,
                                color = if (hotspotState.isActive) Color(0xFF81C784) else MaterialTheme.colorScheme.onSurface
                            )
                        }
                        if (hotspotState.isActive && hotspotState.hotspotInfo != null) {
                            Text(
                                "SSID: ${hotspotState.hotspotInfo?.ssid}",
                                style = MaterialTheme.typography.bodySmall,
                                fontFamily = FontFamily.Monospace,
                                color = Color.White.copy(alpha = 0.9f)
                            )
                        } else {
                            Text(
                                "Direct device-to-device connection without Wi-Fi router",
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant
                            )
                        }
                    }

                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        if (hotspotState.isActive) {
                            IconButton(onClick = { showQrDialog = true }) {
                                Icon(Icons.Default.QrCode, contentDescription = "QR Code", tint = Color.White)
                            }
                        }
                        Button(
                            onClick = {
                                if (!hotspotState.isActive) {
                                    hotspotManager.startHotspot { ok, msg ->
                                        Toast.makeText(context, msg, Toast.LENGTH_SHORT).show()
                                    }
                                } else {
                                    hotspotManager.stopHotspot()
                                }
                            },
                            colors = ButtonDefaults.buttonColors(
                                containerColor = if (hotspotState.isActive) Color(0xFFD32F2F) else MaterialTheme.colorScheme.primary
                            ),
                            shape = RoundedCornerShape(10.dp)
                        ) {
                            Text(if (hotspotState.isActive) "Stop" else "Start 5GHz", fontSize = 13.sp)
                        }
                    }
                }
            }
        }

        // 4. Target Receiver Discovery Section
        item {
            Text(
                "Nearby Receiver",
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Bold
            )
        }

        item {
            if (discoveredReceiver != null) {
                Card(
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(16.dp),
                    colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
                    border = BorderStroke(1.dp, Color(0xFF4CAF50)),
                    elevation = CardDefaults.cardElevation(defaultElevation = 2.dp)
                ) {
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(16.dp),
                        verticalArrangement = Arrangement.spacedBy(14.dp)
                    ) {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.SpaceBetween
                        ) {
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(12.dp)
                            ) {
                                Surface(
                                    shape = CircleShape,
                                    color = Color(0xFF2E7D32).copy(alpha = 0.12f),
                                    modifier = Modifier.size(44.dp)
                                ) {
                                    Box(contentAlignment = Alignment.Center) {
                                        Icon(
                                            Icons.Default.Computer,
                                            contentDescription = null,
                                            tint = Color(0xFF2E7D32),
                                            modifier = Modifier.size(26.dp)
                                        )
                                    }
                                }
                                Column {
                                    Text(
                                        discoveredReceiver!!.displayName,
                                        fontWeight = FontWeight.Bold,
                                        style = MaterialTheme.typography.bodyLarge
                                    )
                                    Text(
                                        discoveredReceiver!!.transport,
                                        style = MaterialTheme.typography.bodySmall,
                                        color = MaterialTheme.colorScheme.primary,
                                        fontWeight = FontWeight.Medium
                                    )
                                }
                            }
                            Surface(
                                color = Color(0xFF2E7D32),
                                shape = RoundedCornerShape(12.dp)
                            ) {
                                Text(
                                    "READY",
                                    modifier = Modifier.padding(horizontal = 8.dp, vertical = 3.dp),
                                    color = Color.White,
                                    fontSize = 10.sp,
                                    fontWeight = FontWeight.Bold
                                )
                            }
                        }

                        Button(
                            onClick = {
                                if (transferQueue.isEmpty()) {
                                    Toast.makeText(context, "Please select at least one file to send", Toast.LENGTH_SHORT).show()
                                } else {
                                    onStartBatchTransfer(discoveredReceiver!!.address)
                                }
                            },
                            modifier = Modifier
                                .fillMaxWidth()
                                .height(52.dp),
                            enabled = transferQueue.isNotEmpty(),
                            shape = RoundedCornerShape(12.dp)
                        ) {
                            Icon(Icons.Default.Send, contentDescription = null, modifier = Modifier.size(18.dp))
                            Spacer(modifier = Modifier.width(8.dp))
                            Text(
                                if (transferQueue.isNotEmpty()) {
                                    val totalBytes = transferQueue.sumOf { it.sizeBytes }
                                    "Send ${transferQueue.size} item(s) (${UriUtils.formatFileSize(totalBytes)})"
                                } else {
                                    "Select Files to Send"
                                },
                                fontSize = 15.sp,
                                fontWeight = FontWeight.Bold
                            )
                        }
                    }
                }
            } else {
                Card(
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(16.dp),
                    colors = CardDefaults.cardColors(
                        containerColor = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.3f)
                    ),
                    border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant)
                ) {
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(20.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(10.dp)
                    ) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(36.dp),
                            strokeWidth = 3.dp,
                            color = MaterialTheme.colorScheme.primary
                        )
                        Text(
                            "Scanning for PC Receiver...",
                            fontWeight = FontWeight.SemiBold,
                            style = MaterialTheme.typography.bodyMedium
                        )
                        Text(
                            "Connect USB cable or run receive mode on PC in the same Wi-Fi hotspot",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            textAlign = TextAlign.Center
                        )
                    }
                }
            }
        }

        // 5. Custom Target Address Accordion
        item {
            TextButton(
                onClick = { showCustomAddressField = !showCustomAddressField },
                modifier = Modifier.fillMaxWidth()
            ) {
                Text(
                    if (showCustomAddressField) "Hide Custom Address ▲" else "Custom Target Address ▼",
                    fontSize = 13.sp
                )
            }

            if (showCustomAddressField) {
                OutlinedTextField(
                    value = customAddress,
                    onValueChange = { customAddress = it },
                    label = { Text("Target Address (IP:Port or Loopback)") },
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(
                        keyboardType = KeyboardType.Ascii,
                        autoCorrect = false,
                        capitalization = KeyboardCapitalization.None
                    ),
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(12.dp)
                )
            }
        }
    }

    if (showQrDialog && hotspotState.hotspotInfo != null) {
        AlertDialog(
            onDismissRequest = { showQrDialog = false },
            title = { Text("5 GHz Hotspot Pairing") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text("SSID: ${hotspotState.hotspotInfo?.ssid}", fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace)
                    Text("Password: ${hotspotState.hotspotInfo?.passphrase}", fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace)
                    Text("Port: 9876", fontFamily = FontFamily.Monospace)
                    Text("Connect your PC or secondary phone to this Wi-Fi network.", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            },
            confirmButton = {
                TextButton(onClick = { showQrDialog = false }) {
                    Text("Close")
                }
            }
        )
    }
}

@Composable
fun CategoryChip(icon: ImageVector, label: String, onClick: () -> Unit) {
    Surface(
        onClick = onClick,
        shape = RoundedCornerShape(12.dp),
        color = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.6f),
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant)
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 14.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp)
        ) {
            Icon(icon, contentDescription = null, modifier = Modifier.size(18.dp), tint = MaterialTheme.colorScheme.primary)
            Text(label, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
        }
    }
}

@Composable
fun SelectedFileItemChip(fileInfo: SelectedFileInfo, onRemove: () -> Unit) {
    Surface(
        shape = RoundedCornerShape(10.dp),
        color = MaterialTheme.colorScheme.surface,
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant)
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp)
        ) {
            val icon = when (fileInfo.category) {
                FileCategory.IMAGE -> Icons.Default.Image
                FileCategory.VIDEO -> Icons.Default.Videocam
                FileCategory.AUDIO -> Icons.Default.Audiotrack
                FileCategory.DOCUMENT -> Icons.Default.InsertDriveFile
                FileCategory.APK -> Icons.Default.Android
                FileCategory.FOLDER -> Icons.Default.Folder
                else -> Icons.Default.Description
            }
            Icon(icon, contentDescription = null, modifier = Modifier.size(16.dp), tint = MaterialTheme.colorScheme.primary)
            Column {
                Text(
                    fileInfo.displayName,
                    fontSize = 12.sp,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.widthIn(max = 120.dp)
                )
                Text(fileInfo.formattedSize, fontSize = 10.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            IconButton(onClick = onRemove, modifier = Modifier.size(18.dp)) {
                Icon(Icons.Default.Close, contentDescription = "Remove", modifier = Modifier.size(14.dp))
            }
        }
    }
}

// -------------------------------------------------------------------------------------------------
// TAB 1: RECEIVE SCREEN (Radar Hub, Device Identity & Hotspot QR, Storage Folder Chip)
// -------------------------------------------------------------------------------------------------

@Composable
fun ReceiveScreen(
    isListening: Boolean,
    statusText: String,
    hotspotManager: WifiHotspotManager,
    hotspotState: WifiHotspotState,
    onToggleReceive: (destDir: String, address: String) -> Unit
) {
    val destDirFromFlow by MainActivity.receiveDestDirFlow.collectAsState()
    var address by remember { mutableStateOf("0.0.0.0:9876") }
    var showQrDialog by remember { mutableStateOf(false) }
    var usbAvailable by remember { mutableStateOf(false) }
    var detectedIps by remember { mutableStateOf<List<String>>(emptyList()) }
    val context = LocalContext.current

    val activeSession by MainActivity.activeSessionFlow.collectAsState()
    val activeTransferId by MainActivity.activeTransferIdFlow.collectAsState()

    val folderPicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
        uri?.let {
            val docId = android.provider.DocumentsContract.getTreeDocumentId(it)
            val path = if (docId.startsWith("primary:")) {
                "/sdcard/" + docId.substringAfter("primary:")
            } else {
                Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS).absolutePath
            }
            MainActivity.receiveDestDirFlow.value = path
        }
    }

    // Network & USB Probe
    LaunchedEffect(Unit) {
        while (true) {
            val (usbOk, ips) = withContext(Dispatchers.IO) {
                var usb = false
                try {
                    java.net.Socket().use { socket ->
                        socket.connect(java.net.InetSocketAddress("127.0.0.1", 9876), 150)
                        usb = true
                    }
                } catch (_: Exception) {}

                val foundIps = mutableListOf<String>()
                try {
                    val interfaces = java.net.NetworkInterface.getNetworkInterfaces()
                    while (interfaces.hasMoreElements()) {
                        val iface = interfaces.nextElement()
                        if (iface.isLoopback || !iface.isUp) continue
                        val addresses = iface.inetAddresses
                        while (addresses.hasMoreElements()) {
                            val addr = addresses.nextElement()
                            if (addr is java.net.Inet4Address && !addr.isLoopbackAddress) {
                                val host = addr.hostAddress
                                if (!host.isNullOrBlank() && !foundIps.contains(host)) {
                                    foundIps.add(host)
                                }
                            }
                        }
                    }
                } catch (_: Exception) {}

                Pair(usb, foundIps)
            }
            usbAvailable = usbOk
            if (usbOk && !MainActivity.isListeningFlow.value) {
                MainActivity.isListeningFlow.value = true
                MainActivity.receiveStatusFlow.value = "Listening on 0.0.0.0:9876"
            }
            detectedIps = ips
            delay(1500)
        }
    }

    // Pulsing radar animation
    val infiniteTransition = rememberInfiniteTransition(label = "pulse")
    val pulseScale by infiniteTransition.animateFloat(
        initialValue = 1f,
        targetValue = 1.35f,
        animationSpec = infiniteRepeatable(
            animation = tween(1500, easing = FastOutSlowInEasing),
            repeatMode = RepeatMode.Reverse
        ),
        label = "pulseScale"
    )

    val isDualChannelReady = usbAvailable && (hotspotState.isActive || detectedIps.isNotEmpty())
    val usbLabel = UsbHardwareHelper.getUsbSpeedLabel(context)
    val primaryIp = if (hotspotState.isActive) "192.168.43.1" else detectedIps.firstOrNull() ?: "127.0.0.1"

    LazyColumn(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(16.dp),
        contentPadding = PaddingValues(vertical = 16.dp)
    ) {
        item {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Text(
                    "Receive Files",
                    style = MaterialTheme.typography.titleLarge,
                    fontWeight = FontWeight.Bold
                )
                if (hotspotState.isActive && hotspotState.hotspotInfo != null) {
                    IconButton(onClick = { showQrDialog = true }) {
                        Icon(Icons.Default.QrCode, contentDescription = "Show Hotspot QR", tint = MaterialTheme.colorScheme.primary)
                    }
                }
            }
        }

        // 1. Animated Radar Graphic Hub
        item {
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(180.dp),
                contentAlignment = Alignment.Center
            ) {
                if (isListening) {
                    Box(
                        modifier = Modifier
                            .size((130 * pulseScale).dp)
                            .background(MaterialTheme.colorScheme.primary.copy(alpha = 0.12f), CircleShape)
                    )
                    Box(
                        modifier = Modifier
                            .size((90 * pulseScale).dp)
                            .background(MaterialTheme.colorScheme.primary.copy(alpha = 0.20f), CircleShape)
                    )
                }

                Surface(
                    shape = CircleShape,
                    color = if (isListening) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.surfaceVariant,
                    modifier = Modifier.size(80.dp),
                    shadowElevation = 6.dp,
                    tonalElevation = 6.dp
                ) {
                    Box(contentAlignment = Alignment.Center) {
                        Icon(
                            imageVector = if (isListening) Icons.Default.DownloadDone else Icons.Default.Download,
                            contentDescription = null,
                            tint = if (isListening) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.size(40.dp)
                        )
                    }
                }
            }
        }

        // 2. Dual-Channel Readiness Badge & Status Card
        item {
            Card(
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(16.dp),
                colors = CardDefaults.cardColors(
                    containerColor = if (isListening) Color(0xFFE8F5E9) else MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.4f)
                ),
                border = BorderStroke(1.dp, if (isListening) Color(0xFF4CAF50) else MaterialTheme.colorScheme.outlineVariant)
            ) {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(10.dp)
                ) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            Box(
                                modifier = Modifier
                                    .size(10.dp)
                                    .background(if (isListening) Color(0xFF4CAF50) else Color.Gray, CircleShape)
                            )
                            Text(
                                text = if (isListening) "Ready to Receive" else "Receiver Inactive",
                                fontWeight = FontWeight.Bold,
                                color = if (isListening) Color(0xFF2E7D32) else MaterialTheme.colorScheme.onSurfaceVariant
                            )
                        }
                        Text("Port 9876", fontFamily = FontFamily.Monospace, fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }

                    // Multi-Channel Badge
                    Surface(
                        shape = RoundedCornerShape(8.dp),
                        color = when {
                            isDualChannelReady -> Color(0xFF1B5E20)
                            hotspotState.isActive || detectedIps.isNotEmpty() -> Color(0xFF0D47A1)
                            usbAvailable -> Color(0xFFE65100)
                            else -> MaterialTheme.colorScheme.surface
                        },
                        border = BorderStroke(1.dp, Color.White.copy(alpha = 0.2f))
                    ) {
                        Row(
                            modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(6.dp)
                        ) {
                            Icon(
                                imageVector = if (isDualChannelReady) Icons.Default.Bolt else Icons.Default.Sensors,
                                contentDescription = null,
                                tint = Color.White,
                                modifier = Modifier.size(16.dp)
                            )
                            Text(
                                text = when {
                                    isDualChannelReady -> "⚡ Dual-Channel Multipath Ready ($usbLabel + 5 GHz Wi-Fi)"
                                    hotspotState.isActive -> "📡 5 GHz Local Hotspot Active"
                                    detectedIps.isNotEmpty() -> "📡 Wi-Fi Direct / LAN Ready"
                                    usbAvailable -> "🔌 $usbLabel (ADB Tunnel) Ready"
                                    else -> "Waiting for USB or Wi-Fi link..."
                                },
                                fontSize = 12.sp,
                                fontWeight = FontWeight.Bold,
                                color = Color.White
                            )
                        }
                    }

                    Text(
                        text = if (isListening) "Listening on 0.0.0.0:9876. Senders can transmit over USB, 5 GHz Wi-Fi, or both simultaneously." else "Tap 'Start Receive Mode' below to accept incoming files.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }
            }
        }

        // 3. 5 GHz Hotspot Card with Direct Toggle & QR Button
        item {
            Card(
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(16.dp),
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
                border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant)
            ) {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(10.dp)
                ) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            Icon(
                                Icons.Default.WifiTethering,
                                contentDescription = null,
                                tint = if (hotspotState.isActive) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant
                            )
                            Column {
                                Text("5 GHz Wi-Fi Hotspot", style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.Bold)
                                Text(
                                    if (hotspotState.isActive) "Active (${hotspotState.hotspotInfo?.band ?: "5 GHz"})" else "Disabled",
                                    style = MaterialTheme.typography.bodySmall,
                                    color = if (hotspotState.isActive) Color(0xFF2E7D32) else MaterialTheme.colorScheme.onSurfaceVariant
                                )
                            }
                        }
                        Switch(
                            checked = hotspotState.isActive,
                            onCheckedChange = { enable ->
                                if (enable) {
                                    hotspotManager.startHotspot { success, msg ->
                                        Toast.makeText(context, msg, Toast.LENGTH_SHORT).show()
                                    }
                                } else {
                                    hotspotManager.stopHotspot()
                                }
                            }
                        )
                    }

                    if (hotspotState.isActive && hotspotState.hotspotInfo != null) {
                        val info = hotspotState.hotspotInfo!!
                        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.5f))
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Column {
                                Text("SSID: ${info.ssid}", fontFamily = FontFamily.Monospace, fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
                                Text("Pass: ${info.passphrase}", fontFamily = FontFamily.Monospace, fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            }
                            OutlinedButton(
                                onClick = { showQrDialog = true },
                                shape = RoundedCornerShape(8.dp),
                                contentPadding = PaddingValues(horizontal = 10.dp, vertical = 4.dp)
                            ) {
                                Icon(Icons.Default.QrCode, contentDescription = null, modifier = Modifier.size(16.dp))
                                Spacer(modifier = Modifier.width(4.dp))
                                Text("QR Code", fontSize = 11.sp)
                            }
                        }
                    }
                }
            }
        }

        // 4. Quick PC CLI Helper & IP Address Card
        item {
            Card(
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(16.dp),
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.3f)),
                border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant)
            ) {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(14.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Text("Send from PC Command", style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.Bold)
                        val pcCmd = if (primaryIp.isNotBlank() && primaryIp != "127.0.0.1") {
                            "turbo send <file> --address 127.0.0.1:9876,$primaryIp:9876"
                        } else {
                            "turbo send <file>"
                        }
                        IconButton(
                            onClick = {
                                val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                                val clip = ClipData.newPlainText("TurboTransfer CLI", pcCmd)
                                clipboard.setPrimaryClip(clip)
                                Toast.makeText(context, "Command copied!", Toast.LENGTH_SHORT).show()
                            },
                            modifier = Modifier.size(28.dp)
                        ) {
                            Icon(Icons.Default.ContentCopy, contentDescription = "Copy command", modifier = Modifier.size(16.dp))
                        }
                    }

                    Text(
                        text = if (primaryIp.isNotBlank() && primaryIp != "127.0.0.1") {
                            "turbo send <file> --address 127.0.0.1:9876,$primaryIp:9876"
                        } else {
                            "turbo send <file>"
                        },
                        fontFamily = FontFamily.Monospace,
                        fontSize = 11.sp,
                        color = MaterialTheme.colorScheme.primary,
                        modifier = Modifier
                            .background(MaterialTheme.colorScheme.surface, RoundedCornerShape(8.dp))
                            .padding(8.dp)
                            .fillMaxWidth()
                    )
                }
            }
        }

        // 5. Storage Destination Folder Card
        item {
            Card(
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(16.dp),
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
                border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant)
            ) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(16.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.SpaceBetween
                ) {
                    Column(modifier = Modifier.weight(1f)) {
                        Text("Save Location", style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.Bold)
                        Text(
                            destDirFromFlow,
                            style = MaterialTheme.typography.bodySmall,
                            fontFamily = FontFamily.Monospace,
                            color = MaterialTheme.colorScheme.primary,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis
                        )
                    }
                    OutlinedButton(
                        onClick = { folderPicker.launch(null) },
                        shape = RoundedCornerShape(8.dp)
                    ) {
                        Text("Change", fontSize = 12.sp)
                    }
                }
            }
        }

        // 6. Active Transfer Preview (if transfer is running)
        if (activeTransferId != null && activeSession?.isOutgoing == false) {
            item {
                Card(
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(16.dp),
                    colors = CardDefaults.cardColors(containerColor = Color(0xFF1B382B)),
                    border = BorderStroke(1.dp, Color(0xFF4CAF50))
                ) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(14.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.SpaceBetween
                    ) {
                        Column(modifier = Modifier.weight(1f)) {
                            Text("📥 Receiving File...", fontWeight = FontWeight.Bold, color = Color(0xFF81C784), fontSize = 13.sp)
                            Text(
                                activeSession?.fileName ?: "Incoming File",
                                fontWeight = FontWeight.Bold,
                                color = Color.White,
                                fontSize = 14.sp,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis
                            )
                        }
                        Button(
                            onClick = { MainActivity.selectedTabFlow.value = 2 },
                            colors = ButtonDefaults.buttonColors(containerColor = Color(0xFF2E7D32)),
                            shape = RoundedCornerShape(8.dp)
                        ) {
                            Text("View Monitor", fontSize = 12.sp)
                        }
                    }
                }
            }
        }

        // 7. Primary Action Button
        item {
            Button(
                onClick = { onToggleReceive(destDirFromFlow, address) },
                modifier = Modifier
                    .fillMaxWidth()
                    .height(54.dp),
                colors = ButtonDefaults.buttonColors(
                    containerColor = if (isListening) Color(0xFFD32F2F) else MaterialTheme.colorScheme.primary
                ),
                shape = RoundedCornerShape(14.dp)
            ) {
                Icon(
                    imageVector = if (isListening) Icons.Default.Stop else Icons.Default.PlayArrow,
                    contentDescription = null,
                    modifier = Modifier.size(20.dp)
                )
                Spacer(modifier = Modifier.width(8.dp))
                Text(
                    if (isListening) "Stop Listening" else "Start Receive Mode",
                    fontSize = 16.sp,
                    fontWeight = FontWeight.Bold
                )
            }
        }
    }

    if (showQrDialog && hotspotState.hotspotInfo != null) {
        val info = hotspotState.hotspotInfo!!
        AlertDialog(
            onDismissRequest = { showQrDialog = false },
            title = { Text("5 GHz Hotspot Pairing") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text("SSID: ${info.ssid}", fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace)
                    Text("Password: ${info.passphrase}", fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace)
                    Text("Band: ${info.band}", fontFamily = FontFamily.Monospace)
                    Text("Port: 9876", fontFamily = FontFamily.Monospace)
                    Text("Connect your PC or sending phone to this Wi-Fi network.", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            },
            confirmButton = {
                TextButton(onClick = { showQrDialog = false }) {
                    Text("Close")
                }
            }
        )
    }
}

// -------------------------------------------------------------------------------------------------
// TAB 2: TRANSFER STATUS & LIVE SPEEDOMETER DASHBOARD
// -------------------------------------------------------------------------------------------------

@Composable
fun TransferScreen(
    activeSession: ActiveTransferSessionState?,
    progress: FfiTransferProgress?,
    transferQueue: List<SelectedFileInfo>,
    currentQueueIndex: Int,
    lastCompletedItem: TransferHistoryItem?,
    onPause: (String) -> Unit,
    onResume: (String) -> Unit,
    onCancel: (String) -> Unit,
    onDismissCompleted: () -> Unit
) {
    val context = LocalContext.current

    LazyColumn(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
        contentPadding = PaddingValues(vertical = 16.dp)
    ) {
        item {
            Text(
                "Transfer Monitor",
                style = MaterialTheme.typography.titleLarge,
                fontWeight = FontWeight.Bold
            )
        }

        // 1. Post-Transfer Completion Summary Card
        if (lastCompletedItem != null && (progress == null || progress.status != FfiTransferStatus.IN_PROGRESS)) {
            val item = lastCompletedItem
            item {
                Card(
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(16.dp),
                    colors = CardDefaults.cardColors(containerColor = Color(0xFF1B382B)),
                    border = BorderStroke(1.dp, Color(0xFF4CAF50))
                ) {
                    Column(
                        modifier = Modifier.padding(18.dp),
                        verticalArrangement = Arrangement.spacedBy(12.dp)
                    ) {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                Icon(Icons.Default.CheckCircle, contentDescription = null, tint = Color(0xFF81C784))
                                Text("Transfer Completed!", fontWeight = FontWeight.Bold, color = Color(0xFF81C784), fontSize = 16.sp)
                            }
                            IconButton(onClick = onDismissCompleted, modifier = Modifier.size(24.dp)) {
                                Icon(Icons.Default.Close, contentDescription = "Close", tint = Color.White.copy(alpha = 0.7f))
                            }
                        }

                        Text(
                            item.fileName,
                            fontWeight = FontWeight.Bold,
                            fontSize = 17.sp,
                            color = Color.White
                        )

                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween
                        ) {
                            Text("Size: ${item.formattedSize}", fontSize = 13.sp, color = Color.White.copy(alpha = 0.8f))
                            Text("Time: ${String.format("%.1fs", item.durationMs / 1000.0)}", fontSize = 13.sp, color = Color.White.copy(alpha = 0.8f))
                        }

                        HorizontalDivider(color = Color.White.copy(alpha = 0.2f))

                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween
                        ) {
                            Column {
                                Text("Average Speed", fontSize = 11.sp, color = Color.White.copy(alpha = 0.6f))
                                Text(String.format("%.2f MB/s", item.avgSpeedMBps), fontWeight = FontWeight.Bold, color = Color(0xFF81C784), fontSize = 14.sp)
                            }
                            Column {
                                Text("Peak Speed", fontSize = 11.sp, color = Color.White.copy(alpha = 0.6f))
                                Text(String.format("%.2f MB/s", item.peakSpeedMBps), fontWeight = FontWeight.Bold, color = Color(0xFF81C784), fontSize = 14.sp)
                            }
                            Column {
                                Text("USB Peak", fontSize = 11.sp, color = Color.White.copy(alpha = 0.6f))
                                Text(String.format("%.2f MB/s", item.usbSpeedMBps), fontWeight = FontWeight.Bold, color = Color(0xFF64B5F6), fontSize = 14.sp)
                            }
                            Column {
                                Text("Wi-Fi Peak", fontSize = 11.sp, color = Color.White.copy(alpha = 0.6f))
                                Text(String.format("%.2f MB/s", item.wifiSpeedMBps), fontWeight = FontWeight.Bold, color = Color(0xFFBA68C8), fontSize = 14.sp)
                            }
                        }

                        if (!item.isOutgoing && item.filePath.isNotBlank()) {
                            Row(
                                modifier = Modifier.fillMaxWidth(),
                                horizontalArrangement = Arrangement.spacedBy(8.dp)
                            ) {
                                Button(
                                    onClick = { UriUtils.openFile(context, item.filePath) },
                                    modifier = Modifier.weight(1f),
                                    colors = ButtonDefaults.buttonColors(containerColor = Color(0xFF2E7D32)),
                                    shape = RoundedCornerShape(10.dp)
                                ) {
                                    Icon(Icons.Default.OpenInNew, contentDescription = null, modifier = Modifier.size(16.dp))
                                    Spacer(modifier = Modifier.width(6.dp))
                                    Text("Open File", fontSize = 13.sp)
                                }
                                OutlinedButton(
                                    onClick = { UriUtils.shareFile(context, item.filePath) },
                                    modifier = Modifier.weight(1f),
                                    colors = ButtonDefaults.outlinedButtonColors(contentColor = Color.White),
                                    border = BorderStroke(1.dp, Color.White.copy(alpha = 0.5f)),
                                    shape = RoundedCornerShape(10.dp)
                                ) {
                                    Icon(Icons.Default.Share, contentDescription = null, modifier = Modifier.size(16.dp))
                                    Spacer(modifier = Modifier.width(6.dp))
                                    Text("Share", fontSize = 13.sp)
                                }
                            }
                        }
                    }
                }
            }
        }

        // 2. Active Transfer Dashboard
        if (progress != null && (progress.status == FfiTransferStatus.IN_PROGRESS || progress.status == FfiTransferStatus.PAUSED)) {
            val p = progress
            val totalSpeedMBps = p.aggregateThroughputBps / (1024.0 * 1024.0)
            val usbSpeedMBps = p.usbThroughputBps / (1024.0 * 1024.0)
            val wifiSpeedMBps = p.wifiThroughputBps / (1024.0 * 1024.0)
            val isOutgoing = activeSession?.isOutgoing ?: true

            item {
                Card(
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(20.dp),
                    elevation = CardDefaults.cardElevation(defaultElevation = 4.dp),
                    colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface)
                ) {
                    Column(
                        modifier = Modifier.padding(20.dp),
                        verticalArrangement = Arrangement.spacedBy(14.dp)
                    ) {
                        // Direction Badge
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Surface(
                                color = if (isOutgoing) MaterialTheme.colorScheme.primaryContainer else Color(0xFFE8F5E9),
                                shape = RoundedCornerShape(12.dp)
                            ) {
                                Row(
                                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 5.dp),
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.spacedBy(6.dp)
                                ) {
                                    Icon(
                                        imageVector = if (isOutgoing) Icons.Default.ArrowUpward else Icons.Default.ArrowDownward,
                                        contentDescription = null,
                                        tint = if (isOutgoing) MaterialTheme.colorScheme.onPrimaryContainer else Color(0xFF2E7D32),
                                        modifier = Modifier.size(16.dp)
                                    )
                                    Text(
                                        if (isOutgoing) "SENDING TO PC" else "RECEIVING FROM PC",
                                        fontWeight = FontWeight.Bold,
                                        fontSize = 12.sp,
                                        color = if (isOutgoing) MaterialTheme.colorScheme.onPrimaryContainer else Color(0xFF2E7D32)
                                    )
                                }
                            }

                            Text(
                                "ETA: ${p.etaSeconds?.let { "${it}s" } ?: "--"}",
                                fontWeight = FontWeight.SemiBold,
                                fontSize = 13.sp,
                                color = MaterialTheme.colorScheme.onSurfaceVariant
                            )
                        }

                        // File Name
                        Text(
                            text = p.fileName,
                            fontWeight = FontWeight.Bold,
                            fontSize = 18.sp,
                            maxLines = 2,
                            overflow = TextOverflow.Ellipsis
                        )

                        // Large Combined Speedometer Display
                        Column(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalAlignment = Alignment.CenterHorizontally
                        ) {
                            Text(
                                String.format("%.2f", totalSpeedMBps),
                                fontSize = 42.sp,
                                fontWeight = FontWeight.ExtraBold,
                                color = Color(0xFF2E7D32)
                            )
                            Text(
                                "MB/s (Combined Throughput)",
                                fontSize = 12.sp,
                                fontWeight = FontWeight.SemiBold,
                                color = MaterialTheme.colorScheme.onSurfaceVariant
                            )
                        }

                        // Progress Bar & Percentage
                        Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                            Row(
                                modifier = Modifier.fillMaxWidth(),
                                horizontalArrangement = Arrangement.SpaceBetween
                            ) {
                                Text(
                                    "${UriUtils.formatFileSize(p.bytesTransferred.toLong())} / ${UriUtils.formatFileSize(p.fileSize.toLong())}",
                                    fontSize = 13.sp
                                )
                                Text(
                                    String.format("%.1f%%", p.percent),
                                    fontWeight = FontWeight.Bold,
                                    color = MaterialTheme.colorScheme.primary,
                                    fontSize = 14.sp
                                )
                            }
                            LinearProgressIndicator(
                                progress = { (p.percent / 100.0).toFloat() },
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .height(10.dp)
                                    .clip(RoundedCornerShape(5.dp)),
                                color = MaterialTheme.colorScheme.primary,
                                trackColor = MaterialTheme.colorScheme.surfaceVariant
                            )
                        }

                        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)

                        // Dual-Channel Visual Breakdown Cards
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(10.dp)
                        ) {
                            // USB Gauge Card
                            Surface(
                                modifier = Modifier.weight(1f),
                                shape = RoundedCornerShape(12.dp),
                                color = Color(0xFF1565C0).copy(alpha = 0.08f),
                                border = BorderStroke(1.dp, Color(0xFF1565C0).copy(alpha = 0.3f))
                            ) {
                                Column(modifier = Modifier.padding(10.dp)) {
                                    Row(
                                        verticalAlignment = Alignment.CenterVertically,
                                        horizontalArrangement = Arrangement.spacedBy(4.dp)
                                    ) {
                                        Icon(Icons.Default.Usb, contentDescription = null, tint = Color(0xFF1565C0), modifier = Modifier.size(16.dp))
                                        Text("USB Link", fontWeight = FontWeight.Bold, fontSize = 12.sp, color = Color(0xFF1565C0))
                                    }
                                    Text(
                                        String.format("%.2f MB/s", usbSpeedMBps),
                                        fontWeight = FontWeight.ExtraBold,
                                        fontSize = 16.sp,
                                        color = Color(0xFF1565C0)
                                    )
                                    Text(
                                        if (usbSpeedMBps > 0.01) "● Active" else "○ Standby",
                                        fontSize = 10.sp,
                                        color = if (usbSpeedMBps > 0.01) Color(0xFF2E7D32) else Color.Gray
                                    )
                                }
                            }

                            // 5GHz Wi-Fi Gauge Card
                            Surface(
                                modifier = Modifier.weight(1f),
                                shape = RoundedCornerShape(12.dp),
                                color = Color(0xFF7B1FA2).copy(alpha = 0.08f),
                                border = BorderStroke(1.dp, Color(0xFF7B1FA2).copy(alpha = 0.3f))
                            ) {
                                Column(modifier = Modifier.padding(10.dp)) {
                                    Row(
                                        verticalAlignment = Alignment.CenterVertically,
                                        horizontalArrangement = Arrangement.spacedBy(4.dp)
                                    ) {
                                        Icon(Icons.Default.Wifi, contentDescription = null, tint = Color(0xFF7B1FA2), modifier = Modifier.size(16.dp))
                                        Text("5 GHz Wi-Fi", fontWeight = FontWeight.Bold, fontSize = 12.sp, color = Color(0xFF7B1FA2))
                                    }
                                    Text(
                                        String.format("%.2f MB/s", wifiSpeedMBps),
                                        fontWeight = FontWeight.ExtraBold,
                                        fontSize = 16.sp,
                                        color = Color(0xFF7B1FA2)
                                    )
                                    Text(
                                        if (wifiSpeedMBps > 0.01) "● Active" else "○ Standby",
                                        fontSize = 10.sp,
                                        color = if (wifiSpeedMBps > 0.01) Color(0xFF2E7D32) else Color.Gray
                                    )
                                }
                            }
                        }

                        // Transfer Control Buttons
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(10.dp)
                        ) {
                            val id = p.transferId
                            if (p.status == FfiTransferStatus.IN_PROGRESS) {
                                OutlinedButton(
                                    onClick = { onPause(id) },
                                    modifier = Modifier.weight(1f),
                                    shape = RoundedCornerShape(10.dp)
                                ) {
                                    Icon(Icons.Default.Pause, contentDescription = null, modifier = Modifier.size(16.dp))
                                    Spacer(modifier = Modifier.width(4.dp))
                                    Text("Pause")
                                }
                            } else {
                                Button(
                                    onClick = { onResume(id) },
                                    modifier = Modifier.weight(1f),
                                    shape = RoundedCornerShape(10.dp)
                                ) {
                                    Icon(Icons.Default.PlayArrow, contentDescription = null, modifier = Modifier.size(16.dp))
                                    Spacer(modifier = Modifier.width(4.dp))
                                    Text("Resume")
                                }
                            }

                            Button(
                                onClick = { onCancel(id) },
                                modifier = Modifier.weight(1f),
                                colors = ButtonDefaults.buttonColors(containerColor = Color(0xFFD32F2F)),
                                shape = RoundedCornerShape(10.dp)
                            ) {
                                Icon(Icons.Default.Close, contentDescription = null, modifier = Modifier.size(16.dp))
                                Spacer(modifier = Modifier.width(4.dp))
                                Text("Cancel")
                            }
                        }
                    }
                }
            }
        }

        // 3. Sequential Multi-File Queue (if multiple items queued)
        if (transferQueue.size > 1) {
            item {
                Text(
                    "Transfer Queue (${currentQueueIndex + 1}/${transferQueue.size})",
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.Bold
                )
            }

            items(transferQueue.size) { idx ->
                val fileInfo = transferQueue[idx]
                val isCurrent = idx == currentQueueIndex
                val isCompleted = idx < currentQueueIndex

                Surface(
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(12.dp),
                    color = if (isCurrent) MaterialTheme.colorScheme.primaryContainer.copy(alpha = 0.4f) else MaterialTheme.colorScheme.surface,
                    border = BorderStroke(1.dp, if (isCurrent) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outlineVariant)
                ) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(12.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.SpaceBetween
                    ) {
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            Icon(
                                imageVector = when {
                                    isCompleted -> Icons.Default.CheckCircle
                                    isCurrent -> Icons.Default.Sync
                                    else -> Icons.Default.Schedule
                                },
                                contentDescription = null,
                                tint = when {
                                    isCompleted -> Color(0xFF2E7D32)
                                    isCurrent -> MaterialTheme.colorScheme.primary
                                    else -> Color.Gray
                                },
                                modifier = Modifier.size(20.dp)
                            )
                            Column {
                                Text(fileInfo.displayName, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
                                Text(fileInfo.formattedSize, fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            }
                        }

                        Text(
                            text = when {
                                isCompleted -> "Done"
                                isCurrent -> "Transferring..."
                                else -> "Queued"
                            },
                            fontSize = 11.sp,
                            fontWeight = FontWeight.Bold,
                            color = when {
                                isCompleted -> Color(0xFF2E7D32)
                                isCurrent -> MaterialTheme.colorScheme.primary
                                else -> Color.Gray
                            }
                        )
                    }
                }
            }
        }

        // Idle State Placeholder
        if ((progress == null || (progress.status != FfiTransferStatus.IN_PROGRESS && progress.status != FfiTransferStatus.PAUSED)) && lastCompletedItem == null) {
            item {
                Card(
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(16.dp),
                    colors = CardDefaults.cardColors(
                        containerColor = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.3f)
                    ),
                    border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant)
                ) {
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(32.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(12.dp)
                    ) {
                        Icon(
                            Icons.Default.Speed,
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.size(48.dp)
                        )
                        Text(
                            "No Active Transfer",
                            style = MaterialTheme.typography.titleMedium,
                            fontWeight = FontWeight.Bold
                        )
                        Text(
                            "Go to the Send tab to pick files or start Receive mode to accept files from PC.",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            textAlign = TextAlign.Center
                        )
                    }
                }
            }
        }
    }
}

// -------------------------------------------------------------------------------------------------
// TAB 3: TRANSFER HISTORY SCREEN
// -------------------------------------------------------------------------------------------------

@Composable
fun HistoryScreen() {
    val historyList by TransferHistoryManager.historyFlow.collectAsState()
    val context = LocalContext.current
    var showClearDialog by remember { mutableStateOf(false) }

    LazyColumn(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
        contentPadding = PaddingValues(vertical = 16.dp)
    ) {
        item {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Text(
                    "Transfer History",
                    style = MaterialTheme.typography.titleLarge,
                    fontWeight = FontWeight.Bold
                )
                if (historyList.isNotEmpty()) {
                    TextButton(onClick = { showClearDialog = true }) {
                        Text("Clear All", color = MaterialTheme.colorScheme.error)
                    }
                }
            }
        }

        if (historyList.isEmpty()) {
            item {
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(top = 60.dp),
                    contentAlignment = Alignment.Center
                ) {
                    Column(
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(8.dp)
                    ) {
                        Icon(
                            Icons.Default.History,
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.size(48.dp)
                        )
                        Text(
                            "No transfers yet",
                            fontWeight = FontWeight.Bold,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                        Text(
                            "Completed file transfers will appear here.",
                            fontSize = 12.sp,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    }
                }
            }
        } else {
            items(historyList) { item ->
                Card(
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(14.dp),
                    colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
                    border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant)
                ) {
                    Column(
                        modifier = Modifier.padding(14.dp),
                        verticalArrangement = Arrangement.spacedBy(8.dp)
                    ) {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Surface(
                                color = if (item.isOutgoing) MaterialTheme.colorScheme.primaryContainer else Color(0xFFE8F5E9),
                                shape = RoundedCornerShape(8.dp)
                            ) {
                                Row(
                                    modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp),
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.spacedBy(4.dp)
                                ) {
                                    Icon(
                                        imageVector = if (item.isOutgoing) Icons.Default.ArrowUpward else Icons.Default.ArrowDownward,
                                        contentDescription = null,
                                        modifier = Modifier.size(12.dp),
                                        tint = if (item.isOutgoing) MaterialTheme.colorScheme.onPrimaryContainer else Color(0xFF2E7D32)
                                    )
                                    Text(
                                        if (item.isOutgoing) "Sent" else "Received",
                                        fontSize = 11.sp,
                                        fontWeight = FontWeight.Bold,
                                        color = if (item.isOutgoing) MaterialTheme.colorScheme.onPrimaryContainer else Color(0xFF2E7D32)
                                    )
                                }
                            }
                            Text(item.formattedDate, fontSize = 11.sp, color = Color.Gray)
                        }

                        Text(
                            item.fileName,
                            fontWeight = FontWeight.Bold,
                            fontSize = 15.sp,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis
                        )

                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween
                        ) {
                            Text("Size: ${item.formattedSize}", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            if (item.avgSpeedMBps > 0.0) {
                                Text(
                                    "Avg Speed: ${String.format("%.2f MB/s", item.avgSpeedMBps)}",
                                    fontSize = 12.sp,
                                    fontWeight = FontWeight.SemiBold,
                                    color = Color(0xFF2E7D32)
                                )
                            }
                        }

                        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.5f))
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.End,
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            if (!item.isOutgoing && item.filePath.isNotBlank() && File(item.filePath).exists()) {
                                TextButton(
                                    onClick = { UriUtils.openFile(context, item.filePath) },
                                    contentPadding = PaddingValues(horizontal = 8.dp)
                                ) {
                                    Icon(Icons.Default.OpenInNew, contentDescription = null, modifier = Modifier.size(14.dp))
                                    Spacer(modifier = Modifier.width(4.dp))
                                    Text("Open", fontSize = 12.sp)
                                }
                                TextButton(
                                    onClick = { UriUtils.shareFile(context, item.filePath) },
                                    contentPadding = PaddingValues(horizontal = 8.dp)
                                ) {
                                    Icon(Icons.Default.Share, contentDescription = null, modifier = Modifier.size(14.dp))
                                    Spacer(modifier = Modifier.width(4.dp))
                                    Text("Share", fontSize = 12.sp)
                                }
                            }
                            IconButton(
                                onClick = { TransferHistoryManager.deleteRecord(item.id) },
                                modifier = Modifier.size(28.dp)
                            ) {
                                Icon(Icons.Default.Delete, contentDescription = "Delete", tint = Color.Gray, modifier = Modifier.size(16.dp))
                            }
                        }
                    }
                }
            }
        }
    }

    if (showClearDialog) {
        AlertDialog(
            onDismissRequest = { showClearDialog = false },
            title = { Text("Clear History") },
            text = { Text("Are you sure you want to clear all transfer history records?") },
            confirmButton = {
                TextButton(onClick = {
                    TransferHistoryManager.clearHistory()
                    showClearDialog = false
                }) {
                    Text("Clear All", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { showClearDialog = false }) {
                    Text("Cancel")
                }
            }
        )
    }
}

// -------------------------------------------------------------------------------------------------
// TAB 4: SETTINGS & DEVELOPER DIAGNOSTICS SCREEN
// -------------------------------------------------------------------------------------------------

@Composable
fun SettingsScreen(
    spikeManager: WifiDirectSpikeManager,
    hotspotManager: WifiHotspotManager,
    hotspotState: WifiHotspotState
) {
    var deviceName by remember { mutableStateOf(Build.MODEL) }
    var prefer5Ghz by remember { mutableStateOf(true) }
    var autoWakeLock by remember { mutableStateOf(true) }
    var showDiagnostics by remember { mutableStateOf(false) }

    val context = LocalContext.current
    val spikeState by spikeManager.state.collectAsState()

    LazyColumn(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
        contentPadding = PaddingValues(vertical = 16.dp)
    ) {
        item {
            Text(
                "Preferences & Settings",
                style = MaterialTheme.typography.titleLarge,
                fontWeight = FontWeight.Bold
            )
        }

        // 1. Device Info Card
        item {
            Card(
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(16.dp),
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
                border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant)
            ) {
                Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text("Device Identity", style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.Bold)
                    OutlinedTextField(
                        value = deviceName,
                        onValueChange = { deviceName = it },
                        label = { Text("Device Name") },
                        singleLine = true,
                        modifier = Modifier.fillMaxWidth(),
                        shape = RoundedCornerShape(10.dp)
                    )
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween
                    ) {
                        Text("Hardware:", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        Text("${Build.MANUFACTURER} ${Build.MODEL} (${Build.HARDWARE})", fontSize = 12.sp, fontWeight = FontWeight.Medium)
                    }
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween
                    ) {
                        Text("Detected USB:", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        Text(UsbHardwareHelper.getUsbSpeedLabel(context), fontSize = 12.sp, fontWeight = FontWeight.Bold, color = Color(0xFF1565C0))
                    }
                }
            }
        }

        // 2. Wireless Transfer Options
        item {
            Card(
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(16.dp),
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
                border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant)
            ) {
                Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text("Wireless Transfer Options", style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.Bold)

                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Column(modifier = Modifier.weight(1f)) {
                            Text("Enforce 5 GHz Band", fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                            Text("802.11ac 5 GHz hotspot for wire-speed transfers", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                        Switch(checked = prefer5Ghz, onCheckedChange = { prefer5Ghz = it })
                    }

                    HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.5f))

                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Column(modifier = Modifier.weight(1f)) {
                            Text("High-Performance Lock", fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                            Text("Prevent Wi-Fi power-save polling during transfers", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                        Switch(checked = autoWakeLock, onCheckedChange = { autoWakeLock = it })
                    }
                }
            }
        }

        // 3. Collapsible Developer Diagnostics & Wi-Fi Spike Tools
        item {
            Card(
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(16.dp),
                colors = CardDefaults.cardColors(
                    containerColor = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.3f)
                ),
                border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant)
            ) {
                Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable { showDiagnostics = !showDiagnostics },
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            Icon(Icons.Default.BugReport, contentDescription = null, tint = MaterialTheme.colorScheme.primary)
                            Text("Developer Diagnostics & M7a Tools", fontWeight = FontWeight.Bold, fontSize = 14.sp)
                        }
                        Icon(
                            imageVector = if (showDiagnostics) Icons.Default.ExpandLess else Icons.Default.ExpandMore,
                            contentDescription = null
                        )
                    }

                    if (showDiagnostics) {
                        HorizontalDivider()

                        Text("Wi-Fi Direct P2P Group Owner Tool", fontWeight = FontWeight.Bold, fontSize = 13.sp)
                        Text(
                            text = if (spikeState.isGroupOwner) "● P2P Group Active (Group Owner)" else "○ P2P Group Inactive",
                            fontWeight = FontWeight.Bold,
                            color = if (spikeState.isGroupOwner) Color(0xFF2E7D32) else MaterialTheme.colorScheme.onSurfaceVariant,
                            fontSize = 12.sp
                        )

                        if (spikeState.ssid != null) {
                            Text("SSID: ${spikeState.ssid}", fontFamily = FontFamily.Monospace, fontSize = 12.sp)
                            Text("Password: ${spikeState.passphrase}", fontFamily = FontFamily.Monospace, fontSize = 12.sp)
                        }

                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(8.dp)
                        ) {
                            Button(
                                onClick = {
                                    spikeManager.createP2pGroup { _, msg -> Toast.makeText(context, msg, Toast.LENGTH_SHORT).show() }
                                },
                                modifier = Modifier.weight(1f),
                                enabled = !spikeState.isGroupOwner,
                                shape = RoundedCornerShape(8.dp)
                            ) {
                                Text("P2P Group", fontSize = 12.sp)
                            }
                            Button(
                                onClick = {
                                    spikeManager.removeP2pGroup { _, _ -> }
                                    spikeManager.stopLocalOnlyHotspot()
                                    Toast.makeText(context, "Stopped", Toast.LENGTH_SHORT).show()
                                },
                                modifier = Modifier.weight(1f),
                                enabled = spikeState.isGroupOwner,
                                colors = ButtonDefaults.buttonColors(containerColor = Color(0xFFD32F2F)),
                                shape = RoundedCornerShape(8.dp)
                            ) {
                                Text("Stop", fontSize = 12.sp)
                            }
                        }

                        Text("Echo Server Status: ${if (spikeState.isServerRunning) "Running on :${spikeState.echoPort}" else "Stopped"}", fontSize = 12.sp)
                        Text("Echo packets received: ${spikeState.echoPacketsReceived}", fontSize = 12.sp)
                    }
                }
            }
        }
    }
}

// -------------------------------------------------------------------------------------------------
// MATERIAL 3 DYNAMIC THEME DEFINITION
// -------------------------------------------------------------------------------------------------

@Composable
fun TurboTransferTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    content: @Composable () -> Unit
) {
    val context = LocalContext.current
    val colorScheme = when {
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> {
            if (darkTheme) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
        }
        darkTheme -> darkColorScheme(
            primary = Color(0xFF64B5F6),
            onPrimary = Color(0xFF003355),
            primaryContainer = Color(0xFF0D47A1),
            onPrimaryContainer = Color(0xFFD1E4FF),
            surface = Color(0xFF1E1E1E),
            background = Color(0xFF121212)
        )
        else -> lightColorScheme(
            primary = Color(0xFF1565C0),
            onPrimary = Color.White,
            primaryContainer = Color(0xFFD1E4FF),
            onPrimaryContainer = Color(0xFF001D36),
            surface = Color(0xFFFFFFFF),
            background = Color(0xFFF8F9FA)
        )
    }

    MaterialTheme(
        colorScheme = colorScheme,
        content = content
    )
}
