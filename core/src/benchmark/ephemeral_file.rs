use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use uuid::Uuid;

pub struct EphemeralFile {
    pub path: PathBuf,
}

impl EphemeralFile {
    /// Creates an ephemeral file of `size_bytes` filled with pseudo-random, non-compressible binary data.
    /// Data is generated in streaming chunks to keep memory footprint negligible.
    pub fn create(size_bytes: u64, target_dir: Option<&Path>) -> std::io::Result<Self> {
        let dir = if let Some(d) = target_dir {
            d.to_path_buf()
        } else {
            std::env::temp_dir().join("turbotransfer_bench")
        };
        std::fs::create_dir_all(&dir)?;

        let filename = format!("bench_{}.bin", Uuid::new_v4());
        let path = dir.join(filename);
        let file = File::create(&path)?;
        let mut writer = BufWriter::with_capacity(4 * 1024 * 1024, file);

        let seed = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x853c49e6748fea9b)
            ^ 0xda942042e4dd58b5;

        let mut rng = XorShift64::new(seed);
        let chunk_size = 64 * 1024; // 64 KiB generation buffer
        let mut buffer = vec![0u8; chunk_size];
        let mut remaining = size_bytes;

        while remaining > 0 {
            let to_write = (remaining as usize).min(chunk_size);
            rng.fill_bytes(&mut buffer[..to_write]);
            writer.write_all(&buffer[..to_write])?;
            remaining -= to_write as u64;
        }

        writer.flush()?;
        Ok(Self { path })
    }
}

impl Drop for EphemeralFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0x5465737453656564 } else { seed },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        let mut chunks = dest.chunks_exact_mut(8);
        for chunk in chunks.by_ref() {
            let val = self.next_u64();
            chunk.copy_from_slice(&val.to_le_bytes());
        }
        let remainder = chunks.into_remainder();
        if !remainder.is_empty() {
            let val = self.next_u64();
            let bytes = val.to_le_bytes();
            remainder.copy_from_slice(&bytes[..remainder.len()]);
        }
    }
}
