// Accumulator but with lots of private generics
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

public struct Withdrawal<phantom T: store> has drop {
    // Owner? Controller? Account?
    owner: address,
    // at signing we check this isn't too big for `T`
    limit: u256,
}

public fun withdrawal_owner<T: store>(withdrawal: &Withdrawal<T>): address {
    withdrawal.owner
}

public fun withdrawal_limit<T: store>(withdrawal: &Withdrawal<T>): u256 {
    withdrawal.limit
}

public fun withdrawal_split<T: store>(withdrawal: &mut Withdrawal<T>, value: u256): Withdrawal<T> {
    assert!(withdrawal.limit >= value);
    withdrawal.limit = withdrawal.limit - value;
    Withdrawal { owner: withdrawal.owner, limit: value }
}

public fun withdrawal_join<T: store>(withdrawal: &mut Withdrawal<T>, other: Withdrawal<T>) {
    assert!(withdrawal.owner == other.owner);
    assert!(std::u256::max_value!() - withdrawal.limit >= other.limit);
    withdrawal.limit = withdrawal.limit + other.limit;
}

// TODO When this becomes `public` we need
// - custom verifier rules for `T`
// - private generic rules for `T`
public(package) fun redeem</* internal */ T: store>(withdrawal: Withdrawal<T>): T {
    let Withdrawal { owner, limit: value } = withdrawal;
    withdraw_impl(owner, value)
}

// TODO When this becomes `public` we need
// - custom verifier rules for `T`
// - private generic rules for `T`
#[allow(unused_mut_parameter)]
public(package) fun withdraw_from_object</* internal */ T: store>(obj: &mut UID, value: u256): T {
    withdraw_impl(obj.to_address(), value)
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
