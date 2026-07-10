// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Rust bindings for `sui::allowance` (SAMPLE for the native allowances proposal).
//!
//! Signing validates a tx's declared (funder, allowance) source against the
//! loaded object and reserves against the funder; execution creates the
//! `AllowanceWithdrawal<T>` that only the allowance's spend paths can unpack.

use crate::SUI_FRAMEWORK_ADDRESS;
use crate::base_types::SuiAddress;
use crate::error::{UserInputError, UserInputResult};
use crate::id::UID;
use crate::object::Object;
use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::{StructTag, TypeTag};
use move_core_types::u256::U256;
use serde::{Deserialize, Serialize};

pub const ALLOWANCE_MODULE_NAME: &IdentStr = ident_str!("allowance");
pub const ALLOWANCE_STRUCT_NAME: &IdentStr = ident_str!("Allowance");
pub const ALLOWANCE_WITHDRAWAL_STRUCT_NAME: &IdentStr = ident_str!("AllowanceWithdrawal");
pub const RESOLVED_ALLOWANCE_WITHDRAWAL_STRUCT: (&AccountAddress, &IdentStr, &IdentStr) = (
    &SUI_FRAMEWORK_ADDRESS,
    ALLOWANCE_MODULE_NAME,
    ALLOWANCE_WITHDRAWAL_STRUCT_NAME,
);

/// BCS mirror of `std::type_name::TypeName`.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct MoveTypeName {
    pub name: String,
}

/// BCS mirror of the Move struct `sui::allowance::RateLimit`.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct RateLimit {
    pub period_ms: u64,
    pub limit: U256,
    pub spent: U256,
    pub window_start_ms: u64,
}

/// BCS mirror of the Move struct `sui::allowance::Allowance<T>`.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Allowance {
    pub id: UID,
    pub funder: SuiAddress,
    pub spender: Option<SuiAddress>,
    pub app: Option<MoveTypeName>,
    pub lifetime_cap: Option<U256>,
    pub current_spend: U256,
    pub start_timestamp_ms: Option<u64>,
    pub expiration_timestamp_ms: Option<u64>,
    pub rate_limit: Option<RateLimit>,
    pub name: String,
}

impl Allowance {
    pub fn type_(type_param: TypeTag) -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: ALLOWANCE_MODULE_NAME.to_owned(),
            name: ALLOWANCE_STRUCT_NAME.to_owned(),
            type_params: vec![type_param],
        }
    }

    pub fn is_allowance(s: &StructTag) -> bool {
        s.address == SUI_FRAMEWORK_ADDRESS
            && s.module.as_ident_str() == ALLOWANCE_MODULE_NAME
            && s.name.as_ident_str() == ALLOWANCE_STRUCT_NAME
            && s.type_params.len() == 1
    }
}

/// Sign-time view of an `Allowance<T>`. The spender can rotate, so never
/// reuse a resolution across transactions.
#[derive(Debug, Clone)]
pub struct ResolvedAllowance {
    pub funder: SuiAddress,
    pub spender: Option<SuiAddress>,
    /// The accumulated type `T` of `Allowance<T>` (e.g. `Balance<SUI>`).
    pub funds_type: TypeTag,
    /// The most one tx could spend: min(lifetime remaining, rate-limit amount).
    /// The full rate amount counts, since the window may reset before execution.
    pub spend_limit: U256,
}

/// Parses an object as an `Allowance`, extracting the sign-time-relevant fields.
pub fn parse_allowance_object(object: &Object) -> UserInputResult<ResolvedAllowance> {
    let invalid = |error: String| UserInputError::InvalidWithdrawReservation { error };
    let id = object.id();
    let Some(move_obj) = object.data.try_as_move() else {
        return Err(invalid(format!("object {id} is not a Move object")));
    };
    let tag = move_obj.type_().clone().into();
    if !Allowance::is_allowance(&tag) {
        return Err(invalid(format!(
            "object {id} is not a sui::allowance::Allowance"
        )));
    }
    if !object.owner.is_shared() {
        return Err(invalid(format!("allowance {id} is not a shared object")));
    }
    let allowance: Allowance = bcs::from_bytes(move_obj.contents())
        .map_err(|e| invalid(format!("failed to deserialize allowance {id}: {e}")))?;
    let funds_type = tag
        .type_params
        .into_iter()
        .next()
        .expect("checked by is_allowance");
    let lifetime_remaining = allowance
        .lifetime_cap
        .map(|cap| cap.checked_sub(allowance.current_spend).unwrap_or(U256::zero()));
    let rate_limit_amount = allowance.rate_limit.as_ref().map(|rl| rl.limit);
    // The tightest limit present; issuance guarantees at least one.
    let spend_limit = lifetime_remaining
        .into_iter()
        .chain(rate_limit_amount)
        .min()
        .ok_or_else(|| invalid(format!("allowance {id} has no lifetime cap or rate limit")))?;
    Ok(ResolvedAllowance {
        funder: allowance.funder,
        spender: allowance.spender,
        funds_type,
        spend_limit,
    })
}
