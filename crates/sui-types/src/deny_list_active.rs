// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Support for the deny list seal/activate protocol.
//!
//! When `enable_deny_list_seal_activate` is on, deny list writes are recorded as pending
//! updates under the `DenyList` object, and the consensus handler issues system transactions
//! that move them into effect within the epoch:
//!
//! * a `seal` at the end of every consensus commit moves the pending updates into a staging
//!   slot, stamped with the commit index (the generation);
//! * an `activate` at the start of a commit applies the generation sealed
//!   `deny_list_activation_lag_commits` commits earlier to the `ActiveDenyList`;
//! * a flush at the last commit of the epoch applies everything still unactivated.
//!
//! Execution reads the `ActiveDenyList` at the version produced by the latest `activate`
//! ordered before the transaction. The version is assigned by the consensus handler, waited on
//! by the execution scheduler, recorded in effects as a read-only consensus object, and
//! reproduced from effects by fullnodes.
//!
//! Only `activate` and the flush write the `ActiveDenyList`, and both are ordered after every
//! reader of the previous version, so the existing pruning rule (readers precede overwriters
//! in checkpoint order) holds. The staging slots form a ring so that a slot is not rewritten
//! before the activation that reads it.

use crate::base_types::{ObjectID, SequenceNumber, SuiAddress};
use crate::committee::EpochId;
use crate::deny_list_v1::{DENY_LIST_COIN_TYPE_INDEX, DENY_LIST_MODULE};
use crate::derived_object::derive_object_id;
use crate::dynamic_field::DynamicFieldKey;
use crate::error::ExecutionError;
use crate::execution_status::ExecutionErrorKind;
use crate::gas_coin::GAS;
use crate::programmable_transaction_builder::ProgrammableTransactionBuilder;
use crate::storage::{DenyListResult, RuntimeObjectResolver};
use crate::transaction::{CallArg, ObjectArg, SharedObjectMutability, TransactionKind};
use crate::{
    MoveTypeTagTrait, SUI_ACTIVE_DENY_LIST_OBJECT_ID, SUI_DENY_LIST_OBJECT_ID,
    SUI_FRAMEWORK_PACKAGE_ID, SUI_SYSTEM_STATE_OBJECT_ID, SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
};
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::{StructTag, TypeTag};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const DENY_LIST_CREATE_ACTIVE_FUNC: &IdentStr = ident_str!("create_active");
pub const DENY_LIST_SEAL_FUNC: &IdentStr = ident_str!("seal");
pub const DENY_LIST_ACTIVATE_FUNC: &IdentStr = ident_str!("activate");
pub const DENY_LIST_FLUSH_PENDING_FUNC: &IdentStr = ident_str!("flush_pending");

fn deny_list_struct_tag(name: &IdentStr) -> StructTag {
    StructTag {
        address: SUI_FRAMEWORK_PACKAGE_ID.into(),
        module: DENY_LIST_MODULE.to_owned(),
        name: name.to_owned(),
        type_params: vec![],
    }
}

/// Rust representation of the Move type 0x2::deny_list::ActiveAddressKey.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ActiveAddressKey {
    pub per_type_index: u64,
    pub per_type_key: Vec<u8>,
    pub addr: SuiAddress,
}

impl MoveTypeTagTrait for ActiveAddressKey {
    fn get_type_tag() -> TypeTag {
        TypeTag::Struct(Box::new(deny_list_struct_tag(ident_str!(
            "ActiveAddressKey"
        ))))
    }
}

/// Rust representation of the Move type 0x2::deny_list::ActiveGlobalPauseKey.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ActiveGlobalPauseKey {
    pub per_type_index: u64,
    pub per_type_key: Vec<u8>,
}

impl MoveTypeTagTrait for ActiveGlobalPauseKey {
    fn get_type_tag() -> TypeTag {
        TypeTag::Struct(Box::new(deny_list_struct_tag(ident_str!(
            "ActiveGlobalPauseKey"
        ))))
    }
}

/// Rust representation of the Move type 0x2::deny_list::StagingSlotKey, the derived object key
/// of a staging slot under the `ActiveDenyList`.
#[derive(Debug, Serialize, Deserialize, Clone)]
struct StagingSlotKey(u64);

impl MoveTypeTagTrait for StagingSlotKey {
    fn get_type_tag() -> TypeTag {
        TypeTag::Struct(Box::new(deny_list_struct_tag(ident_str!("StagingSlotKey"))))
    }
}

/// The ID of staging slot `slot`. Must match `derived_object::claim` in `create_active`.
pub fn deny_list_staging_slot_id(slot: u64) -> ObjectID {
    derive_object_id(
        SUI_ACTIVE_DENY_LIST_OBJECT_ID,
        &StagingSlotKey::get_type_tag(),
        &bcs::to_bytes(&StagingSlotKey(slot)).unwrap(),
    )
    .expect("deriving a staging slot id cannot fail")
}

/// The staging slot a generation is sealed into.
pub fn deny_list_staging_slot_for_generation(generation: u64, num_slots: u64) -> u64 {
    generation % num_slots
}

/// Deny list check against the `ActiveDenyList` at `active_version`, the version assigned to
/// this transaction. Mirrors `check_coin_deny_list_v2_during_execution`, including the gas
/// accounting contract on `num_non_gas_coin_owners`.
pub fn check_coin_deny_list_active_during_execution(
    receiving_funds_type_and_owners: BTreeMap<TypeTag, BTreeSet<SuiAddress>>,
    active_version: SequenceNumber,
    resolver: &dyn RuntimeObjectResolver,
) -> DenyListResult {
    let non_gas_coin_owners = receiving_funds_type_and_owners
        .into_iter()
        .filter_map(|(ty, owners)| {
            if GAS::is_gas_type(&ty) {
                None
            } else {
                Some((ty.to_canonical_string(false), owners))
            }
        })
        .collect::<BTreeMap<_, _>>();
    let num_non_gas_coin_owners = non_gas_coin_owners.values().map(|v| v.len() as u64).sum();
    let result = check_active_entries(non_gas_coin_owners, active_version, resolver);
    DenyListResult {
        result,
        num_non_gas_coin_owners,
    }
}

fn check_active_entries(
    non_gas_coin_owners: BTreeMap<String, BTreeSet<SuiAddress>>,
    active_version: SequenceNumber,
    resolver: &dyn RuntimeObjectResolver,
) -> Result<(), ExecutionError> {
    for (coin_type, owners) in non_gas_coin_owners {
        let per_type_key = coin_type.as_bytes().to_vec();
        let pause_key = ActiveGlobalPauseKey {
            per_type_index: DENY_LIST_COIN_TYPE_INDEX,
            per_type_key: per_type_key.clone(),
        };
        if active_entry_exists(pause_key, active_version, resolver) {
            return Err(ExecutionError::new(
                ExecutionErrorKind::CoinTypeGlobalPause { coin_type },
                None,
            ));
        }
        for owner in owners {
            let address_key = ActiveAddressKey {
                per_type_index: DENY_LIST_COIN_TYPE_INDEX,
                per_type_key: per_type_key.clone(),
                addr: owner,
            };
            if active_entry_exists(address_key, active_version, resolver) {
                return Err(ExecutionError::new(
                    ExecutionErrorKind::AddressDeniedForCoin {
                        address: owner,
                        coin_type,
                    },
                    None,
                ));
            }
        }
    }
    Ok(())
}

/// An entry is denied iff its field exists under the `ActiveDenyList` at `active_version`.
fn active_entry_exists<K: MoveTypeTagTrait + Serialize + std::fmt::Debug>(
    key: K,
    active_version: SequenceNumber,
    resolver: &dyn RuntimeObjectResolver,
) -> bool {
    DynamicFieldKey(SUI_ACTIVE_DENY_LIST_OBJECT_ID, key, K::get_type_tag())
        .into_id_with_bound(active_version)
        .and_then(|id| id.exists(resolver))
        .unwrap_or(false)
}

/// System transactions that mutate objects written by users (the `DenyList` and its pending
/// list carry storage rebates paid by the writers) must take the system state object so the
/// unmetered rebate is conserved into it. Activate only touches system-created objects and
/// does not need it, which keeps it off the system state object's version chain.
fn sui_system_state_input() -> CallArg {
    CallArg::Object(ObjectArg::SharedObject {
        id: SUI_SYSTEM_STATE_OBJECT_ID,
        initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
        mutability: SharedObjectMutability::Mutable,
    })
}

fn deny_list_input(initial_shared_version: SequenceNumber) -> CallArg {
    CallArg::Object(ObjectArg::SharedObject {
        id: SUI_DENY_LIST_OBJECT_ID,
        initial_shared_version,
        mutability: SharedObjectMutability::Mutable,
    })
}

fn active_deny_list_input(initial_shared_version: SequenceNumber) -> CallArg {
    CallArg::Object(ObjectArg::SharedObject {
        id: SUI_ACTIVE_DENY_LIST_OBJECT_ID,
        initial_shared_version,
        mutability: SharedObjectMutability::Mutable,
    })
}

fn staging_slot_input(
    slot: u64,
    initial_shared_version: SequenceNumber,
    mutability: SharedObjectMutability,
) -> CallArg {
    CallArg::Object(ObjectArg::SharedObject {
        id: deny_list_staging_slot_id(slot),
        initial_shared_version,
        mutability,
    })
}

fn system_deny_list_call(
    builder: &mut ProgrammableTransactionBuilder,
    function: &IdentStr,
    args: Vec<CallArg>,
) {
    builder
        .move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            DENY_LIST_MODULE.to_owned(),
            function.to_owned(),
            vec![],
            args,
        )
        .expect("building a deny list system call cannot fail");
}

/// Creates the `ActiveDenyList` and its staging ring. Issued once, in the last commit of the
/// epoch in which the feature is first enabled, so that the next epoch's start configuration
/// records the initial shared version of the new objects.
pub fn deny_list_create_active_tx(
    deny_list_initial_shared_version: SequenceNumber,
    num_staging_slots: u64,
) -> TransactionKind {
    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .input(sui_system_state_input())
        .expect("adding an input cannot fail");
    system_deny_list_call(
        &mut builder,
        DENY_LIST_CREATE_ACTIVE_FUNC,
        vec![
            deny_list_input(deny_list_initial_shared_version),
            CallArg::Pure(bcs::to_bytes(&num_staging_slots).unwrap()),
        ],
    );
    TransactionKind::ProgrammableSystemTransaction(builder.finish())
}

/// Seals the pending updates as `generation` into its staging slot.
pub fn deny_list_seal_tx(
    epoch: EpochId,
    generation: u64,
    num_staging_slots: u64,
    deny_list_initial_shared_version: SequenceNumber,
    active_deny_list_initial_shared_version: SequenceNumber,
) -> TransactionKind {
    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .input(sui_system_state_input())
        .expect("adding an input cannot fail");
    system_deny_list_call(
        &mut builder,
        DENY_LIST_SEAL_FUNC,
        vec![
            deny_list_input(deny_list_initial_shared_version),
            staging_slot_input(
                deny_list_staging_slot_for_generation(generation, num_staging_slots),
                active_deny_list_initial_shared_version,
                SharedObjectMutability::Mutable,
            ),
            CallArg::Pure(bcs::to_bytes(&epoch).unwrap()),
            CallArg::Pure(bcs::to_bytes(&generation).unwrap()),
        ],
    );
    TransactionKind::ProgrammableSystemTransaction(builder.finish())
}

/// Applies the updates sealed as `generation` to the `ActiveDenyList`.
pub fn deny_list_activate_tx(
    epoch: EpochId,
    generation: u64,
    num_staging_slots: u64,
    active_deny_list_initial_shared_version: SequenceNumber,
) -> TransactionKind {
    let mut builder = ProgrammableTransactionBuilder::new();
    add_activate_call(
        &mut builder,
        epoch,
        generation,
        num_staging_slots,
        active_deny_list_initial_shared_version,
    );
    TransactionKind::ProgrammableSystemTransaction(builder.finish())
}

/// Applies the still-unactivated sealed generations, in order, followed by the pending
/// updates. Issued in the last commit of the epoch in place of the seal.
pub fn deny_list_flush_tx(
    epoch: EpochId,
    unactivated_generations: impl IntoIterator<Item = u64>,
    num_staging_slots: u64,
    deny_list_initial_shared_version: SequenceNumber,
    active_deny_list_initial_shared_version: SequenceNumber,
) -> TransactionKind {
    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .input(sui_system_state_input())
        .expect("adding an input cannot fail");
    for generation in unactivated_generations {
        add_activate_call(
            &mut builder,
            epoch,
            generation,
            num_staging_slots,
            active_deny_list_initial_shared_version,
        );
    }
    system_deny_list_call(
        &mut builder,
        DENY_LIST_FLUSH_PENDING_FUNC,
        vec![
            deny_list_input(deny_list_initial_shared_version),
            active_deny_list_input(active_deny_list_initial_shared_version),
            CallArg::Pure(bcs::to_bytes(&epoch).unwrap()),
        ],
    );
    TransactionKind::ProgrammableSystemTransaction(builder.finish())
}

fn add_activate_call(
    builder: &mut ProgrammableTransactionBuilder,
    epoch: EpochId,
    generation: u64,
    num_staging_slots: u64,
    active_deny_list_initial_shared_version: SequenceNumber,
) {
    system_deny_list_call(
        builder,
        DENY_LIST_ACTIVATE_FUNC,
        vec![
            active_deny_list_input(active_deny_list_initial_shared_version),
            staging_slot_input(
                deny_list_staging_slot_for_generation(generation, num_staging_slots),
                active_deny_list_initial_shared_version,
                SharedObjectMutability::Immutable,
            ),
            CallArg::Pure(bcs::to_bytes(&epoch).unwrap()),
            CallArg::Pure(bcs::to_bytes(&generation).unwrap()),
        ],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staging_slot_ids_are_distinct() {
        let ids: BTreeSet<_> = (0..8).map(deny_list_staging_slot_id).collect();
        assert_eq!(ids.len(), 8);
        assert!(!ids.contains(&SUI_ACTIVE_DENY_LIST_OBJECT_ID));
    }
}
