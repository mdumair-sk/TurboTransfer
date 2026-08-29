package com.turbotransfer.presentation.history

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
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
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.turbotransfer.UriUtils
import com.turbotransfer.presentation.components.CyberBadge
import com.turbotransfer.presentation.components.CyberCard
import com.turbotransfer.presentation.theme.*
import java.io.File

@Composable
fun HistoryScreen(
    viewModel: HistoryViewModel = hiltViewModel()
) {
    val uiState by viewModel.uiState.collectAsStateWithLifecycle()
    val context = LocalContext.current

    var searchQuery by remember { mutableStateOf("") }
    var selectedFilterIndex by remember { mutableIntStateOf(0) } // 0: All, 1: Sent, 2: Received

    // Compute Summary Stats
    val totalTransferredBytes = remember(uiState.historyList) {
        uiState.historyList.sumOf { it.fileSize }
    }
    val allTimePeakSpeed = remember(uiState.historyList) {
        uiState.historyList.maxOfOrNull { maxOf(it.peakSpeedMBps, it.avgSpeedMBps) } ?: 0.0
    }

    // Filter list
    val filteredList = remember(uiState.historyList, searchQuery, selectedFilterIndex) {
        uiState.historyList.filter { item ->
            val matchesFilter = when (selectedFilterIndex) {
                1 -> item.isOutgoing
                2 -> !item.isOutgoing
                else -> true
            }
            val matchesSearch = if (searchQuery.isBlank()) true else {
                item.fileName.contains(searchQuery.trim(), ignoreCase = true)
            }
            matchesFilter && matchesSearch
        }
    }

    LazyColumn(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
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
                        "ACTIVITY LOG",
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.Black,
                        letterSpacing = 1.sp,
                        color = CyberTextPrimary
                    )
                    Text(
                        "Completed transmission ledger & statistics",
                        fontSize = 11.sp,
                        color = CyberTextMuted
                    )
                }
                if (uiState.historyList.isNotEmpty()) {
                    TextButton(
                        onClick = { viewModel.setShowClearDialog(true) },
                        contentPadding = PaddingValues(horizontal = 8.dp)
                    ) {
                        Text("Clear All", color = CyberRed, fontSize = 12.sp, fontWeight = FontWeight.Bold)
                    }
                }
            }
        }

        // 2. Statistics Summary Header Banner
        if (uiState.historyList.isNotEmpty()) {
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
                            Text("TOTAL DATA TRANSFERRED", fontSize = 10.sp, fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace, color = CyberTextMuted)
                            Text(
                                UriUtils.formatFileSize(totalTransferredBytes),
                                fontSize = 20.sp,
                                fontWeight = FontWeight.ExtraBold,
                                fontFamily = FontFamily.Monospace,
                                color = CyberCyan
                            )
                        }

                        Column(horizontalAlignment = Alignment.End) {
                            Text("PEAK SPEED RECORD", fontSize = 10.sp, fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace, color = CyberTextMuted)
                            Text(
                                String.format("%.1f MB/s", allTimePeakSpeed),
                                fontSize = 18.sp,
                                fontWeight = FontWeight.ExtraBold,
                                fontFamily = FontFamily.Monospace,
                                color = CyberMint
                            )
                        }
                    }

                    Spacer(modifier = Modifier.height(8.dp))
                    HorizontalDivider(color = CyberCardBorder.copy(alpha = 0.6f))
                    Spacer(modifier = Modifier.height(8.dp))

                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween
                    ) {
                        Text(
                            "Total Completed Transfers: ${uiState.historyList.size}",
                            fontSize = 11.sp,
                            color = CyberTextSecondary
                        )
                        Text(
                            "Snapdragon 8 Elite Multipath",
                            fontSize = 11.sp,
                            fontFamily = FontFamily.Monospace,
                            color = CyberPurple
                        )
                    }
                }
            }

            // 3. Search & Filter Bar
            item {
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    OutlinedTextField(
                        value = searchQuery,
                        onValueChange = { searchQuery = it },
                        placeholder = { Text("Search transfers by filename...", fontSize = 12.sp, color = CyberTextMuted) },
                        leadingIcon = { Icon(Icons.Default.Search, contentDescription = null, tint = CyberCyan, modifier = Modifier.size(18.dp)) },
                        trailingIcon = {
                            if (searchQuery.isNotEmpty()) {
                                IconButton(onClick = { searchQuery = "" }, modifier = Modifier.size(20.dp)) {
                                    Icon(Icons.Default.Close, contentDescription = "Clear", tint = CyberTextMuted, modifier = Modifier.size(14.dp))
                                }
                            }
                        },
                        singleLine = true,
                        colors = OutlinedTextFieldDefaults.colors(
                            focusedBorderColor = CyberCyan,
                            unfocusedBorderColor = CyberCardBorder,
                            focusedTextColor = CyberTextPrimary,
                            unfocusedTextColor = CyberTextPrimary,
                            focusedContainerColor = CyberSurface,
                            unfocusedContainerColor = CyberSurface
                        ),
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(48.dp),
                        shape = RoundedCornerShape(12.dp)
                    )

                    // Filter Chips
                    LazyRow(
                        horizontalArrangement = Arrangement.spacedBy(8.dp)
                    ) {
                        val filters = listOf("All Transfers", "Sent Only", "Received Only")
                        items(filters.size) { idx ->
                            val isSelected = selectedFilterIndex == idx
                            Surface(
                                shape = RoundedCornerShape(10.dp),
                                color = if (isSelected) CyberCyan.copy(alpha = 0.15f) else CyberSurface,
                                border = BorderStroke(1.dp, if (isSelected) CyberCyan else CyberCardBorder),
                                modifier = Modifier.clickable { selectedFilterIndex = idx }
                            ) {
                                Text(
                                    text = filters[idx],
                                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 6.dp),
                                    fontSize = 11.sp,
                                    fontWeight = if (isSelected) FontWeight.Bold else FontWeight.Normal,
                                    color = if (isSelected) CyberCyan else CyberTextSecondary
                                )
                            }
                        }
                    }
                }
            }
        }

        // 4. Empty State
        if (uiState.historyList.isEmpty()) {
            item {
                CyberCard(modifier = Modifier.fillMaxWidth()) {
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(vertical = 40.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(12.dp)
                    ) {
                        Surface(
                            shape = CircleShape,
                            color = CyberCyan.copy(alpha = 0.1f),
                            border = BorderStroke(1.dp, CyberCyan.copy(alpha = 0.3f)),
                            modifier = Modifier.size(56.dp)
                        ) {
                            Box(contentAlignment = Alignment.Center) {
                                Icon(
                                    Icons.Default.History,
                                    contentDescription = null,
                                    tint = CyberCyan,
                                    modifier = Modifier.size(28.dp)
                                )
                            }
                        }
                        Text(
                            "No Transfer History Yet",
                            fontWeight = FontWeight.Bold,
                            fontSize = 15.sp,
                            color = CyberTextPrimary
                        )
                        Text(
                            "Completed and verified transfers will be logged here with full telemetry breakdowns.",
                            fontSize = 11.sp,
                            color = CyberTextMuted,
                            textAlign = androidx.compose.ui.text.style.TextAlign.Center,
                            modifier = Modifier.padding(horizontal = 24.dp)
                        )
                    }
                }
            }
        } else if (filteredList.isEmpty()) {
            item {
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(vertical = 32.dp),
                    contentAlignment = Alignment.Center
                ) {
                    Text("No records match current filter", fontSize = 12.sp, color = CyberTextMuted)
                }
            }
        } else {
            // 5. Transfer Items List
            items(filteredList, key = { it.id }) { item ->
                CyberCard(modifier = Modifier.fillMaxWidth()) {
                    // Header Row: Direction & Date
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        CyberBadge(
                            text = if (item.isOutgoing) "SENT" else "RECEIVED",
                            color = if (item.isOutgoing) CyberCyan else CyberMint,
                            icon = if (item.isOutgoing) Icons.Default.ArrowUpward else Icons.Default.ArrowDownward
                        )

                        Text(
                            item.formattedDate,
                            fontSize = 11.sp,
                            fontFamily = FontFamily.Monospace,
                            color = CyberTextMuted
                        )
                    }

                    Spacer(modifier = Modifier.height(8.dp))

                    // File Name
                    Text(
                        text = item.fileName,
                        fontWeight = FontWeight.Bold,
                        fontSize = 14.sp,
                        color = CyberTextPrimary,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis
                    )

                    Spacer(modifier = Modifier.height(6.dp))

                    // Metrics Row
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Text(
                            item.formattedSize,
                            fontSize = 12.sp,
                            fontFamily = FontFamily.Monospace,
                            color = CyberTextSecondary
                        )

                        if (item.avgSpeedMBps > 0.0) {
                            Text(
                                String.format("%.1f MB/s avg", item.avgSpeedMBps),
                                fontSize = 12.sp,
                                fontWeight = FontWeight.Bold,
                                fontFamily = FontFamily.Monospace,
                                color = CyberMint
                            )
                        }
                    }

                    Spacer(modifier = Modifier.height(8.dp))
                    HorizontalDivider(color = CyberCardBorder.copy(alpha = 0.5f))
                    Spacer(modifier = Modifier.height(6.dp))

                    // Actions Row
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                            if (!item.isOutgoing && item.filePath.isNotBlank() && File(item.filePath).exists()) {
                                TextButton(
                                    onClick = { UriUtils.openFile(context, item.filePath) },
                                    contentPadding = PaddingValues(horizontal = 8.dp, vertical = 2.dp)
                                ) {
                                    Icon(Icons.Default.OpenInNew, contentDescription = null, modifier = Modifier.size(14.dp), tint = CyberCyan)
                                    Spacer(modifier = Modifier.width(4.dp))
                                    Text("Open", fontSize = 11.sp, color = CyberCyan, fontWeight = FontWeight.Bold)
                                }
                                TextButton(
                                    onClick = { UriUtils.shareFile(context, item.filePath) },
                                    contentPadding = PaddingValues(horizontal = 8.dp, vertical = 2.dp)
                                ) {
                                    Icon(Icons.Default.Share, contentDescription = null, modifier = Modifier.size(14.dp), tint = CyberTextSecondary)
                                    Spacer(modifier = Modifier.width(4.dp))
                                    Text("Share", fontSize = 11.sp, color = CyberTextSecondary)
                                }
                            }
                        }

                        IconButton(
                            onClick = { viewModel.deleteRecord(item.id) },
                            modifier = Modifier.size(26.dp)
                        ) {
                            Icon(Icons.Default.DeleteOutline, contentDescription = "Delete", tint = CyberTextMuted, modifier = Modifier.size(16.dp))
                        }
                    }
                }
            }
        }
    }

    if (uiState.showClearDialog) {
        AlertDialog(
            onDismissRequest = { viewModel.setShowClearDialog(false) },
            containerColor = CyberSurface,
            title = {
                Text("Clear Activity Ledger", fontWeight = FontWeight.Bold, color = CyberTextPrimary)
            },
            text = {
                Text(
                    "Are you sure you want to permanently clear all completed transfer history records? Physical files on storage will not be deleted.",
                    color = CyberTextSecondary,
                    fontSize = 13.sp
                )
            },
            confirmButton = {
                Button(
                    onClick = { viewModel.clearHistory() },
                    colors = ButtonDefaults.buttonColors(containerColor = CyberRed)
                ) {
                    Text("Clear All Records", color = Color.White, fontWeight = FontWeight.Bold)
                }
            },
            dismissButton = {
                TextButton(onClick = { viewModel.setShowClearDialog(false) }) {
                    Text("Cancel", color = CyberTextSecondary)
                }
            }
        )
    }
}
