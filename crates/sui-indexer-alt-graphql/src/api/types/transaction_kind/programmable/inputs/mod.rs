// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod balance_withdraw;
pub mod object;
pub mod pure;

use async_graphql::Union;

use sui_types::transaction::CallArg;
use sui_types::transaction::ObjectArg;

use crate::api::scalars::base64::Base64;
use crate::scope::Scope;

pub use balance_withdraw::BalanceWithdraw;
pub use object::OwnedOrImmutable;
pub use object::Receiving;
pub use object::SharedInput;
pub use pure::Pure;

/// Input argument to a Programmable Transaction Block (PTB) command.
#[derive(Union)]
pub enum TransactionInput {
    Pure(Pure),
    OwnedOrImmutable(OwnedOrImmutable),
    SharedInput(SharedInput),
    Receiving(Receiving),
    BalanceWithdraw(BalanceWithdraw),
}

impl TransactionInput {
    pub fn from(input: CallArg, scope: Scope) -> Self {
        match input {
            CallArg::Pure(bytes) => Self::Pure(Pure {
                bytes: Some(Base64::from(bytes)),
            }),
            CallArg::Object(obj_arg) => match obj_arg {
                ObjectArg::ImmOrOwnedObject((object_id, version, digest)) => {
                    Self::OwnedOrImmutable(OwnedOrImmutable::from_object_ref(
                        object_id, version, digest, scope,
                    ))
                }
                ObjectArg::SharedObject {
                    id,
                    initial_shared_version,
                    mutability,
                } => Self::SharedInput(SharedInput::from_shared_object(
                    id,
                    initial_shared_version,
                    // TODO: extend schema to expose the full mutability enum
                    mutability.is_exclusive(),
                )),
                ObjectArg::Receiving((object_id, version, digest)) => Self::Receiving(
                    Receiving::from_object_ref(object_id, version, digest, scope),
                ),
            },
            CallArg::FundsWithdrawal(withdrawal) => {
                Self::BalanceWithdraw(BalanceWithdraw::from_native(withdrawal, scope))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use move_core_types::account_address::AccountAddress;
    use sui_types::{
        transaction::{
            FundsWithdrawalArg, Reservation as NativeReservation, WithdrawFrom as NativeWithdrawFrom,
            WithdrawalTypeArg as NativeWithdrawalTypeArg,
        },
        type_input::{StructInput, TypeInput as NativeTypeInput},
    };

    use super::balance_withdraw::{WithdrawFrom, WithdrawalReservation};

    #[test]
    fn test_from_funds_withdrawal() {
        let withdrawal = FundsWithdrawalArg {
            reservation: NativeReservation::MaxAmountU64(42),
            type_arg: NativeWithdrawalTypeArg::Balance(NativeTypeInput::Struct(Box::new(
                StructInput {
                    address: AccountAddress::from_hex_literal("0x2").unwrap(),
                    module: "sui".to_string(),
                    name: "SUI".to_string(),
                    type_params: vec![],
                },
            ))),
            withdraw_from: NativeWithdrawFrom::Sender,
        };
        let expected_type_tag = withdrawal.type_arg.to_type_tag().unwrap();

        let input = TransactionInput::from(
            sui_types::transaction::CallArg::FundsWithdrawal(withdrawal),
            Scope::for_tests(),
        );

        let TransactionInput::BalanceWithdraw(withdraw) = input else {
            panic!("expected BalanceWithdraw input");
        };

        let Some(WithdrawalReservation::MaxAmountU64(reservation)) = withdraw.reservation else {
            panic!("expected MaxAmountU64 reservation");
        };
        assert_eq!(reservation.amount.map(u64::from), Some(42));
        assert!(matches!(withdraw.withdraw_from, Some(WithdrawFrom::Sender)));

        let type_tag = withdraw
            .type_arg
            .as_ref()
            .and_then(|t| t.to_type_tag())
            .unwrap();
        assert_eq!(type_tag, expected_type_tag);
    }
}
