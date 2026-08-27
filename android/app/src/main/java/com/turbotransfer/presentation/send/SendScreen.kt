package com.turbotransfer.presentation.send

import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.turbotransfer.UriUtils
import com.turbotransfer.presentation.components.CategoryChip
import com.turbotransfer.presentation.components.HotspotQrDialog
import com.turbotransfer.presentation.components.SelectedFileItemChip

@Composable
fun SendScreen(
    viewModel: SendViewModel = hiltViewModel(),
    onNavigateToTransfer: () -> Unit = {}
) {
    val uiState by viewModel.uiState.collectAsStateWithLifecycle()
    val context = LocalContext.current

    LaunchedEffect(uiState.userMessage) {
        uiState.userMessage?.let { msg ->
            Toast.makeText(context, msg, Toast.LENGTH_SHORT).show()
            viewModel.clearUserMessage()
        }
    }

    val multiDocLauncher = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        val items = UriUtils.resolveSelectedUris(context, uris)
        if (items.isNotEmpty()) {
            viewModel.addFilesToQueue(items)
        }
    }

    val folderLauncher = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { treeUri ->
        treeUri?.let {
            val items = UriUtils.resolveDirectoryUri(context, it)
            if (items.isNotEmpty()) {
                viewModel.addFilesToQueue(items)
                Toast.makeText(context, "Added ${items.size} files from folder", Toast.LENGTH_SHORT).show()
            } else {
                Toast.makeText(context, "No readable files found in folder", Toast.LENGTH_SHORT).show()
            }
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
        if (uiState.transferQueue.isNotEmpty()) {
            val totalBytes = uiState.transferQueue.sumOf { it.sizeBytes }
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
                                "${uiState.transferQueue.size} items selected (${UriUtils.formatFileSize(totalBytes)})",
                                fontWeight = FontWeight.Bold,
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onPrimaryContainer
                            )
                            TextButton(
                                onClick = { viewModel.clearQueue() },
                                contentPadding = PaddingValues(horizontal = 8.dp)
                            ) {
                                Text("Clear All", color = MaterialTheme.colorScheme.error, fontSize = 13.sp)
                            }
                        }

                        LazyRow(
                            horizontalArrangement = Arrangement.spacedBy(8.dp)
                        ) {
                            items(uiState.transferQueue) { fileInfo ->
                                SelectedFileItemChip(
                                    fileInfo = fileInfo,
                                    onRemove = { viewModel.removeFileFromQueue(fileInfo) }
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
        val hotspotState = uiState.hotspotState
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
                        if (hotspotState.isActive && hotspotState.credentials != null) {
                            Text(
                                "SSID: ${hotspotState.credentials.ssid}",
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
                        if (hotspotState.isActive && hotspotState.credentials != null) {
                            IconButton(onClick = { viewModel.setShowQrDialog(true) }) {
                                Icon(Icons.Default.QrCode, contentDescription = "QR Code", tint = Color.White)
                            }
                        }
                        Button(
                            onClick = { viewModel.toggleHotspot() },
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

        val discoveredReceiver = uiState.discoveredReceiver
        val effectiveAddress = if (uiState.showCustomAddressField && uiState.customAddress.isNotBlank()) {
            uiState.customAddress
        } else {
            discoveredReceiver?.address ?: uiState.customAddress.ifBlank { "127.0.0.1:9876" }
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
                                        discoveredReceiver.displayName,
                                        fontWeight = FontWeight.Bold,
                                        style = MaterialTheme.typography.bodyLarge
                                    )
                                    Text(
                                        discoveredReceiver.transport,
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
                                viewModel.startBatchTransfer(effectiveAddress) {
                                    onNavigateToTransfer()
                                }
                            },
                            modifier = Modifier
                                .fillMaxWidth()
                                .height(52.dp),
                            enabled = uiState.transferQueue.isNotEmpty(),
                            shape = RoundedCornerShape(12.dp)
                        ) {
                            Icon(Icons.Default.Send, contentDescription = null, modifier = Modifier.size(18.dp))
                            Spacer(modifier = Modifier.width(8.dp))
                            Text(
                                if (uiState.transferQueue.isNotEmpty()) {
                                    val totalBytes = uiState.transferQueue.sumOf { it.sizeBytes }
                                    "Send ${uiState.transferQueue.size} item(s) (${UriUtils.formatFileSize(totalBytes)})"
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
                            .padding(16.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(12.dp)
                    ) {
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(10.dp)
                        ) {
                            CircularProgressIndicator(
                                modifier = Modifier.size(24.dp),
                                strokeWidth = 2.5.dp,
                                color = MaterialTheme.colorScheme.primary
                            )
                            Text(
                                "Scanning for PC Receiver...",
                                fontWeight = FontWeight.SemiBold,
                                style = MaterialTheme.typography.bodyMedium
                            )
                        }
                        Text(
                            "Connect USB cable or run receive mode on PC in the same Wi-Fi network",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            textAlign = TextAlign.Center
                        )

                        if (uiState.transferQueue.isNotEmpty()) {
                            Button(
                                onClick = {
                                    viewModel.startBatchTransfer(effectiveAddress) {
                                        onNavigateToTransfer()
                                    }
                                },
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .height(48.dp),
                                shape = RoundedCornerShape(12.dp)
                            ) {
                                Icon(Icons.Default.Send, contentDescription = null, modifier = Modifier.size(16.dp))
                                Spacer(modifier = Modifier.width(8.dp))
                                val totalBytes = uiState.transferQueue.sumOf { it.sizeBytes }
                                Text(
                                    "Send Now (${UriUtils.formatFileSize(totalBytes)})",
                                    fontSize = 14.sp,
                                    fontWeight = FontWeight.Bold
                                )
                            }
                        }
                    }
                }
            }
        }

        // 5. Custom Target Address Accordion
        item {
            TextButton(
                onClick = { viewModel.toggleCustomAddressField() },
                modifier = Modifier.fillMaxWidth()
            ) {
                Text(
                    if (uiState.showCustomAddressField) "Hide Custom Address ▲" else "Custom Target Address ▼",
                    fontSize = 13.sp
                )
            }

            if (uiState.showCustomAddressField) {
                OutlinedTextField(
                    value = uiState.customAddress,
                    onValueChange = { viewModel.setCustomAddress(it) },
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

    val creds = uiState.hotspotState.credentials
    if (uiState.showQrDialog && creds != null) {
        HotspotQrDialog(
            credentials = creds,
            onDismiss = { viewModel.setShowQrDialog(false) }
        )
    }
}
