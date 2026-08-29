package com.turbotransfer.presentation.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.turbotransfer.domain.model.FileCategory
import com.turbotransfer.domain.model.SelectedFileInfo
import com.turbotransfer.presentation.theme.*

@Composable
fun CategoryChip(
    icon: ImageVector,
    label: String,
    accentColor: Color = CyberCyan,
    badgeCount: Int = 0,
    onClick: () -> Unit
) {
    Surface(
        onClick = onClick,
        shape = RoundedCornerShape(14.dp),
        color = CyberCardGlass,
        border = BorderStroke(1.dp, if (badgeCount > 0) accentColor else CyberCardBorder)
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 14.dp, vertical = 11.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            Surface(
                shape = RoundedCornerShape(8.dp),
                color = accentColor.copy(alpha = 0.15f),
                modifier = Modifier.size(28.dp)
            ) {
                Box(contentAlignment = Alignment.Center) {
                    Icon(
                        icon,
                        contentDescription = null,
                        modifier = Modifier.size(16.dp),
                        tint = accentColor
                    )
                }
            }
            Text(
                label,
                fontWeight = FontWeight.Bold,
                fontSize = 13.sp,
                color = CyberTextPrimary
            )
            if (badgeCount > 0) {
                Surface(
                    shape = RoundedCornerShape(10.dp),
                    color = accentColor
                ) {
                    Text(
                        "$badgeCount",
                        fontSize = 10.sp,
                        fontWeight = FontWeight.Bold,
                        color = Color.Black,
                        modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp)
                    )
                }
            }
        }
    }
}

@Composable
fun SelectedFileItemChip(
    fileInfo: SelectedFileInfo,
    onRemove: () -> Unit
) {
    val categoryColor = when (fileInfo.category) {
        FileCategory.IMAGE -> CyberCyan
        FileCategory.VIDEO -> CyberPurple
        FileCategory.AUDIO -> CyberMint
        FileCategory.DOCUMENT -> CyberOrange
        FileCategory.APK -> CyberBlue
        FileCategory.FOLDER -> Color(0xFFFFC107)
        else -> CyberCyan
    }

    val icon = when (fileInfo.category) {
        FileCategory.IMAGE -> Icons.Default.Image
        FileCategory.VIDEO -> Icons.Default.Videocam
        FileCategory.AUDIO -> Icons.Default.Audiotrack
        FileCategory.DOCUMENT -> Icons.Default.InsertDriveFile
        FileCategory.APK -> Icons.Default.Android
        FileCategory.FOLDER -> Icons.Default.Folder
        else -> Icons.Default.Description
    }

    Surface(
        shape = RoundedCornerShape(12.dp),
        color = CyberSurface,
        border = BorderStroke(1.dp, categoryColor.copy(alpha = 0.4f))
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 10.dp, vertical = 7.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            Surface(
                shape = RoundedCornerShape(6.dp),
                color = categoryColor.copy(alpha = 0.15f),
                modifier = Modifier.size(24.dp)
            ) {
                Box(contentAlignment = Alignment.Center) {
                    Icon(
                        icon,
                        contentDescription = null,
                        modifier = Modifier.size(14.dp),
                        tint = categoryColor
                    )
                }
            }

            Column {
                Text(
                    fileInfo.displayName,
                    fontSize = 12.sp,
                    fontWeight = FontWeight.Bold,
                    color = CyberTextPrimary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.widthIn(max = 130.dp)
                )
                Text(
                    fileInfo.formattedSize,
                    fontSize = 10.sp,
                    fontFamily = FontFamily.Monospace,
                    color = CyberTextMuted
                )
            }

            IconButton(
                onClick = onRemove,
                modifier = Modifier.size(20.dp)
            ) {
                Icon(
                    Icons.Default.Close,
                    contentDescription = "Remove",
                    tint = CyberTextSecondary,
                    modifier = Modifier.size(14.dp)
                )
            }
        }
    }
}
