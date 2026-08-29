package com.turbotransfer.presentation.send

import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
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
import com.turbotransfer.presentation.components.*
import com.turbotransfer.presentation.theme.*

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
        contentPadding = PaddingValues(top = 16.dp, bottom = 24.dp)
    ) {
        // 1. Header & Media Categories
        item {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Column {
                    Text(
                        "DISPATCH HUB",
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.Black,
                        letterSpacing = 1.sp,
                        color = CyberTextPrimary
                    )
                    Text(
                        "Select files or directories to transmit",
                        fontSize = 11.sp,
                        color = CyberTextMuted
                    )
                }
                if (uiState.transferQueue.isNotEmpty()) {
                    CyberBadge(
                        text = "${uiState.transferQueue.size} QUEUED",
                        color = CyberCyan,
                        pulsing = true
                    )
                }
            }
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
                        accentColor = CyberCyan,
                        onClick = { multiDocLauncher.launch(arrayOf("image/*")) }
                    )
                }
                item {
                    CategoryChip(
                        icon = Icons.Default.Videocam,
                        label = "Videos",
                        accentColor = CyberPurple,
                        onClick = { multiDocLauncher.launch(arrayOf("video/*")) }
                    )
                }
                item {
                    CategoryChip(
                        icon = Icons.Default.Audiotrack,
                        label = "Audio",
                        accentColor = CyberMint,
                        onClick = { multiDocLauncher.launch(arrayOf("audio/*")) }
                    )
                }
                item {
                    CategoryChip(
                        icon = Icons.Default.InsertDriveFile,
                        label = "Files",
                        accentColor = CyberOrange,
                        onClick = { multiDocLauncher.launch(arrayOf("*/*")) }
                    )
                }
                item {
                    CategoryChip(
                        icon = Icons.Default.Android,
                        label = "APKs",
                        accentColor = CyberBlue,
                        onClick = { multiDocLauncher.launch(arrayOf("application/vnd.android.package-archive", "*/*")) }
                    )
                }
                item {
                    CategoryChip(
                        icon = Icons.Default.Folder,
                        label = "Folder",
                        accentColor = Color(0xFFFFC107),
                        onClick = { folderLauncher.launch(null) }
                    )
                }
            }
        }

        // 2. Selection Cart Tray Card
        if (uiState.transferQueue.isNotEmpty()) {
            val totalBytes = uiState.transferQueue.sumOf { it.sizeBytes }
            item {
                CyberCard(
                    modifier = Modifier.fillMaxWidth(),
                    borderColor = CyberCyan,
                    borderGlow = true
                ) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Column {
                            Text(
                                "SELECTION CART",
                                fontWeight = FontWeight.Bold,
                                fontSize = 11.sp,
                                fontFamily = FontFamily.Monospace,
                                color = CyberCyan
                            )
                            Text(
                                "${uiState.transferQueue.size} file(s) • ${UriUtils.formatFileSize(totalBytes)}",
                                fontWeight = FontWeight.ExtraBold,
                                fontSize = 14.sp,
                                color = CyberTextPrimary
                            )
                        }
                        TextButton(
                            onClick = { viewModel.clearQueue() },
                            contentPadding = PaddingValues(horizontal = 8.dp)
                        ) {
                            Text("Clear All", color = CyberRed, fontSize = 12.sp, fontWeight = FontWeight.Bold)
                        }
                    }

                    Spacer(modifier = Modifier.height(10.dp))

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
        } else {
            item {
                CyberCard(
                    modifier = Modifier.fillMaxWidth(),
                    onClick = { multiDocLauncher.launch(arrayOf("*/*")) }
                ) {
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(vertical = 14.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(10.dp)
                    ) {
                        Surface(
                            shape = CircleShape,
                            color = CyberCyan.copy(alpha = 0.12f),
                            border = BorderStroke(1.dp, CyberCyan.copy(alpha = 0.4f)),
                            modifier = Modifier.size(54.dp)
                        ) {
                            Box(contentAlignment = Alignment.Center) {
                                Icon(
                                    Icons.Default.CloudUpload,
                                    contentDescription = null,
                                    tint = CyberCyan,
                                    modifier = Modifier.size(28.dp)
                                )
                            }
                        }
                        Text(
                            "Tap to Select Files or Folders",
                            fontWeight = FontWeight.Bold,
                            fontSize = 15.sp,
                            color = CyberTextPrimary
                        )
                        Text(
                            "Ultra-high speed 802.11ac 5 GHz and USB 3.0 dual-channel ready",
                            fontSize = 11.sp,
                            fontFamily = FontFamily.Monospace,
                            color = CyberTextMuted,
                            textAlign = TextAlign.Center
                        )
                    }
                }
            }
        }

        // 3. 5 GHz Direct Hotspot Hub Card
        val hotspotState = uiState.hotspotState
        item {
            CyberCard(
                modifier = Modifier.fillMaxWidth(),
                borderColor = if (hotspotState.isActive) CyberMint else CyberCardBorder,
                borderGlow = hotspotState.isActive
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.SpaceBetween
                ) {
                    Row(
                        modifier = Modifier.weight(1f),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(12.dp)
                    ) {
                        Surface(
                            shape = RoundedCornerShape(10.dp),
                            color = if (hotspotState.isActive) CyberMint.copy(alpha = 0.15f) else CyberSurfaceVariant,
                            border = BorderStroke(1.dp, if (hotspotState.isActive) CyberMint else CyberCardBorder),
                            modifier = Modifier.size(40.dp)
                        ) {
                            Box(contentAlignment = Alignment.Center) {
                                Icon(
                                    Icons.Default.WifiTethering,
                                    contentDescription = null,
                                    tint = if (hotspotState.isActive) CyberMint else CyberTextSecondary,
                                    modifier = Modifier.size(22.dp)
                                )
                            }
                        }
                        Column {
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(6.dp)
                            ) {
                                Text(
                                    "5 GHz Direct Hotspot",
                                    fontWeight = FontWeight.Bold,
                                    fontSize = 14.sp,
                                    color = CyberTextPrimary
                                )
                                if (hotspotState.isActive) {
                                    CyberBadge("ACTIVE", color = CyberMint, pulsing = true)
                                }
                            }
                            if (hotspotState.isActive && hotspotState.credentials != null) {
                                Text(
                                    "SSID: ${hotspotState.credentials.ssid}",
                                    fontSize = 11.sp,
                                    fontFamily = FontFamily.Monospace,
                                    color = CyberMint
                                )
                            } else {
                                Text(
                                    "Direct phone-to-PC link without router",
                                    fontSize = 11.sp,
                                    color = CyberTextMuted
                                )
                            }
                        }
                    }

                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                        if (hotspotState.isActive && hotspotState.credentials != null) {
                            IconButton(
                                onClick = { viewModel.setShowQrDialog(true) },
                                modifier = Modifier.size(36.dp)
                            ) {
                                Icon(Icons.Default.QrCode, contentDescription = "QR Code", tint = CyberCyan)
                            }
                        }
                        Button(
                            onClick = { viewModel.toggleHotspot() },
                            colors = ButtonDefaults.buttonColors(
                                containerColor = if (hotspotState.isActive) CyberRed else CyberCyan
                            ),
                            shape = RoundedCornerShape(10.dp),
                            contentPadding = PaddingValues(horizontal = 12.dp, vertical = 6.dp)
                        ) {
                            Text(
                                if (hotspotState.isActive) "Stop" else "Start 5G",
                                fontSize = 12.sp,
                                fontWeight = FontWeight.Bold,
                                color = if (hotspotState.isActive) Color.White else Color.Black
                            )
                        }
                    }
                }
            }
        }

        // 4. Target Receiver Discovery & Transmit Action Section
        item {
            Text(
                "TARGET RECEIVER",
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Black,
                letterSpacing = 1.sp,
                color = CyberTextPrimary
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
                CyberCard(
                    modifier = Modifier.fillMaxWidth(),
                    borderColor = CyberMint,
                    borderGlow = true
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
                                color = CyberMint.copy(alpha = 0.15f),
                                border = BorderStroke(1.dp, CyberMint),
                                modifier = Modifier.size(44.dp)
                            ) {
                                Box(contentAlignment = Alignment.Center) {
                                    Icon(
                                        Icons.Default.Computer,
                                        contentDescription = null,
                                        tint = CyberMint,
                                        modifier = Modifier.size(24.dp)
                                    )
                                }
                            }
                            Column {
                                Text(
                                    discoveredReceiver.displayName,
                                    fontWeight = FontWeight.ExtraBold,
                                    fontSize = 15.sp,
                                    color = CyberTextPrimary
                                )
                                Text(
                                    discoveredReceiver.transport,
                                    fontSize = 11.sp,
                                    fontFamily = FontFamily.Monospace,
                                    color = CyberCyan
                                )
                            }
                        }
                        CyberBadge("READY", color = CyberMint, pulsing = true)
                    }

                    Spacer(modifier = Modifier.height(14.dp))

                    Button(
                        onClick = {
                            viewModel.startBatchTransfer(effectiveAddress) {
                                onNavigateToTransfer()
                            }
                        },
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(52.dp)
                            .shadow(8.dp, RoundedCornerShape(12.dp), spotColor = CyberCyan.copy(alpha = 0.5f)),
                        enabled = uiState.transferQueue.isNotEmpty(),
                        colors = ButtonDefaults.buttonColors(
                            containerColor = CyberCyan,
                            disabledContainerColor = CyberSurfaceVariant
                        ),
                        shape = RoundedCornerShape(12.dp)
                    ) {
                        Icon(Icons.Default.Send, contentDescription = null, modifier = Modifier.size(18.dp), tint = Color.Black)
                        Spacer(modifier = Modifier.width(8.dp))
                        val totalBytes = uiState.transferQueue.sumOf { it.sizeBytes }
                        Text(
                            if (uiState.transferQueue.isNotEmpty()) {
                                "TRANSMIT ${uiState.transferQueue.size} ITEM(S) (${UriUtils.formatFileSize(totalBytes)})"
                            } else {
                                "SELECT FILES TO TRANSMIT"
                            },
                            fontSize = 13.sp,
                            fontWeight = FontWeight.Black,
                            letterSpacing = 0.5.sp,
                            color = Color.Black
                        )
                    }
                }
            } else {
                CyberCard(modifier = Modifier.fillMaxWidth()) {
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(vertical = 12.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(12.dp)
                    ) {
                        PulsingRadar(
                            isScanning = true,
                            modifier = Modifier.size(80.dp),
                            tint = CyberCyan
                        )

                        Text(
                            "Scanning for PC Receiver...",
                            fontWeight = FontWeight.Bold,
                            fontSize = 14.sp,
                            color = CyberTextPrimary
                        )

                        Text(
                            "Connect USB cable (ADB) or run TurboTransfer PC app on same Wi-Fi",
                            fontSize = 11.sp,
                            color = CyberTextMuted,
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
                                colors = ButtonDefaults.buttonColors(containerColor = CyberCyan),
                                shape = RoundedCornerShape(12.dp)
                            ) {
                                Icon(Icons.Default.Send, contentDescription = null, modifier = Modifier.size(16.dp), tint = Color.Black)
                                Spacer(modifier = Modifier.width(8.dp))
                                val totalBytes = uiState.transferQueue.sumOf { it.sizeBytes }
                                Text(
                                    "Transmit Now (${UriUtils.formatFileSize(totalBytes)})",
                                    fontSize = 13.sp,
                                    fontWeight = FontWeight.Bold,
                                    color = Color.Black
                                )
                            }
                        }
                    }
                }
            }
        }

        // 5. Custom Target Address Accordion
        item {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                TextButton(
                    onClick = { viewModel.toggleCustomAddressField() },
                    modifier = Modifier.fillMaxWidth()
                ) {
                    Text(
                        if (uiState.showCustomAddressField) "Hide Manual Address Configuration ▲" else "Manual Target IP:Port Override ▼",
                        fontSize = 11.sp,
                        fontFamily = FontFamily.Monospace,
                        color = CyberCyan
                    )
                }

                if (uiState.showCustomAddressField) {
                    OutlinedTextField(
                        value = uiState.customAddress,
                        onValueChange = { viewModel.setCustomAddress(it) },
                        label = { Text("Target IP:Port (e.g. 192.168.1.50:9876)", fontSize = 11.sp) },
                        singleLine = true,
                        keyboardOptions = KeyboardOptions(
                            keyboardType = KeyboardType.Ascii,
                            autoCorrect = false,
                            capitalization = KeyboardCapitalization.None
                        ),
                        colors = OutlinedTextFieldDefaults.colors(
                            focusedBorderColor = CyberCyan,
                            unfocusedBorderColor = CyberCardBorder,
                            focusedTextColor = CyberTextPrimary,
                            unfocusedTextColor = CyberTextPrimary,
                            focusedContainerColor = CyberSurface,
                            unfocusedContainerColor = CyberSurface
                        ),
                        modifier = Modifier.fillMaxWidth(),
                        shape = RoundedCornerShape(12.dp)
                    )
                }
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
