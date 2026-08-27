use bytes::{Bytes, BytesMut};
use std::sync::{Arc, Mutex};
use tokio::sync::Semaphore;

/// Bounded memory buffer pool ensuring strict memory caps (§10.2).
///
/// Pre-allocates and recycles chunk memory buffers up to `max_buffers`
/// without allocating arbitrary amounts of RAM or repeatedly allocating/freeing on the heap.
#[derive(Clone)]
pub struct BufferPool {
    chunk_size: usize,
    semaphore: Arc<Semaphore>,
    pool: Arc<Mutex<Vec<BytesMut>>>,
}

pub struct PooledBuffer {
    buffer: Option<BytesMut>,
    pool: Option<Arc<Mutex<Vec<BytesMut>>>>,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl PooledBuffer {
    /// Returns a mutable slice of exact length `len` to write chunk bytes into.
    pub fn get_mut_slice(&mut self, len: usize) -> &mut [u8] {
        let b = self.buffer.as_mut().expect("PooledBuffer already consumed");
        if b.capacity() < len {
            b.reserve(len - b.capacity());
        }
        if b.len() < len {
            b.resize(len, 0);
        } else {
            b.truncate(len);
        }
        &mut b[..len]
    }

    /// Returns a mutable reference to the underlying buffer.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        self.buffer.as_mut().expect("PooledBuffer already consumed")
    }

    /// Returns a slice view of the initialized buffer bytes.
    pub fn as_slice(&self) -> &[u8] {
        self.buffer.as_ref().expect("PooledBuffer already consumed")
    }

    /// Freezes the buffer into immutable `Bytes` suitable for frame transport.
    /// Note: This consumes the buffer from the pool recycling path.
    pub fn freeze(mut self) -> Bytes {
        let buf = self.buffer.take().expect("PooledBuffer already consumed");
        buf.freeze()
    }
}

impl Drop for PooledBuffer {
    fn drop(&mut self) {
        if let Some(mut buf) = self.buffer.take() {
            buf.clear();
            if let Some(pool) = &self.pool {
                if let Ok(mut lock) = pool.lock() {
                    lock.push(buf);
                }
            }
        }
    }
}

impl BufferPool {
    /// Creates a new buffer pool with the given capacity (default 8 buffers * chunk_size).
    pub fn new(max_buffers: usize, chunk_size: usize) -> Self {
        let mut preallocated = Vec::with_capacity(max_buffers);
        for _ in 0..max_buffers {
            preallocated.push(BytesMut::with_capacity(chunk_size));
        }

        Self {
            chunk_size,
            semaphore: Arc::new(Semaphore::new(max_buffers)),
            pool: Arc::new(Mutex::new(preallocated)),
        }
    }

    /// Acquires a buffer from the pool, waiting asynchronously if all buffers are currently in flight.
    pub async fn acquire(&self) -> PooledBuffer {
        let permit = self.semaphore.clone().acquire_owned().await.unwrap();
        let buf = {
            let mut lock = self.pool.lock().unwrap();
            lock.pop()
        }
        .unwrap_or_else(|| BytesMut::with_capacity(self.chunk_size));

        PooledBuffer {
            buffer: Some(buf),
            pool: Some(Arc::clone(&self.pool)),
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

    /// Returns the number of recycled buffers currently sitting idle in the pool.
    pub fn idle_count(&self) -> usize {
        self.pool.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_buffer_pool_bounded_allocation() {
        let pool = BufferPool::new(2, 1024);
        assert_eq!(pool.available_slots(), 2);
        assert_eq!(pool.idle_count(), 2);

        let buf1 = pool.acquire().await;
        assert_eq!(pool.available_slots(), 1);
        assert_eq!(pool.idle_count(), 1);

        let buf2 = pool.acquire().await;
        assert_eq!(pool.available_slots(), 0);
        assert_eq!(pool.idle_count(), 0);

        drop(buf1);
        assert_eq!(pool.available_slots(), 1);
        assert_eq!(pool.idle_count(), 1);

        drop(buf2);
        assert_eq!(pool.available_slots(), 2);
        assert_eq!(pool.idle_count(), 2);
    }

    #[tokio::test]
    async fn test_buffer_pool_slice_and_recycling() {
        let pool = BufferPool::new(1, 1024);
        {
            let mut buf = pool.acquire().await;
            let slice = buf.get_mut_slice(512);
            slice[0] = 42;
            slice[511] = 99;
            assert_eq!(buf.as_slice().len(), 512);
        }
        // Dropped -> returned to pool
        assert_eq!(pool.available_slots(), 1);
        assert_eq!(pool.idle_count(), 1);

        {
            let mut buf = pool.acquire().await;
            let slice = buf.get_mut_slice(256);
            assert_eq!(slice.len(), 256);
        }
    }
}

