package com.turbotransfer

import android.content.Context
import android.content.SharedPreferences
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import org.json.JSONArray
import org.json.JSONObject
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

data class TransferHistoryItem(
    val id: String,
    val fileName: String,
    val fileSize: Long,
    val formattedSize: String,
    val filePath: String,
    val isOutgoing: Boolean, // true = Sent, false = Received
    val timestamp: Long,
    val formattedDate: String,
    val durationMs: Long,
    val avgSpeedMBps: Double,
    val peakSpeedMBps: Double,
    val usbSpeedMBps: Double,
    val wifiSpeedMBps: Double,
    val status: String // Completed, Failed, Cancelled
)

object TransferHistoryManager {

    private const val PREFS_NAME = "turbotransfer_history"
    private const val KEY_HISTORY = "history_json"

    private val _historyFlow = MutableStateFlow<List<TransferHistoryItem>>(emptyList())
    val historyFlow = _historyFlow.asStateFlow()

    private var prefs: SharedPreferences? = null

    fun init(context: Context) {
        if (prefs == null) {
            prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
            loadHistory()
        }
    }

    private fun loadHistory() {
        val raw = prefs?.getString(KEY_HISTORY, "[]") ?: "[]"
        try {
            val arr = JSONArray(raw)
            val list = mutableListOf<TransferHistoryItem>()
            for (i in 0 until arr.length()) {
                val obj = arr.getJSONObject(i)
                list.add(
                    TransferHistoryItem(
                        id = obj.optString("id", ""),
                        fileName = obj.optString("fileName", "Unknown File"),
                        fileSize = obj.optLong("fileSize", 0L),
                        formattedSize = obj.optString("formattedSize", UriUtils.formatFileSize(obj.optLong("fileSize", 0L))),
                        filePath = obj.optString("filePath", ""),
                        isOutgoing = obj.optBoolean("isOutgoing", true),
                        timestamp = obj.optLong("timestamp", System.currentTimeMillis()),
                        formattedDate = obj.optString("formattedDate", ""),
                        durationMs = obj.optLong("durationMs", 0L),
                        avgSpeedMBps = obj.optDouble("avgSpeedMBps", 0.0),
                        peakSpeedMBps = obj.optDouble("peakSpeedMBps", 0.0),
                        usbSpeedMBps = obj.optDouble("usbSpeedMBps", 0.0),
                        wifiSpeedMBps = obj.optDouble("wifiSpeedMBps", 0.0),
                        status = obj.optString("status", "Completed")
                    )
                )
            }
            _historyFlow.value = list
        } catch (e: Exception) {
            e.printStackTrace()
        }
    }

    @Synchronized
    fun addTransferRecord(
        id: String,
        fileName: String,
        fileSize: Long,
        filePath: String,
        isOutgoing: Boolean,
        durationMs: Long,
        avgSpeedMBps: Double,
        peakSpeedMBps: Double,
        usbSpeedMBps: Double,
        wifiSpeedMBps: Double,
        status: String
    ) {
        val now = System.currentTimeMillis()
        val sdf = SimpleDateFormat("MMM dd, yyyy HH:mm", Locale.getDefault())
        val formattedDate = sdf.format(Date(now))

        val item = TransferHistoryItem(
            id = id,
            fileName = fileName,
            fileSize = fileSize,
            formattedSize = UriUtils.formatFileSize(fileSize),
            filePath = filePath,
            isOutgoing = isOutgoing,
            timestamp = now,
            formattedDate = formattedDate,
            durationMs = durationMs,
            avgSpeedMBps = avgSpeedMBps,
            peakSpeedMBps = peakSpeedMBps,
            usbSpeedMBps = usbSpeedMBps,
            wifiSpeedMBps = wifiSpeedMBps,
            status = status
        )

        val currentList = _historyFlow.value.toMutableList()
        // Deduplicate if same ID exists
        currentList.removeAll { it.id == id }
        currentList.add(0, item) // newest first

        // Keep maximum 100 history items
        val trimmed = if (currentList.size > 100) currentList.subList(0, 100) else currentList
        _historyFlow.value = trimmed
        saveHistory(trimmed)
    }

    private fun saveHistory(list: List<TransferHistoryItem>) {
        try {
            val arr = JSONArray()
            for (item in list) {
                val obj = JSONObject().apply {
                    put("id", item.id)
                    put("fileName", item.fileName)
                    put("fileSize", item.fileSize)
                    put("formattedSize", item.formattedSize)
                    put("filePath", item.filePath)
                    put("isOutgoing", item.isOutgoing)
                    put("timestamp", item.timestamp)
                    put("formattedDate", item.formattedDate)
                    put("durationMs", item.durationMs)
                    put("avgSpeedMBps", item.avgSpeedMBps)
                    put("peakSpeedMBps", item.peakSpeedMBps)
                    put("usbSpeedMBps", item.usbSpeedMBps)
                    put("wifiSpeedMBps", item.wifiSpeedMBps)
                    put("status", item.status)
                }
                arr.put(obj)
            }
            prefs?.edit()?.putString(KEY_HISTORY, arr.toString())?.apply()
        } catch (e: Exception) {
            e.printStackTrace()
        }
    }

    @Synchronized
    fun deleteRecord(id: String) {
        val currentList = _historyFlow.value.toMutableList()
        currentList.removeAll { it.id == id }
        _historyFlow.value = currentList
        saveHistory(currentList)
    }

    @Synchronized
    fun clearHistory() {
        _historyFlow.value = emptyList()
        prefs?.edit()?.remove(KEY_HISTORY)?.apply()
    }
}
