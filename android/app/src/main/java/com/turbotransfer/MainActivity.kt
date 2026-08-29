package com.turbotransfer

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.Bundle
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.viewModels
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.ui.Modifier
import androidx.core.content.ContextCompat
import com.turbotransfer.presentation.main.MainViewModel
import com.turbotransfer.presentation.main.TurboTransferApp
import com.turbotransfer.presentation.theme.TurboTransferTheme
import dagger.hilt.android.AndroidEntryPoint

import androidx.activity.enableEdgeToEdge

@AndroidEntryPoint
class MainActivity : ComponentActivity() {

    private val mainViewModel: MainViewModel by viewModels()

    private val transferReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context?, intent: Intent?) {
            when (intent?.action) {
                "com.turbotransfer.START_TRANSFER" -> {
                    val path = intent.getStringExtra("file_path") ?: return
                    val address = intent.getStringExtra("address") ?: "127.0.0.1:9876"
                    Log.d("TurboTransfer", "Broadcast received: START_TRANSFER path=$path, address=$address")
                    mainViewModel.handleStartTransferBroadcast(path, address)
                }
                "com.turbotransfer.START_HOTSPOT" -> {
                    Log.d("TurboTransfer", "Broadcast received: START_HOTSPOT")
                    mainViewModel.handleStartHotspotBroadcast()
                }
                "com.turbotransfer.STOP_HOTSPOT" -> {
                    Log.d("TurboTransfer", "Broadcast received: STOP_HOTSPOT")
                    mainViewModel.handleStopHotspotBroadcast()
                }
                "com.turbotransfer.ENTER_RECEIVE" -> {
                    val dest = intent.getStringExtra("dest_dir")
                    Log.d("TurboTransfer", "Broadcast received: ENTER_RECEIVE dest=$dest")
                    mainViewModel.handleEnterReceiveBroadcast(dest)
                }
                "com.turbotransfer.STOP_RECEIVE" -> {
                    Log.d("TurboTransfer", "Broadcast received: STOP_RECEIVE")
                    mainViewModel.handleStopReceiveBroadcast()
                }
            }
        }
    }

    private val permissionLauncher = registerForActivityResult(
        androidx.activity.result.contract.ActivityResultContracts.RequestMultiplePermissions()
    ) { permissions ->
        Log.d("TurboTransfer", "Permissions result: $permissions")
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)

        requestRequiredPermissions()

        val filter = IntentFilter().apply {
            addAction("com.turbotransfer.START_TRANSFER")
            addAction("com.turbotransfer.START_HOTSPOT")
            addAction("com.turbotransfer.STOP_HOTSPOT")
            addAction("com.turbotransfer.ENTER_RECEIVE")
            addAction("com.turbotransfer.STOP_RECEIVE")
        }
        ContextCompat.registerReceiver(
            this,
            transferReceiver,
            filter,
            ContextCompat.RECEIVER_EXPORTED
        )

        setContent {
            TurboTransferTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.background
                ) {
                    TurboTransferApp()
                }
            }
        }
    }

    private fun requestRequiredPermissions() {
        val permissions = mutableListOf<String>()
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.TIRAMISU) {
            permissions.add(android.Manifest.permission.READ_MEDIA_IMAGES)
            permissions.add(android.Manifest.permission.READ_MEDIA_VIDEO)
            permissions.add(android.Manifest.permission.READ_MEDIA_AUDIO)
            permissions.add(android.Manifest.permission.NEARBY_WIFI_DEVICES)
            permissions.add(android.Manifest.permission.POST_NOTIFICATIONS)
        } else {
            permissions.add(android.Manifest.permission.READ_EXTERNAL_STORAGE)
            permissions.add(android.Manifest.permission.WRITE_EXTERNAL_STORAGE)
        }
        permissions.add(android.Manifest.permission.ACCESS_FINE_LOCATION)

        val ungranted = permissions.filter {
            ContextCompat.checkSelfPermission(this, it) != android.content.pm.PackageManager.PERMISSION_GRANTED
        }
        if (ungranted.isNotEmpty()) {
            permissionLauncher.launch(ungranted.toTypedArray())
        }

        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.R && !android.os.Environment.isExternalStorageManager()) {
            try {
                val intent = android.content.Intent(android.provider.Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION).apply {
                    data = android.net.Uri.parse("package:$packageName")
                }
                startActivity(intent)
            } catch (_: Exception) {
                try {
                    val intent = android.content.Intent(android.provider.Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION)
                    startActivity(intent)
                } catch (_: Exception) {}
            }
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        try {
            unregisterReceiver(transferReceiver)
        } catch (_: Exception) {}
    }
}
