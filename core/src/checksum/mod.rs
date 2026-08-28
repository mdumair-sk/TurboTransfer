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

/// Polynomial for Castagnoli CRC32C in reflected (LSB-first) representation.
const CRC32C_POLY: u32 = 0x82F63B78;

fn gf2_matrix_times(mat: &[u32; 32], mut vec: u32) -> u32 {
    let mut sum = 0u32;
    let mut idx = 0;
    while vec != 0 {
        if (vec & 1) != 0 {
            sum ^= mat[idx];
        }
        vec >>= 1;
        idx += 1;
    }
    sum
}

fn gf2_matrix_square(square: &mut [u32; 32], mat: &[u32; 32]) {
    for n in 0..32 {
        square[n] = gf2_matrix_times(mat, mat[n]);
    }
}

/// Combines two Castagnoli CRC32C checksums into the CRC32C checksum of their concatenation.
///
/// If `crc1 = CRC32C(A)` and `crc2 = CRC32C(B)` where `len2 = len(B)` in bytes,
/// then `crc32c_combine(crc1, crc2, len2) == CRC32C(A || B)`.
///
/// Operates in $O(\log \text{len2})$ time via $GF(2)$ Galois Field matrix exponentiation (< 1 microsecond).
pub fn crc32c_combine(crc1: u32, crc2: u32, mut len2: usize) -> u32 {
    if len2 == 0 {
        return crc1;
    }

    let mut even = [0u32; 32];
    let mut odd = [0u32; 32];

    // Initialize operator for 1-bit shift in GF(2) with Castagnoli polynomial
    even[0] = CRC32C_POLY;
    let mut row = 1u32;
    for n in 1..32 {
        even[n] = row;
        row <<= 1;
    }

    // Square 3 times to produce the 8-bit (1-byte) shift operator
    gf2_matrix_square(&mut odd, &even);
    gf2_matrix_square(&mut even, &odd);
    gf2_matrix_square(&mut odd, &even);
    even = odd;

    let mut c1 = crc1;
    loop {
        // Apply matrix corresponding to current bit of len2
        gf2_matrix_square(&mut odd, &even);
        if (len2 & 1) != 0 {
            c1 = gf2_matrix_times(&even, c1);
        }
        len2 >>= 1;
        if len2 == 0 {
            break;
        }

        // Apply matrix corresponding to next bit of len2
        gf2_matrix_square(&mut even, &odd);
        if (len2 & 1) != 0 {
            c1 = gf2_matrix_times(&odd, c1);
        }
        len2 >>= 1;
        if len2 == 0 {
            break;
        }
    }

    c1 ^ crc2
}

/// Streaming in-flight Castagnoli CRC32C accumulator.
#[derive(Debug, Clone, Copy, Default)]
pub struct Crc32cAccumulator {
    current_crc: u32,
    total_bytes: u64,
}

impl Crc32cAccumulator {
    /// Creates a new, empty CRC32C accumulator.
    pub fn new() -> Self {
        Self {
            current_crc: 0,
            total_bytes: 0,
        }
    }

    /// Appends a byte slice to the running CRC32C checksum.
    pub fn update(&mut self, data: &[u8]) {
        if !data.is_empty() {
            if self.total_bytes == 0 {
                self.current_crc = compute_crc32c(data);
            } else {
                self.current_crc = crc32c::crc32c_append(self.current_crc, data);
            }
            self.total_bytes += data.len() as u64;
        }
    }

    /// Appends a pre-computed chunk CRC with known chunk length using GF(2) combination.
    pub fn combine(&mut self, chunk_crc: u32, chunk_len: usize) {
        if chunk_len > 0 {
            if self.total_bytes == 0 {
                self.current_crc = chunk_crc;
            } else {
                self.current_crc = crc32c_combine(self.current_crc, chunk_crc, chunk_len);
            }
            self.total_bytes += chunk_len as u64;
        }
    }

    /// Finalizes and returns the completed Castagnoli CRC32C checksum.
    pub fn finalize(&self) -> u32 {
        self.current_crc
    }

    /// Returns the total bytes accumulated so far.
    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }
}

/// Computes Castagnoli CRC32C checksum over an entire file by reading it in 2 MiB blocks.
pub fn compute_file_crc32c<P: AsRef<Path>>(path: P) -> Result<u32, std::io::Error> {
    let mut file = File::open(path)?;
    let mut buffer = vec![0u8; 2 * 1024 * 1024]; // 2 MiB streaming buffer
    let mut current_crc = 0u32;
    let mut first_block = true;

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        if first_block {
            current_crc = crc32c::crc32c(&buffer[..bytes_read]);
            first_block = false;
        } else {
            current_crc = crc32c::crc32c_append(current_crc, &buffer[..bytes_read]);
        }
    }

    Ok(current_crc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc32c_combine_basic() {
        let part1 = b"Hello, ";
        let part2 = b"world! This is a test of in-flight Castagnoli CRC32C combination.";
        let full = [part1.as_slice(), part2.as_slice()].concat();

        let crc_part1 = compute_crc32c(part1);
        let crc_part2 = compute_crc32c(part2);
        let crc_full = compute_crc32c(&full);

        let combined = crc32c_combine(crc_part1, crc_part2, part2.len());
        assert_eq!(combined, crc_full, "Combined CRC must match full data CRC");
    }

    #[test]
    fn test_crc32c_combine_multi_chunks() {
        let chunk1 = vec![0xABu8; 4096];
        let chunk2 = vec![0xCDu8; 8192];
        let chunk3 = vec![0xEFu8; 65536];
        let chunk4 = vec![0x12u8; 1048576]; // 1 MiB

        let mut full = Vec::new();
        full.extend_from_slice(&chunk1);
        full.extend_from_slice(&chunk2);
        full.extend_from_slice(&chunk3);
        full.extend_from_slice(&chunk4);

        let expected_full_crc = compute_crc32c(&full);

        let mut acc = Crc32cAccumulator::new();
        acc.combine(compute_crc32c(&chunk1), chunk1.len());
        acc.combine(compute_crc32c(&chunk2), chunk2.len());
        acc.combine(compute_crc32c(&chunk3), chunk3.len());
        acc.combine(compute_crc32c(&chunk4), chunk4.len());

        assert_eq!(acc.finalize(), expected_full_crc);
        assert_eq!(acc.total_bytes(), full.len() as u64);
    }
}
