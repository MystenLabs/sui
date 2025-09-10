// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A module for accumulating funds, i.e. Balance-like types.
module sui::funds_accumulator;

/// Allows calling `.split()` on a `Withdrawal` create a sub withdrawal from it.
public use fun withdrawal_split as Withdrawal.split;

/// Allows calling `.join()` on a `Withdrawal` to combine two withdrawals.
public use fun withdrawal_join as Withdrawal.join;

/// Allows calling `.limit()` on a `Withdrawal` to get its remaining limit.
public use fun withdrawal_limit as Withdrawal.limit;

/// Allows calling `.owner()` on a `Withdrawal` to get its owner address.
public use fun withdrawal_owner as Withdrawal.owner;

/// Attempted to withdraw more than the maximum value of the underlying integer type.
#[allow(unused_const)]
const EOverflow: u64 = 0;

/// Attempt to split more than the current limit of a `Withdrawal`.
#[error(code = 1)]
const EInvalidSubLimit: vector<u8> = b"Sub-limit exceeds current withdrawal limit";

/// Attempted to join two withdrawals with different owners.
#[error(code = 2)]
const EOwnerMismatch: vector<u8> = b"Withdrawal owners do not match";

/// Allows for withdrawing funds from a given address. The `Withdrawal` can be created in PTBs for
/// the transaction sender, or dynamically from an object via `withdraw_from_object`.
/// The redemption of the funds must be initiated from the module that defines `T`.
public struct Withdrawal<phantom T: store> has drop {
    /// The owner of the funds, either an object or a transaction sender
    owner: address,
    /// At signing we check the limit <= balance when taking this as a call arg.
    /// If this was generated from an object, we cannot check this until redemption.
    limit: u256,
}

/// Returns the owner, either a sender's address or an object, of the withdrawal.
public fun withdrawal_owner<T: store>(withdrawal: &Withdrawal<T>): address {
    withdrawal.owner
}

/// Returns the remaining limit of the withdrawal.
public fun withdrawal_limit<T: store>(withdrawal: &Withdrawal<T>): u256 {
    withdrawal.limit
}

/// Split a `Withdrawal` and take a sub-withdrawal from it with the specified sub-limit.
public fun withdrawal_split<T: store>(
    withdrawal: &mut Withdrawal<T>,
    sub_limit: u256,
): Withdrawal<T> {
    assert!(withdrawal.limit >= sub_limit, EInvalidSubLimit);
    withdrawal.limit = withdrawal.limit - sub_limit;
    Withdrawal { owner: withdrawal.owner, limit: sub_limit }
}

/// Join two withdrawals together, increasing the limit of `self` by the limit of `other`.
/// Aborts with `EOwnerMismatch` if the owners are not equal.
/// Aborts with `EOverflow` if the resulting limit would overflow `u256`.
public fun withdrawal_join<T: store>(withdrawal: &mut Withdrawal<T>, other: Withdrawal<T>) {
    assert!(withdrawal.owner == other.owner, EOwnerMismatch);
    assert!(std::u256::max_value!() - withdrawal.limit >= other.limit, EOverflow);
    withdrawal.limit = withdrawal.limit + other.limit;
}

// TODO When this becomes `public` we need
// - custom verifier rules for `T`
// - private generic rules for `T`
public(package) fun redeem</* internal */ T: store>(withdrawal: Withdrawal<T>): T {
    let Withdrawal { owner, limit: value } = withdrawal;
    withdraw_impl(owner, value)
}

// Allows for creating a withdrawal from an object
// (note no internal check needed since this will be gated at redemption)
// Does not abort even if the value is greater than the amount in the object, unless we keep track
// at each withdraw from object, we need to check it again at redeem so this seems fine?
// TODO When this becomes `public` we need
// - custom verifier rules for `T`
#[allow(unused_mut_parameter)]
public(package) fun withdraw_from_object<T: store>(obj: &mut UID, limit: u256): Withdrawal<T> {
    let owner = obj.to_address();
    Withdrawal { owner, limit }
}

// TODO when funds become public, we likely need to wrap T
public(package) fun add_impl<T: store>(value: T, recipient: address) {
    let accumulator = sui::accumulator::accumulator_address<T>(recipient);
    add_to_accumulator_address<T>(accumulator, recipient, value)
}

// TODO when funds become public, we likely need to wrap T
fun withdraw_impl<T: store>(owner: address, value: u256): T {
    let accumulator = sui::accumulator::accumulator_address<T>(owner);
    withdraw_from_accumulator_address<T>(accumulator, owner, value)
}

// TODO when this becomes public we will need
// - custom verifier rules for `T` that it is a struct with a single unsigned integer field.
//   Or a struct with a single field that satisfies this property recursively.
// - private generic rules for `T`
native fun add_to_accumulator_address<T: store>(accumulator: address, recipient: address, value: T);

// aborts if the value is greater than the amount in the withdrawal
// Do we need to charge a small fee since we cannot charge storage fees?
// We should limit withdraws to `u*::max` for a `owner`
// in a given transaction for the given `u*` in `T`
native fun withdraw_from_accumulator_address<T: store>(
    accumulator: address,
    owner: address,
    value: u256,
): T;

// TODO remove once Withdrawal is supported in PTBs
public(package) fun create_withdrawal<T: store>(owner: address, limit: u256): Withdrawal<T> {
    Withdrawal { owner, limit }
}
