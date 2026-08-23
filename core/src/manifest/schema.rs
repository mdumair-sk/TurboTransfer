use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransferRole {
    Sender,
    Receiver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferStatus {
    InProgress,
    Paused,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportStats {
    pub bytes: u64,
    pub errors: u64,
    pub retries: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportStatsMap {
    pub usb: TransportStats,
    pub wifi_direct: TransportStats,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferMeta {
    pub transfer_id: Uuid,
    pub file_id: Uuid,
    pub file_name: String,
    pub file_size: u64,
    pub chunk_size: u32,
    pub total_chunks: u32,
    pub role: TransferRole,
    pub peer_device_id: Uuid,
    pub status: TransferStatus,
    pub completed_ranges: Vec<(u32, u32)>,
    pub created_at: String,
    pub updated_at: String,
    pub transport_stats: TransportStatsMap,
}

impl TransferMeta {
    pub fn new(
        transfer_id: Uuid,
        file_id: Uuid,
        file_name: String,
        file_size: u64,
        chunk_size: u32,
        total_chunks: u32,
        role: TransferRole,
        peer_device_id: Uuid,
    ) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            transfer_id,
            file_id,
            file_name,
            file_size,
            chunk_size,
            total_chunks,
            role,
            peer_device_id,
            status: TransferStatus::InProgress,
            completed_ranges: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
            transport_stats: TransportStatsMap::default(),
        }
    }
}

/// Coalesces an in-memory `HashSet<u32>` of completed chunk IDs into a minimal,
/// sorted list of non-overlapping, non-adjacent inclusive `[start, end]` ranges.
pub fn coalesce_ranges(completed_chunks: &HashSet<u32>) -> Vec<(u32, u32)> {
    if completed_chunks.is_empty() {
        return Vec::new();
    }

    let mut chunk_ids: Vec<u32> = completed_chunks.iter().copied().collect();
    chunk_ids.sort_unstable();

    let mut ranges = Vec::new();
    let mut start = chunk_ids[0];
    let mut prev = start;

    for &id in &chunk_ids[1..] {
        if id == prev + 1 {
            prev = id;
        } else if id > prev + 1 {
            ranges.push((start, prev));
            start = id;
            prev = id;
        }
    }
    ranges.push((start, prev));

    ranges
}

/// Expands a list of inclusive `[start, end]` ranges back into an in-memory `HashSet<u32>`.
pub fn expand_ranges(ranges: &[(u32, u32)]) -> HashSet<u32> {
    let mut set = HashSet::new();
    for &(start, end) in ranges {
        for id in start..=end {
            set.insert(id);
        }
    }
    set
}

/// Represents file metadata and chunk count generated for a transfer offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileManifest {
    pub file_id: Uuid,
    pub file_name: String,
    pub file_size: u64,
    pub chunk_size: u32,
    pub total_chunks: u32,
}

/// Generates a `FileManifest` for the file at `file_path`.
pub fn generate_manifest<P: AsRef<std::path::Path>>(
    file_path: P,
    chunk_size: u32,
) -> Result<FileManifest, std::io::Error> {
    use std::io::Seek;
    let path = file_path.as_ref();
    let mut file = std::fs::File::open(path)?;
    let file_size = match file.metadata() {
        Ok(m) if m.len() > 0 => m.len(),
        _ => file.seek(std::io::SeekFrom::End(0))?,
    };
    let resolved_path = std::fs::read_link(path).unwrap_or_else(|_| path.to_path_buf());
    let file_name = resolved_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let num_chunks = crate::chunk::total_chunks(file_size, chunk_size);
    let file_id = Uuid::new_v4();

    Ok(FileManifest {
        file_id,
        file_name,
        file_size,
        chunk_size,
        total_chunks: num_chunks,
    })
}

