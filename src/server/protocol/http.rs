//! Functions related to HTTP parsing.

use tokio::io::{self, AsyncReadExt};
use tokio::net::TcpStream;

use crate::server::protocol::command::{Command, ParseError};

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

/// Parse `a=b&c=d` and return the value slice for `name` if present (no allocations).
fn find_param<'a>(query: &'a [u8], name: &[u8]) -> Option<&'a [u8]> {
    let mut i = 0;
    while i <= query.len() {
        let seg_end = query[i..]
            .iter()
            .position(|&c| c == b'&')
            .map(|p| i + p)
            .unwrap_or(query.len());

        let seg = &query[i..seg_end];
        if let Some(eq) = find_byte(seg, b'=') {
            let k = &seg[..eq];
            if k == name {
                return Some(&seg[eq + 1..]);
            }
        }

        if seg_end == query.len() {
            break;
        }
        i = seg_end + 1;
    }
    None
}

/// Parses only the request line: `METHOD SP TARGET SP HTTP/VERSION`.
/// The returned slices borrow from `request_line`.
pub fn parse_command_from_request_line<'a>(
    request_line: &'a [u8],
) -> Result<Command<'a>, ParseError> {
    // METHOD
    let sp1 = find_byte(request_line, b' ').ok_or(ParseError::BadRequestLine)?;
    let method = &request_line[..sp1];

    // TARGET
    let rest = &request_line[sp1 + 1..];
    let sp2 = find_byte(rest, b' ').ok_or(ParseError::BadRequestLine)?;
    let target = &rest[..sp2];

    // Split target into path + query
    let (path, query) = match find_byte(target, b'?') {
        Some(q) => (&target[..q], Some(&target[q + 1..])),
        None => (target, None),
    };

    match method {
        b"GET" => {
            // Get the number of keys stored
            if path == b"/dbkeysnum" {
                return Ok(Command::DbKeysNumber);
            }

            // Get a list of the keys stored
            if path == b"/dbkeys" {
                return Ok(Command::DbKeys);
            }

            // Get a list of the values stored
            if path == b"/dbvals" {
                return Ok(Command::DbVals);
            }

            // Get a list of the key/value pairs stored
            if path == b"/dbkeysvals" {
                return Ok(Command::DbKeyAndVals);
            }

            if path != b"/get" {
                return Err(ParseError::UnsupportedPath);
            }
            let q = query.ok_or(ParseError::MissingParam("key"))?;
            let key = find_param(q, b"key").ok_or(ParseError::MissingParam("key"))?;
            Ok(Command::Get { key })
        }
        b"DELETE" => {
            if path != b"/del" {
                return Err(ParseError::UnsupportedPath);
            }
            let q = query.ok_or(ParseError::MissingParam("key"))?;
            let key = find_param(q, b"key").ok_or(ParseError::MissingParam("key"))?;
            Ok(Command::Del { key })
        }
        b"POST" => {
            if path != b"/set" {
                return Err(ParseError::UnsupportedPath);
            }
            let q = query.ok_or(ParseError::MissingParam("key/value"))?;
            let key = find_param(q, b"key").ok_or(ParseError::MissingParam("key"))?;
            let value = find_param(q, b"value").ok_or(ParseError::MissingParam("value"))?;
            Ok(Command::Set { key, value })
        }
        _ => Err(ParseError::UnsupportedMethod),
    }
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
