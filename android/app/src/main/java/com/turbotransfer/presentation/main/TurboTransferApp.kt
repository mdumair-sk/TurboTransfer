package com.turbotransfer.presentation.main

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
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
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.turbotransfer.domain.model.TransferStatus
import com.turbotransfer.presentation.components.TransferCapsuleMiniPlayer
import com.turbotransfer.presentation.history.HistoryScreen
import com.turbotransfer.presentation.receive.ReceiveScreen
import com.turbotransfer.presentation.send.SendScreen
import com.turbotransfer.presentation.settings.SettingsScreen
import com.turbotransfer.presentation.theme.*
import com.turbotransfer.presentation.transfer.TransferScreen

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TurboTransferApp(
    viewModel: MainViewModel = hiltViewModel()
) {
    val selectedTab by viewModel.selectedTab.collectAsStateWithLifecycle()
    val activeSession by viewModel.activeSession.collectAsStateWithLifecycle()
    val currentProgress by viewModel.currentProgress.collectAsStateWithLifecycle()

    val context = LocalContext.current

    // Request permissions needed for Wi-Fi Direct and Storage
    val permissionsLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.RequestMultiplePermissions()
    ) { permissions ->
        val allGranted = permissions.values.all { it }
        if (!allGranted) {
            Toast.makeText(context, "Permissions required for maximum throughput", Toast.LENGTH_SHORT).show()
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

    Scaffold(
        containerColor = CyberBackground,
        topBar = {
            TopAppBar(
                title = {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(10.dp)
                    ) {
                        Surface(
                            shape = RoundedCornerShape(10.dp),
                            color = CyberCyan.copy(alpha = 0.15f),
                            border = BorderStroke(1.dp, CyberCyan.copy(alpha = 0.5f)),
                            modifier = Modifier.size(36.dp)
                        ) {
                            Box(contentAlignment = Alignment.Center) {
                                Icon(
                                    imageVector = Icons.Default.Bolt,
                                    contentDescription = null,
                                    tint = CyberCyan,
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
                                    "TURBOTRANSFER",
                                    fontWeight = FontWeight.Black,
                                    fontSize = 16.sp,
                                    letterSpacing = 1.sp,
                                    color = CyberTextPrimary
                                )
                                Surface(
                                    shape = RoundedCornerShape(4.dp),
                                    color = CyberMint.copy(alpha = 0.15f)
                                ) {
                                    Text(
                                        "v2.0",
                                        modifier = Modifier.padding(horizontal = 4.dp, vertical = 1.dp),
                                        fontSize = 9.sp,
                                        fontWeight = FontWeight.Bold,
                                        fontFamily = FontFamily.Monospace,
                                        color = CyberMint
                                    )
                                }
                            }
                            Text(
                                "Dual-Channel Multipath Link (USB + 5GHz)",
                                fontSize = 10.sp,
                                fontFamily = FontFamily.Monospace,
                                color = CyberTextMuted
                            )
                        }
                    }
                },
                actions = {
                    if (activeSession != null && currentProgress != null && currentProgress?.status == TransferStatus.IN_PROGRESS) {
                        val mbps = currentProgress!!.aggregateSpeedMBps
                        Surface(
                            color = CyberMint.copy(alpha = 0.15f),
                            shape = RoundedCornerShape(16.dp),
                            border = BorderStroke(1.dp, CyberMint.copy(alpha = 0.6f)),
                            modifier = Modifier
                                .padding(end = 12.dp)
                                .clickable { viewModel.selectTab(2) }
                        ) {
                            Row(
                                modifier = Modifier.padding(horizontal = 10.dp, vertical = 5.dp),
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(6.dp)
                            ) {
                                Box(
                                    modifier = Modifier
                                        .size(8.dp)
                                        .background(CyberMint, CircleShape)
                                )
                                Text(
                                    String.format("%.1f MB/s", mbps),
                                    fontSize = 12.sp,
                                    fontWeight = FontWeight.Bold,
                                    fontFamily = FontFamily.Monospace,
                                    color = CyberMint
                                )
                            }
                        }
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = CyberSurface,
                    titleContentColor = CyberTextPrimary
                )
            )
        },
        bottomBar = {
            Column(modifier = Modifier.fillMaxWidth()) {
                // Persistent Floating Mini Player when active transfer exists and not on Transfer tab
                TransferCapsuleMiniPlayer(
                    progress = currentProgress,
                    visible = selectedTab != 2,
                    onExpand = { viewModel.selectTab(2) }
                )

                // Cyber Bottom Navigation Bar
                NavigationBar(
                    containerColor = CyberSurface,
                    tonalElevation = 12.dp,
                    modifier = Modifier.shadow(8.dp, spotColor = CyberCyan.copy(alpha = 0.15f))
                ) {
                    val tabs = listOf(
                        Triple(0, "Send", Icons.Default.Send),
                        Triple(1, "Receive", Icons.Default.Download),
                        Triple(2, "Monitor", Icons.Default.Speed),
                        Triple(3, "History", Icons.Default.History),
                        Triple(4, "Settings", Icons.Default.Settings)
                    )

                    tabs.forEach { (index, label, icon) ->
                        val isSelected = selectedTab == index
                        NavigationBarItem(
                            selected = isSelected,
                            onClick = { viewModel.selectTab(index) },
                            icon = {
                                if (index == 2 && activeSession != null && currentProgress?.status == TransferStatus.IN_PROGRESS) {
                                    BadgedBox(
                                        badge = {
                                            Badge(containerColor = CyberMint) {
                                                Text("●", fontSize = 8.sp)
                                            }
                                        }
                                    ) {
                                        Icon(
                                            imageVector = icon,
                                            contentDescription = label,
                                            tint = if (isSelected) CyberCyan else CyberTextMuted
                                        )
                                    }
                                } else {
                                    Icon(
                                        imageVector = icon,
                                        contentDescription = label,
                                        tint = if (isSelected) CyberCyan else CyberTextMuted
                                    )
                                }
                            },
                            label = {
                                Text(
                                    label,
                                    fontSize = 11.sp,
                                    fontWeight = if (isSelected) FontWeight.Bold else FontWeight.Normal,
                                    color = if (isSelected) CyberCyan else CyberTextMuted
                                )
                            },
                            colors = NavigationBarItemDefaults.colors(
                                indicatorColor = CyberCyan.copy(alpha = 0.15f),
                                selectedIconColor = CyberCyan,
                                unselectedIconColor = CyberTextMuted,
                                selectedTextColor = CyberCyan,
                                unselectedTextColor = CyberTextMuted
                            )
                        )
                    }
                }
            }
        }
    ) { innerPadding ->
        Box(
            modifier = Modifier
                .fillMaxSize()
                .padding(innerPadding)
                .background(CyberBackground)
        ) {
            AnimatedContent(
                targetState = selectedTab,
                transitionSpec = {
                    fadeIn(animationSpec = tween(220)) togetherWith fadeOut(animationSpec = tween(220))
                },
                label = "tabTransition"
            ) { tabIndex ->
                when (tabIndex) {
                    0 -> SendScreen(onNavigateToTransfer = { viewModel.selectTab(2) })
                    1 -> ReceiveScreen(onNavigateToTransfer = { viewModel.selectTab(2) })
                    2 -> TransferScreen()
                    3 -> HistoryScreen()
                    4 -> SettingsScreen()
                }
            }
        }
    }
}
