// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::proto::TryFromProtoError;

use super::BalanceChange;

impl From<sui_sdk_types::BalanceChange> for BalanceChange {
    fn from(value: sui_sdk_types::BalanceChange) -> Self {
        Self {
            address: Some(value.address.to_string()),
            coin_type: Some(value.coin_type.to_string()),
            amount: Some(value.amount.to_string()),
        }
    }
}

impl TryFrom<&BalanceChange> for sui_sdk_types::BalanceChange {
    type Error = TryFromProtoError;

    fn try_from(value: &BalanceChange) -> Result<Self, Self::Error> {
        Ok(Self {
            address: value
                .address()
                .parse()
                .map_err(TryFromProtoError::from_error)?,
            coin_type: value
                .coin_type()
                .parse()
                .map_err(TryFromProtoError::from_error)?,
            amount: value
                .amount()
                .parse()
                .map_err(TryFromProtoError::from_error)?,
        })
    }
}
