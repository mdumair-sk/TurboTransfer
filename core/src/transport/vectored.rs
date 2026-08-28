use std::io::{Error, ErrorKind, IoSlice};
use tokio::io::{AsyncWrite, AsyncWriteExt};

/// Asynchronously writes both `header` and `payload` slices to `writer` using OS-level vectored I/O (`writev` / `WSASend`).
///
/// Handles partial writes in a loop until all bytes across both slices have been completely transferred.
pub async fn write_all_vectored<W: AsyncWrite + Unpin>(
    writer: &mut W,
    header: &[u8],
    payload: &[u8],
) -> Result<(), Error> {
    if payload.is_empty() {
        return writer.write_all(header).await;
    }
    if header.is_empty() {
        return writer.write_all(payload).await;
    }

    let mut header_offset = 0;
    let mut payload_offset = 0;

    while header_offset < header.len() || payload_offset < payload.len() {
        if header_offset < header.len() {
            let mut slices = [
                IoSlice::new(&header[header_offset..]),
                IoSlice::new(&payload[payload_offset..]),
            ];
            let n = writer.write_vectored(&mut slices).await?;
            if n == 0 {
                return Err(Error::new(
                    ErrorKind::WriteZero,
                    "failed to write whole frame (zero bytes written)",
                ));
            }
            if header_offset + n <= header.len() {
                header_offset += n;
            } else {
                let header_rem = header.len() - header_offset;
                header_offset = header.len();
                payload_offset += n - header_rem;
            }
        } else {
            // Header is completely written, finish writing remaining payload
            writer.write_all(&payload[payload_offset..]).await?;
            break;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_write_all_vectored() {
        let header = b"HEADER_1234";
        let payload = b"PAYLOAD_DATA_XYZ_56789";

        let mut output = Vec::new();
        write_all_vectored(&mut output, header, payload)
            .await
            .expect("Vectored write must succeed");

        let mut expected = Vec::new();
        expected.extend_from_slice(header);
        expected.extend_from_slice(payload);

        assert_eq!(output, expected);
    }

    #[tokio::test]
    async fn test_write_all_vectored_empty_payload() {
        let header = b"CONTROL_HEADER";
        let payload = b"";

        let mut output = Vec::new();
        write_all_vectored(&mut output, header, payload)
            .await
            .expect("Vectored write must succeed");

        assert_eq!(output, header);
    }
}
