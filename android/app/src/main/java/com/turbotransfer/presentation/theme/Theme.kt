package com.turbotransfer.presentation.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color

// ==========================================
// TURBOTRANSFER CYBER DASHBOARD COLOR SYSTEM
// ==========================================

// Core Backgrounds & Surfaces (Deep Space AMOLED)
val CyberBackground = Color(0xFF080C14)
val CyberSurface = Color(0xFF0F172A)
val CyberSurfaceVariant = Color(0xFF162036)
val CyberCardGlass = Color(0xFF131D33)
val CyberCardBorder = Color(0xFF243656)
val CyberCardBorderGlow = Color(0xFF00E5FF).copy(alpha = 0.35f)

// Neon & Accent Colors
val CyberCyan = Color(0xFF00E5FF)       // Primary Link & USB Accent
val CyberCyanGlow = Color(0x3300E5FF)
val CyberMint = Color(0xFF00E676)       // Speed & Success Accent
val CyberMintGlow = Color(0x3300E676)
val CyberPurple = Color(0xFFD500F9)     // 5GHz Wi-Fi Accent
val CyberPurpleGlow = Color(0x33D500F9)
val CyberOrange = Color(0xFFFF9100)     // Warning & Intermediate Accent
val CyberRed = Color(0xFFFF1744)        // Alert & Disconnect Accent
val CyberBlue = Color(0xFF2979FF)       // Secondary USB Accent

// Text & Content Colors
val CyberTextPrimary = Color(0xFFF1F5F9)
val CyberTextSecondary = Color(0xFF94A3B8)
val CyberTextMuted = Color(0xFF64748B)

// Gradients
val CyberPrimaryGradient = Brush.horizontalGradient(
    colors = listOf(CyberCyan, Color(0xFF00B0FF))
)

val CyberSpeedGradient = Brush.horizontalGradient(
    colors = listOf(CyberMint, Color(0xFF00B0FF))
)

val CyberWifiGradient = Brush.horizontalGradient(
    colors = listOf(CyberPurple, Color(0xFF7C4DFF))
)

val CyberUsbGradient = Brush.horizontalGradient(
    colors = listOf(CyberCyan, Color(0xFF2979FF))
)

val CyberCardGradient = Brush.verticalGradient(
    colors = listOf(
        Color(0xFF16233B),
        Color(0xFF0E1626)
    )
)

val CyberActiveCardGradient = Brush.verticalGradient(
    colors = listOf(
        Color(0xFF12283A),
        Color(0xFF0A1926)
    )
)

private val CyberColorScheme = darkColorScheme(
    primary = CyberCyan,
    onPrimary = Color(0xFF001E2B),
    primaryContainer = Color(0xFF004D66),
    onPrimaryContainer = Color(0xFFB8EAFF),
    secondary = CyberMint,
    onSecondary = Color(0xFF003919),
    secondaryContainer = Color(0xFF005227),
    onSecondaryContainer = Color(0xFF66FFA3),
    tertiary = CyberPurple,
    onTertiary = Color(0xFF380044),
    tertiaryContainer = Color(0xFF5B006D),
    onTertiaryContainer = Color(0xFFFFD6FE),
    background = CyberBackground,
    onBackground = CyberTextPrimary,
    surface = CyberSurface,
    onSurface = CyberTextPrimary,
    surfaceVariant = CyberSurfaceVariant,
    onSurfaceVariant = CyberTextSecondary,
    outline = CyberCardBorder,
    outlineVariant = Color(0xFF1E2D4A),
    error = CyberRed,
    onError = Color.White
)

@Composable
fun TurboTransferTheme(
    darkTheme: Boolean = true,
    content: @Composable () -> Unit
) {
    MaterialTheme(
        colorScheme = CyberColorScheme,
        content = content
    )
}
