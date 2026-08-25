package com.turbotransfer.presentation.transfer

import android.widget.Toast
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.turbotransfer.UriUtils
import com.turbotransfer.domain.model.TransferStatus

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
        if (lastCompletedItem != null && (progress == null || progress.status != TransferStatus.IN_PROGRESS)) {
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
                            IconButton(onClick = { viewModel.dismissCompleted() }, modifier = Modifier.size(24.dp)) {
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
        if (progress != null && (progress.status == TransferStatus.IN_PROGRESS || progress.status == TransferStatus.PAUSED)) {
            val p = progress
            val totalSpeedMBps = p.aggregateSpeedMBps
            val usbSpeedMBps = p.usbSpeedMBps
            val wifiSpeedMBps = p.wifiSpeedMBps
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
                                    "${UriUtils.formatFileSize(p.bytesTransferred)} / ${UriUtils.formatFileSize(p.fileSize)}",
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
                            if (p.status == TransferStatus.IN_PROGRESS) {
                                OutlinedButton(
                                    onClick = { viewModel.pauseTransfer(id) },
                                    modifier = Modifier.weight(1f),
                                    shape = RoundedCornerShape(10.dp)
                                ) {
                                    Icon(Icons.Default.Pause, contentDescription = null, modifier = Modifier.size(16.dp))
                                    Spacer(modifier = Modifier.width(4.dp))
                                    Text("Pause")
                                }
                            } else {
                                Button(
                                    onClick = { viewModel.resumeTransfer(id) },
                                    modifier = Modifier.weight(1f),
                                    shape = RoundedCornerShape(10.dp)
                                ) {
                                    Icon(Icons.Default.PlayArrow, contentDescription = null, modifier = Modifier.size(16.dp))
                                    Spacer(modifier = Modifier.width(4.dp))
                                    Text("Resume")
                                }
                            }

                            Button(
                                onClick = { viewModel.cancelTransfer(id) },
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

        // Idle State Placeholder
        if ((progress == null || (progress.status != TransferStatus.IN_PROGRESS && progress.status != TransferStatus.PAUSED)) && lastCompletedItem == null) {
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
