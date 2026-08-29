package com.turbotransfer.presentation.transfer

import android.widget.Toast
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.turbotransfer.UriUtils
import com.turbotransfer.domain.model.TransferStatus
import com.turbotransfer.presentation.components.*
import com.turbotransfer.presentation.theme.*

@Composable
fun TransferScreen(
    viewModel: TransferViewModel = hiltViewModel()
) {
    val uiState by viewModel.uiState.collectAsStateWithLifecycle()
    val context = LocalContext.current

    LaunchedEffect(uiState.userMessage) {
        uiState.userMessage?.let { msg ->
            Toast.makeText(context, msg, Toast.LENGTH_SHORT).show()
            viewModel.clearUserMessage()
        }
    }

    val progress = uiState.progress
    val activeSession = uiState.activeSession
    val lastCompletedItem = uiState.lastCompletedItem

    // Real-time speed history for telemetry waveform (max 30 points)
    val totalSpeedHistory = remember { mutableStateListOf<Float>() }
    val usbSpeedHistory = remember { mutableStateListOf<Float>() }
    val wifiSpeedHistory = remember { mutableStateListOf<Float>() }

    LaunchedEffect(progress?.aggregateSpeedMBps) {
        if (progress != null && progress.status == TransferStatus.IN_PROGRESS) {
            totalSpeedHistory.add(progress.aggregateSpeedMBps.toFloat())
            if (totalSpeedHistory.size > 30) totalSpeedHistory.removeAt(0)

            usbSpeedHistory.add(progress.usbSpeedMBps.toFloat())
            if (usbSpeedHistory.size > 30) usbSpeedHistory.removeAt(0)

            wifiSpeedHistory.add(progress.wifiSpeedMBps.toFloat())
            if (wifiSpeedHistory.size > 30) wifiSpeedHistory.removeAt(0)
        } else if (progress == null) {
            totalSpeedHistory.clear()
            usbSpeedHistory.clear()
            wifiSpeedHistory.clear()
        }
    }

    LazyColumn(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
        contentPadding = PaddingValues(top = 16.dp, bottom = 24.dp)
    ) {
        item {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Column {
                    Text(
                        "TRANSFER TELEMETRY",
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.Black,
                        letterSpacing = 1.sp,
                        color = CyberTextPrimary
                    )
                    Text(
                        "Real-time dual-link multiplexer HUD",
                        fontSize = 11.sp,
                        color = CyberTextMuted
                    )
                }

                if (progress != null && progress.status == TransferStatus.IN_PROGRESS) {
                    CyberBadge(
                        text = "LIVE LINK",
                        color = CyberMint,
                        pulsing = true
                    )
                } else if (lastCompletedItem != null) {
                    CyberBadge(
                        text = "COMPLETED",
                        color = CyberMint
                    )
                } else {
                    CyberBadge(
                        text = "STANDBY",
                        color = CyberTextMuted
                    )
                }
            }
        }

        // 1. Post-Transfer Completion Summary Card
        if (lastCompletedItem != null && (progress == null || progress.status != TransferStatus.IN_PROGRESS)) {
            val item = lastCompletedItem
            item {
                CyberCard(
                    modifier = Modifier.fillMaxWidth(),
                    borderColor = CyberMint,
                    borderGlow = true
                ) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            Surface(
                                shape = CircleShape,
                                color = CyberMint.copy(alpha = 0.15f),
                                border = BorderStroke(1.dp, CyberMint),
                                modifier = Modifier.size(32.dp)
                            ) {
                                Box(contentAlignment = Alignment.Center) {
                                    Icon(Icons.Default.Check, contentDescription = null, tint = CyberMint, modifier = Modifier.size(18.dp))
                                }
                            }
                            Text("Transmission Completed!", fontWeight = FontWeight.Black, color = CyberMint, fontSize = 15.sp)
                        }
                        IconButton(onClick = { viewModel.dismissCompleted() }, modifier = Modifier.size(24.dp)) {
                            Icon(Icons.Default.Close, contentDescription = "Close", tint = CyberTextMuted)
                        }
                    }

                    Spacer(modifier = Modifier.height(10.dp))

                    Text(
                        item.fileName,
                        fontWeight = FontWeight.Bold,
                        fontSize = 16.sp,
                        color = CyberTextPrimary,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis
                    )

                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween
                    ) {
                        Text("Size: ${item.formattedSize}", fontSize = 12.sp, color = CyberTextSecondary, fontFamily = FontFamily.Monospace)
                        Text("Elapsed: ${String.format("%.1fs", item.durationMs / 1000.0)}", fontSize = 12.sp, color = CyberTextSecondary, fontFamily = FontFamily.Monospace)
                    }

                    Spacer(modifier = Modifier.height(10.dp))
                    HorizontalDivider(color = CyberCardBorder.copy(alpha = 0.6f))
                    Spacer(modifier = Modifier.height(10.dp))

                    // Channel Statistics Grid
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween
                    ) {
                        Column {
                            Text("Combined Avg", fontSize = 10.sp, color = CyberTextMuted)
                            Text(String.format("%.1f MB/s", item.avgSpeedMBps), fontWeight = FontWeight.ExtraBold, color = CyberMint, fontSize = 14.sp, fontFamily = FontFamily.Monospace)
                            Text("Peak: ${String.format("%.1f", item.peakSpeedMBps)}", fontSize = 10.sp, color = CyberTextMuted, fontFamily = FontFamily.Monospace)
                        }
                        Column {
                            Text("USB Avg", fontSize = 10.sp, color = CyberTextMuted)
                            Text(String.format("%.1f MB/s", item.usbSpeedMBps), fontWeight = FontWeight.ExtraBold, color = CyberCyan, fontSize = 14.sp, fontFamily = FontFamily.Monospace)
                            Text("Peak: ${String.format("%.1f", item.peakUsbSpeedMBps)}", fontSize = 10.sp, color = CyberTextMuted, fontFamily = FontFamily.Monospace)
                        }
                        Column {
                            Text("5 GHz Wi-Fi Avg", fontSize = 10.sp, color = CyberTextMuted)
                            Text(String.format("%.1f MB/s", item.wifiSpeedMBps), fontWeight = FontWeight.ExtraBold, color = CyberPurple, fontSize = 14.sp, fontFamily = FontFamily.Monospace)
                            Text("Peak: ${String.format("%.1f", item.peakWifiSpeedMBps)}", fontSize = 10.sp, color = CyberTextMuted, fontFamily = FontFamily.Monospace)
                        }
                    }

                    if (!item.isOutgoing && item.filePath.isNotBlank()) {
                        Spacer(modifier = Modifier.height(14.dp))
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(10.dp)
                        ) {
                            Button(
                                onClick = { UriUtils.openFile(context, item.filePath) },
                                modifier = Modifier.weight(1f),
                                colors = ButtonDefaults.buttonColors(containerColor = CyberMint),
                                shape = RoundedCornerShape(10.dp)
                            ) {
                                Icon(Icons.Default.OpenInNew, contentDescription = null, modifier = Modifier.size(16.dp), tint = Color.Black)
                                Spacer(modifier = Modifier.width(6.dp))
                                Text("Open File", fontSize = 13.sp, fontWeight = FontWeight.Bold, color = Color.Black)
                            }
                            OutlinedButton(
                                onClick = { UriUtils.shareFile(context, item.filePath) },
                                modifier = Modifier.weight(1f),
                                border = BorderStroke(1.dp, CyberCyan.copy(alpha = 0.6f)),
                                shape = RoundedCornerShape(10.dp)
                            ) {
                                Icon(Icons.Default.Share, contentDescription = null, modifier = Modifier.size(16.dp), tint = CyberCyan)
                                Spacer(modifier = Modifier.width(6.dp))
                                Text("Share", fontSize = 13.sp, color = CyberCyan, fontWeight = FontWeight.Bold)
                            }
                        }
                    }
                }
            }
        }

        // 2. Active Transfer Dashboard
        if (progress != null && (progress.status == TransferStatus.IN_PROGRESS || progress.status == TransferStatus.PAUSED)) {
            val p = progress
            val totalSpeedMBps = p.aggregateSpeedMBps
            val usbSpeedMBps = p.usbSpeedMBps
            val wifiSpeedMBps = p.wifiSpeedMBps
            val isOutgoing = activeSession?.isOutgoing ?: true

            item {
                CyberCard(
                    modifier = Modifier.fillMaxWidth(),
                    borderColor = CyberCyan,
                    borderGlow = true
                ) {
                    // Direction Header & ETA
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        CyberBadge(
                            text = if (isOutgoing) "OUTBOUND -> PC" else "INBOUND <- PC",
                            color = if (isOutgoing) CyberCyan else CyberMint,
                            pulsing = p.status == TransferStatus.IN_PROGRESS
                        )

                        Text(
                            "ETA: ${UriUtils.formatEta(p.etaSeconds)}",
                            fontWeight = FontWeight.Bold,
                            fontSize = 12.sp,
                            fontFamily = FontFamily.Monospace,
                            color = CyberTextSecondary
                        )
                    }

                    Spacer(modifier = Modifier.height(10.dp))

                    // File Name
                    Text(
                        text = p.fileName,
                        fontWeight = FontWeight.ExtraBold,
                        fontSize = 17.sp,
                        color = CyberTextPrimary,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis
                    )

                    Spacer(modifier = Modifier.height(16.dp))

                    // Speedometer Ring HUD
                    Box(
                        modifier = Modifier.fillMaxWidth(),
                        contentAlignment = Alignment.Center
                    ) {
                        SpeedometerHUD(
                            speedMBps = totalSpeedMBps,
                            peakSpeedMBps = uiState.peakTotalSpeed,
                            progressPercent = p.percent,
                            etaSeconds = p.etaSeconds
                        )
                    }

                    Spacer(modifier = Modifier.height(16.dp))

                    // Progress Details
                    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween
                        ) {
                            Text(
                                "${UriUtils.formatFileSize(p.bytesTransferred)} / ${UriUtils.formatFileSize(p.fileSize)}",
                                fontSize = 12.sp,
                                fontFamily = FontFamily.Monospace,
                                color = CyberTextSecondary
                            )
                            Text(
                                String.format("%.1f%%", p.percent),
                                fontWeight = FontWeight.Bold,
                                fontFamily = FontFamily.Monospace,
                                color = CyberCyan,
                                fontSize = 13.sp
                            )
                        }
                        LinearProgressIndicator(
                            progress = { (p.percent / 100.0).toFloat().coerceIn(0f, 1f) },
                            modifier = Modifier
                                .fillMaxWidth()
                                .height(8.dp)
                                .clip(RoundedCornerShape(4.dp)),
                            color = CyberCyan,
                            trackColor = CyberCardBorder
                        )
                    }

                    Spacer(modifier = Modifier.height(16.dp))

                    // Real-Time Telemetry Waveform Speed Graph
                    Text(
                        "THROUGHPUT WAVEFORM",
                        fontSize = 10.sp,
                        fontWeight = FontWeight.Bold,
                        fontFamily = FontFamily.Monospace,
                        color = CyberTextMuted
                    )
                    Spacer(modifier = Modifier.height(6.dp))
                    SpeedWaveformGraph(
                        speedHistory = totalSpeedHistory,
                        usbHistory = usbSpeedHistory,
                        wifiHistory = wifiSpeedHistory
                    )

                    Spacer(modifier = Modifier.height(14.dp))

                    // Split Channel Breakdown Cards
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(10.dp)
                    ) {
                        // USB Channel
                        Surface(
                            modifier = Modifier.weight(1f),
                            shape = RoundedCornerShape(12.dp),
                            color = CyberSurface,
                            border = BorderStroke(1.dp, CyberBlue.copy(alpha = 0.5f))
                        ) {
                            Column(modifier = Modifier.padding(10.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                Row(
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.spacedBy(4.dp)
                                ) {
                                    Icon(Icons.Default.Usb, contentDescription = null, tint = CyberBlue, modifier = Modifier.size(16.dp))
                                    Text("USB Link", fontWeight = FontWeight.Bold, fontSize = 11.sp, color = CyberBlue)
                                }
                                Text(
                                    String.format("%.1f MB/s", usbSpeedMBps),
                                    fontWeight = FontWeight.ExtraBold,
                                    fontSize = 16.sp,
                                    fontFamily = FontFamily.Monospace,
                                    color = CyberBlue
                                )
                                Text(
                                    if (usbSpeedMBps > 0.01) "● Active Link" else "○ Standby",
                                    fontSize = 10.sp,
                                    fontFamily = FontFamily.Monospace,
                                    color = if (usbSpeedMBps > 0.01) CyberMint else CyberTextMuted
                                )
                            }
                        }

                        // 5 GHz Wi-Fi Channel
                        Surface(
                            modifier = Modifier.weight(1f),
                            shape = RoundedCornerShape(12.dp),
                            color = CyberSurface,
                            border = BorderStroke(1.dp, CyberPurple.copy(alpha = 0.5f))
                        ) {
                            Column(modifier = Modifier.padding(10.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                Row(
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.spacedBy(4.dp)
                                ) {
                                    Icon(Icons.Default.Wifi, contentDescription = null, tint = CyberPurple, modifier = Modifier.size(16.dp))
                                    Text("5 GHz Wi-Fi", fontWeight = FontWeight.Bold, fontSize = 11.sp, color = CyberPurple)
                                }
                                Text(
                                    String.format("%.1f MB/s", wifiSpeedMBps),
                                    fontWeight = FontWeight.ExtraBold,
                                    fontSize = 16.sp,
                                    fontFamily = FontFamily.Monospace,
                                    color = CyberPurple
                                )
                                Text(
                                    if (wifiSpeedMBps > 0.01) "● Active Link" else "○ Standby",
                                    fontSize = 10.sp,
                                    fontFamily = FontFamily.Monospace,
                                    color = if (wifiSpeedMBps > 0.01) CyberMint else CyberTextMuted
                                )
                            }
                        }
                    }

                    Spacer(modifier = Modifier.height(16.dp))

                    // Transfer Control Action Buttons
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(10.dp)
                    ) {
                        val id = p.transferId
                        if (p.status == TransferStatus.IN_PROGRESS) {
                            OutlinedButton(
                                onClick = { viewModel.pauseTransfer(id) },
                                modifier = Modifier.weight(1f),
                                border = BorderStroke(1.dp, CyberCyan.copy(alpha = 0.6f)),
                                shape = RoundedCornerShape(12.dp)
                            ) {
                                Icon(Icons.Default.Pause, contentDescription = null, modifier = Modifier.size(16.dp), tint = CyberCyan)
                                Spacer(modifier = Modifier.width(6.dp))
                                Text("Pause", color = CyberCyan, fontWeight = FontWeight.Bold)
                            }
                        } else {
                            Button(
                                onClick = { viewModel.resumeTransfer(id) },
                                modifier = Modifier.weight(1f),
                                colors = ButtonDefaults.buttonColors(containerColor = CyberCyan),
                                shape = RoundedCornerShape(12.dp)
                            ) {
                                Icon(Icons.Default.PlayArrow, contentDescription = null, modifier = Modifier.size(16.dp), tint = Color.Black)
                                Spacer(modifier = Modifier.width(6.dp))
                                Text("Resume", color = Color.Black, fontWeight = FontWeight.Bold)
                            }
                        }

                        Button(
                            onClick = { viewModel.cancelTransfer(id) },
                            modifier = Modifier.weight(1f),
                            colors = ButtonDefaults.buttonColors(containerColor = CyberRed),
                            shape = RoundedCornerShape(12.dp)
                        ) {
                            Icon(Icons.Default.Close, contentDescription = null, modifier = Modifier.size(16.dp), tint = Color.White)
                            Spacer(modifier = Modifier.width(6.dp))
                            Text("Abort", color = Color.White, fontWeight = FontWeight.Bold)
                        }
                    }
                }
            }
        }

        // 3. Idle Standby State
        if ((progress == null || (progress.status != TransferStatus.IN_PROGRESS && progress.status != TransferStatus.PAUSED)) && lastCompletedItem == null) {
            item {
                CyberCard(modifier = Modifier.fillMaxWidth()) {
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(vertical = 32.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(14.dp)
                    ) {
                        Surface(
                            shape = CircleShape,
                            color = CyberCyan.copy(alpha = 0.1f),
                            border = BorderStroke(1.dp, CyberCyan.copy(alpha = 0.3f)),
                            modifier = Modifier.size(64.dp)
                        ) {
                            Box(contentAlignment = Alignment.Center) {
                                Icon(
                                    Icons.Default.Speed,
                                    contentDescription = null,
                                    tint = CyberCyan,
                                    modifier = Modifier.size(32.dp)
                                )
                            }
                        }
                        Text(
                            "Telemetry Link Standby",
                            fontWeight = FontWeight.Bold,
                            fontSize = 16.sp,
                            color = CyberTextPrimary
                        )
                        Text(
                            "Select files in the Send tab or start Receive mode to accept incoming transmissions.",
                            fontSize = 12.sp,
                            color = CyberTextMuted,
                            textAlign = TextAlign.Center,
                            modifier = Modifier.padding(horizontal = 24.dp)
                        )
                    }
                }
            }
        }
    }
}
