// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use fastcrypto::encoding::{Base58, Encoding};
use serde::de::Deserializer;
use serde::{Deserialize, Serialize, Serializer};
use serde_with::{serde_as, Bytes, DeserializeAs, SerializeAs};
use std::fmt;
use std::marker::PhantomData;

const SUI_TRANSACTION_DIGEST_LENGTH: usize = 32;

#[serde_as]
#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Copy)]
pub(crate) struct TransactionDigest(
    #[serde_as(as = "Readable<Base58, Bytes>")] [u8; SUI_TRANSACTION_DIGEST_LENGTH],
);
scalar!(TransactionDigest, "TransactionDigest");

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
        S: Serializer,
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
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            H::deserialize_as(deserializer)
        } else {
            R::deserialize_as(deserializer)
        }
    }
}

impl TransactionDigest {
    pub fn into_array(self) -> [u8; SUI_TRANSACTION_DIGEST_LENGTH] {
        self.0
    }

    pub fn from_array(arr: [u8; SUI_TRANSACTION_DIGEST_LENGTH]) -> Self {
        TransactionDigest(arr)
    }
}
impl std::str::FromStr for TransactionDigest {
    type Err = InputValueError<String>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut result = [0u8; SUI_TRANSACTION_DIGEST_LENGTH];
        result.copy_from_slice(&Base58::decode(s).map_err(InputValueError::custom)?);
        Ok(TransactionDigest(result))
    }
}

impl fmt::Debug for TransactionDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("TransactionDigest")
            .field(&Base58::encode(self.0))
            .finish()
    }
}

impl fmt::LowerHex for TransactionDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "0x")?;
        }

        for byte in self.0 {
            write!(f, "{:02x}", byte)?;
        }

        Ok(())
    }
}

impl fmt::UpperHex for TransactionDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "0x")?;
        }

        for byte in self.0 {
            write!(f, "{:02X}", byte)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_from_value() {
        let digest = [
            183u8, 119, 223, 39, 204, 68, 220, 4, 126, 234, 232, 146, 106, 249, 98, 12, 170, 209,
            98, 203, 243, 77, 154, 225, 177, 216, 169, 101, 51, 116, 79, 223,
        ];
        assert_eq!(
            <TransactionDigest as ScalarType>::parse(Value::String(
                "DMBdBZnpYR4EeTXzXL8A6BtVafqGjAWGsFZhP2zJYmXU".to_string()
            ))
            .unwrap(),
            TransactionDigest(digest)
        );
        assert_eq!(
            <TransactionDigest as InputType>::parse(Some(Value::String(
                "DMBdBZnpYR4EeTXzXL8A6BtVafqGjAWGsFZhP2zJYmXU".to_string()
            )))
            .unwrap(),
            TransactionDigest(digest)
        );

        assert_eq!(
            Value::String("DMBdBZnpYR4EeTXzXL8A6BtVafqGjAWGsFZhP2zJYmXU".to_string()),
            async_graphql::ScalarType::to_value(&TransactionDigest(digest))
        );
        assert_eq!(
            <TransactionDigest as InputType>::parse(Some(Value::String(
                "DMBdBZnpYR4EeTXzXL8A6BtVafqGjAWGsFZhP2zJYmXU".to_string()
            )))
            .unwrap(),
            TransactionDigest(digest)
        );

        assert!(<TransactionDigest as ScalarType>::parse(Value::String(
            "DMBdBZnpYR4EeTXzXL8A6BtVafqGjAWGsFZhP2zJYmXUu".to_string()
        ))
        .is_err());
        assert!(<TransactionDigest as InputType>::parse(Some(Value::String(
            "DMBdBZnpYR4EeTXzXL8A6BtVafqGjAWGsFZhP2zJYmXUu".to_string()
        )))
        .is_err());

        assert!(<TransactionDigest as ScalarType>::parse(Value::String(
            "BjPzxBNEehY3JPsCBKwTgmUZt4VdJQZ5SYWnX2UGWu".to_string()
        ))
        .is_err());
        assert!(<TransactionDigest as InputType>::parse(Some(Value::String(
            "BUFtjBNEehY3JPsCBKwTgmUZt4VdJQZ5SYWnX2UGWu".to_string()
        )))
        .is_err());
    }
}
