package com.turbotransfer.presentation.receive

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.os.Environment
import android.os.StatFs
import android.provider.DocumentsContract
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.turbotransfer.presentation.components.*
import com.turbotransfer.presentation.theme.*

@Composable
fun ReceiveScreen(
    viewModel: ReceiveViewModel = hiltViewModel(),
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

    val folderPicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
        uri?.let {
            val docId = DocumentsContract.getTreeDocumentId(it)
            val path = if (docId.startsWith("primary:")) {
                "/sdcard/" + docId.substringAfter("primary:")
            } else {
                Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS).absolutePath
            }
            viewModel.setDestinationDir(path)
        }
    }

    val isListening = uiState.isListening
    val hotspotState = uiState.hotspotState

    // Calculate free storage space dynamically
    val (usedStorageBytes, totalStorageBytes) = remember(uiState.destDir) {
        try {
            val stat = StatFs(uiState.destDir)
            val total = stat.totalBytes
            val free = stat.availableBytes
            Pair(total - free, total)
        } catch (e: Exception) {
            Pair(0L, 0L)
        }
    }

    LazyColumn(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
        contentPadding = PaddingValues(top = 16.dp, bottom = 24.dp)
    ) {
        // 1. Header
        item {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Column {
                    Text(
                        "INBOUND BEACON",
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.Black,
                        letterSpacing = 1.sp,
                        color = CyberTextPrimary
                    )
                    Text(
                        "Dual-channel high-speed receiver listener",
                        fontSize = 11.sp,
                        color = CyberTextMuted
                    )
                }
                CyberBadge(
                    text = if (isListening) "LISTENING :9876" else "OFFLINE",
                    color = if (isListening) CyberMint else CyberTextMuted,
                    pulsing = isListening
                )
            }
        }

        // 2. Cyber Beacon Hero Status Card
        item {
            CyberCard(
                modifier = Modifier.fillMaxWidth(),
                borderColor = if (isListening) CyberMint else CyberCardBorder,
                borderGlow = isListening
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.SpaceBetween
                ) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(14.dp)
                    ) {
                        PulsingRadar(
                            isScanning = isListening,
                            modifier = Modifier.size(64.dp),
                            tint = if (isListening) CyberMint else CyberTextMuted
                        )
                        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                            Text(
                                if (isListening) "Receiver Active & Ready" else "Receiver Standby",
                                fontWeight = FontWeight.ExtraBold,
                                fontSize = 15.sp,
                                color = CyberTextPrimary
                            )
                            Text(
                                if (isListening) "Bound to 0.0.0.0:9876" else "Tap 'Start Receive Mode' below",
                                fontSize = 11.sp,
                                fontFamily = FontFamily.Monospace,
                                color = if (isListening) CyberMint else CyberTextMuted
                            )
                        }
                    }
                }

                Spacer(modifier = Modifier.height(14.dp))

                // Multi-Channel Antenna Status Grid
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    // USB ADB Link Badge
                    Surface(
                        modifier = Modifier.weight(1f),
                        shape = RoundedCornerShape(10.dp),
                        color = CyberSurface,
                        border = BorderStroke(1.dp, if (uiState.usbAvailable) CyberCyan else CyberCardBorder)
                    ) {
                        Row(
                            modifier = Modifier.padding(horizontal = 8.dp, vertical = 6.dp),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(6.dp)
                        ) {
                            Icon(
                                Icons.Default.Usb,
                                contentDescription = null,
                                tint = if (uiState.usbAvailable) CyberCyan else CyberTextMuted,
                                modifier = Modifier.size(16.dp)
                            )
                            Column {
                                Text("USB Link", fontSize = 10.sp, fontWeight = FontWeight.Bold, color = CyberTextPrimary)
                                Text(
                                    if (uiState.usbAvailable) "127.0.0.1 (ADB)" else "Standby",
                                    fontSize = 9.sp,
                                    fontFamily = FontFamily.Monospace,
                                    color = if (uiState.usbAvailable) CyberCyan else CyberTextMuted
                                )
                            }
                        }
                    }

                    // 5GHz Wi-Fi / Hotspot Badge
                    Surface(
                        modifier = Modifier.weight(1f),
                        shape = RoundedCornerShape(10.dp),
                        color = CyberSurface,
                        border = BorderStroke(1.dp, if (hotspotState.isActive || uiState.detectedIps.isNotEmpty()) CyberPurple else CyberCardBorder)
                    ) {
                        Row(
                            modifier = Modifier.padding(horizontal = 8.dp, vertical = 6.dp),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(6.dp)
                        ) {
                            Icon(
                                Icons.Default.Wifi,
                                contentDescription = null,
                                tint = if (hotspotState.isActive || uiState.detectedIps.isNotEmpty()) CyberPurple else CyberTextMuted,
                                modifier = Modifier.size(16.dp)
                            )
                            Column {
                                Text("5 GHz Wi-Fi", fontSize = 10.sp, fontWeight = FontWeight.Bold, color = CyberTextPrimary)
                                Text(
                                    if (hotspotState.isActive) "Hotspot 5G" else uiState.primaryIp.ifBlank { "Standby" },
                                    fontSize = 9.sp,
                                    fontFamily = FontFamily.Monospace,
                                    color = if (hotspotState.isActive || uiState.detectedIps.isNotEmpty()) CyberPurple else CyberTextMuted,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis
                                )
                            }
                        }
                    }
                }
            }
        }

        // 3. 5 GHz Hotspot & Instant QR Code Card
        item {
            CyberCard(
                modifier = Modifier.fillMaxWidth(),
                borderColor = if (hotspotState.isActive) CyberPurple else CyberCardBorder,
                borderGlow = hotspotState.isActive
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(10.dp)
                    ) {
                        Surface(
                            shape = RoundedCornerShape(10.dp),
                            color = if (hotspotState.isActive) CyberPurple.copy(alpha = 0.15f) else CyberSurfaceVariant,
                            border = BorderStroke(1.dp, if (hotspotState.isActive) CyberPurple else CyberCardBorder),
                            modifier = Modifier.size(38.dp)
                        ) {
                            Box(contentAlignment = Alignment.Center) {
                                Icon(
                                    Icons.Default.WifiTethering,
                                    contentDescription = null,
                                    tint = if (hotspotState.isActive) CyberPurple else CyberTextSecondary,
                                    modifier = Modifier.size(20.dp)
                                )
                            }
                        }
                        Column {
                            Text(
                                "5 GHz Wi-Fi Direct Hotspot",
                                fontWeight = FontWeight.Bold,
                                fontSize = 14.sp,
                                color = CyberTextPrimary
                            )
                            Text(
                                if (hotspotState.isActive) "Band: ${hotspotState.credentials?.band ?: "5 GHz"} • Active" else "Generate local 5GHz network",
                                fontSize = 11.sp,
                                color = if (hotspotState.isActive) CyberMint else CyberTextMuted
                            )
                        }
                    }

                    Switch(
                        checked = hotspotState.isActive,
                        onCheckedChange = { viewModel.toggleHotspot() },
                        colors = SwitchDefaults.colors(
                            checkedThumbColor = Color.Black,
                            checkedTrackColor = CyberPurple,
                            uncheckedThumbColor = CyberTextMuted,
                            uncheckedTrackColor = CyberSurfaceVariant
                        )
                    )
                }

                if (hotspotState.isActive && hotspotState.credentials != null) {
                    val info = hotspotState.credentials
                    Spacer(modifier = Modifier.height(12.dp))
                    HorizontalDivider(color = CyberCardBorder.copy(alpha = 0.6f))
                    Spacer(modifier = Modifier.height(12.dp))

                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Column(modifier = Modifier.weight(1f)) {
                            Text("SSID: ${info.ssid}", fontFamily = FontFamily.Monospace, fontSize = 12.sp, fontWeight = FontWeight.Bold, color = CyberCyan)
                            Text("Pass: ${info.passphrase}", fontFamily = FontFamily.Monospace, fontSize = 12.sp, color = CyberMint)
                        }

                        Button(
                            onClick = { viewModel.setShowQrDialog(true) },
                            colors = ButtonDefaults.buttonColors(containerColor = CyberPurple),
                            shape = RoundedCornerShape(10.dp),
                            contentPadding = PaddingValues(horizontal = 12.dp, vertical = 6.dp)
                        ) {
                            Icon(Icons.Default.QrCode, contentDescription = null, modifier = Modifier.size(16.dp), tint = Color.White)
                            Spacer(modifier = Modifier.width(6.dp))
                            Text("Show QR", fontSize = 12.sp, fontWeight = FontWeight.Bold, color = Color.White)
                        }
                    }
                }
            }
        }

        // 4. Send from PC Terminal Command Card
        item {
            CyberCard(modifier = Modifier.fillMaxWidth()) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp)
                    ) {
                        Icon(Icons.Default.Terminal, contentDescription = null, tint = CyberCyan, modifier = Modifier.size(18.dp))
                        Text(
                            "TRANSMIT FROM PC CLI",
                            fontWeight = FontWeight.Bold,
                            fontSize = 12.sp,
                            fontFamily = FontFamily.Monospace,
                            color = CyberCyan
                        )
                    }
                    val pcCmd = if (uiState.primaryIp.isNotBlank() && uiState.primaryIp != "127.0.0.1") {
                        "turbo send <file> --address 127.0.0.1:9876,${uiState.primaryIp}:9876"
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
                        Icon(Icons.Default.ContentCopy, contentDescription = "Copy command", tint = CyberCyan, modifier = Modifier.size(16.dp))
                    }
                }

                Spacer(modifier = Modifier.height(8.dp))

                Surface(
                    shape = RoundedCornerShape(8.dp),
                    color = CyberBackground,
                    border = BorderStroke(1.dp, CyberCardBorder),
                    modifier = Modifier.fillMaxWidth()
                ) {
                    Text(
                        text = if (uiState.primaryIp.isNotBlank() && uiState.primaryIp != "127.0.0.1") {
                            "turbo send <file> --address 127.0.0.1:9876,${uiState.primaryIp}:9876"
                        } else {
                            "turbo send <file>"
                        },
                        fontFamily = FontFamily.Monospace,
                        fontSize = 11.sp,
                        color = CyberMint,
                        modifier = Modifier.padding(10.dp)
                    )
                }
            }
        }

        // 5. Storage Destination & Capacity Card
        item {
            CyberCard(modifier = Modifier.fillMaxWidth()) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            "DESTINATION STORAGE",
                            fontWeight = FontWeight.Bold,
                            fontSize = 11.sp,
                            fontFamily = FontFamily.Monospace,
                            color = CyberTextSecondary
                        )
                        Text(
                            uiState.destDir,
                            style = MaterialTheme.typography.bodySmall,
                            fontFamily = FontFamily.Monospace,
                            color = CyberCyan,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis
                        )
                    }
                    OutlinedButton(
                        onClick = { folderPicker.launch(null) },
                        shape = RoundedCornerShape(10.dp),
                        border = BorderStroke(1.dp, CyberCyan.copy(alpha = 0.5f)),
                        contentPadding = PaddingValues(horizontal = 12.dp, vertical = 4.dp)
                    ) {
                        Text("Change", fontSize = 11.sp, color = CyberCyan, fontWeight = FontWeight.Bold)
                    }
                }

                if (totalStorageBytes > 0) {
                    Spacer(modifier = Modifier.height(10.dp))
                    StorageSpaceGauge(
                        usedBytes = usedStorageBytes,
                        totalBytes = totalStorageBytes
                    )
                }
            }
        }

        // 6. Active Transfer Preview Banner (if transfer running)
        if (uiState.activeIncomingSession != null) {
            item {
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
                        Column(modifier = Modifier.weight(1f)) {
                            CyberBadge("INCOMING TRANSFER", color = CyberMint, pulsing = true)
                            Spacer(modifier = Modifier.height(4.dp))
                            Text(
                                uiState.activeIncomingSession!!.fileName,
                                fontWeight = FontWeight.Bold,
                                color = CyberTextPrimary,
                                fontSize = 14.sp,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis
                            )
                        }
                        Button(
                            onClick = onNavigateToTransfer,
                            colors = ButtonDefaults.buttonColors(containerColor = CyberMint),
                            shape = RoundedCornerShape(10.dp)
                        ) {
                            Text("Open Monitor", fontSize = 12.sp, color = Color.Black, fontWeight = FontWeight.Bold)
                        }
                    }
                }
            }
        }

        // 7. Master Toggle Action Button
        item {
            Button(
                onClick = { viewModel.toggleReceiveMode() },
                modifier = Modifier
                    .fillMaxWidth()
                    .height(54.dp)
                    .shadow(
                        10.dp,
                        RoundedCornerShape(14.dp),
                        spotColor = if (isListening) CyberRed.copy(alpha = 0.6f) else CyberMint.copy(alpha = 0.6f)
                    ),
                colors = ButtonDefaults.buttonColors(
                    containerColor = if (isListening) CyberRed else CyberMint
                ),
                shape = RoundedCornerShape(14.dp)
            ) {
                Icon(
                    imageVector = if (isListening) Icons.Default.Stop else Icons.Default.PlayArrow,
                    contentDescription = null,
                    tint = if (isListening) Color.White else Color.Black,
                    modifier = Modifier.size(22.dp)
                )
                Spacer(modifier = Modifier.width(8.dp))
                Text(
                    if (isListening) "STOP RECEIVER BEACON" else "START RECEIVE MODE",
                    fontSize = 14.sp,
                    fontWeight = FontWeight.Black,
                    letterSpacing = 1.sp,
                    color = if (isListening) Color.White else Color.Black
                )
            }
        }
    }

    if (uiState.showQrDialog && hotspotState.credentials != null) {
        HotspotQrDialog(
            credentials = hotspotState.credentials,
            onDismiss = { viewModel.setShowQrDialog(false) }
        )
    }
}
