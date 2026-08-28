package com.turbotransfer.service

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import com.turbotransfer.MainActivity
import com.turbotransfer.R
import com.turbotransfer.UriUtils
import com.turbotransfer.core.common.DispatcherProvider
import com.turbotransfer.core.util.TransferLockManager
import com.turbotransfer.domain.model.TransferProgressInfo
import com.turbotransfer.domain.model.TransferSession
import com.turbotransfer.domain.model.TransferStatus
import com.turbotransfer.domain.repository.TransferRepository
import dagger.hilt.android.AndroidEntryPoint
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch
import javax.inject.Inject

@AndroidEntryPoint
class TransferService : Service() {

    @Inject
    lateinit var transferRepository: TransferRepository

    @Inject
    lateinit var transferLockManager: TransferLockManager

    @Inject
    lateinit var dispatcherProvider: DispatcherProvider

    private val serviceScope by lazy {
        CoroutineScope(dispatcherProvider.main + SupervisorJob())
    }

    private var progressCollectorJob: Job? = null
    private var lastNotificationUpdateTime = 0L

    companion object {
        const val CHANNEL_ID = "turbotransfer_active_transfers"
        const val CHANNEL_NAME = "File Transfers"
        const val NOTIFICATION_ID = 1001

        const val ACTION_START = "com.turbotransfer.action.START_TRANSFER_SERVICE"
        const val ACTION_STOP = "com.turbotransfer.action.STOP_TRANSFER_SERVICE"
        const val ACTION_PAUSE = "com.turbotransfer.action.PAUSE_TRANSFER"
        const val ACTION_RESUME = "com.turbotransfer.action.RESUME_TRANSFER"
        const val ACTION_CANCEL = "com.turbotransfer.action.CANCEL_TRANSFER"
        const val EXTRA_TRANSFER_ID = "extra_transfer_id"

        fun start(context: Context, transferId: String? = null) {
            val intent = Intent(context, TransferService::class.java).apply {
                action = ACTION_START
                transferId?.let { putExtra(EXTRA_TRANSFER_ID, it) }
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                context.startForegroundService(intent)
            } else {
                context.startService(intent)
            }
        }

        fun stop(context: Context) {
            val intent = Intent(context, TransferService::class.java).apply {
                action = ACTION_STOP
            }
            context.startService(intent)
        }
    }

    override fun onCreate() {
        super.onCreate()
        createNotificationChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val action = intent?.action ?: ACTION_START
        val transferId = intent?.getStringExtra(EXTRA_TRANSFER_ID)

        when (action) {
            ACTION_PAUSE -> {
                transferId?.let { id ->
                    serviceScope.launch(dispatcherProvider.io) {
                        transferRepository.pauseTransfer(id)
                    }
                }
            }
            ACTION_RESUME -> {
                transferId?.let { id ->
                    serviceScope.launch(dispatcherProvider.io) {
                        transferRepository.resumeTransfer(id)
                    }
                }
            }
            ACTION_CANCEL -> {
                transferId?.let { id ->
                    serviceScope.launch(dispatcherProvider.io) {
                        transferRepository.cancelTransfer(id)
                    }
                }
            }
            ACTION_STOP -> {
                stopForegroundService()
                return START_NOT_STICKY
            }
            ACTION_START -> {
                startForegroundWithInitialNotification()
                observeActiveTransfers()
            }
        }

        return START_STICKY
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onDestroy() {
        progressCollectorJob?.cancel()
        serviceScope.cancel()
        transferLockManager.releaseLocks()
        super.onDestroy()
    }

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                CHANNEL_NAME,
                NotificationManager.IMPORTANCE_LOW
            ).apply {
                description = "Shows live progress, speed, and controls for active file transfers"
                setShowBadge(false)
                enableVibration(false)
                setSound(null, null)
            }
            val notificationManager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            notificationManager.createNotificationChannel(channel)
        }
    }

    private fun startForegroundWithInitialNotification() {
        val initialNotification = buildNotification(
            title = "TurboTransfer Active",
            contentText = "Initializing transfer channel...",
            progressPercent = 0,
            isIndeterminate = true,
            actions = emptyList()
        )

        val serviceType = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC
        } else {
            0
        }

        ServiceCompat.startForeground(
            this,
            NOTIFICATION_ID,
            initialNotification,
            serviceType
        )
        transferLockManager.acquireLocks()
    }

    private fun observeActiveTransfers() {
        progressCollectorJob?.cancel()
        progressCollectorJob = serviceScope.launch {
            transferRepository.activeSessionFlow.collectLatest { session ->
                if (session != null) {
                    transferLockManager.acquireLocks()
                    transferRepository.observeTransferProgress(session.transferId).collect { progress ->
                        handleProgressUpdate(session, progress)
                    }
                } else {
                    stopForegroundService()
                }
            }
        }
    }

    private fun handleProgressUpdate(session: TransferSession, progress: TransferProgressInfo?) {
        if (progress == null) return

        val now = System.currentTimeMillis()
        val isTerminal = progress.status == TransferStatus.COMPLETED ||
                progress.status == TransferStatus.FAILED ||
                progress.status == TransferStatus.CANCELLED

        // Throttle active progress notification updates to 500ms
        if (!isTerminal && (now - lastNotificationUpdateTime < 500L)) {
            return
        }
        lastNotificationUpdateTime = now

        when (progress.status) {
            TransferStatus.IN_PROGRESS -> {
                val actionText = if (session.isOutgoing) "Sending" else "Receiving"
                val percent = progress.percent.toInt().coerceIn(0, 100)
                val speed = UriUtils.formatSpeed(progress.aggregateSpeedMBps)
                val eta = UriUtils.formatEta(progress.etaSeconds)
                val content = "$percent% • $speed • ETA: $eta"

                val subText = if (progress.usbSpeedMBps > 0.01 && progress.wifiSpeedMBps > 0.01) {
                    "USB: ${UriUtils.formatSpeed(progress.usbSpeedMBps)} | Wi-Fi: ${UriUtils.formatSpeed(progress.wifiSpeedMBps)}"
                } else if (progress.wifiSpeedMBps > 0.01) {
                    "5 GHz Wi-Fi • ${UriUtils.formatSpeed(progress.wifiSpeedMBps)}"
                } else {
                    "USB • ${UriUtils.formatSpeed(progress.usbSpeedMBps)}"
                }

                val actions = listOf(
                    createAction(
                        icon = android.R.drawable.ic_media_pause,
                        title = "Pause",
                        action = ACTION_PAUSE,
                        transferId = session.transferId
                    ),
                    createAction(
                        icon = android.R.drawable.ic_menu_close_clear_cancel,
                        title = "Cancel",
                        action = ACTION_CANCEL,
                        transferId = session.transferId
                    )
                )

                val notification = buildNotification(
                    title = "$actionText ${session.fileName}",
                    contentText = content,
                    progressPercent = percent,
                    isIndeterminate = false,
                    subText = subText,
                    actions = actions
                )
                notify(notification)
            }
            TransferStatus.PAUSED -> {
                val percent = progress.percent.toInt().coerceIn(0, 100)
                val actions = listOf(
                    createAction(
                        icon = android.R.drawable.ic_media_play,
                        title = "Resume",
                        action = ACTION_RESUME,
                        transferId = session.transferId
                    ),
                    createAction(
                        icon = android.R.drawable.ic_menu_close_clear_cancel,
                        title = "Cancel",
                        action = ACTION_CANCEL,
                        transferId = session.transferId
                    )
                )
                val notification = buildNotification(
                    title = "Paused: ${session.fileName}",
                    contentText = "$percent% completed",
                    progressPercent = percent,
                    isIndeterminate = false,
                    actions = actions
                )
                notify(notification)
            }
            TransferStatus.COMPLETED -> {
                val actionText = if (session.isOutgoing) "Sent" else "Received"
                val avgSpeed = UriUtils.formatSpeed(progress.aggregateSpeedMBps)
                val notification = buildNotification(
                    title = "Transfer Complete",
                    contentText = "$actionText ${session.fileName} ($avgSpeed)",
                    progressPercent = 100,
                    isIndeterminate = false,
                    isOngoing = false
                )
                val manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
                manager.notify(NOTIFICATION_ID + 1, notification)
                stopForegroundService()
            }
            TransferStatus.FAILED -> {
                val notification = buildNotification(
                    title = "Transfer Failed",
                    contentText = "Failed to transfer ${session.fileName}",
                    progressPercent = 0,
                    isIndeterminate = false,
                    isOngoing = false
                )
                val manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
                manager.notify(NOTIFICATION_ID + 2, notification)
                stopForegroundService()
            }
            TransferStatus.CANCELLED -> {
                stopForegroundService()
            }
            TransferStatus.IDLE -> {
                // No action needed for idle state
            }
        }
    }

    private fun createAction(
        icon: Int,
        title: String,
        action: String,
        transferId: String
    ): NotificationCompat.Action {
        val intent = Intent(this, TransferService::class.java).apply {
            this.action = action
            putExtra(EXTRA_TRANSFER_ID, transferId)
        }
        val flags = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        } else {
            PendingIntent.FLAG_UPDATE_CURRENT
        }
        val pendingIntent = PendingIntent.getService(
            this,
            action.hashCode() xor transferId.hashCode(),
            intent,
            flags
        )
        return NotificationCompat.Action.Builder(icon, title, pendingIntent).build()
    }

    private fun buildNotification(
        title: String,
        contentText: String,
        progressPercent: Int = 0,
        isIndeterminate: Boolean = false,
        subText: String? = null,
        isOngoing: Boolean = true,
        actions: List<NotificationCompat.Action> = emptyList()
    ): Notification {
        val contentIntent = Intent(this, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP
        }
        val flags = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        } else {
            PendingIntent.FLAG_UPDATE_CURRENT
        }
        val pendingContentIntent = PendingIntent.getActivity(
            this,
            0,
            contentIntent,
            flags
        )

        val smallIcon = R.drawable.ic_turbotransfer_rocket

        val builder = NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle(title)
            .setContentText(contentText)
            .setSmallIcon(smallIcon)
            .setContentIntent(pendingContentIntent)
            .setOngoing(isOngoing)
            .setOnlyAlertOnce(true)
            .setCategory(NotificationCompat.CATEGORY_PROGRESS)
            .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)

        if (subText != null) {
            builder.setStyle(NotificationCompat.BigTextStyle().bigText("$contentText\n$subText"))
        }

        if (isOngoing) {
            builder.setProgress(100, progressPercent, isIndeterminate)
        }

        for (action in actions) {
            builder.addAction(action)
        }

        return builder.build()
    }

    private fun notify(notification: Notification) {
        val manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        manager.notify(NOTIFICATION_ID, notification)
    }

    private fun stopForegroundService() {
        transferLockManager.releaseLocks()
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
        stopSelf()
    }
}
