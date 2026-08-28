package com.turbotransfer.data.source.local

import android.content.Context
import android.content.SharedPreferences
import com.turbotransfer.UriUtils
import com.turbotransfer.domain.model.HistoryItem
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import org.json.JSONArray
import org.json.JSONObject
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class HistoryLocalDataSource @Inject constructor(
    @ApplicationContext private val context: Context
) {
    private val prefsName = "turbotransfer_history"
    private val keyHistory = "history_json"

    private val _historyFlow = MutableStateFlow<List<HistoryItem>>(emptyList())
    val historyFlow = _historyFlow.asStateFlow()

    private val prefs: SharedPreferences by lazy {
        context.getSharedPreferences(prefsName, Context.MODE_PRIVATE)
    }

    init {
        loadHistory()
    }

    private fun loadHistory() {
        val raw = prefs.getString(keyHistory, "[]") ?: "[]"
        try {
            val arr = JSONArray(raw)
            val list = mutableListOf<HistoryItem>()
            for (i in 0 until arr.length()) {
                val obj = arr.getJSONObject(i)
                val fileSize = obj.optLong("fileSize", 0L)
                list.add(
                    HistoryItem(
                        id = obj.optString("id", ""),
                        fileName = obj.optString("fileName", "Unknown File"),
                        fileSize = fileSize,
                        formattedSize = obj.optString("formattedSize", UriUtils.formatFileSize(fileSize)),
                        filePath = obj.optString("filePath", ""),
                        isOutgoing = obj.optBoolean("isOutgoing", true),
                        timestamp = obj.optLong("timestamp", System.currentTimeMillis()),
                        formattedDate = obj.optString("formattedDate", ""),
                        durationMs = obj.optLong("durationMs", 0L),
                        avgSpeedMBps = obj.optDouble("avgSpeedMBps", 0.0),
                        peakSpeedMBps = obj.optDouble("peakSpeedMBps", 0.0),
                        usbSpeedMBps = obj.optDouble("usbSpeedMBps", 0.0),
                        wifiSpeedMBps = obj.optDouble("wifiSpeedMBps", 0.0),
                        peakUsbSpeedMBps = obj.optDouble("peakUsbSpeedMBps", obj.optDouble("usbSpeedMBps", 0.0)),
                        peakWifiSpeedMBps = obj.optDouble("peakWifiSpeedMBps", obj.optDouble("wifiSpeedMBps", 0.0)),
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
    fun addTransferRecord(record: HistoryItem) {
        val currentList = _historyFlow.value.toMutableList()
        currentList.removeAll { it.id == record.id }
        currentList.add(0, record)

        val trimmed = if (currentList.size > 100) currentList.subList(0, 100) else currentList
        _historyFlow.value = trimmed
        saveHistory(trimmed)
    }

    private fun saveHistory(list: List<HistoryItem>) {
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
                    put("peakUsbSpeedMBps", item.peakUsbSpeedMBps)
                    put("peakWifiSpeedMBps", item.peakWifiSpeedMBps)
                    put("status", item.status)
                }
                arr.put(obj)
            }
            prefs.edit().putString(keyHistory, arr.toString()).apply()
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
        prefs.edit().remove(keyHistory).apply()
    }
}
