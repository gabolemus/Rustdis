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
