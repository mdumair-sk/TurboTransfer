package com.turbotransfer.presentation.components

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.widget.Toast
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.*
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.OpenInNew
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
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
import com.turbotransfer.UriUtils
import com.turbotransfer.domain.model.HistoryItem
import com.turbotransfer.domain.model.HotspotCredentials
import com.turbotransfer.domain.model.TransferProgressInfo
import com.turbotransfer.domain.model.TransferStatus
import java.io.File

/**
 * Minimalist Compose Canvas QR Code module.
 */
@Composable
fun QrCodeView(
    content: String,
    modifier: Modifier = Modifier.size(180.dp),
    darkColor: Color = MaterialTheme.colorScheme.onSurface,
    lightColor: Color = MaterialTheme.colorScheme.surfaceContainerHighest
) {
    val matrix = remember(content) {
        try {
            QrCodeEncoder.encode(content)
        } catch (e: Exception) {
            Array(21) { BooleanArray(21) }
        }
    }

    Surface(
        modifier = modifier,
        shape = RoundedCornerShape(16.dp),
        color = lightColor
    ) {
        Canvas(
            modifier = Modifier
                .fillMaxSize()
                .padding(14.dp)
        ) {
            val count = matrix.size
            if (count == 0) return@Canvas

            val cellSize = size.minDimension / count

            for (r in 0 until count) {
                for (c in 0 until count) {
                    if (matrix[r][c]) {
                        drawRoundRect(
                            color = darkColor,
                            topLeft = Offset(c * cellSize, r * cellSize),
                            size = Size(cellSize, cellSize),
                            cornerRadius = CornerRadius(cellSize * 0.25f, cellSize * 0.25f)
                        )
                    }
                }
            }
        }
    }
}

/**
 * Clean Material 3 QR Code Dialog for 5 GHz Wi-Fi hotspot connection.
 */
@Composable
fun QrCodeDialog(
    credentials: HotspotCredentials,
    onDismiss: () -> Unit
) {
    val context = LocalContext.current
    val wifiQrContent = "WIFI:S:${credentials.ssid};T:WPA;P:${credentials.passphrase};;"

    AlertDialog(
        onDismissRequest = onDismiss,
        title = {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                Icon(
                    imageVector = Icons.Default.QrCode,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.primary
                )
                Text(
                    "Hotspot Quick Pair",
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.Bold
                )
            }
        },
        text = {
            Column(
                modifier = Modifier.fillMaxWidth(),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(14.dp)
            ) {
                Box(
                    modifier = Modifier
                        .background(MaterialTheme.colorScheme.surfaceContainerHigh, RoundedCornerShape(20.dp))
                        .padding(12.dp),
                    contentAlignment = Alignment.Center
                ) {
                    QrCodeView(
                        content = wifiQrContent,
                        modifier = Modifier.size(180.dp),
                        darkColor = MaterialTheme.colorScheme.onSurface,
                        lightColor = MaterialTheme.colorScheme.surfaceContainerHigh
                    )
                }

                Text(
                    "Scan with PC camera or another phone to connect to the 5GHz Wi-Fi network instantly.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    textAlign = TextAlign.Center
                )

                Surface(
                    shape = RoundedCornerShape(12.dp),
                    color = MaterialTheme.colorScheme.surfaceContainerLow,
                    modifier = Modifier.fillMaxWidth()
                ) {
                    Column(
                        modifier = Modifier.padding(12.dp),
                        verticalArrangement = Arrangement.spacedBy(6.dp)
                    ) {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Column {
                                Text("SSID", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                Text(credentials.ssid, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace)
                            }
                            IconButton(
                                onClick = {
                                    val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                                    cm.setPrimaryClip(ClipData.newPlainText("SSID", credentials.ssid))
                                    Toast.makeText(context, "SSID copied", Toast.LENGTH_SHORT).show()
                                },
                                modifier = Modifier.size(24.dp)
                            ) {
                                Icon(Icons.Default.ContentCopy, contentDescription = "Copy SSID", modifier = Modifier.size(16.dp))
                            }
                        }

                        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.5f))

                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Column {
                                Text("Password", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                Text(credentials.passphrase, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace)
                            }
                            IconButton(
                                onClick = {
                                    val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                                    cm.setPrimaryClip(ClipData.newPlainText("Password", credentials.passphrase))
                                    Toast.makeText(context, "Password copied", Toast.LENGTH_SHORT).show()
                                },
                                modifier = Modifier.size(24.dp)
                            ) {
                                Icon(Icons.Default.ContentCopy, contentDescription = "Copy Password", modifier = Modifier.size(16.dp))
                            }
                        }
                    }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = onDismiss) {
                Text("Done")
            }
        }
    )
}

/**
 * Modal Bottom Sheet for Advanced Options (5GHz Hotspot, Manual IP, CLI snippets).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AdvancedOptionsBottomSheet(
    onDismiss: () -> Unit,
    hotspotActive: Boolean,
    onToggleHotspot: () -> Unit,
    hotspotCredentials: HotspotCredentials?,
    onShowQr: () -> Unit,
    customAddress: String,
    onCustomAddressChange: (String) -> Unit,
    primaryIp: String
) {
    val context = LocalContext.current
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        containerColor = MaterialTheme.colorScheme.surfaceContainerLow
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 24.dp, vertical = 12.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp)
        ) {
            Text(
                "Connection & Advanced Setup",
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Bold
            )

            // 1. 5 GHz Hotspot Card
            Card(
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
                shape = RoundedCornerShape(16.dp)
            ) {
                Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            Icon(Icons.Default.WifiTethering, contentDescription = null, tint = MaterialTheme.colorScheme.primary)
                            Column {
                                Text("5 GHz Direct Hotspot", fontWeight = FontWeight.Bold, style = MaterialTheme.typography.bodyMedium)
                                Text(
                                    if (hotspotActive) "Active (5 GHz band)" else "Direct link without Wi-Fi router",
                                    style = MaterialTheme.typography.bodySmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant
                                )
                            }
                        }
                        Switch(
                            checked = hotspotActive,
                            onCheckedChange = { onToggleHotspot() }
                        )
                    }

                    if (hotspotActive && hotspotCredentials != null) {
                        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.5f))
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Column {
                                Text("SSID: ${hotspotCredentials.ssid}", style = MaterialTheme.typography.bodySmall, fontFamily = FontFamily.Monospace)
                                Text("Pass: ${hotspotCredentials.passphrase}", style = MaterialTheme.typography.bodySmall, fontFamily = FontFamily.Monospace, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            }
                            FilledTonalButton(
                                onClick = onShowQr,
                                shape = RoundedCornerShape(8.dp),
                                contentPadding = PaddingValues(horizontal = 10.dp, vertical = 4.dp)
                            ) {
                                Icon(Icons.Default.QrCode, contentDescription = null, modifier = Modifier.size(16.dp))
                                Spacer(modifier = Modifier.width(4.dp))
                                Text("QR", fontSize = 11.sp)
                            }
                        }
                    }
                }
            }

            // 2. Manual Target IP:Port Override
            Card(
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
                shape = RoundedCornerShape(16.dp)
            ) {
                Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("Manual Target IP:Port Override", fontWeight = FontWeight.Bold, style = MaterialTheme.typography.bodyMedium)
                    Text("Optional: specify exact target IP address and port (e.g. 192.168.1.100:9876)", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    OutlinedTextField(
                        value = customAddress,
                        onValueChange = onCustomAddressChange,
                        placeholder = { Text("127.0.0.1:9876 or IP:Port", fontSize = 12.sp) },
                        singleLine = true,
                        keyboardOptions = KeyboardOptions(
                            keyboardType = KeyboardType.Ascii,
                            autoCorrect = false,
                            capitalization = KeyboardCapitalization.None
                        ),
                        modifier = Modifier.fillMaxWidth(),
                        shape = RoundedCornerShape(10.dp)
                    )
                }
            }

            // 3. PC CLI Command Snippet
            Card(
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
                shape = RoundedCornerShape(16.dp)
            ) {
                Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Text("Send from PC Command", fontWeight = FontWeight.Bold, style = MaterialTheme.typography.bodyMedium)
                        val pcCmd = if (primaryIp.isNotBlank() && primaryIp != "127.0.0.1") {
                            "turbo send <file> --address 127.0.0.1:9876,$primaryIp:9876"
                        } else {
                            "turbo send <file>"
                        }
                        IconButton(
                            onClick = {
                                val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                                cm.setPrimaryClip(ClipData.newPlainText("PC Command", pcCmd))
                                Toast.makeText(context, "Command copied", Toast.LENGTH_SHORT).show()
                            },
                            modifier = Modifier.size(24.dp)
                        ) {
                            Icon(Icons.Default.ContentCopy, contentDescription = "Copy command", modifier = Modifier.size(16.dp))
                        }
                    }
                    Surface(
                        shape = RoundedCornerShape(8.dp),
                        color = MaterialTheme.colorScheme.surfaceContainerHigh,
                        modifier = Modifier.fillMaxWidth()
                    ) {
                        Text(
                            text = if (primaryIp.isNotBlank() && primaryIp != "127.0.0.1") {
                                "turbo send <file> --address 127.0.0.1:9876,$primaryIp:9876"
                            } else {
                                "turbo send <file>"
                            },
                            fontFamily = FontFamily.Monospace,
                            fontSize = 11.sp,
                            color = MaterialTheme.colorScheme.primary,
                            modifier = Modifier.padding(10.dp)
                        )
                    }
                }
            }

            Spacer(modifier = Modifier.height(16.dp))
        }
    }
}

private fun copyLogsToClipboard(context: Context, text: String) {
    val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
    val clip = ClipData.newPlainText("TurboTransfer Engine Logs", text)
    clipboard?.setPrimaryClip(clip)
    Toast.makeText(context, "Logs copied to clipboard", Toast.LENGTH_SHORT).show()
}

private fun loadTransferLogs(context: Context, transferId: String, item: HistoryItem): String {
    // 1. Try reading raw .log file from disk
    val logFile = File(context.filesDir, "logs/$transferId.log")
    if (logFile.exists() && logFile.length() > 0) {
        try {
            return logFile.readText()
        } catch (_: Exception) {}
    }

    // 2. Try in-memory UniFFI event stream
    try {
        val events = uniffi.turbotransfer_core.getTransferLogs(transferId, null)
        if (events.isNotEmpty()) {
            val sb = StringBuilder()
            sb.appendLine("=== TurboTransfer Engine Telemetry: $transferId ===")
            sb.appendLine("File: ${item.fileName} (${item.formattedSize})")
            sb.appendLine("Status: ${item.status}")
            sb.appendLine("--------------------------------------------------------------------------------")
            for (ev in events) {
                val relMs = String.format("%6d", ev.relativeMs)
                sb.appendLine("$relMs | ${ev.level.padEnd(5)} | ${ev.stage.padEnd(10)} | ${ev.channel.padEnd(14)} | ${ev.message}")
            }
            return sb.toString()
        }
    } catch (_: Exception) {}

    // 3. Try reading .json report from disk
    val jsonFile = File(context.filesDir, "logs/$transferId.json")
    if (jsonFile.exists() && jsonFile.length() > 0) {
        try {
            return jsonFile.readText()
        } catch (_: Exception) {}
    }

    // 4. Synthesized session telemetry fallback
    return """
=== TurboTransfer Engine Telemetry Summary ===
Transfer ID:       $transferId
File Name:         ${item.fileName}
File Size:         ${item.formattedSize} (${item.fileSize} bytes)
Direction:         ${if (item.isOutgoing) "Sender (Sent to PC)" else "Receiver (Received from PC)"}
Status:            ${item.status}
Elapsed Time:      ${String.format("%.2f", item.durationMs / 1000.0)}s (${item.durationMs} ms)
Average Speed:     ${String.format("%.2f", item.avgSpeedMBps)} MB/s
Peak Speed:        ${String.format("%.2f", item.peakSpeedMBps)} MB/s
USB Throughput:    ${String.format("%.2f", item.usbSpeedMBps)} MB/s
Wi-Fi Throughput:  ${String.format("%.2f", item.wifiSpeedMBps)} MB/s
Storage Path:      ${item.filePath}
Timestamp:         ${java.util.Date(item.timestamp)}
--------------------------------------------------------------------------------
No event log stream recorded for this session.
    """.trimIndent()
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TransferLogViewerFullScreenDialog(
    item: HistoryItem,
    logContent: String,
    onDismiss: () -> Unit
) {
    val context = LocalContext.current
    val vScroll = rememberScrollState()
    val hScroll = rememberScrollState()

    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false)
    ) {
        Scaffold(
            topBar = {
                TopAppBar(
                    title = {
                        Column {
                            Text("Engine Logs", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
                            Text(
                                item.fileName,
                                style = MaterialTheme.typography.bodySmall,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                                color = MaterialTheme.colorScheme.onSurfaceVariant
                            )
                        }
                    },
                    navigationIcon = {
                        IconButton(onClick = onDismiss) {
                            Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                        }
                    },
                    actions = {
                        IconButton(onClick = { copyLogsToClipboard(context, logContent) }) {
                            Icon(Icons.Default.ContentCopy, contentDescription = "Copy logs")
                        }
                        IconButton(onClick = onDismiss) {
                            Icon(Icons.Default.FullscreenExit, contentDescription = "Exit fullscreen")
                        }
                    },
                    colors = TopAppBarDefaults.topAppBarColors(
                        containerColor = MaterialTheme.colorScheme.surface
                    )
                )
            }
        ) { paddingValues ->
            Surface(
                color = Color(0xFF121212),
                modifier = Modifier
                    .fillMaxSize()
                    .padding(paddingValues)
            ) {
                SelectionContainer {
                    Box(
                        modifier = Modifier
                            .fillMaxSize()
                            .verticalScroll(vScroll)
                            .horizontalScroll(hScroll)
                            .padding(16.dp)
                    ) {
                        Text(
                            text = logContent.ifBlank { "No logs available for this transfer." },
                            fontFamily = FontFamily.Monospace,
                            fontSize = 11.sp,
                            color = Color(0xFFD4D4D4),
                            lineHeight = 16.sp
                        )
                    }
                }
            }
        }
    }
}

/**
 * Detailed Transfer Metrics Dialog with "Open Logs" action for History items.
 */
@Composable
fun TransferDetailsDialog(
    item: HistoryItem,
    onDismiss: () -> Unit,
    onOpen: (() -> Unit)?,
    onShare: (() -> Unit)?,
    onDelete: () -> Unit
) {
    val context = LocalContext.current
    var showLogs by remember { mutableStateOf(false) }
    var isFullScreen by remember { mutableStateOf(false) }
    val logContent = remember(item.id, showLogs) {
        if (showLogs) loadTransferLogs(context, item.id, item) else ""
    }

    if (isFullScreen) {
        TransferLogViewerFullScreenDialog(
            item = item,
            logContent = logContent,
            onDismiss = { isFullScreen = false }
        )
    }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = {
            if (showLogs) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text(
                        "Engine Logs",
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.Bold
                    )
                    Row {
                        IconButton(
                            onClick = { copyLogsToClipboard(context, logContent) },
                            modifier = Modifier.size(36.dp)
                        ) {
                            Icon(Icons.Default.ContentCopy, contentDescription = "Copy logs", modifier = Modifier.size(18.dp))
                        }
                        IconButton(
                            onClick = { isFullScreen = true },
                            modifier = Modifier.size(36.dp)
                        ) {
                            Icon(Icons.Default.Fullscreen, contentDescription = "Full Screen", modifier = Modifier.size(20.dp))
                        }
                    }
                }
            } else {
                Text(
                    "Transfer Details",
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.Bold
                )
            }
        },
        text = {
            if (showLogs) {
                Surface(
                    shape = RoundedCornerShape(8.dp),
                    color = Color(0xFF141414),
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(min = 200.dp, max = 340.dp)
                ) {
                    SelectionContainer {
                        Box(
                            modifier = Modifier
                                .fillMaxSize()
                                .verticalScroll(rememberScrollState())
                                .horizontalScroll(rememberScrollState())
                                .padding(10.dp)
                        ) {
                            Text(
                                text = logContent.ifBlank { "No logs recorded for this transfer." },
                                fontFamily = FontFamily.Monospace,
                                fontSize = 10.5.sp,
                                color = Color(0xFFE0E0E0),
                                lineHeight = 15.sp
                            )
                        }
                    }
                }
            } else {
                Column(
                    modifier = Modifier.fillMaxWidth(),
                    verticalArrangement = Arrangement.spacedBy(10.dp)
                ) {
                    Text(
                        item.fileName,
                        fontWeight = FontWeight.Bold,
                        style = MaterialTheme.typography.bodyLarge,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis
                    )

                    Surface(
                        shape = RoundedCornerShape(12.dp),
                        color = MaterialTheme.colorScheme.surfaceContainerLow,
                        modifier = Modifier.fillMaxWidth()
                    ) {
                        Column(
                            modifier = Modifier.padding(12.dp),
                            verticalArrangement = Arrangement.spacedBy(6.dp)
                        ) {
                            Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                                Text("Direction:", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                Text(if (item.isOutgoing) "Sent to PC" else "Received from PC", fontWeight = FontWeight.Bold, style = MaterialTheme.typography.bodySmall)
                            }
                            Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                                Text("Payload Size:", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                Text(item.formattedSize, fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
                            }
                            Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                                Text("Elapsed Time:", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                Text("${String.format("%.1f", item.durationMs / 1000.0)}s", fontWeight = FontWeight.Bold, style = MaterialTheme.typography.bodySmall)
                            }
                            Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                                Text("Average Speed:", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                Text(String.format("%.1f MB/s", item.avgSpeedMBps), fontWeight = FontWeight.Bold, color = MaterialTheme.colorScheme.primary, style = MaterialTheme.typography.bodySmall)
                            }
                            Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                                Text("Peak Throughput:", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                Text(String.format("%.1f MB/s", item.peakSpeedMBps), fontWeight = FontWeight.Bold, style = MaterialTheme.typography.bodySmall)
                            }
                            if (item.usbSpeedMBps > 0.01 || item.wifiSpeedMBps > 0.01) {
                                HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f))
                                Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                                    Text("USB / Wi-Fi Split:", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                    Text("${String.format("%.1f", item.usbSpeedMBps)} / ${String.format("%.1f", item.wifiSpeedMBps)} MB/s", fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
                                }
                            }
                        }
                    }

                    OutlinedButton(
                        onClick = { showLogs = true },
                        modifier = Modifier.fillMaxWidth(),
                        shape = RoundedCornerShape(8.dp)
                    ) {
                        Icon(Icons.Default.Terminal, contentDescription = null, modifier = Modifier.size(16.dp))
                        Spacer(modifier = Modifier.width(6.dp))
                        Text("View Engine Logs", fontSize = 12.sp)
                    }
                }
            }
        },
        confirmButton = {
            if (showLogs) {
                Box(
                    modifier = Modifier.fillMaxWidth(),
                    contentAlignment = Alignment.Center
                ) {
                    FilledTonalButton(
                        onClick = { showLogs = false },
                        shape = RoundedCornerShape(12.dp),
                        contentPadding = PaddingValues(horizontal = 24.dp, vertical = 10.dp)
                    ) {
                        Icon(
                            Icons.AutoMirrored.Filled.ArrowBack,
                            contentDescription = null,
                            modifier = Modifier.size(16.dp)
                        )
                        Spacer(modifier = Modifier.width(8.dp))
                        Text("Back to Details", fontWeight = FontWeight.SemiBold)
                    }
                }
            } else {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    if (onOpen != null) {
                        Button(onClick = onOpen) {
                            Text("Open")
                        }
                    }
                    if (onShare != null) {
                        FilledTonalButton(onClick = onShare) {
                            Text("Share")
                        }
                    }
                }
            }
        },
        dismissButton = {
            if (!showLogs) {
                TextButton(onClick = onDismiss) {
                    Text("Close")
                }
            }
        }
    )
}

/**
 * Floating Mini-Player banner when a transfer is active in the background.
 */
@Composable
fun TransferMiniBanner(
    progress: TransferProgressInfo?,
    visible: Boolean,
    onExpand: () -> Unit,
    modifier: Modifier = Modifier
) {
    AnimatedVisibility(
        visible = visible && progress != null && progress.status == TransferStatus.IN_PROGRESS,
        enter = slideInVertically(initialOffsetY = { it }) + fadeIn(),
        exit = slideOutVertically(targetOffsetY = { it }) + fadeOut(),
        modifier = modifier
    ) {
        if (progress != null) {
            Surface(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp, vertical = 6.dp)
                    .clip(RoundedCornerShape(16.dp))
                    .clickable(onClick = onExpand),
                shape = RoundedCornerShape(16.dp),
                color = MaterialTheme.colorScheme.primaryContainer,
                tonalElevation = 6.dp
            ) {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 14.dp, vertical = 10.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp)
                ) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                            modifier = Modifier.weight(1f)
                        ) {
                            Box(
                                modifier = Modifier
                                    .size(8.dp)
                                    .background(MaterialTheme.colorScheme.primary, CircleShape)
                            )
                            Text(
                                text = progress.fileName,
                                fontWeight = FontWeight.Bold,
                                fontSize = 13.sp,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                                color = MaterialTheme.colorScheme.onPrimaryContainer
                            )
                        }

                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(6.dp)
                        ) {
                            Text(
                                text = String.format("%.1f MB/s", progress.aggregateSpeedMBps),
                                fontSize = 13.sp,
                                fontWeight = FontWeight.Bold,
                                fontFamily = FontFamily.Monospace,
                                color = MaterialTheme.colorScheme.primary
                            )
                            Icon(
                                imageVector = Icons.AutoMirrored.Filled.OpenInNew,
                                contentDescription = null,
                                tint = MaterialTheme.colorScheme.onPrimaryContainer,
                                modifier = Modifier.size(16.dp)
                            )
                        }
                    }

                    LinearProgressIndicator(
                        progress = { (progress.percent / 100.0).toFloat().coerceIn(0f, 1f) },
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(4.dp)
                            .clip(RoundedCornerShape(2.dp)),
                        color = MaterialTheme.colorScheme.primary,
                        trackColor = MaterialTheme.colorScheme.onPrimaryContainer.copy(alpha = 0.2f)
                    )
                }
            }
        }
    }
}
