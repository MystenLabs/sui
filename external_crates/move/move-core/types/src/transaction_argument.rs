// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{account_address::AccountAddress, u256, value::MoveValue};
use anyhow::{anyhow, Error, Result};
use serde::{Deserialize, Serialize};
use std::{convert::TryFrom, fmt};

#[derive(Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum TransactionArgument {
    U8(u8),
    U64(u64),
    U128(u128),
    Address(AccountAddress),
    U8Vector(#[serde(with = "serde_bytes")] Vec<u8>),
    Bool(bool),
    // NOTE: Added in bytecode version v6, do not reorder!
    U16(u16),
    U32(u32),
    U256(u256::U256),
}

impl fmt::Debug for TransactionArgument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransactionArgument::U8(value) => write!(f, "{{U8: {}}}", value),
            TransactionArgument::U64(value) => write!(f, "{{U64: {}}}", value),
            TransactionArgument::U128(value) => write!(f, "{{U128: {}}}", value),
            TransactionArgument::Bool(boolean) => write!(f, "{{BOOL: {}}}", boolean),
            TransactionArgument::Address(address) => write!(f, "{{ADDRESS: {:?}}}", address),
            TransactionArgument::U8Vector(vector) => {
                write!(f, "{{U8Vector: 0x{}}}", hex::encode(vector))
            }
            TransactionArgument::U16(value) => write!(f, "{{U16: {}}}", value),
            TransactionArgument::U32(value) => write!(f, "{{U32: {}}}", value),
            TransactionArgument::U256(value) => write!(f, "{{U256: {}}}", value),
        }
    }
}

impl From<TransactionArgument> for MoveValue {
    fn from(val: TransactionArgument) -> Self {
        match val {
            TransactionArgument::U8(i) => MoveValue::U8(i),
            TransactionArgument::U64(i) => MoveValue::U64(i),
            TransactionArgument::U128(i) => MoveValue::U128(i),
            TransactionArgument::Address(a) => MoveValue::Address(a),
            TransactionArgument::Bool(b) => MoveValue::Bool(b),
            TransactionArgument::U8Vector(v) => MoveValue::vector_u8(v),
            TransactionArgument::U16(i) => MoveValue::U16(i),
            TransactionArgument::U32(i) => MoveValue::U32(i),
            TransactionArgument::U256(i) => MoveValue::U256(i),
        }
    }
}

impl TryFrom<MoveValue> for TransactionArgument {
    type Error = Error;
    fn try_from(val: MoveValue) -> Result<Self> {
        Ok(match val {
            MoveValue::U8(i) => TransactionArgument::U8(i),
            MoveValue::U64(i) => TransactionArgument::U64(i),
            MoveValue::U128(i) => TransactionArgument::U128(i),
            MoveValue::Address(a) => TransactionArgument::Address(a),
            MoveValue::Bool(b) => TransactionArgument::Bool(b),
            MoveValue::Vector(v) => TransactionArgument::U8Vector(
                v.into_iter()
                    .map(|mv| {
                        if let MoveValue::U8(byte) = mv {
                            Ok(byte)
                        } else {
                            Err(anyhow!("unexpected value in bytes: {:?}", mv))
                        }
                    })
                    .collect::<Result<Vec<u8>>>()?,
            ),
            MoveValue::Signer(_) | MoveValue::Struct(_) => {
                return Err(anyhow!("invalid transaction argument: {:?}", val))
            }
            MoveValue::U16(i) => TransactionArgument::U16(i),
            MoveValue::U32(i) => TransactionArgument::U32(i),
            MoveValue::U256(i) => TransactionArgument::U256(i),
        })
    }
}

/// Convert the transaction arguments into Move values.
pub fn convert_txn_args(args: &[TransactionArgument]) -> Vec<Vec<u8>> {
    args.iter()
        .map(|arg| {
            MoveValue::from(arg.clone())
                .simple_serialize()
                .expect("transaction arguments must serialize")
        })
        .collect()
}

/// Struct for encoding vector<vector<u8>> arguments for script functions
#[derive(Clone, Hash, Eq, PartialEq, Deserialize)]
pub struct VecBytes(Vec<serde_bytes::ByteBuf>);

impl VecBytes {
    pub fn from(vec_bytes: Vec<Vec<u8>>) -> Self {
        VecBytes(
            vec_bytes
                .into_iter()
                .map(serde_bytes::ByteBuf::from)
                .collect(),
        )
    }

    pub fn into_vec(self) -> Vec<Vec<u8>> {
        self.0
            .into_iter()
            .map(|byte_buf| byte_buf.into_vec())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::convert::{From, TryInto};

    use crate::{
        account_address::AccountAddress, transaction_argument::TransactionArgument, u256::U256,
        value::MoveValue,
    };

    #[test]
    fn test_from_and_to_move_value() {
        let vals = vec![
            TransactionArgument::U8(1),
            TransactionArgument::U16(u16::MAX),
            TransactionArgument::U32(u32::MAX),
            TransactionArgument::U64(u64::MAX),
            TransactionArgument::U128(u128::MAX),
            TransactionArgument::U256(U256::max_value()),
            TransactionArgument::Bool(true),
            TransactionArgument::Address(AccountAddress::from_hex_literal("0x1").unwrap()),
            TransactionArgument::U8Vector(vec![1, 2, 3, 4]),
        ];
        for val in vals {
            let ret: TransactionArgument = MoveValue::from(val.clone()).try_into().unwrap();
            assert_eq!(ret, val);
        }
    }
}
