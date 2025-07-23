// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::accumulator;

use sui::bag;
use sui::dynamic_field;
use sui::object::sui_accumulator_root_address;

const ENotSystemAddress: u64 = 0;
const EInvalidSplitAmount: u64 = 1;
const EInvariantViolation: u64 = 2;

public struct AccumulatorRoot has key {
    id: UID,
}

#[allow(unused_function)]
fun create(ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    transfer::share_object(AccumulatorRoot {
        id: object::sui_accumulator_root_object_id(),
    })
}

// === Accumulator address computation ===

/// `Key` is used only for computing the field id of accumulator objects.
/// `T` is the type of the accumulated value, e.g. `Balance<SUI>`
public struct Key<phantom T> has copy, drop, store {
    address: address,
}

public(package) fun accumulator_address<T>(address: address): address {
    let key = Key<T> { address };
    dynamic_field::hash_type_and_key(sui_accumulator_root_address(), key)
}

// === Adding, removing, and mutating accumulator objects ===

/// Balance object methods
fun root_has_accumulator<K, V: store>(accumulator_root: &AccumulatorRoot, name: Key<K>): bool {
    dynamic_field::exists_with_type<Key<K>, V>(&accumulator_root.id, name)
}

use fun root_has_accumulator as AccumulatorRoot.has_accumulator;

fun root_add_accumulator<K, V: store>(
    accumulator_root: &mut AccumulatorRoot,
    name: Key<K>,
    value: V,
) {
    dynamic_field::add(&mut accumulator_root.id, name, value);
}

use fun root_add_accumulator as AccumulatorRoot.add_accumulator;

fun root_borrow_accumulator_mut<K, V: store>(
    accumulator_root: &mut AccumulatorRoot,
    name: Key<K>,
): &mut V {
    dynamic_field::borrow_mut<Key<K>, V>(&mut accumulator_root.id, name)
}

use fun root_borrow_accumulator_mut as AccumulatorRoot.borrow_accumulator_mut;

fun root_remove_accumulator<K, V: store>(accumulator_root: &mut AccumulatorRoot, name: Key<K>): V {
    dynamic_field::remove<Key<K>, V>(&mut accumulator_root.id, name)
}

use fun root_remove_accumulator as AccumulatorRoot.remove_accumulator;

/// === Accumulator metadata ===
///
/// Accumulator metadata is organized as follows:
/// - Each address that holds at least one type of accumulator has an owner object attached
///   to the accumulator root.
/// - For each type of accumulator held by that address, there is an AccumulatorMetadata object
///   attached to the owner object.
/// - When the value of an accumulator drops to zero, the metadata object is removed.
/// - If the owner object has no more accumulator metadata objects attached to it, it is removed
///   as well.

public struct OwnerKey has copy, drop, store {
    owner: address,
}

/// An owner object, to which all AccumulatorMetadata objects for the owner are
/// attached.
public struct Owner has store {
    /// The individual balances owned by the owner.
    balances: bag::Bag,
    owner: address,
}

public struct MetadataKey<phantom T> has copy, drop, store {}

/// A metadata object for a balance object with type T.
public struct Metadata<phantom T> has store {
    /// Any per-balance fields we wish to add in the future.
    fields: bag::Bag,
}

/// === Owner functions ===

/// Check if there is an owner object attached to the accumulator root.
fun accumulator_root_owner_exists(accumulator_root: &AccumulatorRoot, owner: address): bool {
    dynamic_field::exists_with_type<OwnerKey, Owner>(&accumulator_root.id, OwnerKey { owner })
}

use fun accumulator_root_owner_exists as AccumulatorRoot.owner_exists;

/// Borrow an owner object mutably.
fun accumulator_root_borrow_owner_mut(
    accumulator_root: &mut AccumulatorRoot,
    owner: address,
): &mut Owner {
    dynamic_field::borrow_mut(&mut accumulator_root.id, OwnerKey { owner })
}

use fun accumulator_root_borrow_owner_mut as AccumulatorRoot.borrow_owner_mut;

/// Attach an owner object to the accumulator root.
fun accumulator_root_attach_owner(accumulator_root: &mut AccumulatorRoot, owner: Owner) {
    dynamic_field::add(&mut accumulator_root.id, OwnerKey { owner: owner.owner }, owner);
}

use fun accumulator_root_attach_owner as AccumulatorRoot.attach_owner;

/// Detach an owner object from the accumulator root.
fun accumulator_root_detach_owner(accumulator_root: &mut AccumulatorRoot, owner: address): Owner {
    dynamic_field::remove(&mut accumulator_root.id, OwnerKey { owner })
}

use fun accumulator_root_detach_owner as AccumulatorRoot.detach_owner;

/// === Metadata functions ===

/// Create a metadata object for a new balance object with type T.
/// The metadata will be attached to the owner object `owner`.
/// If the owner object does not exist, it will be created.
fun create_accumulator_metadata<T>(
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

use fun create_accumulator_metadata as AccumulatorRoot.create_metadata;

/// Remove the metadata object for a balance object with type T.
/// The metadata will be detached from the owner object `owner`.
/// If there are no more balance objects attached to the owner object,
/// the owner object will be destroyed.
fun accumulator_metadata_remove<T>(accumulator_root: &mut AccumulatorRoot, owner: address) {
    let is_empty = {
        let accumulator_owner = accumulator_root.borrow_owner_mut(owner);
        let Metadata { fields } = accumulator_owner.detach_metadata<T>();
        fields.destroy_empty();
        accumulator_owner.balances.is_empty()
    };

    if (is_empty) {
        let owner = accumulator_root.detach_owner(owner);
        owner.destroy();
    }
}

use fun accumulator_metadata_remove as AccumulatorRoot.remove_metadata;

/// Attach a metadata object for type T to the owner object.
fun accumulator_owner_attach_metadata<T>(self: &mut Owner, metadata: Metadata<T>) {
    self.balances.add(MetadataKey<T> {}, metadata);
}

use fun accumulator_owner_attach_metadata as Owner.attach_metadata;

/// Detach a metadata object for type T from the owner object.
fun accumulator_owner_detach_metadata<T>(self: &mut Owner): Metadata<T> {
    self.balances.remove(MetadataKey<T> {})
}

use fun accumulator_owner_detach_metadata as Owner.detach_metadata;

/// Destroy an owner object.
fun accumulator_owner_destroy(this: Owner) {
    let Owner { balances, .. } = this;
    balances.destroy_empty();
}

use fun accumulator_owner_destroy as Owner.destroy;

// === Settlement storage types and entry points ===

/// Storage for 128-bit accumulator values.
///
/// Currently only used to represent the sum of 64 bit values (such as `Balance<T>`).
/// The additional bits are necessary to prevent overflow, as it would take 2^64 deposits of U64_MAX
/// to cause an overflow.
public struct U128 has store {
    value: u128,
}

/// Called by settlement transactions to ensure that the settlement transaction has a unique
/// digest.
#[allow(unused_function)]
fun settlement_prologue(_epoch: u64, _checkpoint_height: u64, _idx: u64, ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);
}

#[allow(unused_function)]
fun settle_u128<T>(
    accumulator_root: &mut AccumulatorRoot,
    owner: address,
    merge: u128,
    split: u128,
    ctx: &mut TxContext,
) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);
    // Merge and split should be netted out prior to calling this function.
    assert!((merge == 0 ) != (split == 0), EInvalidSplitAmount);

    let name = Key<T> { address: owner };

    if (accumulator_root.has_accumulator<T, U128>(name)) {
        let is_zero = {
            let value: &mut U128 = accumulator_root.borrow_accumulator_mut(name);
            value.value = value.value + merge - split;

            value.value == 0
        };

        if (is_zero) {
            let U128 { value: _ } = accumulator_root.remove_accumulator<T, U128>(
                name,
            );
            accumulator_root.remove_metadata<T>(owner);
        }
    } else {
        // cannot split if the field does not yet exist
        assert!(split == 0, EInvalidSplitAmount);
        let value = U128 {
            value: merge,
        };

        accumulator_root.add_accumulator(name, value);
        accumulator_root.create_metadata<T>(owner, ctx);
    };
}

// === Natives for emitting accumulator events ===

public(package) native fun emit_deposit_event<T>(
    accumulator: address,
    recipient: address,
    amount: u64,
);
public(package) native fun emit_withdraw_event<T>(
    accumulator: address,
    owner: address,
    amount: u64,
);
