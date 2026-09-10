package com.turbotransfer.presentation.history

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.turbotransfer.data.repository.HistoryRepositoryImpl
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch
import javax.inject.Inject

@HiltViewModel
class HistoryViewModel @Inject constructor(
    private val historyRepository: HistoryRepositoryImpl
) : ViewModel() {

    private val _uiState = MutableStateFlow(HistoryUiState())
    val uiState: StateFlow<HistoryUiState> = _uiState.asStateFlow()

    init {
        viewModelScope.launch {
            historyRepository.historyFlow.collect { list ->
                _uiState.update { it.copy(historyList = list) }
            }
        }
    }

    fun setShowClearDialog(show: Boolean) {
        _uiState.update { it.copy(showClearDialog = show) }
    }

    fun deleteRecord(id: String) {
        historyRepository.deleteRecord(id)
    }

    fun clearHistory() {
        historyRepository.clearHistory()
        _uiState.update { it.copy(showClearDialog = false) }
    }
}
