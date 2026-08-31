use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use uuid::Uuid;

use crate::checksum::compute_xxhash64;

/// Represents a single stateless data-plane chunk (§5.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chunk {
    pub transfer_id: Uuid,
    pub file_id: Uuid,
    /// Sequence index, 0-based
    pub chunk_id: u32,
    pub file_offset: u64,
    pub payload_length: u32,
    /// xxHash64 of payload
    pub checksum: u64,
    pub payload: Bytes,
}

/// Represents the planned offset and size for a chunk without reading file contents into memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkPlanEntry {
    pub chunk_id: u32,
    pub file_offset: u64,
    pub payload_length: u32,
}

/// Selects an optimal chunk size dynamically based on file size and transport profile (§4a).
///
/// - Tiny files (< 4 MiB): 256 KiB – 512 KiB (low pipeline latency, fine-grained progress)
/// - Medium files (4 MiB – 128 MiB): 1 MiB (standard Wi-Fi Direct streaming)
/// - Large files (> 128 MiB): 2 MiB – 4 MiB (maximum wire efficiency for multi-gigabit bonded links)
pub fn select_optimal_chunk_size(file_size: u64, _is_high_speed_link: bool) -> u32 {
    if file_size < 1024 * 1024 {
        256 * 1024 // 256 KiB
    } else if file_size < 4 * 1024 * 1024 {
        512 * 1024 // 512 KiB
    } else if file_size < 64 * 1024 * 1024 {
        1024 * 1024 // 1 MiB
    } else {
        2 * 1024 * 1024 // 2 MiB (Optimal wire & pipeline sizing for 100+ MB/s Wi-Fi Direct)
    }
}

/// Calculates the total number of chunks needed for a given file size and chunk size.
pub fn total_chunks(file_size: u64, chunk_size: u32) -> u32 {
    if file_size == 0 || chunk_size == 0 {
        return 0;
    }
    ((file_size + chunk_size as u64 - 1) / chunk_size as u64) as u32
}

/// Generates the chunk plan for a file given its size and chunk size.
pub fn calculate_chunk_plan(file_size: u64, chunk_size: u32) -> Vec<ChunkPlanEntry> {
    let num_chunks = total_chunks(file_size, chunk_size);
    let mut plan = Vec::with_capacity(num_chunks as usize);

    for i in 0..num_chunks {
        let file_offset = i as u64 * chunk_size as u64;
        let remaining = file_size.saturating_sub(file_offset);
        let payload_length = remaining.min(chunk_size as u64) as u32;

        plan.push(ChunkPlanEntry {
            chunk_id: i,
            file_offset,
            payload_length,
        });
    }

    plan
}

/// Reads a specific chunk payload from disk at the given offset and length.
pub fn read_chunk_at<P: AsRef<Path>>(
    path: P,
    offset: u64,
    length: u32,
) -> Result<Bytes, std::io::Error> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;

    let mut buf = vec![0u8; length as usize];
    file.read_exact(&mut buf)?;
    Ok(Bytes::from(buf))
}

/// Reads a chunk from an open file handle directly into a destination buffer slice at the given offset.
pub fn read_chunk_into_slice(
    file: &mut File,
    offset: u64,
    dest: &mut [u8],
) -> Result<(), std::io::Error> {
    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(dest)?;
    Ok(())
}

/// Constructs a full `Chunk` struct from a `ChunkPlanEntry` and file on disk.
pub fn create_chunk<P: AsRef<Path>>(
    transfer_id: Uuid,
    file_id: Uuid,
    entry: &ChunkPlanEntry,
    file_path: P,
) -> Result<Chunk, std::io::Error> {
    let payload = read_chunk_at(file_path, entry.file_offset, entry.payload_length)?;
    let checksum = compute_xxhash64(&payload);

    Ok(Chunk {
        transfer_id,
        file_id,
        chunk_id: entry.chunk_id,
        file_offset: entry.file_offset,
        payload_length: entry.payload_length,
        checksum,
        payload,
    })
}
