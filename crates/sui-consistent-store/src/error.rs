// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Error types for the crate.
//!
//! Error structs in this module wrap a boxed inner so they fit in a
//! single pointer on the success path of `Result<T, _>`. The boxed
//! payload is allocated only when an error actually occurs.

use std::borrow::Cow;
use std::error::Error as StdError;
use std::fmt;

/// Type-erased dynamic error used as the source on error chains.
type DynError = Box<dyn StdError + Send + Sync + 'static>;

/// An error returned by [`Encode::encode_into`].
///
/// Carries a free-form message and an optional source error. The
/// struct is one pointer wide; the payload lives on the heap and is
/// allocated only when an error actually fires.
///
/// [`Encode::encode_into`]: crate::Encode::encode_into
///
/// # Examples
///
/// ```
/// use sui_consistent_store::error::EncodeError;
///
/// let e = EncodeError::msg("buffer too small");
/// assert_eq!(e.to_string(), "encode failed: buffer too small");
/// ```
#[derive(Debug)]
pub struct EncodeError(Box<EncodeErrorInner>);

#[derive(Debug)]
struct EncodeErrorInner {
    message: Cow<'static, str>,
    source: Option<DynError>,
}

/// An error returned by [`Decode::decode`].
///
/// Carries a free-form message and an optional source error. The
/// struct is one pointer wide; the payload lives on the heap and is
/// allocated only when an error actually fires.
///
/// [`Decode::decode`]: crate::Decode::decode
///
/// # Examples
///
/// ```
/// use sui_consistent_store::error::DecodeError;
///
/// let e = DecodeError::msg("expected 8 bytes, got 4");
/// assert_eq!(e.to_string(), "decode failed: expected 8 bytes, got 4");
/// ```
#[derive(Debug)]
pub struct DecodeError(Box<DecodeErrorInner>);

#[derive(Debug)]
struct DecodeErrorInner {
    message: Cow<'static, str>,
    source: Option<DynError>,
}

impl EncodeError {
    /// Construct an error from a message alone.
    pub fn msg(message: impl Into<Cow<'static, str>>) -> Self {
        Self(Box::new(EncodeErrorInner {
            message: message.into(),
            source: None,
        }))
    }

    /// Construct an error with a message and an underlying source.
    ///
    /// The source is exposed via [`std::error::Error::source`] so that
    /// callers walking the error chain can recover the original
    /// failure.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::error::Error as StdError;
    /// use std::io;
    ///
    /// use sui_consistent_store::error::EncodeError;
    ///
    /// let io_err = io::Error::other("disk full");
    /// let e = EncodeError::with_source("write failed", io_err);
    /// assert!(e.source().is_some());
    /// ```
    pub fn with_source(message: impl Into<Cow<'static, str>>, source: impl Into<DynError>) -> Self {
        Self(Box::new(EncodeErrorInner {
            message: message.into(),
            source: Some(source.into()),
        }))
    }
}

impl DecodeError {
    /// Construct an error from a message alone.
    pub fn msg(message: impl Into<Cow<'static, str>>) -> Self {
        Self(Box::new(DecodeErrorInner {
            message: message.into(),
            source: None,
        }))
    }

    /// Construct an error with a message and an underlying source.
    ///
    /// The source is exposed via [`std::error::Error::source`] so that
    /// callers walking the error chain can recover the original
    /// failure.
    pub fn with_source(message: impl Into<Cow<'static, str>>, source: impl Into<DynError>) -> Self {
        Self(Box::new(DecodeErrorInner {
            message: message.into(),
            source: Some(source.into()),
        }))
    }
}

/// An error returned when opening a database.
///
/// Carries a free-form message and an optional source error. Most
/// underlying failures (`rocksdb::Error`, schema construction errors,
/// and so on) are exposed via the source chain.
///
/// # Examples
///
/// ```
/// use sui_consistent_store::error::OpenError;
///
/// let e = OpenError::msg("database directory missing");
/// assert_eq!(e.to_string(), "open failed: database directory missing");
/// ```
#[derive(Debug)]
pub struct OpenError(Box<OpenErrorInner>);

#[derive(Debug)]
struct OpenErrorInner {
    message: Cow<'static, str>,
    source: Option<DynError>,
}

impl OpenError {
    /// Construct an error from a message alone.
    pub fn msg(message: impl Into<Cow<'static, str>>) -> Self {
        Self(Box::new(OpenErrorInner {
            message: message.into(),
            source: None,
        }))
    }

    /// Construct an error with a message and an underlying source.
    ///
    /// The source is exposed via [`std::error::Error::source`] so that
    /// callers walking the error chain can recover the original
    /// failure.
    pub fn with_source(message: impl Into<Cow<'static, str>>, source: impl Into<DynError>) -> Self {
        Self(Box::new(OpenErrorInner {
            message: message.into(),
            source: Some(source.into()),
        }))
    }
}

impl From<rocksdb::Error> for OpenError {
    fn from(err: rocksdb::Error) -> Self {
        Self::with_source("rocksdb operation failed", err)
    }
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "encode failed: {}", self.0.message)
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "decode failed: {}", self.0.message)
    }
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "open failed: {}", self.0.message)
    }
}

impl StdError for EncodeError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.0
            .source
            .as_deref()
            .map(|e| e as &(dyn StdError + 'static))
    }
}

impl StdError for DecodeError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.0
            .source
            .as_deref()
            .map(|e| e as &(dyn StdError + 'static))
    }
}

impl StdError for OpenError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.0
            .source
            .as_deref()
            .map(|e| e as &(dyn StdError + 'static))
    }
}

/// Top-level error type returned by database operations.
///
/// This is the error type exposed by the read and write methods on
/// typed column-family handles. Each variant wraps a more specific
/// failure mode and can be matched on directly. Marked
/// `#[non_exhaustive]` so future variants are added without a
/// breaking change for callers.
///
/// # Examples
///
/// ```
/// use sui_consistent_store::error::DecodeError;
/// use sui_consistent_store::error::Error;
///
/// let e: Error = DecodeError::msg("bad bytes").into();
/// assert!(matches!(e, Error::Decode(_)));
/// ```
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A key or value could not be encoded.
    #[error(transparent)]
    Encode(#[from] EncodeError),

    /// A value could not be decoded after a successful read.
    #[error(transparent)]
    Decode(#[from] DecodeError),

    /// The underlying RocksDB operation failed.
    #[error(transparent)]
    Rocksdb(#[from] rocksdb::Error),

    /// A column family expected by a typed handle is not registered
    /// on the database. This indicates a programmer error in the
    /// schema definition or in `DbMap` construction.
    #[error("column family `{0}` is not registered")]
    MissingColumnFamily(String),

    /// A defensive invariant was violated by an underlying RocksDB
    /// component (for example, a raw iterator that reports `valid`
    /// but yields `None` for its current key or value). This should
    /// not occur in practice; if it does, the operation is aborted
    /// and the error is surfaced rather than silently swallowed.
    #[error("internal invariant violated: {0}")]
    Internal(&'static str),

    /// A resume cursor passed to
    /// [`iter_prefix_from`](crate::DbMap::iter_prefix_from) does not
    /// lie within the prefix it claims to resume: its encoded bytes
    /// do not start with the prefix's encoding. Scanning from it
    /// would leak rows from outside the prefix into the result, so
    /// the call is rejected instead. Cursors typically derive from
    /// client-supplied page tokens, so callers should treat this as
    /// invalid input rather than a storage fault.
    #[error("resume cursor does not lie within the iteration prefix")]
    CursorOutsidePrefix,
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;

    #[test]
    fn encode_error_size_is_one_pointer() {
        assert_eq!(
            std::mem::size_of::<EncodeError>(),
            std::mem::size_of::<usize>(),
        );
    }

    #[test]
    fn decode_error_size_is_one_pointer() {
        assert_eq!(
            std::mem::size_of::<DecodeError>(),
            std::mem::size_of::<usize>(),
        );
    }

    #[test]
    fn encode_error_display() {
        let e = EncodeError::msg("oops");
        assert_eq!(e.to_string(), "encode failed: oops");
    }

    #[test]
    fn decode_error_display() {
        let e = DecodeError::msg("nope");
        assert_eq!(e.to_string(), "decode failed: nope");
    }

    #[test]
    fn encode_error_source_chain() {
        let inner = io::Error::other("underlying");
        let e = EncodeError::with_source("wrapper", inner);
        let src = StdError::source(&e).expect("source should be set");
        assert_eq!(src.to_string(), "underlying");
    }

    #[test]
    fn decode_error_source_chain() {
        let inner = io::Error::new(io::ErrorKind::InvalidData, "bad bytes");
        let e = DecodeError::with_source("wrapper", inner);
        let src = StdError::source(&e).expect("source should be set");
        assert_eq!(src.to_string(), "bad bytes");
    }

    #[test]
    fn encode_error_no_source_when_msg_only() {
        let e = EncodeError::msg("alone");
        assert!(StdError::source(&e).is_none());
    }

    #[test]
    fn decode_error_accepts_owned_or_static_message() {
        let owned = DecodeError::msg(String::from("owned"));
        let borrowed = DecodeError::msg("static");
        assert_eq!(owned.to_string(), "decode failed: owned");
        assert_eq!(borrowed.to_string(), "decode failed: static");
    }

    #[test]
    fn open_error_size_is_one_pointer() {
        assert_eq!(
            std::mem::size_of::<OpenError>(),
            std::mem::size_of::<usize>(),
        );
    }

    #[test]
    fn open_error_display() {
        let e = OpenError::msg("missing path");
        assert_eq!(e.to_string(), "open failed: missing path");
    }

    #[test]
    fn open_error_source_chain() {
        let inner = io::Error::other("disk full");
        let e = OpenError::with_source("wrapper", inner);
        let src = StdError::source(&e).expect("source should be set");
        assert_eq!(src.to_string(), "disk full");
    }
}
