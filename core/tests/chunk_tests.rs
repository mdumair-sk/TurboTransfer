use std::io::Write;
use tempfile::NamedTempFile;
use turbotransfer_core::checksum::{compute_crc32c, compute_file_crc32c, compute_xxhash64};
use turbotransfer_core::chunk::{calculate_chunk_plan, create_chunk, read_chunk_at, total_chunks, ChunkPlanEntry};
use turbotransfer_core::manifest::generate_manifest;
use uuid::Uuid;

#[test]
fn test_total_chunks_formula() {
    assert_eq!(total_chunks(0, 64), 0);
    assert_eq!(total_chunks(1, 64), 1);
    assert_eq!(total_chunks(63, 64), 1);
    assert_eq!(total_chunks(64, 64), 1);
    assert_eq!(total_chunks(65, 64), 2);
    assert_eq!(total_chunks(128, 64), 2);
    assert_eq!(total_chunks(130, 64), 3);
    assert_eq!(total_chunks(100, 0), 0);
}

#[test]
fn test_chunk_boundary_math_zero_byte_file() {
    let plan = calculate_chunk_plan(0, 64);
    assert!(plan.is_empty());
}

#[test]
fn test_chunk_boundary_math_single_byte_file() {
    let plan = calculate_chunk_plan(1, 64);
    assert_eq!(
        plan,
        vec![ChunkPlanEntry {
            chunk_id: 0,
            file_offset: 0,
            payload_length: 1,
        }]
    );
}

#[test]
fn test_chunk_boundary_math_exact_multiple() {
    let plan = calculate_chunk_plan(128, 64);
    assert_eq!(
        plan,
        vec![
            ChunkPlanEntry {
                chunk_id: 0,
                file_offset: 0,
                payload_length: 64,
            },
            ChunkPlanEntry {
                chunk_id: 1,
                file_offset: 64,
                payload_length: 64,
            },
        ]
    );
}

#[test]
fn test_chunk_boundary_math_with_remainder() {
    let plan = calculate_chunk_plan(130, 64);
    assert_eq!(
        plan,
        vec![
            ChunkPlanEntry {
                chunk_id: 0,
                file_offset: 0,
                payload_length: 64,
            },
            ChunkPlanEntry {
                chunk_id: 1,
                file_offset: 64,
                payload_length: 64,
            },
            ChunkPlanEntry {
                chunk_id: 2,
                file_offset: 128,
                payload_length: 2,
            },
        ]
    );
}

#[test]
fn test_xxhash64_reference_vectors() {
    // xxHash64 reference vectors (seed 0)
    assert_eq!(compute_xxhash64(b""), 0xEF46DB3751D8E999);
    assert_eq!(compute_xxhash64(b"123456789"), 10139926970967174787u64);
}




#[test]
fn test_crc32c_reference_vectors() {
    // Official Castagnoli CRC32C reference vectors
    assert_eq!(compute_crc32c(b""), 0x00000000);
    assert_eq!(compute_crc32c(b"123456789"), 0xE3069283);
    assert_eq!(
        compute_crc32c(b"The quick brown fox jumps over the lazy dog"),
        0x22620404
    );
}

#[test]
fn test_file_chunk_reading_and_checksum() {
    let mut temp_file = NamedTempFile::new().unwrap();
    let data = b"Hello TurboTransfer Chunk Engine!";
    temp_file.write_all(data).unwrap();
    temp_file.flush().unwrap();

    let path = temp_file.path();

    // 1. Check file CRC32C
    let file_crc = compute_file_crc32c(path).unwrap();
    assert_eq!(file_crc, compute_crc32c(data));

    // 2. Read chunk at offset
    let chunk_bytes = read_chunk_at(path, 6, 13).unwrap();
    assert_eq!(&chunk_bytes[..], b"TurboTransfer");

    // 3. Create full Chunk struct
    let t_id = Uuid::new_v4();
    let f_id = Uuid::new_v4();
    let entry = ChunkPlanEntry {
        chunk_id: 0,
        file_offset: 0,
        payload_length: data.len() as u32,
    };
    let chunk = create_chunk(t_id, f_id, &entry, path).unwrap();

    assert_eq!(chunk.transfer_id, t_id);
    assert_eq!(chunk.file_id, f_id);
    assert_eq!(chunk.chunk_id, 0);
    assert_eq!(chunk.file_offset, 0);
    assert_eq!(chunk.payload_length, data.len() as u32);
    assert_eq!(chunk.checksum, compute_xxhash64(data));
    assert_eq!(&chunk.payload[..], data);
}

#[test]
fn test_manifest_generator() {
    let mut temp_file = NamedTempFile::new().unwrap();
    let data = vec![0u8; 150];
    temp_file.write_all(&data).unwrap();
    temp_file.flush().unwrap();

    let path = temp_file.path();
    let manifest = generate_manifest(path, 64).unwrap();

    assert_eq!(manifest.file_size, 150);
    assert_eq!(manifest.chunk_size, 64);
    assert_eq!(manifest.total_chunks, 3);
}
