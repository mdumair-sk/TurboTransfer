package com.turbotransfer.presentation.components

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.*
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
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
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.*
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.turbotransfer.UriUtils
import com.turbotransfer.domain.model.TransferProgressInfo
import com.turbotransfer.domain.model.TransferStatus
import com.turbotransfer.presentation.theme.*
import kotlin.math.cos
import kotlin.math.sin

/**
 * High-tech glassmorphic card container with gradient background and neon border.
 */
@Composable
fun CyberCard(
    modifier: Modifier = Modifier,
    borderColor: Color = CyberCardBorder,
    borderGlow: Boolean = false,
    gradient: Brush = CyberCardGradient,
    cornerRadius: Dp = 16.dp,
    onClick: (() -> Unit)? = null,
    content: @Composable ColumnScope.() -> Unit
) {
    val clickModifier = if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier

    Surface(
        modifier = modifier
            .then(
                if (borderGlow) Modifier.shadow(
                    elevation = 8.dp,
                    shape = RoundedCornerShape(cornerRadius),
                    spotColor = borderColor.copy(alpha = 0.5f),
                    ambientColor = borderColor.copy(alpha = 0.3f)
                ) else Modifier
            )
            .clip(RoundedCornerShape(cornerRadius))
            .background(gradient)
            .then(clickModifier),
        shape = RoundedCornerShape(cornerRadius),
        color = Color.Transparent,
        border = BorderStroke(
            1.dp,
            if (borderGlow) borderColor else CyberCardBorder
        )
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            content = content
        )
    }
}

/**
 * Cyber status badge with pulsating status dot.
 */
@Composable
fun CyberBadge(
    text: String,
    modifier: Modifier = Modifier,
    color: Color = CyberCyan,
    pulsing: Boolean = false,
    icon: ImageVector? = null
) {
    val infiniteTransition = rememberInfiniteTransition(label = "pulse")
    val alpha by if (pulsing) {
        infiniteTransition.animateFloat(
            initialValue = 0.4f,
            targetValue = 1.0f,
            animationSpec = infiniteRepeatable(
                animation = tween(800, easing = LinearEasing),
                repeatMode = RepeatMode.Reverse
            ),
            label = "badgePulse"
        )
    } else {
        remember { mutableFloatStateOf(1f) }
    }

    Surface(
        modifier = modifier,
        shape = RoundedCornerShape(20.dp),
        color = color.copy(alpha = 0.12f),
        border = BorderStroke(1.dp, color.copy(alpha = 0.4f * alpha))
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(5.dp)
        ) {
            Box(
                modifier = Modifier
                    .size(7.dp)
                    .background(color.copy(alpha = alpha), CircleShape)
            )
            if (icon != null) {
                Icon(
                    imageVector = icon,
                    contentDescription = null,
                    tint = color,
                    modifier = Modifier.size(12.dp)
                )
            }
            Text(
                text = text,
                color = color,
                fontSize = 11.sp,
                fontWeight = FontWeight.Bold,
                fontFamily = FontFamily.Monospace,
                letterSpacing = 0.5.sp
            )
        }
    }
}

/**
 * Real-time Canvas Telemetry Waveform Speed Graph.
 * Displays live throughput curves for Total, USB, and Wi-Fi channels.
 */
@Composable
fun SpeedWaveformGraph(
    speedHistory: List<Float>,
    usbHistory: List<Float> = emptyList(),
    wifiHistory: List<Float> = emptyList(),
    modifier: Modifier = Modifier
        .fillMaxWidth()
        .height(110.dp)
) {
    Canvas(modifier = modifier) {
        val width = size.width
        val height = size.height

        if (speedHistory.size < 2) {
            // Draw empty grid baseline
            drawLine(
                color = CyberCardBorder.copy(alpha = 0.5f),
                start = Offset(0f, height - 10f),
                end = Offset(width, height - 10f),
                strokeWidth = 1.dp.toPx()
            )
            return@Canvas
        }

        val maxSpeed = maxOf(
            speedHistory.maxOrNull() ?: 10f,
            usbHistory.maxOrNull() ?: 0f,
            wifiHistory.maxOrNull() ?: 0f,
            20f
        ) * 1.15f

        // Draw horizontal grid guide lines
        val gridLines = 3
        for (i in 1..gridLines) {
            val y = height * (1f - (i.toFloat() / (gridLines + 1)))
            drawLine(
                color = CyberCardBorder.copy(alpha = 0.3f),
                start = Offset(0f, y),
                end = Offset(width, y),
                strokeWidth = 0.8.dp.toPx(),
                pathEffect = PathEffect.dashPathEffect(floatArrayOf(10f, 10f), 0f)
            )
        }

        fun drawCurve(data: List<Float>, strokeColor: Color, fillGradient: Brush? = null) {
            if (data.size < 2) return
            val stepX = width / (data.size - 1)

            val path = Path()
            val fillPath = Path()

            val points = data.mapIndexed { index, speed ->
                val x = index * stepX
                val normalized = (speed / maxSpeed).coerceIn(0f, 1f)
                val y = height - (normalized * (height - 15f)) - 5f
                Offset(x, y)
            }

            path.moveTo(points.first().x, points.first().y)
            fillPath.moveTo(points.first().x, height)
            fillPath.lineTo(points.first().x, points.first().y)

            for (i in 0 until points.size - 1) {
                val p0 = points[i]
                val p1 = points[i + 1]
                val cx = (p0.x + p1.x) / 2f
                path.cubicTo(cx, p0.y, cx, p1.y, p1.x, p1.y)
                fillPath.cubicTo(cx, p0.y, cx, p1.y, p1.x, p1.y)
            }

            fillPath.lineTo(points.last().x, height)
            fillPath.close()

            if (fillGradient != null) {
                drawPath(
                    path = fillPath,
                    brush = fillGradient
                )
            }

            drawPath(
                path = path,
                color = strokeColor,
                style = Stroke(
                    width = 2.5.dp.toPx(),
                    cap = StrokeCap.Round,
                    join = StrokeJoin.Round
                )
            )
        }

        // 1. Draw Wi-Fi Curve (Purple)
        if (wifiHistory.isNotEmpty()) {
            drawCurve(
                wifiHistory,
                CyberPurple,
                Brush.verticalGradient(
                    colors = listOf(CyberPurple.copy(alpha = 0.2f), Color.Transparent),
                    startY = 0f,
                    endY = height
                )
            )
        }

        // 2. Draw USB Curve (Cyan)
        if (usbHistory.isNotEmpty()) {
            drawCurve(
                usbHistory,
                CyberBlue,
                Brush.verticalGradient(
                    colors = listOf(CyberBlue.copy(alpha = 0.2f), Color.Transparent),
                    startY = 0f,
                    endY = height
                )
            )
        }

        // 3. Draw Total Combined Curve (Neon Mint)
        drawCurve(
            speedHistory,
            CyberMint,
            Brush.verticalGradient(
                colors = listOf(CyberMint.copy(alpha = 0.28f), Color.Transparent),
                startY = 0f,
                endY = height
            )
        )
    }
}

/**
 * Circular HUD Speedometer with glowing neon progress arc.
 */
@Composable
fun SpeedometerHUD(
    speedMBps: Double,
    peakSpeedMBps: Double,
    progressPercent: Double,
    etaSeconds: Long?,
    modifier: Modifier = Modifier.size(200.dp)
) {
    val animatedProgress by animateFloatAsState(
        targetValue = (progressPercent / 100.0).toFloat().coerceIn(0f, 1f),
        animationSpec = tween(400, easing = FastOutSlowInEasing),
        label = "progress"
    )

    Box(
        modifier = modifier,
        contentAlignment = Alignment.Center
    ) {
        Canvas(modifier = Modifier.fillMaxSize()) {
            val strokeWidth = 10.dp.toPx()
            val diameter = size.minDimension - strokeWidth
            val arcSize = Size(diameter, diameter)
            val topLeft = Offset(strokeWidth / 2f, strokeWidth / 2f)

            // Background Track
            drawArc(
                color = CyberCardBorder.copy(alpha = 0.6f),
                startAngle = 135f,
                sweepAngle = 270f,
                useCenter = false,
                topLeft = topLeft,
                size = arcSize,
                style = Stroke(width = strokeWidth, cap = StrokeCap.Round)
            )

            // Active Glowing Neon Progress Arc
            if (animatedProgress > 0f) {
                drawArc(
                    brush = Brush.sweepGradient(
                        0.0f to CyberCyan,
                        0.5f to CyberMint,
                        1.0f to CyberPurple
                    ),
                    startAngle = 135f,
                    sweepAngle = 270f * animatedProgress,
                    useCenter = false,
                    topLeft = topLeft,
                    size = arcSize,
                    style = Stroke(width = strokeWidth, cap = StrokeCap.Round)
                )
            }
        }

        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center
        ) {
            Text(
                text = String.format("%.1f", speedMBps),
                fontSize = 38.sp,
                fontWeight = FontWeight.ExtraBold,
                color = CyberMint,
                fontFamily = FontFamily.Monospace,
                letterSpacing = (-1).sp
            )
            Text(
                text = "MB/s",
                fontSize = 13.sp,
                fontWeight = FontWeight.Bold,
                color = CyberCyan
            )
            Spacer(modifier = Modifier.height(4.dp))
            Row(
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Text(
                    text = "${String.format("%.1f", progressPercent)}%",
                    fontSize = 12.sp,
                    fontWeight = FontWeight.Bold,
                    color = CyberTextPrimary,
                    fontFamily = FontFamily.Monospace
                )
                Text(
                    text = "•",
                    fontSize = 10.sp,
                    color = CyberTextMuted
                )
                Text(
                    text = "ETA ${UriUtils.formatEta(etaSeconds)}",
                    fontSize = 11.sp,
                    color = CyberTextSecondary,
                    fontFamily = FontFamily.Monospace
                )
            }
        }
    }
}

/**
 * Visual Storage Capacity Progress Bar.
 */
@Composable
fun StorageSpaceGauge(
    usedBytes: Long,
    totalBytes: Long,
    modifier: Modifier = Modifier.fillMaxWidth()
) {
    val usedRatio = if (totalBytes > 0) (usedBytes.toFloat() / totalBytes.toFloat()).coerceIn(0f, 1f) else 0f
    val freeBytes = (totalBytes - usedBytes).coerceAtLeast(0)

    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween
        ) {
            Text(
                text = "Internal Storage",
                fontSize = 12.sp,
                fontWeight = FontWeight.SemiBold,
                color = CyberTextSecondary
            )
            Text(
                text = "${UriUtils.formatFileSize(freeBytes)} Free of ${UriUtils.formatFileSize(totalBytes)}",
                fontSize = 11.sp,
                fontWeight = FontWeight.Bold,
                fontFamily = FontFamily.Monospace,
                color = CyberCyan
            )
        }

        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(6.dp)
                .clip(RoundedCornerShape(3.dp))
                .background(CyberCardBorder)
        ) {
            Box(
                modifier = Modifier
                    .fillMaxHeight()
                    .fillMaxWidth(fraction = usedRatio)
                    .clip(RoundedCornerShape(3.dp))
                    .background(
                        when {
                            usedRatio > 0.9f -> CyberRed
                            usedRatio > 0.75f -> CyberOrange
                            else -> CyberCyan
                        }
                    )
            )
        }
    }
}

/**
 * Pure Compose Canvas QR Code generator.
 * Zero external libraries needed; high contrast, sharp rendering.
 */
@Composable
fun QrCodeCanvas(
    content: String,
    modifier: Modifier = Modifier.size(190.dp),
    darkColor: Color = Color.White,
    lightColor: Color = CyberSurface
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
        shape = RoundedCornerShape(12.dp),
        color = lightColor,
        border = BorderStroke(1.dp, CyberCyan.copy(alpha = 0.4f))
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
                            cornerRadius = CornerRadius(cellSize * 0.2f, cellSize * 0.2f)
                        )
                    }
                }
            }
        }
    }
}

/**
 * Animated Concentric Radar Scanner for Receiver discovery.
 */
@Composable
fun PulsingRadar(
    isScanning: Boolean,
    modifier: Modifier = Modifier.size(110.dp),
    tint: Color = CyberCyan
) {
    val infiniteTransition = rememberInfiniteTransition(label = "radar")
    val wave1 by infiniteTransition.animateFloat(
        initialValue = 0.2f,
        targetValue = 1.0f,
        animationSpec = infiniteRepeatable(
            animation = tween(2000, easing = LinearEasing),
            repeatMode = RepeatMode.Restart
        ),
        label = "wave1"
    )
    val wave2 by infiniteTransition.animateFloat(
        initialValue = 0.2f,
        targetValue = 1.0f,
        animationSpec = infiniteRepeatable(
            animation = tween(2000, easing = LinearEasing, delayMillis = 1000),
            repeatMode = RepeatMode.Restart
        ),
        label = "wave2"
    )

    Box(modifier = modifier, contentAlignment = Alignment.Center) {
        if (isScanning) {
            Canvas(modifier = Modifier.fillMaxSize()) {
                val maxRadius = size.minDimension / 2f

                // Expanding wave 1
                drawCircle(
                    color = tint.copy(alpha = (1f - wave1) * 0.35f),
                    radius = maxRadius * wave1,
                    style = Stroke(width = 2.dp.toPx())
                )
                // Expanding wave 2
                drawCircle(
                    color = tint.copy(alpha = (1f - wave2) * 0.35f),
                    radius = maxRadius * wave2,
                    style = Stroke(width = 2.dp.toPx())
                )
            }
        }

        Surface(
            shape = CircleShape,
            color = tint.copy(alpha = 0.15f),
            border = BorderStroke(1.5.dp, tint),
            modifier = Modifier.size(48.dp)
        ) {
            Box(contentAlignment = Alignment.Center) {
                Icon(
                    imageVector = Icons.Default.Sensors,
                    contentDescription = null,
                    tint = tint,
                    modifier = Modifier.size(24.dp)
                )
            }
        }
    }
}

/**
 * Floating Persistent Transfer Mini-Player Capsule.
 * Displays when a transfer is active in the background and user is in another tab.
 */
@Composable
fun TransferCapsuleMiniPlayer(
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
                    .shadow(12.dp, RoundedCornerShape(16.dp), spotColor = CyberCyan.copy(alpha = 0.6f))
                    .clip(RoundedCornerShape(16.dp))
                    .clickable(onClick = onExpand),
                color = CyberSurface,
                border = BorderStroke(1.dp, CyberCyan.copy(alpha = 0.6f))
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
                                    .background(CyberMint, CircleShape)
                            )
                            Text(
                                text = progress.fileName,
                                fontWeight = FontWeight.Bold,
                                fontSize = 13.sp,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                                color = CyberTextPrimary
                            )
                        }

                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(8.dp)
                        ) {
                            Text(
                                text = String.format("%.1f MB/s", progress.aggregateSpeedMBps),
                                fontSize = 13.sp,
                                fontWeight = FontWeight.ExtraBold,
                                fontFamily = FontFamily.Monospace,
                                color = CyberMint
                            )
                            Icon(
                                imageVector = Icons.Default.OpenInFull,
                                contentDescription = "Expand Transfer Monitor",
                                tint = CyberCyan,
                                modifier = Modifier.size(16.dp)
                            )
                        }
                    }

                    // Mini Progress Bar
                    LinearProgressIndicator(
                        progress = { (progress.percent / 100.0).toFloat().coerceIn(0f, 1f) },
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(4.dp)
                            .clip(RoundedCornerShape(2.dp)),
                        color = CyberCyan,
                        trackColor = CyberCardBorder
                    )
                }
            }
        }
    }
}
