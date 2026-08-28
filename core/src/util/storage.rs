use std::fs::File;
use std::path::Path;

/// Issues OS-level kernel readahead advisories to optimize sequential reading of large files into page cache.
///
/// On Linux/Android, issues `posix_fadvise(..., POSIX_FADV_SEQUENTIAL | POSIX_FADV_WILLNEED)` to trigger
/// asynchronous page-cache population ahead of chunk reader threads.
pub fn advise_sequential_read(file: &File, file_size: u64) {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let fd = file.as_raw_fd();
        unsafe {
            libc::posix_fadvise(fd, 0, file_size as libc::off_t, libc::POSIX_FADV_SEQUENTIAL);
            libc::posix_fadvise(fd, 0, file_size as libc::off_t, libc::POSIX_FADV_WILLNEED);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (file, file_size);
    }
}

/// Opens a file for high-throughput sequential reading with platform-optimized kernel flags.
///
/// On Windows, sets `FILE_FLAG_SEQUENTIAL_SCAN` (0x08000000).
/// On Linux/Android, opens the file and applies `POSIX_FADV_SEQUENTIAL`.
pub fn open_sequential_read<P: AsRef<Path>>(path: P) -> std::io::Result<File> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_SEQUENTIAL_SCAN: u32 = 0x08000000;
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_SEQUENTIAL_SCAN)
            .open(path.as_ref())
    }
    #[cfg(unix)]
    {
        let file = File::open(path.as_ref())?;
        if let Ok(meta) = file.metadata() {
            advise_sequential_read(&file, meta.len());
        }
        Ok(file)
    }
    #[cfg(not(any(unix, windows)))]
    {
        File::open(path.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_open_sequential_read() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test_sequential.bin");
        {
            let mut f = File::create(&file_path).unwrap();
            f.write_all(b"SEQUENTIAL_DATA_TEST_12345").unwrap();
        }

        let file = open_sequential_read(&file_path).expect("Should open sequential file");
        let meta = file.metadata().unwrap();
        advise_sequential_read(&file, meta.len());
        assert_eq!(meta.len(), 26);
    }
}
