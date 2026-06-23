// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Encoding traits for keys and values stored in the database.
//!
//! Encoding is a property of the Rust type, not of the column family.
//! Schema authors implement [`Encode`] and [`Decode`] on bespoke wrapper
//! types and pin the on-disk byte representation at the point each new
//! type is introduced. Two motivations:
//!
//! - Migration. Introducing a new wrapper type alongside an old one
//!   (with a different on-disk representation) is a non-disruptive way
//!   to evolve a schema. Dual-write the two types, then cut over.
//! - Explicit choice. The on-disk representation is a decision; binding
//!   the encoding to the type forces the author to make it deliberately.
//!
//! # The append contract is type-enforced
//!
//! [`Encode::encode_into`] takes `&mut impl BufMut`, which exposes
//! only `put_*` methods (no `clear`, `truncate`, or rewind). Call
//! sites can encode multiple values sequentially into one buffer
//! (recording offsets) and pass non-overlapping subslices to
//! functions that need several byte strings live at once.
//! `Vec<u8>: BufMut`, so the thread-local scratch buffer continues
//! to work; outside the `encode_into` call, the caller still has
//! `Vec`-level methods like `as_slice` and `len`.
//!
//! # Owned vs. borrowed values
//!
//! Only owned decode is supported in this version of the crate. A
//! borrowed-decode trait was scoped out and may return when a real
//! schema needs zero-copy value views; the byte path is already
//! reachable today via `DbMap::get_raw`.

use bytes::Buf;
use bytes::BufMut;
use bytes::Bytes;

use crate::error::DecodeError;
use crate::error::EncodeError;

/// Encode a value into bytes.
///
/// Implementations append to the supplied [`BufMut`]. Call sites may
/// pass a buffer that already contains data (for example, when
/// encoding several values into the same allocation); the
/// `BufMut`-only API guarantees previously-written bytes cannot be
/// overwritten or removed.
///
/// # Examples
///
/// ```
/// use bytes::BufMut;
///
/// use sui_consistent_store::Encode;
/// use sui_consistent_store::error::EncodeError;
///
/// struct U64BeKey(u64);
///
/// impl Encode for U64BeKey {
///     fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
///         buf.put_slice(&self.0.to_be_bytes());
///         Ok(())
///     }
/// }
///
/// let key = U64BeKey(42);
/// assert_eq!(key.encode().unwrap(), [0, 0, 0, 0, 0, 0, 0, 42]);
/// ```
pub trait Encode {
    /// Append the encoded form of `self` to `buf`.
    ///
    /// `BufMut` exposes only `put_*` methods, so existing bytes in
    /// `buf` cannot be modified or removed.
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError>;

    /// Encode `self` into a freshly allocated `Vec<u8>`.
    ///
    /// A convenience wrapper over [`encode_into`](Self::encode_into).
    /// Internal call sites in this crate prefer the in-place form so
    /// they can amortize buffer allocations across many encodes.
    fn encode(&self) -> Result<Vec<u8>, EncodeError> {
        let mut buf = Vec::new();
        self.encode_into(&mut buf)?;
        Ok(buf)
    }
}

/// Decode a value from bytes.
///
/// Implementations consume bytes from the supplied [`Buf`]. The
/// crate's call sites pass a buffer that contains exactly one
/// value's bytes; implementations should consume them all and
/// surface an error if leftover bytes remain in the buffer that
/// the implementation does not recognize.
///
/// # Examples
///
/// ```
/// use bytes::Buf;
/// use bytes::BufMut;
///
/// use sui_consistent_store::Decode;
/// use sui_consistent_store::Encode;
/// use sui_consistent_store::error::DecodeError;
/// use sui_consistent_store::error::EncodeError;
///
/// #[derive(Debug, PartialEq, Eq)]
/// struct U64BeKey(u64);
///
/// impl Encode for U64BeKey {
///     fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
///         buf.put_slice(&self.0.to_be_bytes());
///         Ok(())
///     }
/// }
///
/// impl Decode for U64BeKey {
///     fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
///         if buf.remaining() != 8 {
///             return Err(DecodeError::msg(format!(
///                 "expected 8 bytes for U64BeKey, got {}",
///                 buf.remaining(),
///             )));
///         }
///         Ok(U64BeKey(buf.get_u64()))
///     }
/// }
///
/// let bytes = U64BeKey(42).encode().unwrap();
/// assert_eq!(U64BeKey::decode(&mut &bytes[..]).unwrap(), U64BeKey(42));
/// ```
pub trait Decode: Sized {
    /// Decode a value from `buf`.
    ///
    /// `Buf` exposes only forward-consuming reads (no rewind). The
    /// implementation reads as many bytes as it needs; the crate's
    /// internal call sites supply a buffer containing exactly one
    /// value's encoded bytes, so implementations should consume the
    /// whole buffer and report an error otherwise.
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError>;
}

// Pass-through encoding for raw byte buffers. Useful for schemas
// whose key or value is opaque bytes — for example, a CF storing
// pre-serialized payloads from a higher layer.

impl Encode for Vec<u8> {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(self);
        Ok(())
    }
}

impl Decode for Vec<u8> {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        let mut out = Vec::with_capacity(buf.remaining());
        while buf.has_remaining() {
            let chunk = buf.chunk();
            out.extend_from_slice(chunk);
            let len = chunk.len();
            buf.advance(len);
        }
        Ok(out)
    }
}

impl Encode for Bytes {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(self);
        Ok(())
    }
}

impl Decode for Bytes {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        Ok(buf.copy_to_bytes(buf.remaining()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand-rolled big-endian `u64` for tests. Big-endian gives a
    /// prefix-preserving lexicographic ordering when used as a key.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct U64BeKey(u64);

    impl Encode for U64BeKey {
        fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
            buf.put_slice(&self.0.to_be_bytes());
            Ok(())
        }
    }

    impl Decode for U64BeKey {
        fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
            if buf.remaining() != 8 {
                return Err(DecodeError::msg(format!(
                    "expected 8 bytes for U64BeKey, got {}",
                    buf.remaining(),
                )));
            }
            Ok(U64BeKey(buf.get_u64()))
        }
    }

    #[test]
    fn round_trip() {
        let original = U64BeKey(0x0123_4567_89AB_CDEF);
        let bytes = original.encode().unwrap();
        let decoded = U64BeKey::decode(&mut &bytes[..]).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn encode_default_method_matches_encode_into() {
        let value = U64BeKey(42);
        let mut into_buf = Vec::new();
        value.encode_into(&mut into_buf).unwrap();
        assert_eq!(value.encode().unwrap(), into_buf);
    }

    #[test]
    fn encode_into_appends() {
        let mut buf = vec![0xAA, 0xBB, 0xCC];
        U64BeKey(1).encode_into(&mut buf).unwrap();
        assert_eq!(buf[..3], [0xAA, 0xBB, 0xCC]);
        assert_eq!(buf[3..], [0, 0, 0, 0, 0, 0, 0, 1]);
    }

    #[test]
    fn two_values_share_one_buffer() {
        // Demonstrates the call-site pattern used by `Batch::put` once
        // it lands: encode key, record offset, encode value, slice.
        let mut buf = Vec::new();
        U64BeKey(1).encode_into(&mut buf).unwrap();
        let k_end = buf.len();
        U64BeKey(2).encode_into(&mut buf).unwrap();

        let bytes = buf.as_slice();
        let k_slice = &bytes[..k_end];
        let v_slice = &bytes[k_end..];

        assert_eq!(U64BeKey::decode(&mut &*k_slice).unwrap(), U64BeKey(1));
        assert_eq!(U64BeKey::decode(&mut &*v_slice).unwrap(), U64BeKey(2));
    }

    #[test]
    fn decode_short_bytes_errors() {
        let err = U64BeKey::decode(&mut &[0, 0, 0, 0][..]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("got 4"), "unexpected message: {msg}");
    }

    #[test]
    fn decode_long_bytes_errors() {
        let err = U64BeKey::decode(&mut &[0u8; 9][..]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("got 9"), "unexpected message: {msg}");
    }
}
