// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Rust bindings for `sui::gas_allowance`.

use crate::{
    SUI_FRAMEWORK_ADDRESS,
    balance::Balance,
    base_types::{ObjectID, SuiAddress},
    digests::TransactionDigest,
    dynamic_field::derive_dynamic_field_id,
    error::{UserInputError, UserInputResult},
    id::UID,
    object::Object,
};
use move_core_types::{
    ident_str,
    identifier::IdentStr,
    language_storage::{StructTag, TypeTag},
};
use mysten_common::debug_fatal;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use sui_protocol_config::ProtocolConfig;

pub const GAS_ALLOWANCE_MODULE_NAME: &IdentStr = ident_str!("gas_allowance");
pub const GAS_ALLOWANCE_STRUCT_NAME: &IdentStr = ident_str!("GasAllowance");

/// Mirror of the shared Move struct `sui::gas_allowance::GasAllowance`.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct GasAllowance {
    pub id: UID,
    pub funder: SuiAddress,
    pub spender: SuiAddress,
    pub balance: Balance,
    pub revoked: bool,
}

impl GasAllowance {
    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: GAS_ALLOWANCE_MODULE_NAME.to_owned(),
            name: GAS_ALLOWANCE_STRUCT_NAME.to_owned(),
            type_params: vec![],
        }
    }

    /// Checks the same policy against signing-time or consensus-sequenced escrow state.
    pub fn check_authorization(&self, spender: SuiAddress, budget: u64) -> UserInputResult<()> {
        let id = self.id.object_id();
        if self.spender != spender {
            return Err(UserInputError::IncorrectUserSignature {
                error: format!(
                    "Gas allowance {id} is assigned to {}, not transaction sender {spender}",
                    self.spender,
                ),
            });
        }
        if self.revoked {
            return Err(UserInputError::InvalidWithdrawReservation {
                error: format!("Gas allowance {id} is revoked"),
            });
        }
        if self.balance.value() < budget {
            return Err(UserInputError::GasBalanceTooLow {
                gas_balance: u128::from(self.balance.value()),
                needed_gas_amount: u128::from(budget),
            });
        }
        Ok(())
    }

    /// Parses an object only when it is the shared gas allowance object type.
    pub fn from_object(object: &Object) -> UserInputResult<Self> {
        let invalid = |error| UserInputError::InvalidWithdrawReservation { error };
        let id = object.id();
        let Some(move_obj) = object.data.try_as_move() else {
            return Err(invalid(format!(
                "Specified gas allowance {id} is not a Move object"
            )));
        };
        if !move_obj.type_().is(&Self::type_()) {
            return Err(invalid(format!(
                "Specified gas allowance {id} is not a sui::gas_allowance::GasAllowance"
            )));
        }
        if !object.owner.is_shared() {
            return Err(invalid(format!(
                "Gas allowance {id} is not a shared object"
            )));
        }
        let Ok(allowance) = bcs::from_bytes::<Self>(move_obj.contents()) else {
            debug_fatal!("gas allowance {id} did not match the `GasAllowance` rust type");
            return Err(invalid(format!("Failed to read gas allowance {id}")));
        };
        Ok(allowance)
    }
}

/// Native-only storage attribution, or an address-owned claim for a released protected deposit.
/// For an object-owned marker, `amount` is the protected portion of its parent's raw rebate.
/// An address-owned claim has `amount == 0` and holds the deferred refund in its own storage rebate.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct StorageRebate {
    pub id: UID,
    pub recipient: SuiAddress,
    pub amount: u64,
}

impl StorageRebate {
    /// Fixed prepayment bytes for every exclusive mutable input, including lookup misses.
    pub fn input_read_bytes(mutable_input_count: usize, config: &ProtocolConfig) -> usize {
        mutable_input_count.saturating_mul(config.storage_rebate_lookup_bytes() as usize)
    }

    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: GAS_ALLOWANCE_MODULE_NAME.to_owned(),
            name: ident_str!("StorageRebate").to_owned(),
            type_params: vec![],
        }
    }

    pub fn child_id(parent: ObjectID) -> ObjectID {
        Self::derive_id(parent, &[])
    }

    pub fn claim_id(recipient: SuiAddress, transaction: &TransactionDigest) -> ObjectID {
        // Nonempty keys are disjoint from object-owned markers and unique per transaction.
        Self::derive_id(recipient.into(), transaction.as_ref())
    }

    fn derive_id(parent: ObjectID, key: &[u8]) -> ObjectID {
        static TAG: LazyLock<TypeTag> =
            LazyLock::new(|| TypeTag::Struct(Box::new(StorageRebate::type_())));
        derive_dynamic_field_id(parent, &TAG, key).expect("fixed storage rebate type")
    }
}
