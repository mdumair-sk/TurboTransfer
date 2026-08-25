package com.turbotransfer.domain.model

enum class FileCategory(val label: String) {
    IMAGE("Photos"),
    VIDEO("Videos"),
    AUDIO("Audio"),
    DOCUMENT("Documents"),
    APK("Apps"),
    ARCHIVE("Archives"),
    FOLDER("Folders"),
    OTHER("Files");

    companion object {
        fun fromExtension(ext: String): FileCategory {
            return when (ext.lowercase()) {
                "jpg", "jpeg", "png", "gif", "webp", "bmp", "heic", "svg" -> IMAGE
                "mp4", "mkv", "mov", "avi", "webm", "flv", "3gp", "ts" -> VIDEO
                "mp3", "flac", "wav", "m4a", "aac", "ogg", "opus" -> AUDIO
                "pdf", "doc", "docx", "txt", "xls", "xlsx", "ppt", "pptx", "epub", "csv", "json", "xml", "md" -> DOCUMENT
                "apk", "xapk", "apks" -> APK
                "zip", "rar", "7z", "tar", "gz", "bz2" -> ARCHIVE
                else -> OTHER
            }
        }
    }
}

data class SelectedFileInfo(
    val path: String,
    val displayName: String,
    val sizeBytes: Long,
    val formattedSize: String,
    val category: FileCategory = FileCategory.OTHER
)
