// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! [`Protobuf<T>`] — a transparent newtype that forwards
//! [`Encode`]/[`Decode`] through [`prost::Message`].
//!
//! Useful when the schema's source of truth is a `.proto` file and
//! the storage layer should hold the corresponding generated Rust
//! message verbatim. Wrap the generated type in [`Protobuf`] at the
//! [`DbMap`](crate::DbMap) boundary; the byte representation on
//! disk is the message's canonical protobuf serialization.
//!
//! # Use without the wrapper
//!
//! Callers who write their own [`Encode`]/[`Decode`] impls
//! (perhaps a struct with a protobuf-encoded field alongside
//! other fields) can reuse the same encoding path without wrapping
//! via the module-level [`encode`], [`encode_into`], and
//! [`decode`] free functions. The wrapper's trait impls delegate
//! to them, so the byte representation is identical either way.
//!
//! # Sort order
//!
//! Protobuf's wire format is *not* sort-stable: changing a field's
//! value can reorder its bytes arbitrarily, and the wire format
//! interleaves tag numbers with values. Do not use
//! [`Protobuf<T>`] as a key whose sort order is meaningful.
//! Restrict its use to values or to keys where iteration order
//! does not matter.

use std::fmt;
use std::ops::Deref;
use std::ops::DerefMut;

use bytes::Buf;
use bytes::BufMut;

use crate::Decode;
use crate::Encode;
use crate::error::DecodeError;
use crate::error::EncodeError;

/// Transparent wrapper that delegates [`Encode`]/[`Decode`] to
/// [`prost::Message::encode`] / [`prost::Message::decode`].
///
/// The inner value is exposed via [`Deref`] / [`DerefMut`], the
/// public `0` field, and [`into_inner`](Self::into_inner). The
/// wrapper carries no runtime overhead beyond the trait dispatch.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Protobuf<T>(pub T);

impl<T> Protobuf<T> {
    /// Wrap `value`.
    pub fn new(value: T) -> Self {
        Self(value)
    }

    /// Unwrap into the inner value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: prost::Message> Encode for Protobuf<T> {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        encode_into(&self.0, buf)
    }
}

impl<T: prost::Message + Default> Decode for Protobuf<T> {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        decode(buf).map(Protobuf)
    }
}

/// Encode a [`prost::Message`] directly to bytes — equivalent to
/// `Protobuf(value.clone()).encode()` but without the wrapping.
///
/// Useful when [`Encode`] is being implemented by hand for a
/// composite key/value and one of its fields happens to be a
/// protobuf message.
pub fn encode<T: prost::Message>(value: &T) -> Result<Vec<u8>, EncodeError> {
    let mut buf = Vec::new();
    encode_into(value, &mut buf)?;
    Ok(buf)
}

/// Encode a [`prost::Message`] into an existing buffer — the
/// in-place form of [`encode`], suitable for sharing one buffer
/// across multiple field encodings.
///
/// Errors from [`prost::Message::encode`] are wrapped as
/// [`EncodeError`] with the prost error preserved as the source.
pub fn encode_into<T: prost::Message, B: BufMut>(
    value: &T,
    buf: &mut B,
) -> Result<(), EncodeError> {
    value
        .encode(buf)
        .map_err(|e| EncodeError::with_source("prost encode failed", e))
}

/// Decode a [`prost::Message`] directly from a buffer — equivalent
/// to `Protobuf::<T>::decode(buf).map(Protobuf::into_inner)` but
/// without the wrapping.
///
/// Reads from `buf` until prost reports the message is complete or
/// the buffer is exhausted. Errors from [`prost::Message::decode`]
/// are wrapped as [`DecodeError`] with the prost error preserved
/// as the source.
pub fn decode<T: prost::Message + Default, B: Buf>(buf: &mut B) -> Result<T, DecodeError> {
    // `Message::decode` takes `impl Buf` by value but `&mut B`
    // satisfies the `Buf` trait via the standard blanket impl,
    // so we can forward the borrow without consuming the outer
    // buffer.
    T::decode(buf).map_err(|e| DecodeError::with_source("prost decode failed", e))
}

impl<T> From<T> for Protobuf<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}

impl<T> Deref for Protobuf<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> DerefMut for Protobuf<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: fmt::Debug> fmt::Debug for Protobuf<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Protobuf").field(&self.0).finish()
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::CfDescriptor;
    use crate::Db;
    use crate::DbMap;
    use crate::DbOptions;
    use crate::Schema;
    use crate::error::OpenError;

    /// Minimal prost message used by the round-trip tests.
    #[derive(Clone, PartialEq, prost::Message)]
    struct TestMsg {
        #[prost(string, tag = "1")]
        name: String,
        #[prost(uint64, tag = "2")]
        value: u64,
    }

    #[derive(Debug)]
    struct PbSchema {
        items: DbMap<Vec<u8>, Protobuf<TestMsg>>,
    }

    impl Schema for PbSchema {
        fn cfs(opts: &crate::options::CfOptionsResolver) -> Vec<CfDescriptor> {
            vec![CfDescriptor::new("items", opts.options("items"))]
        }

        fn open(db: &Db) -> Result<Self, OpenError> {
            Ok(Self {
                items: DbMap::new(db.clone(), "items")?,
            })
        }
    }

    fn msg(name: &str, value: u64) -> TestMsg {
        TestMsg {
            name: name.to_string(),
            value,
        }
    }

    #[test]
    fn encode_decode_round_trip() {
        let original = Protobuf(msg("hello", 42));
        let bytes = original.encode().unwrap();
        let decoded = Protobuf::<TestMsg>::decode(&mut &bytes[..]).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn empty_message_round_trips() {
        // Prost messages with all default-valued fields encode to
        // zero bytes; decode of an empty buffer should yield the
        // default value.
        let original = Protobuf(TestMsg::default());
        let bytes = original.encode().unwrap();
        assert!(bytes.is_empty());
        let decoded = Protobuf::<TestMsg>::decode(&mut &bytes[..]).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn decode_invalid_bytes_errors() {
        // A non-empty buffer whose first byte does not look like a
        // valid prost tag-and-wire-type byte should error.
        let bytes = [0xFFu8, 0x00];
        let err = Protobuf::<TestMsg>::decode(&mut &bytes[..]).unwrap_err();
        assert!(
            err.to_string().contains("prost decode failed"),
            "unexpected error: {err}",
        );
    }

    #[test]
    fn into_inner_returns_owned_value() {
        let wrapped = Protobuf::new(msg("x", 7));
        let inner = wrapped.into_inner();
        assert_eq!(inner.name, "x");
        assert_eq!(inner.value, 7);
    }

    #[test]
    fn deref_exposes_inner_fields() {
        let wrapped = Protobuf::new(msg("foo", 1));
        // Deref means we can read fields without unwrapping first.
        assert_eq!(wrapped.name, "foo");
        assert_eq!(wrapped.value, 1);
    }

    #[test]
    fn from_t_wraps() {
        let wrapped: Protobuf<TestMsg> = msg("bar", 9).into();
        assert_eq!(wrapped.0.name, "bar");
    }

    #[test]
    fn free_encode_matches_wrapper_encode() {
        let m = msg("free", 7);
        let direct = super::encode(&m).unwrap();
        let via_wrapper = Protobuf(m.clone()).encode().unwrap();
        assert_eq!(direct, via_wrapper);
    }

    #[test]
    fn free_decode_matches_wrapper_decode() {
        let m = msg("free", 7);
        let bytes = super::encode(&m).unwrap();
        let direct: TestMsg = super::decode(&mut &bytes[..]).unwrap();
        let via_wrapper = Protobuf::<TestMsg>::decode(&mut &bytes[..]).unwrap();
        assert_eq!(direct, via_wrapper.into_inner());
        assert_eq!(direct, m);
    }

    #[test]
    fn free_encode_into_appends() {
        // Encode several fields into one buffer to mirror the
        // hand-rolled `Encode` use case (mixed protobuf + non-
        // protobuf fields in the same encoded value).
        let mut buf = Vec::new();
        buf.put_slice(&7u32.to_be_bytes());
        super::encode_into(&msg("composite", 1), &mut buf).unwrap();

        // First four bytes are the BE u32 we prepended; the rest is
        // the protobuf-encoded message.
        assert_eq!(&buf[..4], &7u32.to_be_bytes());
        let decoded: TestMsg = super::decode(&mut &buf[4..]).unwrap();
        assert_eq!(decoded, msg("composite", 1));
    }

    #[test]
    fn free_decode_invalid_bytes_errors() {
        let bytes = [0xFFu8, 0x00];
        let err = super::decode::<TestMsg, _>(&mut &bytes[..]).unwrap_err();
        assert!(
            err.to_string().contains("prost decode failed"),
            "unexpected error: {err}",
        );
    }

    /// Demonstrates the hand-rolled `Encode`/`Decode` use case the
    /// free helpers are meant to serve: a struct with mixed
    /// non-protobuf and protobuf fields whose owner does not want
    /// to wrap each field in `Protobuf`.
    #[derive(Debug, PartialEq)]
    struct Composite {
        version: u32,
        payload: TestMsg,
    }

    impl Encode for Composite {
        fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
            buf.put_u32(self.version);
            super::encode_into(&self.payload, buf)
        }
    }

    impl Decode for Composite {
        fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
            if buf.remaining() < 4 {
                return Err(DecodeError::msg("Composite truncated header"));
            }
            let version = buf.get_u32();
            let payload = super::decode(buf)?;
            Ok(Self { version, payload })
        }
    }

    #[test]
    fn composite_round_trip_uses_free_helpers() {
        let original = Composite {
            version: 9,
            payload: msg("composite", 99),
        };
        let bytes = original.encode().unwrap();
        let decoded = Composite::decode(&mut &bytes[..]).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn round_trips_through_dbmap() {
        // End-to-end: store a Protobuf-wrapped message in a CF and
        // read it back. Confirms the Encode/Decode impls integrate
        // with the typed DbMap path.
        let dir = TempDir::new().unwrap();
        let (_db, schema) = Db::open::<PbSchema>(dir.path(), DbOptions::default()).unwrap();

        let key = b"k".to_vec();
        let value = Protobuf(msg("payload", 1729));

        let mut batch = _db.batch();
        batch.put(&schema.items, &key, &value).unwrap();
        batch.commit().unwrap();

        let got = schema.items.get(&key).unwrap();
        assert_eq!(got, Some(value));
    }
}
