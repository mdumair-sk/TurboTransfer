package com.turbotransfer.presentation.settings

import android.os.Build
import android.widget.Toast
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle

@Composable
fun SettingsScreen(
    viewModel: SettingsViewModel = hiltViewModel()
) {
    val uiState by viewModel.uiState.collectAsStateWithLifecycle()
    val context = LocalContext.current

    LaunchedEffect(uiState.userMessage) {
        uiState.userMessage?.let { msg ->
            Toast.makeText(context, msg, Toast.LENGTH_SHORT).show()
            viewModel.clearUserMessage()
        }
    }

    val spikeState = uiState.spikeState

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
                        value = uiState.deviceName,
                        onValueChange = { viewModel.setDeviceName(it) },
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
                        Text(uiState.usbLabel, fontSize = 12.sp, fontWeight = FontWeight.Bold, color = Color(0xFF1565C0))
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
                        Switch(
                            checked = uiState.prefer5Ghz,
                            onCheckedChange = { viewModel.setPrefer5Ghz(it) }
                        )
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
                        Switch(
                            checked = uiState.autoWakeLock,
                            onCheckedChange = { viewModel.setAutoWakeLock(it) }
                        )
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
                            .clickable { viewModel.toggleShowDiagnostics() },
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            Icon(Icons.Default.BugReport, contentDescription = null, tint = MaterialTheme.colorScheme.primary)
                            Text("Developer Diagnostics & M7a Tools", fontWeight = FontWeight.Bold, fontSize = 14.sp)
                        }
                        Icon(
                            imageVector = if (uiState.showDiagnostics) Icons.Default.ExpandLess else Icons.Default.ExpandMore,
                            contentDescription = null
                        )
                    }

                    if (uiState.showDiagnostics) {
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
                                onClick = { viewModel.createP2pGroup() },
                                modifier = Modifier.weight(1f),
                                enabled = !spikeState.isGroupOwner,
                                shape = RoundedCornerShape(8.dp)
                            ) {
                                Text("P2P Group", fontSize = 12.sp)
                            }
                            Button(
                                onClick = { viewModel.removeP2pGroup() },
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
