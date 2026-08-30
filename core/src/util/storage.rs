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

pub fn preallocate_file(file: &File, file_size: u64) -> std::io::Result<()> {
    // Always set logical size first — required for read_exact / file cursor operations to work
    file.set_len(file_size)?;

    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let fd = file.as_raw_fd();
        // Advisory: pre-allocate contiguous blocks to avoid fragmentation and random write stalls
        let ret = unsafe {
            libc::posix_fallocate(fd, 0, file_size as libc::off_t)
        };
        if ret != 0 && ret != libc::EOPNOTSUPP && ret != libc::EINVAL {
            return Err(std::io::Error::from_raw_os_error(ret));
        }
    }

    Ok(())
}

/// Sanitizes an untrusted incoming filename to prevent path traversal (TRD §12, C1).
///
/// Strips directory components (`/`, `\`), traversal markers (`..`, `.`),
/// invalid filesystem characters, control characters, and leading/trailing whitespace/dots.
pub fn sanitize_filename(name: &str) -> String {
    // 1. Take only the file name component (removes any directory path prefix)
    let base = Path::new(name)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or(name);

    // 2. Filter out control characters, nulls, and illegal path separators
    let filtered: String = base
        .chars()
        .filter(|c| !c.is_control() && *c != '/' && *c != '\\' && *c != '\0')
        .collect();

    // 3. Trim whitespace and dots which could cause issues on Windows / POSIX
    let trimmed = filtered.trim().trim_matches('.').to_string();

    // 4. Reject relative path specifiers and empty names
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        "download_unnamed".to_string()
    } else {
        trimmed
    }
}

/// Resolves and validates that the target file and `.part` file reside strictly within `dest_dir`.
pub fn resolve_secure_paths<P: AsRef<Path>>(
    dest_dir: P,
    file_name: &str,
) -> Result<(std::path::PathBuf, std::path::PathBuf), std::io::Error> {
    let dest = dest_dir.as_ref();
    let safe_name = sanitize_filename(file_name);
    let part_path = dest.join(format!("{}.part", safe_name));
    let final_path = dest.join(&safe_name);

    if !part_path.starts_with(dest) || !final_path.starts_with(dest) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("Path traversal attempt detected in filename: {:?}", file_name),
        ));
    }

    Ok((part_path, final_path))
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

    #[test]
    fn test_sanitize_filename_traversal_prevention() {
        assert_eq!(sanitize_filename("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_filename("..\\..\\Windows\\System32\\cmd.exe"), "cmd.exe");
        assert_eq!(sanitize_filename("/var/log/syslog"), "syslog");
        assert_eq!(sanitize_filename("C:\\Users\\Admin\\evil.bat"), "evil.bat");
        assert_eq!(sanitize_filename("..."), "download_unnamed");
        assert_eq!(sanitize_filename(".."), "download_unnamed");
        assert_eq!(sanitize_filename("."), "download_unnamed");
        assert_eq!(sanitize_filename("  my report.pdf  "), "my report.pdf");
        assert_eq!(sanitize_filename("my\0evil\nfile.txt"), "myevilfile.txt");
    }

    #[test]
    fn test_resolve_secure_paths() {
        let temp_dir = tempfile::tempdir().unwrap();
        let dest = temp_dir.path();
        let (part, final_p) = resolve_secure_paths(dest, "../../evil.sh").unwrap();
        assert!(part.starts_with(dest));
        assert!(final_p.starts_with(dest));
        assert_eq!(final_p.file_name().unwrap(), "evil.sh");
        assert_eq!(part.file_name().unwrap(), "evil.sh.part");
    }
}
