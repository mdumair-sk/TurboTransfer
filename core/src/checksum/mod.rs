use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Computes xxHash64 checksum for the given byte payload using seed 0.
pub fn compute_xxhash64(data: &[u8]) -> u64 {
    xxhash_rust::xxh64::xxh64(data, 0)
}

/// Computes Castagnoli CRC32C checksum for the given byte slice.
pub fn compute_crc32c(data: &[u8]) -> u32 {
    crc32c::crc32c(data)
}

/// Computes Castagnoli CRC32C checksum over an entire file by reading it in 2 MiB blocks.
pub fn compute_file_crc32c<P: AsRef<Path>>(path: P) -> Result<u32, std::io::Error> {
    let mut file = File::open(path)?;
    let mut buffer = vec![0u8; 2 * 1024 * 1024]; // 2 MiB streaming buffer
    let mut current_crc = 0u32;

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        current_crc = crc32c::crc32c_append(current_crc, &buffer[..bytes_read]);
    }

    Ok(current_crc)
}
