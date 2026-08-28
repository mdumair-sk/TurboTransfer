package com.turbotransfer

import android.content.ContentResolver
import android.content.ContentUris
import android.content.Context
import android.database.Cursor
import android.net.Uri
import android.os.Environment
import android.provider.DocumentsContract
import android.provider.MediaStore
import android.provider.OpenableColumns
import android.util.Log
import java.io.File
import java.io.FileOutputStream
import java.io.InputStream

import com.turbotransfer.domain.model.FileCategory
import com.turbotransfer.domain.model.SelectedFileInfo

private const val TAG = "UriUtils"

object UriUtils {

    fun formatFileSize(bytes: Long): String {
        return when {
            bytes >= 1024L * 1024L * 1024L -> String.format("%.2f GB", bytes.toDouble() / (1024.0 * 1024.0 * 1024.0))
            bytes >= 1024L * 1024L -> String.format("%.2f MB", bytes.toDouble() / (1024.0 * 1024.0))
            bytes >= 1024L -> String.format("%.1f KB", bytes.toDouble() / 1024.0)
            else -> "$bytes B"
        }
    }

    fun formatEta(etaSeconds: Long?): String {
        if (etaSeconds == null || etaSeconds <= 0L) return "--"
        return when {
            etaSeconds < 60L -> "${etaSeconds}s"
            etaSeconds < 3600L -> {
                val mins = etaSeconds / 60L
                val secs = etaSeconds % 60L
                "${mins}m ${secs}s"
            }
            else -> {
                val hours = etaSeconds / 3600L
                val mins = (etaSeconds % 3600L) / 60L
                "${hours}h ${mins}m"
            }
        }
    }

    fun formatSpeed(mbps: Double): String {
        return String.format(java.util.Locale.US, "%.2f MB/s", mbps)
    }

    fun resolveSelectedFile(context: Context, uri: Uri): SelectedFileInfo? {
        try {
            var displayName = "Unknown"
            var sizeBytes = 0L

            // 1. Query metadata via OpenableColumns
            context.contentResolver.query(uri, null, null, null, null)?.use { cursor ->
                if (cursor.moveToFirst()) {
                    val nameIndex = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                    if (nameIndex != -1) {
                        displayName = cursor.getString(nameIndex) ?: displayName
                    }
                    val sizeIndex = cursor.getColumnIndex(OpenableColumns.SIZE)
                    if (sizeIndex != -1) {
                        sizeBytes = cursor.getLong(sizeIndex)
                    }
                }
            }

            if (displayName.startsWith("transfer_cache_")) {
                displayName = displayName.substringAfter("transfer_cache_")
            }

            // 2. Try direct physical filesystem path
            var realPath = getRealPathFromUri(context, uri)
            var isDirectlyReadable = false

            if (realPath != null) {
                try {
                    val f = File(realPath)
                    if (f.exists() && f.isFile && f.canRead()) {
                        java.io.FileInputStream(f).use { it.read() }
                        isDirectlyReadable = true
                    }
                } catch (_: Exception) {
                    isDirectlyReadable = false
                }
            }

            // 2.5 Common public storage direct path match by displayName and size
            if (!isDirectlyReadable && displayName != "Unknown") {
                val commonDirs = arrayOf(
                    Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS),
                    Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_MOVIES),
                    Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOCUMENTS),
                    Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DCIM),
                    Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_PICTURES),
                    Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_MUSIC),
                    Environment.getExternalStorageDirectory()
                )
                for (dir in commonDirs) {
                    val candidate = File(dir, displayName)
                    if (candidate.exists() && candidate.isFile && candidate.canRead() && (sizeBytes == 0L || candidate.length() == sizeBytes)) {
                        try {
                            java.io.FileInputStream(candidate).use { it.read() }
                            realPath = candidate.absolutePath
                            isDirectlyReadable = true
                            break
                        } catch (_: Exception) {}
                    }
                }
            }

            // 3. Fallback: Stage content stream to application cache for unrestricted POSIX/Rust access
            if (!isDirectlyReadable) {
                try {
                    val stagingDir = File(context.cacheDir, "transfer_staging").apply { mkdirs() }
                    val safeName = displayName.replace(Regex("[/\\\\:*?\"<>|]"), "_")
                    val targetFile = File(stagingDir, safeName)

                    // Re-use if already fully staged
                    if (!targetFile.exists() || sizeBytes == 0L || targetFile.length() != sizeBytes) {
                        context.contentResolver.openInputStream(uri)?.use { input ->
                            FileOutputStream(targetFile).use { output ->
                                val buffer = ByteArray(256 * 1024)
                                var bytesRead: Int
                                while (input.read(buffer).also { bytesRead = it } != -1) {
                                    output.write(buffer, 0, bytesRead)
                                }
                                output.flush()
                            }
                        }
                    }

                    if (targetFile.exists() && targetFile.length() > 0L) {
                        realPath = targetFile.absolutePath
                        sizeBytes = targetFile.length()
                        Log.i(TAG, "Staged content to cache for Rust engine: $realPath ($sizeBytes bytes)")
                    }
                } catch (e: Exception) {
                    Log.e(TAG, "Failed to stage content from $uri", e)
                }
            } else {
                if (sizeBytes == 0L && realPath != null) {
                    sizeBytes = File(realPath).length()
                }
                if (displayName == "Unknown" && realPath != null) {
                    displayName = File(realPath).name
                }
            }

            val ext = displayName.substringAfterLast('.', "")
            val category = FileCategory.fromExtension(ext)

            return if (realPath != null && File(realPath).exists()) {
                SelectedFileInfo(
                    path = realPath,
                    displayName = displayName,
                    sizeBytes = sizeBytes,
                    formattedSize = formatFileSize(sizeBytes),
                    category = category
                )
            } else {
                null
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to resolve URI: $uri", e)
            return null
        }
    }

    fun resolveSelectedUris(context: Context, uris: List<Uri>): List<SelectedFileInfo> {
        val result = mutableListOf<SelectedFileInfo>()
        for (uri in uris) {
            val fileInfo = resolveSelectedFile(context, uri)
            if (fileInfo != null) {
                result.add(fileInfo)
            }
        }
        return result
    }

    fun resolveDirectoryUri(context: Context, treeUri: Uri): List<SelectedFileInfo> {
        val result = mutableListOf<SelectedFileInfo>()
        try {
            val docId = DocumentsContract.getTreeDocumentId(treeUri)
            val dirPath = getRealPathFromUri(context, treeUri)
            if (dirPath != null && File(dirPath).exists() && File(dirPath).isDirectory) {
                collectFilesRecursively(File(dirPath), result)
            } else {
                // Fallback: try common root matching
                if (docId.startsWith("primary:")) {
                    val relPath = docId.substringAfter("primary:")
                    val dir = File(Environment.getExternalStorageDirectory(), relPath)
                    if (dir.exists() && dir.isDirectory) {
                        collectFilesRecursively(dir, result)
                    }
                }
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to resolve directory URI: $treeUri", e)
        }
        return result
    }

    private fun collectFilesRecursively(dir: File, result: MutableList<SelectedFileInfo>) {
        dir.listFiles()?.forEach { file ->
            if (file.isDirectory) {
                collectFilesRecursively(file, result)
            } else if (file.isFile && file.canRead()) {
                val ext = file.extension
                result.add(
                    SelectedFileInfo(
                        path = file.absolutePath,
                        displayName = file.name,
                        sizeBytes = file.length(),
                        formattedSize = formatFileSize(file.length()),
                        category = FileCategory.fromExtension(ext)
                    )
                )
            }
        }
    }

    fun openFile(context: Context, filePath: String) {
        try {
            val file = File(filePath)
            if (!file.exists()) {
                android.widget.Toast.makeText(context, "File does not exist", android.widget.Toast.LENGTH_SHORT).show()
                return
            }

            val uri = androidx.core.content.FileProvider.getUriForFile(
                context,
                "${context.packageName}.fileprovider",
                file
            )
            val mimeType = getMimeType(filePath)
            val intent = android.content.Intent(android.content.Intent.ACTION_VIEW).apply {
                setDataAndType(uri, mimeType)
                addFlags(android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION)
                addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            context.startActivity(intent)
        } catch (e: Exception) {
            try {
                // Direct file fallback
                val file = File(filePath)
                val uri = Uri.fromFile(file)
                val intent = android.content.Intent(android.content.Intent.ACTION_VIEW).apply {
                    setDataAndType(uri, getMimeType(filePath))
                    addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
                }
                context.startActivity(intent)
            } catch (ex: Exception) {
                Log.e(TAG, "Could not open file: $filePath", ex)
                android.widget.Toast.makeText(context, "Cannot open file: ${ex.message}", android.widget.Toast.LENGTH_SHORT).show()
            }
        }
    }

    fun shareFile(context: Context, filePath: String) {
        try {
            val file = File(filePath)
            if (!file.exists()) return

            val uri = androidx.core.content.FileProvider.getUriForFile(
                context,
                "${context.packageName}.fileprovider",
                file
            )
            val intent = android.content.Intent(android.content.Intent.ACTION_SEND).apply {
                type = getMimeType(filePath)
                putExtra(android.content.Intent.EXTRA_STREAM, uri)
                addFlags(android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
            context.startActivity(android.content.Intent.createChooser(intent, "Share via").addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK))
        } catch (e: Exception) {
            Log.e(TAG, "Could not share file: $filePath", e)
        }
    }

    fun getMimeType(filePath: String): String {
        val ext = filePath.substringAfterLast('.', "").lowercase()
        return android.webkit.MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext) ?: "*/*"
    }

    private fun getRealPathFromUri(context: Context, uri: Uri): String? {
        // DocumentProvider
        if (DocumentsContract.isDocumentUri(context, uri)) {
            val authority = uri.authority
            // ExternalStorageProvider
            if ("com.android.externalstorage.documents" == authority) {
                val docId = DocumentsContract.getDocumentId(uri)
                val split = docId.split(":")
                val type = split.getOrNull(0) ?: ""
                val relativePath = split.getOrNull(1).orEmpty()

                if ("primary".equals(type, ignoreCase = true)) {
                    val path = Environment.getExternalStorageDirectory().toString() + "/" + relativePath
                    if (File(path).exists()) return path
                } else {
                    // SD Card or secondary storage
                    val path = "/storage/$type/" + relativePath
                    if (File(path).exists()) return path
                }
            }
            // DownloadsProvider
            else if ("com.android.providers.downloads.documents" == authority) {
                val id = DocumentsContract.getDocumentId(uri)
                if (id != null) {
                    if (id.startsWith("raw:")) {
                        return id.substring(4)
                    }
                    if (id.startsWith("msf:")) {
                        val fileId = id.substring(4)
                        val contentUri = MediaStore.Files.getContentUri("external")
                        val path = getDataColumn(context, contentUri, "_id=?", arrayOf(fileId))
                        if (path != null && File(path).exists()) return path
                    }
                    try {
                        val contentUri = ContentUris.withAppendedId(
                            Uri.parse("content://downloads/public_downloads"),
                            id.toLong()
                        )
                        val path = getDataColumn(context, contentUri, null, null)
                        if (path != null && File(path).exists()) return path
                    } catch (e: Exception) {
                        Log.w(TAG, "Failed to resolve download id $id: ${e.message}")
                    }
                }
            }
            // MediaProvider
            else if ("com.android.providers.media.documents" == authority) {
                val docId = DocumentsContract.getDocumentId(uri)
                val split = docId.split(":")
                val type = split.getOrNull(0) ?: ""
                val id = split.getOrNull(1)

                val contentUri = when (type.lowercase()) {
                    "image" -> MediaStore.Images.Media.EXTERNAL_CONTENT_URI
                    "video" -> MediaStore.Video.Media.EXTERNAL_CONTENT_URI
                    "audio" -> MediaStore.Audio.Media.EXTERNAL_CONTENT_URI
                    else -> MediaStore.Files.getContentUri("external")
                }

                if (id != null) {
                    val path = getDataColumn(context, contentUri, "_id=?", arrayOf(id))
                    if (path != null && File(path).exists()) return path
                }
            }
        }
        // MediaStore (general content://)
        else if (ContentResolver.SCHEME_CONTENT.equals(uri.scheme, ignoreCase = true)) {
            val path = getDataColumn(context, uri, null, null)
            if (path != null && File(path).exists()) return path
        }
        // File URI (file://)
        else if (ContentResolver.SCHEME_FILE.equals(uri.scheme, ignoreCase = true)) {
            return uri.path
        }

        return null
    }

    private fun getDataColumn(
        context: Context,
        uri: Uri,
        selection: String?,
        selectionArgs: Array<String>?
    ): String? {
        var cursor: Cursor? = null
        val column = MediaStore.MediaColumns.DATA
        val projection = arrayOf(column)

        try {
            cursor = context.contentResolver.query(uri, projection, selection, selectionArgs, null)
            if (cursor != null && cursor.moveToFirst()) {
                val columnIndex = cursor.getColumnIndexOrThrow(column)
                return cursor.getString(columnIndex)
            }
        } catch (e: Exception) {
            Log.w(TAG, "getDataColumn failed for $uri: ${e.message}")
        } finally {
            cursor?.close()
        }
        return null
    }
}
