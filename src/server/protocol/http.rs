//! Functions related to HTTP parsing.

use tokio::io::{self, AsyncReadExt};
use tokio::net::TcpStream;

const MAX_HEADER_BYTES: usize = 32 * 1024; // Safety cap
const TMP_BUF_SIZE: usize = 4096;

/// Finds the position where `needle` appears in `haystack` if it exists within.
pub fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Find first occurrence of a byte in a slice.
pub fn find_byte(h: &[u8], b: u8) -> Option<usize> {
    h.iter().position(|&x| x == b)
}

/// Convert raw bytes to String (allocates). For now, require valid UTF-8.
/// (If you later want Redis-like binary-safe keys/values, switch to Vec<u8> keys/values.)
pub fn bytes_to_string(b: &[u8]) -> Result<String, ()> {
    std::str::from_utf8(b).map(|s| s.to_owned()).map_err(|_| ())
}

/// Reads until the end of headers (`\r\n\r\n`) with safety caps, returning:
/// - Full buffer (includes headers + any extra read).
/// - Index right after header delimiter.
pub async fn read_until_headers(stream: &mut TcpStream) -> io::Result<(Vec<u8>, usize)> {
    let mut buf: Vec<u8> = Vec::with_capacity(TMP_BUF_SIZE);
    let mut tmp = [0u8; TMP_BUF_SIZE];

    let header_end = loop {
        if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
            break pos + 4;
        }

        if buf.len() >= MAX_HEADER_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Headers too large",
            ));
        }

        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Client closed before sending full headers",
            ));
        }

        buf.extend_from_slice(&tmp[..n]);
    };

    Ok((buf, header_end))
}

/// Extract the request line (bytes) from within `buf[..header_end]`.
pub fn request_line_from_headers<'a>(buf: &'a [u8], header_end: usize) -> io::Result<&'a [u8]> {
    let headers = &buf[..header_end];
    let req_line_end = find_subslice(headers, b"\r\n")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Missing request line CRLF"))?;
    Ok(&buf[..req_line_end])
}
