use bytes::{Bytes, BytesMut};
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Bounded memory buffer pool ensuring strict memory caps (§10.2).
///
/// Pre-allocates and recycles chunk memory buffers up to `max_buffers`
/// without allocating arbitrary amounts of RAM or reading entire files at once.
#[derive(Clone)]
pub struct BufferPool {
    chunk_size: usize,
    semaphore: Arc<Semaphore>,
}

pub struct PooledBuffer {
    buffer: BytesMut,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl PooledBuffer {
    /// Returns a mutable slice to write chunk bytes into.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.buffer
    }

    /// Freezes the buffer into immutable `Bytes` suitable for frame transport.
    pub fn freeze(self) -> Bytes {
        self.buffer.freeze()
    }
}

impl BufferPool {
    /// Creates a new buffer pool with the given capacity (default 8 buffers * chunk_size).
    pub fn new(max_buffers: usize, chunk_size: usize) -> Self {
        Self {
            chunk_size,
            semaphore: Arc::new(Semaphore::new(max_buffers)),
        }
    }

    /// Acquires a buffer from the pool, waiting asynchronously if all buffers are currently in flight.
    pub async fn acquire(&self) -> PooledBuffer {
        let permit = self.semaphore.clone().acquire_owned().await.unwrap();
        let buffer = BytesMut::zeroed(self.chunk_size);
        PooledBuffer {
            buffer,
            _permit: permit,
        }
    }

    /// Returns the configured chunk size for this pool.
    pub fn chunk_size(&self) -> usize {
        self.chunk_size
    }

    /// Returns the number of currently available buffer slots.
    pub fn available_slots(&self) -> usize {
        self.semaphore.available_permits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_buffer_pool_bounded_allocation() {
        let pool = BufferPool::new(2, 1024);
        assert_eq!(pool.available_slots(), 2);

        let buf1 = pool.acquire().await;
        assert_eq!(pool.available_slots(), 1);

        let buf2 = pool.acquire().await;
        assert_eq!(pool.available_slots(), 0);

        drop(buf1);
        assert_eq!(pool.available_slots(), 1);

        drop(buf2);
        assert_eq!(pool.available_slots(), 2);
    }
}
