// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::accumulator_metadata;

use sui::accumulator::AccumulatorRoot;
use sui::bag;
use sui::dynamic_field;

const EInvariantViolation: u64 = 0;

/// === Accumulator metadata ===
///
/// Accumulator metadata is organized as follows:
/// - Each address that holds at least one type of accumulator has an owner field attached
///   to the accumulator root.
/// - For each type of accumulator held by that address, there is an AccumulatorMetadata field
///   attached to the owner field.
/// - When the value of an accumulator drops to zero, the metadata field is removed.
/// - If the owner field has no more accumulator metadata field attached to it, it is removed
///   as well.

public struct OwnerKey has copy, drop, store {
    owner: address,
}

/// An owner field, to which all AccumulatorMetadata fields for the owner are
/// attached.
public struct Owner has store {
    /// The individual balances owned by the owner.
    balances: bag::Bag,
    owner: address,
}

public struct MetadataKey<phantom T>() has copy, drop, store;

/// A metadata field for a balance field with type T.
public struct Metadata<phantom T> has store {
    /// Any per-balance fields we wish to add in the future.
    fields: bag::Bag,
}

/// === Owner functions ===

/// Check if there is an owner field attached to the accumulator root.
fun accumulator_root_owner_exists(accumulator_root: &AccumulatorRoot, owner: address): bool {
    dynamic_field::exists_with_type<OwnerKey, Owner>(accumulator_root.id(), OwnerKey { owner })
}

use fun accumulator_root_owner_exists as AccumulatorRoot.owner_exists;

/// Borrow an owner field mutably.
fun accumulator_root_borrow_owner_mut(
    accumulator_root: &mut AccumulatorRoot,
    owner: address,
): &mut Owner {
    dynamic_field::borrow_mut(accumulator_root.id_mut(), OwnerKey { owner })
}

use fun accumulator_root_borrow_owner_mut as AccumulatorRoot.borrow_owner_mut;

/// Attach an owner field to the accumulator root.
fun accumulator_root_attach_owner(accumulator_root: &mut AccumulatorRoot, owner: Owner) {
    dynamic_field::add(accumulator_root.id_mut(), OwnerKey { owner: owner.owner }, owner);
}

use fun accumulator_root_attach_owner as AccumulatorRoot.attach_owner;

/// Detach an owner field from the accumulator root.
fun accumulator_root_detach_owner(accumulator_root: &mut AccumulatorRoot, owner: address): Owner {
    dynamic_field::remove(accumulator_root.id_mut(), OwnerKey { owner })
}

use fun accumulator_root_detach_owner as AccumulatorRoot.detach_owner;

/// === Metadata functions ===

/// Create a metadata field for a new balance field with type T.
/// The metadata will be attached to the owner field `owner`.
/// If the owner field does not exist, it will be created.
public(package) fun create_accumulator_metadata<T>(
    accumulator_root: &mut AccumulatorRoot,
    owner: address,
    ctx: &mut TxContext,
) {
    let metadata = Metadata<T> {
        fields: bag::new(ctx),
    };

    if (accumulator_root.owner_exists(owner)) {
        let accumulator_owner = accumulator_root.borrow_owner_mut(owner);
        assert!(accumulator_owner.owner == owner, EInvariantViolation);
        accumulator_owner.attach_metadata(metadata);
    } else {
        let mut accumulator_owner = Owner {
            balances: bag::new(ctx),
            owner,
        };
        accumulator_owner.attach_metadata(metadata);
        accumulator_root.attach_owner(accumulator_owner);
    }
}

/// Remove the metadata field for a balance field with type T.
/// The metadata will be detached from the owner field `owner`.
/// If there are no more balance fields attached to the owner field,
/// the owner field will be destroyed.
public(package) fun remove_accumulator_metadata<T>(
    accumulator_root: &mut AccumulatorRoot,
    owner: address,
) {
    let is_empty = {
        let accumulator_owner = accumulator_root.borrow_owner_mut(owner);
        let Metadata { fields } = accumulator_owner.detach_metadata<T>();
        fields.destroy_empty();
        accumulator_owner.balances.is_empty()
    };

    if (is_empty) {
        accumulator_root.detach_owner(owner).destroy();
    }
}

/// Attach a metadata field for type T to the owner field.
fun accumulator_owner_attach_metadata<T>(self: &mut Owner, metadata: Metadata<T>) {
    self.balances.add(MetadataKey<T>(), metadata);
}

use fun accumulator_owner_attach_metadata as Owner.attach_metadata;

/// Detach a metadata field for type T from the owner field.
fun accumulator_owner_detach_metadata<T>(self: &mut Owner): Metadata<T> {
    self.balances.remove(MetadataKey<T>())
}

use fun accumulator_owner_detach_metadata as Owner.detach_metadata;

/// Destroy an owner field.
fun accumulator_owner_destroy(this: Owner) {
    let Owner { balances, .. } = this;
    balances.destroy_empty();
}

use fun accumulator_owner_destroy as Owner.destroy;
