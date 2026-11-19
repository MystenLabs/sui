// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, rc::Rc};
use sui_move_natives::object_runtime::{BalanceTotals, BalanceTracking, add_to_totals};
use sui_types::{
    accumulator_event::AccumulatorEvent,
    balance::Balance,
    base_types::{ObjectID, SuiAddress},
    effects::{AccumulatorOperation, AccumulatorValue},
    error::ExecutionError,
    execution::ExecutionResultsV2,
    object::Owner,
};

/// What a user PTB read and wrote, from which its `BalanceTracking` is built.
pub(crate) struct ObjectData<'a> {
    /// The sender, the sponsor, and each declared withdrawal's funder and allowance ID, whether or
    /// not the withdrawal is redeemed.
    pub transaction_addresses: Vec<SuiAddress>,
    /// Object inputs, including the gas coin, with their owners at the start of the transaction.
    pub inputs: Vec<(ObjectID, Owner)>,
    /// Objects loaded during execution: mutable inputs, dynamic field children, and received
    /// objects.
    pub loaded_runtime_objects: BTreeSet<ObjectID>,
    /// UIDs found in the objects read from storage.
    pub contained_uids: BTreeSet<ObjectID>,
    /// Includes IDs created and then deleted in the transaction.
    pub generated_ids: BTreeSet<ObjectID>,
    pub results: &'a ExecutionResultsV2,
}

/// Fails if an object is written or deleted without being read or created, since its UID was then
/// missed when walking the objects read.
pub(crate) fn finish(
    before: BalanceTotals,
    after: BalanceTotals,
    objects: ObjectData<'_>,
) -> Result<BalanceTracking, ExecutionError> {
    let ObjectData {
        transaction_addresses,
        inputs,
        loaded_runtime_objects,
        contained_uids,
        generated_ids,
        results,
    } = objects;
    let mut seen: BTreeSet<ObjectID> = inputs.iter().map(|(id, _)| *id).collect();
    seen.extend(
        contained_uids
            .iter()
            .chain(&loaded_runtime_objects)
            .chain(&generated_ids),
    );
    let unseen = results
        .written_objects
        .iter()
        .filter(|(_, object)| !object.is_package())
        .map(|(id, _)| id)
        .chain(&results.deleted_object_ids)
        .find(|id| !seen.contains(id));
    if let Some(id) = unseen {
        return Err(make_invariant_violation!(
            "Object {id} was written or deleted without being read or created"
        ));
    }
    let mut addresses: BTreeSet<SuiAddress> = transaction_addresses.into_iter().collect();
    let mut packages = BTreeSet::new();
    for (id, owner) in &inputs {
        addresses.insert((*id).into());
        add_owner_addresses(&mut addresses, owner);
    }
    for (id, object) in &results.written_objects {
        if object.is_package() {
            packages.insert(SuiAddress::from(*id));
        }
        addresses.insert((*id).into());
        add_owner_addresses(&mut addresses, &object.owner);
    }
    let ids = contained_uids
        .iter()
        .chain(&generated_ids)
        .chain(&loaded_runtime_objects)
        .chain(&results.deleted_object_ids);
    addresses.extend(ids.map(|id| SuiAddress::from(*id)));
    let mut tracking = BalanceTracking {
        before,
        after,
        addresses,
    };
    add_accumulator_events(&mut tracking, &results.accumulator_events)?;
    // Freezing a package or a system object blocks nothing.
    tracking
        .addresses
        .retain(|a| !packages.contains(a) && !ObjectID::from(*a).is_system_object());
    Ok(tracking)
}

/// Adds each raw `Balance<T>` Split to `before` and each raw Merge to `after`, and records every
/// event's address as involved.
///
/// The events must not be merged yet, so a withdraw-and-redeposit counts on both sides.
pub(crate) fn add_accumulator_events<'a>(
    tracking: &mut BalanceTracking,
    events: impl IntoIterator<Item = &'a AccumulatorEvent>,
) -> Result<(), ExecutionError> {
    for event in events {
        let address = event.write.address.address;
        tracking.addresses.insert(address);
        let Some(type_) = Balance::maybe_get_balance_type_param(&event.write.address.ty) else {
            continue;
        };
        let AccumulatorValue::Integer(amount) = event.write.value else {
            continue;
        };
        let totals = match event.write.operation {
            AccumulatorOperation::Split => &mut tracking.before,
            AccumulatorOperation::Merge => &mut tracking.after,
        };
        add_to_totals(totals, address, &Rc::new(type_), u128::from(amount))
            .map_err(ExecutionError::invariant_violation)?;
    }
    Ok(())
}

/// Adds the addresses that own an object: a parent object's ID, or every member of a party.
fn add_owner_addresses(addresses: &mut BTreeSet<SuiAddress>, owner: &Owner) {
    match owner {
        Owner::AddressOwner(address)
        | Owner::ObjectOwner(address)
        | Owner::ConsensusAddressOwner { owner: address, .. } => {
            addresses.insert(*address);
        }
        Owner::Party { permissions, .. } => addresses.extend(permissions.members()),
        Owner::Shared { .. } | Owner::Immutable => (),
    }
}
