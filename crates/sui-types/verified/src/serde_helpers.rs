// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Minimal serde helpers needed for `AuthorityPublicKeyBytes` serialisation.
//! Copied from `sui-types::sui_serde` to avoid a dependency cycle:
//!
//!   sui-types  →  sui-types-verified  (desired)
//!   sui-types-verified  →  sui-types  (would cause cycle if not eliminated)

use serde_with::{DeserializeAs, SerializeAs};
use std::marker::PhantomData;

/// Chooses between a human-readable representation (`H`) and a compact
/// binary representation (`R`) based on the serialiser's mode.
///
/// This is identical to `sui_types::sui_serde::Readable` — see there for
/// usage examples.
pub struct Readable<H, R> {
    human_readable: PhantomData<H>,
    non_human_readable: PhantomData<R>,
}

impl<T: ?Sized, H, R> SerializeAs<T> for Readable<H, R>
where
    H: SerializeAs<T>,
    R: SerializeAs<T>,
{
    fn serialize_as<S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if serializer.is_human_readable() {
            H::serialize_as(value, serializer)
        } else {
            R::serialize_as(value, serializer)
        }
    }
}

impl<'de, R, H, T> DeserializeAs<'de, T> for Readable<H, R>
where
    H: DeserializeAs<'de, T>,
    R: DeserializeAs<'de, T>,
{
    fn deserialize_as<D>(deserializer: D) -> Result<T, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            H::deserialize_as(deserializer)
        } else {
            R::deserialize_as(deserializer)
        }
    }
}
