package com.turbotransfer.presentation.settings

import android.os.Build
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.turbotransfer.presentation.components.CyberBadge
import com.turbotransfer.presentation.components.CyberCard
import com.turbotransfer.presentation.theme.*

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

    LazyColumn(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
        contentPadding = PaddingValues(top = 16.dp, bottom = 24.dp)
    ) {
        // 1. Header
        item {
            Column {
                Text(
                    "SYSTEM CONFIGURATION",
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.Black,
                    letterSpacing = 1.sp,
                    color = CyberTextPrimary
                )
                Text(
                    "Hardware tuning, network parameters, and core engine",
                    fontSize = 11.sp,
                    color = CyberTextMuted
                )
            }
        }

        // 2. Device Identity & Hardware Specs Card
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
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp)
                    ) {
                        Icon(Icons.Default.PhoneAndroid, contentDescription = null, tint = CyberCyan, modifier = Modifier.size(20.dp))
                        Text("DEVICE & HARDWARE SPECS", fontWeight = FontWeight.Bold, fontSize = 12.sp, fontFamily = FontFamily.Monospace, color = CyberCyan)
                    }
                    CyberBadge(
                        text = Build.SUPPORTED_ABIS.firstOrNull() ?: "arm64-v8a",
                        color = CyberMint
                    )
                }

                Spacer(modifier = Modifier.height(12.dp))

                OutlinedTextField(
                    value = uiState.deviceName,
                    onValueChange = { viewModel.setDeviceName(it) },
                    label = { Text("Broadcast Device Name", fontSize = 11.sp) },
                    singleLine = true,
                    colors = OutlinedTextFieldDefaults.colors(
                        focusedBorderColor = CyberCyan,
                        unfocusedBorderColor = CyberCardBorder,
                        focusedTextColor = CyberTextPrimary,
                        unfocusedTextColor = CyberTextPrimary,
                        focusedContainerColor = CyberSurface,
                        unfocusedContainerColor = CyberSurface
                    ),
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(10.dp)
                )

                Spacer(modifier = Modifier.height(12.dp))
                HorizontalDivider(color = CyberCardBorder.copy(alpha = 0.6f))
                Spacer(modifier = Modifier.height(10.dp))

                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween
                ) {
                    Text("Manufacturer / Model:", fontSize = 11.sp, color = CyberTextSecondary)
                    Text("${Build.MANUFACTURER.uppercase()} ${Build.MODEL}", fontSize = 11.sp, fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace, color = CyberTextPrimary)
                }

                Spacer(modifier = Modifier.height(4.dp))

                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween
                ) {
                    Text("Chipset Platform:", fontSize = 11.sp, color = CyberTextSecondary)
                    Text(
                        "${Build.HARDWARE} / Snapdragon 8 Elite Ready",
                        fontSize = 11.sp,
                        fontWeight = FontWeight.Bold,
                        fontFamily = FontFamily.Monospace,
                        color = CyberMint
                    )
                }
            }
        }

        // 3. Wireless Transfer & Channel Bonding Options
        item {
            CyberCard(modifier = Modifier.fillMaxWidth()) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    Icon(Icons.Default.Tune, contentDescription = null, tint = CyberPurple, modifier = Modifier.size(20.dp))
                    Text("NETWORK & MULTIPATH TUNING", fontWeight = FontWeight.Bold, fontSize = 12.sp, fontFamily = FontFamily.Monospace, color = CyberPurple)
                }

                Spacer(modifier = Modifier.height(14.dp))

                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Column(modifier = Modifier.weight(1f)) {
                        Text("Enforce 5 GHz Direct Band", fontWeight = FontWeight.Bold, fontSize = 13.sp, color = CyberTextPrimary)
                        Text("Forces 802.11ac/ax 5GHz hotspot channel for line-rate transmission speed", fontSize = 11.sp, color = CyberTextMuted)
                    }
                    Switch(
                        checked = uiState.prefer5Ghz,
                        onCheckedChange = { viewModel.setPrefer5Ghz(it) },
                        colors = SwitchDefaults.colors(
                            checkedThumbColor = Color.Black,
                            checkedTrackColor = CyberPurple,
                            uncheckedThumbColor = CyberTextMuted,
                            uncheckedTrackColor = CyberSurfaceVariant
                        )
                    )
                }

                Spacer(modifier = Modifier.height(10.dp))
                HorizontalDivider(color = CyberCardBorder.copy(alpha = 0.5f))
                Spacer(modifier = Modifier.height(10.dp))

                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Column(modifier = Modifier.weight(1f)) {
                        Text("High-Performance WakeLock", fontWeight = FontWeight.Bold, fontSize = 13.sp, color = CyberTextPrimary)
                        Text("Suppresses Android Wi-Fi power-save polling and CPU core throttling during transfers", fontSize = 11.sp, color = CyberTextMuted)
                    }
                    Switch(
                        checked = uiState.autoWakeLock,
                        onCheckedChange = { viewModel.setAutoWakeLock(it) },
                        colors = SwitchDefaults.colors(
                            checkedThumbColor = Color.Black,
                            checkedTrackColor = CyberCyan,
                            uncheckedThumbColor = CyberTextMuted,
                            uncheckedTrackColor = CyberSurfaceVariant
                        )
                    )
                }
            }
        }

        // 4. Core Native Engine Status Card
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
                        Icon(Icons.Default.Memory, contentDescription = null, tint = CyberMint, modifier = Modifier.size(20.dp))
                        Text("CORE RUST ENGINE", fontWeight = FontWeight.Bold, fontSize = 12.sp, fontFamily = FontFamily.Monospace, color = CyberMint)
                    }
                    CyberBadge(
                        text = "TOKIO ASYNC",
                        color = CyberMint
                    )
                }

                Spacer(modifier = Modifier.height(12.dp))

                Surface(
                    shape = RoundedCornerShape(10.dp),
                    color = CyberBackground,
                    border = BorderStroke(1.dp, CyberCardBorder),
                    modifier = Modifier.fillMaxWidth()
                ) {
                    Column(
                        modifier = Modifier.padding(12.dp),
                        verticalArrangement = Arrangement.spacedBy(6.dp)
                    ) {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween
                        ) {
                            Text("Engine Library:", fontSize = 11.sp, color = CyberTextMuted)
                            Text("libturbotransfer_core.so", fontSize = 11.sp, fontFamily = FontFamily.Monospace, color = CyberCyan)
                        }
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween
                        ) {
                            Text("Multipath Protocol:", fontSize = 11.sp, color = CyberTextMuted)
                            Text("Framed v1.0 (Zero-Copy Ring)", fontSize = 11.sp, fontFamily = FontFamily.Monospace, color = CyberMint)
                        }
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween
                        ) {
                            Text("ADB USB Forwarding:", fontSize = 11.sp, color = CyberTextMuted)
                            Text("127.0.0.1:9876 -> Android:9876", fontSize = 11.sp, fontFamily = FontFamily.Monospace, color = CyberTextSecondary)
                        }
                    }
                }
            }
        }
    }
}
