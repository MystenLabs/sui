
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{Enum, SimpleObject, Union};
use sui_types::transaction::{
    FundsWithdrawalArg as NativeFundsWithdrawalArg, Reservation as NativeReservation,
    WithdrawFrom as NativeWithdrawFrom,
};

use crate::{api::scalars::uint53::UInt53, api::types::move_type::MoveType, scope::Scope};

/// Input for withdrawing funds from an accumulator.
#[derive(SimpleObject, Clone)]
pub struct BalanceWithdraw {
    pub reservation: Option<WithdrawalReservation>,

    /// The type of the funds accumulator to withdraw from (e.g. `0x2::balance::Balance<0x2::sui::SUI>`).
    pub type_arg: Option<MoveType>,

    pub withdraw_from: Option<WithdrawFrom>,
}

/// Reservation details for a withdrawal.
#[derive(Union, Clone)]
pub enum WithdrawalReservation {
    EntireBalance(EntireBalance),
    MaxAmountU64(MaxAmountU64),
}

#[derive(SimpleObject, Clone)]
pub struct EntireBalance {
    /// Placeholder field.
    #[graphql(name = "_")]
    pub dummy: Option<bool>,
}

#[derive(SimpleObject, Clone)]
pub struct MaxAmountU64 {
    pub amount: Option<UInt53>,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum WithdrawFrom {
    Sender,
    Sponsor,
}

impl BalanceWithdraw {
    pub fn from_native(withdrawal: NativeFundsWithdrawalArg, scope: Scope) -> Self {
        let NativeFundsWithdrawalArg {
            reservation,
            type_arg,
            withdraw_from,
        } = withdrawal;

        let reservation = Some(match reservation {
            NativeReservation::EntireBalance => {
                WithdrawalReservation::EntireBalance(EntireBalance { dummy: None })
            }
            NativeReservation::MaxAmountU64(amount) => {
                WithdrawalReservation::MaxAmountU64(MaxAmountU64 {
                    amount: Some(amount.into()),
                })
            }
        });

        let withdraw_from = Some(match withdraw_from {
            NativeWithdrawFrom::Sender => WithdrawFrom::Sender,
            NativeWithdrawFrom::Sponsor => WithdrawFrom::Sponsor,
        });

        let type_arg = type_arg
            .to_type_tag()
            .ok()
            .map(|tag| MoveType::from_native(tag, scope));

        Self {
            reservation,
            type_arg,
            withdraw_from,
        }
    }
}
