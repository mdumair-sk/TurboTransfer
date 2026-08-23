use std::collections::HashSet;
use uuid::Uuid;

/// Trait defining the contract for tracking chunk completion and idempotent writes (§5.1 & §5.3).
pub trait ChunkTracker: Send + Sync {
    /// Checks if a chunk matching `(transfer_id, file_id, chunk_id, checksum)` has already been completed.
    fn is_chunk_completed(
        &self,
        transfer_id: Uuid,
        file_id: Uuid,
        chunk_id: u32,
        checksum: u64,
    ) -> bool;

    /// Marks a chunk as completed.
    fn mark_chunk_completed(
        &mut self,
        transfer_id: Uuid,
        file_id: Uuid,
        chunk_id: u32,
        checksum: u64,
    );

    /// Returns the coalesced list of completed chunk ID ranges `[start, end]` (inclusive).
    fn get_completed_ranges(&self) -> Option<Vec<(u32, u32)>>;
}

/// A simple isolated in-memory implementation of `ChunkTracker` for milestone 3.
#[derive(Debug, Default, Clone)]
pub struct InMemoryChunkTracker {
    completed: HashSet<(Uuid, Uuid, u32, u64)>,
}

impl InMemoryChunkTracker {
    pub fn new() -> Self {
        Self {
            completed: HashSet::new(),
        }
    }

    /// Initializes a tracker populated with pre-existing completed ranges (e.g. from meta.json for cold resume).
    pub fn from_ranges(transfer_id: Uuid, file_id: Uuid, ranges: &[(u32, u32)]) -> Self {
        let mut tracker = Self::new();
        for &(start, end) in ranges {
            for cid in start..=end {
                tracker.completed.insert((transfer_id, file_id, cid, 0));
            }
        }
        tracker
    }
}

impl ChunkTracker for InMemoryChunkTracker {
    fn is_chunk_completed(
        &self,
        transfer_id: Uuid,
        file_id: Uuid,
        chunk_id: u32,
        checksum: u64,
    ) -> bool {
        self.completed
            .contains(&(transfer_id, file_id, chunk_id, checksum))
            || self
                .completed
                .contains(&(transfer_id, file_id, chunk_id, 0))
    }

    fn mark_chunk_completed(
        &mut self,
        transfer_id: Uuid,
        file_id: Uuid,
        chunk_id: u32,
        checksum: u64,
    ) {
        self.completed
            .insert((transfer_id, file_id, chunk_id, checksum));
    }

    fn get_completed_ranges(&self) -> Option<Vec<(u32, u32)>> {
        if self.completed.is_empty() {
            return None;
        }

        let mut chunk_ids: Vec<u32> = self.completed.iter().map(|&(_, _, id, _)| id).collect();
        chunk_ids.sort_unstable();
        chunk_ids.dedup();

        let mut ranges = Vec::new();
        let mut start = chunk_ids[0];
        let mut prev = start;

        for &id in &chunk_ids[1..] {
            if id == prev + 1 {
                prev = id;
            } else {
                ranges.push((start, prev));
                start = id;
                prev = id;
            }
        }
        ranges.push((start, prev));

        Some(ranges)
    }
}
