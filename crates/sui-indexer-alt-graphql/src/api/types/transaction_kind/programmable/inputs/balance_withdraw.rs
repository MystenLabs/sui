// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Enum;
use async_graphql::SimpleObject;
use async_graphql::Union;
use move_core_types::account_address::AccountAddress;
use sui_types::transaction::FundsWithdrawalArg as NativeFundsWithdrawalArg;
use sui_types::transaction::Reservation as NativeReservation;
use sui_types::transaction::WithdrawFrom as NativeWithdrawFrom;
use sui_types::transaction::WithdrawalTypeArg as NativeWithdrawalTypeArg;
use sui_types::type_input::StructInput;
use sui_types::type_input::TypeInput;

use crate::api::scalars::big_int::BigInt;
use crate::api::types::move_type::MoveType;
use crate::scope::Scope;

/// Input for withdrawing funds from an accumulator.
#[derive(SimpleObject, Clone)]
pub struct BalanceWithdraw {
    /// How much to withdraw from the accumulator.
    pub reservation: Option<WithdrawalReservation>,

    /// The type of the funds accumulator to withdraw from (e.g. `0x2::balance::Balance<0x2::sui::SUI>`).
    #[graphql(name = "type")]
    pub type_: Option<MoveType>,

    /// The account to withdraw funds from.
    pub withdraw_from: Option<WithdrawFrom>,
}

/// Reservation details for a withdrawal.
#[derive(Union, Clone)]
pub enum WithdrawalReservation {
    MaxAmountU64(WithdrawMaxAmountU64),
}

#[derive(SimpleObject, Clone)]
pub struct WithdrawMaxAmountU64 {
    pub amount: Option<BigInt>,
}

/// The account to withdraw funds from.
#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum WithdrawFrom {
    /// The funds are withdrawn from the transaction sender's account.
    Sender,
    /// The funds are withdrawn from the sponsor's account.
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
            NativeReservation::MaxAmountU64(amount) => {
                WithdrawalReservation::MaxAmountU64(WithdrawMaxAmountU64 {
                    amount: Some(amount.into()),
                })
            }
        });

        let withdraw_from = Some(match withdraw_from {
            NativeWithdrawFrom::Sender => WithdrawFrom::Sender,
            NativeWithdrawFrom::Sponsor => WithdrawFrom::Sponsor,
        });

        let type_ = {
            let NativeWithdrawalTypeArg::Balance(t) = type_arg;
            let balance_struct = StructInput {
                address: AccountAddress::TWO,
                module: "balance".to_string(),
                name: "Balance".to_string(),
                type_params: vec![t.into()],
            };
            Some(MoveType::from_input(
                TypeInput::Struct(Box::new(balance_struct)),
                scope,
            ))
        };

        Self {
            reservation,
            type_,
            withdraw_from,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use async_graphql::ScalarType;
    use sui_types::gas_coin::GAS;
    use sui_types::transaction::FundsWithdrawalArg;
    use sui_types::transaction::Reservation as NativeReservation;
    use sui_types::transaction::WithdrawFrom as NativeWithdrawFrom;
    use sui_types::transaction::WithdrawalTypeArg as NativeWithdrawalTypeArg;

    #[test]
    fn test_from_native() {
        let withdrawal = FundsWithdrawalArg {
            reservation: NativeReservation::MaxAmountU64(42),
            type_arg: NativeWithdrawalTypeArg::Balance(GAS::type_tag()),
            withdraw_from: NativeWithdrawFrom::Sender,
        };
        let expected_type_tag = withdrawal.type_arg.to_type_tag();

        let withdraw = BalanceWithdraw::from_native(withdrawal, Scope::for_tests());

        let Some(WithdrawalReservation::MaxAmountU64(reservation)) = withdraw.reservation else {
            panic!("expected MaxAmountU64 reservation");
        };
        assert_eq!(
            reservation.amount.as_ref().map(|a| {
                let async_graphql::Value::String(s) = a.to_value() else {
                    panic!("expected string value");
                };
                s
            }),
            Some("42".to_string())
        );
        assert!(matches!(withdraw.withdraw_from, Some(WithdrawFrom::Sender)));

        let type_tag = withdraw
            .type_
            .as_ref()
            .and_then(|t| t.to_type_tag())
            .unwrap();

        assert_eq!(type_tag, expected_type_tag);
    }
}
