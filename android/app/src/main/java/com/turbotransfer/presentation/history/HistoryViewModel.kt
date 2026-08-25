package com.turbotransfer.presentation.history

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.turbotransfer.domain.usecase.history.ClearHistoryUseCase
import com.turbotransfer.domain.usecase.history.DeleteHistoryRecordUseCase
import com.turbotransfer.domain.usecase.history.GetHistoryUseCase
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch
import javax.inject.Inject

@HiltViewModel
class HistoryViewModel @Inject constructor(
    private val getHistoryUseCase: GetHistoryUseCase,
    private val deleteHistoryRecordUseCase: DeleteHistoryRecordUseCase,
    private val clearHistoryUseCase: ClearHistoryUseCase
) : ViewModel() {

    private val _uiState = MutableStateFlow(HistoryUiState())
    val uiState: StateFlow<HistoryUiState> = _uiState.asStateFlow()

    init {
        viewModelScope.launch {
            getHistoryUseCase().collect { list ->
                _uiState.update { it.copy(historyList = list) }
            }
        }
    }

    fun setShowClearDialog(show: Boolean) {
        _uiState.update { it.copy(showClearDialog = show) }
    }

    fun deleteRecord(id: String) {
        deleteHistoryRecordUseCase(id)
    }

    fun clearHistory() {
        clearHistoryUseCase()
        _uiState.update { it.copy(showClearDialog = false) }
    }
}
