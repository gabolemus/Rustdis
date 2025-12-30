//! Enum and errors used by the Rustdis server.

/// Different commands that can be applied on the datastore.
#[derive(Debug)]
pub enum Command<'a> {
    /// Get the `key` from the store.
    Get { key: &'a [u8] },
    /// Set the `key` to the given `value`. The `key` is overwritten if it already existed.
    Set { key: &'a [u8], value: &'a [u8] },
    /// Delete the `key` from the store if it exists.
    Del { key: &'a [u8] },
    /// Returns the number of keys stored.
    DbKeysNumber,
    /// Returns a list of all the keys stored.
    DbKeys,
    /// Returns a list of all the values stored.
    DbVals,
    /// Returns a list of all key/value pairs stored.
    DbKeyAndVals,
}

/// Errors that can occur while parsing the HTTP request.
#[derive(Debug)]
pub enum ParseError {
    /// Malformed HTTP request.
    BadRequestLine,
    /// Unsupported HTTP method.
    UnsupportedMethod,
    /// Unsupported HTTP endpoint.
    UnsupportedPath,
    /// The operation defined in the HTTP request is missing a parameter.
    MissingParam(&'static str),
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

/// Find first occurrence of a byte in a slice.
fn find_byte(h: &[u8], b: u8) -> Option<usize> {
    h.iter().position(|&x| x == b)
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
