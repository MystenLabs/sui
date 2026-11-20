// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Rust bindings for `sui::allowance`.
//!
//! At signing, a transaction's declared (funder, allowance) pair is checked against the loaded
//! `Allowance` object, and the withdrawal is reserved against the funder's balance. At execution,
//! the adapter mints the `AllowanceWithdrawal<T>` that the allowance's spend paths consume.

use crate::SUI_FRAMEWORK_ADDRESS;
use crate::base_types::{ObjectID, SuiAddress};
use crate::error::{UserInputError, UserInputResult};
use crate::id::UID;
use crate::object::Object;
use crate::transaction::{
    InputObjectKind, InputObjects, SharedObjectMutability, TransactionData, TransactionDataAPI,
    WithdrawFrom,
};
use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::{StructTag, TypeTag};
use move_core_types::u256::U256;
use mysten_common::debug_fatal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const ALLOWANCE_MODULE_NAME: &IdentStr = ident_str!("allowance");
pub const ALLOWANCE_STRUCT_NAME: &IdentStr = ident_str!("Allowance");
pub const ALLOWANCE_BALANCE_SPEND_FUNCTION_NAME: &IdentStr = ident_str!("balance_spend");
pub const ALLOWANCE_WITHDRAWAL_STRUCT_NAME: &IdentStr = ident_str!("AllowanceWithdrawal");
pub const RESOLVED_ALLOWANCE_WITHDRAWAL_STRUCT: (&AccountAddress, &IdentStr, &IdentStr) = (
    &SUI_FRAMEWORK_ADDRESS,
    ALLOWANCE_MODULE_NAME,
    ALLOWANCE_WITHDRAWAL_STRUCT_NAME,
);

/// Mirror of the Move struct `sui::allowance::Allowance<T>`.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Allowance {
    pub id: UID,
    pub settings: Settings,
    pub current_spend: U256,
}

/// Mirror of the Move struct `sui::allowance::Settings`.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Settings {
    pub funder: SuiAddress,
    pub spender: Option<SuiAddress>,
    /// `Option<std::type_name::TypeName>`.
    pub app: Option<String>,
    pub lifetime_cap: Option<U256>,
    pub start_timestamp_ms: Option<u64>,
    pub expiration_timestamp_ms: Option<u64>,
    pub rate_limit: Option<RateLimit>,
    pub name: String,
}

/// Mirror of the Move enum `sui::allowance::RateLimit`.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub enum RateLimit {
    Windowed {
        limit: U256,
        spent: U256,
        anchor_ms: Option<u64>,
        index: u64,
        window: Window,
    },
}

/// Mirror of the Move enum `sui::allowance::Window`.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub enum Window {
    PeriodicMs(u64),
    CalendarMonths(u8),
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

/// Sign-time view of an `Allowance<T>`.
/// NB: The spender can rotate from transaction to transaction.
#[derive(Debug, Clone)]
pub struct ResolvedAllowance {
    pub funder: SuiAddress,
    pub spender: Option<SuiAddress>,
    /// The accumulated type `T` of `Allowance<T>` (e.g. `Balance<SUI>`).
    pub funds_type: TypeTag,
    /// App-bound allowances can only be spent through `app_balance_spend`.
    pub is_app_bound: bool,
    pub lifetime_cap: Option<U256>,
    pub expiration_timestamp_ms: Option<u64>,
    /// The per-window limit of the rate limit, if any.
    pub rate_limit: Option<U256>,
    /// Mutable state, so only meaningful at signing.
    pub current_spend: U256,
}

impl ResolvedAllowance {
    /// Whether `consume` must abort when spending `amount` at a Clock time of at least `now_ms`.
    /// The expiration and limits are fixed at creation and `current_spend` only grows, so a true
    /// result also holds at every later version of the allowance.
    pub fn spend_must_abort(&self, amount: u128, now_ms: Option<u64>) -> bool {
        let amount = U256::from(amount);
        let expired = now_ms
            .zip(self.expiration_timestamp_ms)
            .is_some_and(|(now, expiration)| now >= expiration);
        let over_lifetime_cap = self.lifetime_cap.is_some_and(|cap| {
            self.current_spend
                .checked_add(amount)
                .is_none_or(|total| total > cap)
        });
        let over_rate_limit = self.rate_limit.is_some_and(|limit| amount > limit);
        expired || over_lifetime_cap || over_rate_limit
    }
}

/// This map holds a transaction's `Allowance` inputs, each decoded once. Only
/// `resolve_allowance_inputs` constructs it; callers must pass it the transaction and inputs it is
/// later checked against.
#[derive(Debug)]
pub struct ResolvedAllowances {
    declared: BTreeSet<ObjectID>,
    resolved: BTreeMap<ObjectID, ResolvedAllowance>,
}

impl ResolvedAllowances {
    pub fn get(&self, id: &ObjectID) -> Option<&ResolvedAllowance> {
        self.resolved.get(id)
    }

    /// Returns the allowances that the transaction declares withdrawals from.
    pub fn declared(&self) -> &BTreeSet<ObjectID> {
        &self.declared
    }
}

/// Decodes every input that `transaction` declares an allowance withdrawal from, failing if one is
/// not an `Allowance`, and with `include_mutable` every `Allowance` it takes mutably. Undeclared
/// allowances that are only read are not decoded.
pub fn resolve_allowance_inputs(
    transaction: &TransactionData,
    input_objects: &InputObjects,
    include_mutable: bool,
) -> UserInputResult<ResolvedAllowances> {
    let declared: BTreeSet<ObjectID> = transaction
        .kind()
        .get_funds_withdrawals()
        .filter_map(|w| match w.withdraw_from {
            WithdrawFrom::SenderAllowance { allowance, .. } => Some(allowance),
            WithdrawFrom::Sender | WithdrawFrom::Sponsor | WithdrawFrom::FrozenAddress { .. } => {
                None
            }
        })
        .collect();
    if declared.is_empty() && !include_mutable {
        return Ok(ResolvedAllowances {
            declared,
            resolved: BTreeMap::new(),
        });
    }
    let resolved = input_objects
        .iter()
        .filter_map(|input| {
            let object = input.as_object()?;
            let is_mutable_allowance = include_mutable
                && matches!(
                    input.input_object_kind,
                    InputObjectKind::SharedMoveObject { mutability, .. }
                        if mutability != SharedObjectMutability::Immutable
                )
                && object
                    .data
                    .try_as_move()
                    .is_some_and(|move_obj| move_obj.type_().is_allowance());
            (declared.contains(&object.id()) || is_mutable_allowance).then_some(object)
        })
        .map(|object| Ok((object.id(), parse_allowance_object(object)?)))
        .collect::<UserInputResult<_>>()?;
    Ok(ResolvedAllowances { declared, resolved })
}

/// Parses an object as an `Allowance`, extracting the sign-time-relevant fields.
pub fn parse_allowance_object(object: &Object) -> UserInputResult<ResolvedAllowance> {
    let invalid = |error: String| UserInputError::InvalidWithdrawReservation { error };
    let id = object.id();
    let Some(move_obj) = object.data.try_as_move() else {
        return Err(invalid(format!(
            "Specified allowance {id} is not a Move object"
        )));
    };
    let tag: StructTag = move_obj.type_().clone().into();
    if !Allowance::is_allowance(&tag) {
        return Err(invalid(format!(
            "Specified allowance {id} is not a sui::allowance::Allowance"
        )));
    }
    if !object.owner.is_shared() {
        return Err(invalid(format!("Allowance {id} is not a shared object")));
    }
    let funds_type = tag
        .type_params
        .into_iter()
        .next()
        .expect("checked by is_allowance");

    let Ok(allowance) = bcs::from_bytes::<Allowance>(move_obj.contents()) else {
        debug_fatal!("allowance {id} did not match the `Allowance` rust type");
        return Err(invalid(format!("Failed to read allowance {id}")));
    };

    Ok(ResolvedAllowance {
        funder: allowance.settings.funder,
        spender: allowance.settings.spender,
        funds_type,
        is_app_bound: allowance.settings.app.is_some(),
        lifetime_cap: allowance.settings.lifetime_cap,
        expiration_timestamp_ms: allowance.settings.expiration_timestamp_ms,
        rate_limit: allowance
            .settings
            .rate_limit
            .map(|RateLimit::Windowed { limit, .. }| limit),
        current_spend: allowance.current_spend,
    })
}
