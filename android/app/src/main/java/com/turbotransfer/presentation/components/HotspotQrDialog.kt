package com.turbotransfer.presentation.components

import androidx.compose.runtime.Composable
import com.turbotransfer.domain.model.HotspotCredentials

@Composable
fun HotspotQrDialog(
    credentials: HotspotCredentials,
    onDismiss: () -> Unit
) {
    QrCodeDialog(
        credentials = credentials,
        onDismiss = onDismiss
    )
}
