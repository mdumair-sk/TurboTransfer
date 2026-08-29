package com.turbotransfer.presentation.components

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.widget.Toast
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.QrCode
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import com.turbotransfer.domain.model.HotspotCredentials
import com.turbotransfer.presentation.theme.*

@Composable
fun HotspotQrDialog(
    credentials: HotspotCredentials,
    onDismiss: () -> Unit
) {
    val context = LocalContext.current
    val wifiQrContent = "WIFI:S:${credentials.ssid};T:WPA;P:${credentials.passphrase};;"

    Dialog(onDismissRequest = onDismiss) {
        Surface(
            shape = RoundedCornerShape(24.dp),
            color = CyberSurface,
            border = BorderStroke(1.5.dp, CyberCyan.copy(alpha = 0.5f)),
            modifier = Modifier.fillMaxWidth()
        ) {
            Column(
                modifier = Modifier.padding(24.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(16.dp)
            ) {
                // Header
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp)
                    ) {
                        Icon(
                            imageVector = Icons.Default.QrCode,
                            contentDescription = null,
                            tint = CyberCyan,
                            modifier = Modifier.size(22.dp)
                        )
                        Text(
                            text = "5 GHz Hotspot Pairing",
                            fontWeight = FontWeight.Bold,
                            fontSize = 17.sp,
                            color = CyberTextPrimary
                        )
                    }
                    CyberBadge(
                        text = credentials.band,
                        color = CyberPurple,
                        pulsing = true
                    )
                }

                // Rendered QR Code Matrix
                Box(
                    modifier = Modifier
                        .background(CyberBackground, RoundedCornerShape(16.dp))
                        .padding(12.dp),
                    contentAlignment = Alignment.Center
                ) {
                    QrCodeCanvas(
                        content = wifiQrContent,
                        modifier = Modifier.size(190.dp),
                        darkColor = Color.White,
                        lightColor = CyberBackground
                    )
                }

                Text(
                    text = "Scan with PC camera or another phone to connect instantly",
                    fontSize = 12.sp,
                    color = CyberTextSecondary,
                    textAlign = TextAlign.Center
                )

                // Credentials Info Box
                Surface(
                    shape = RoundedCornerShape(12.dp),
                    color = CyberCardGlass,
                    border = BorderStroke(1.dp, CyberCardBorder),
                    modifier = Modifier.fillMaxWidth()
                ) {
                    Column(
                        modifier = Modifier.padding(12.dp),
                        verticalArrangement = Arrangement.spacedBy(6.dp)
                    ) {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Column {
                                Text("SSID", fontSize = 10.sp, color = CyberTextMuted)
                                Text(
                                    credentials.ssid,
                                    fontSize = 12.sp,
                                    fontWeight = FontWeight.Bold,
                                    fontFamily = FontFamily.Monospace,
                                    color = CyberCyan
                                )
                            }
                            IconButton(
                                onClick = {
                                    val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                                    cm.setPrimaryClip(ClipData.newPlainText("SSID", credentials.ssid))
                                    Toast.makeText(context, "SSID copied!", Toast.LENGTH_SHORT).show()
                                },
                                modifier = Modifier.size(24.dp)
                            ) {
                                Icon(Icons.Default.ContentCopy, contentDescription = "Copy SSID", tint = CyberCyan, modifier = Modifier.size(14.dp))
                            }
                        }

                        HorizontalDivider(color = CyberCardBorder.copy(alpha = 0.5f))

                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Column {
                                Text("Password", fontSize = 10.sp, color = CyberTextMuted)
                                Text(
                                    credentials.passphrase,
                                    fontSize = 12.sp,
                                    fontWeight = FontWeight.Bold,
                                    fontFamily = FontFamily.Monospace,
                                    color = CyberMint
                                )
                            }
                            IconButton(
                                onClick = {
                                    val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                                    cm.setPrimaryClip(ClipData.newPlainText("Password", credentials.passphrase))
                                    Toast.makeText(context, "Password copied!", Toast.LENGTH_SHORT).show()
                                },
                                modifier = Modifier.size(24.dp)
                            ) {
                                Icon(Icons.Default.ContentCopy, contentDescription = "Copy Password", tint = CyberMint, modifier = Modifier.size(14.dp))
                            }
                        }
                    }
                }

                // Close Button
                Button(
                    onClick = onDismiss,
                    modifier = Modifier.fillMaxWidth(),
                    colors = ButtonDefaults.buttonColors(containerColor = CyberCyan),
                    shape = RoundedCornerShape(12.dp)
                ) {
                    Text("Done", color = Color.Black, fontWeight = FontWeight.Bold)
                }
            }
        }
    }
}
