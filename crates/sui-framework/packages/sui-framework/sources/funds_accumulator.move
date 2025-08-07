// Accumulator but with lots of private generics
module sui::funds_accumulator;

public struct Withdrawal<phantom T: store> has drop {
    // Owner? Controller? Account?
    owner: address,
    // at signing we check this isn't too big for `T`
    limit: u256,
}

// TODO when this becomes public we will need
// - custom verifier rules for `T` that it is a struct with a single unsigned integer field.
//   Or a struct with a single field that satisfies this property recursively.
// - private generic rules for `T`
public(package) native fun add_impl<T: store>(value: T, recipient: address);


public use fun withdraw_from as Withdrawal.withdraw;

// TODO When this becomes `public` we need
// - custom verifier rules for `T`
// - private generic rules for `T`
public(package) fun withdraw_from</* internal */ T: store>(
    withdrawal: &mut Withdrawal<T>,
    value: u256,
): T {
    assert!(withdrawal.limit >= value);
    withdrawal.limit = withdrawal.limit - value;
    withdraw_impl(withdrawal.owner, value)
}

// TODO When this becomes `public` we need
// - custom verifier rules for `T`
// - private generic rules for `T`
public(package) fun withdraw_from_object</* internal */ T: store>(
    obj: &mut UID,
    value: u256,
): T {
    withdraw_impl(obj.to_address(), value)
}

// aborts if the value is greater than the amount in the withdrawal
// Do we need to charge a small fee since we cannot charge storage fees?
// We should limit withdraws to `u*::max` for a `owner`
// in a given transaction for the given `u*` in `T`
native fun withdraw_impl<T>(owner: address, value: u256): T;

public use fun withdrawal_owner as Withdrawal.owner;

public fun withdrawal_owner<T: store>(withdrawal: &Withdrawal<T>): address {
    withdrawal.owner
}

public use fun withdrawal_limit as Withdrawal.limit;

public fun withdrawal_limit<T: store>(withdrawal: &Withdrawal<T>): u256 {
    withdrawal.limit
}

public use fun withdrawal_split as Withdrawal.split;

public fun withdrawal_split<T: store>(
    withdrawal: &mut Withdrawal<T>,
    value: u256,
): Withdrawal<T> {
    assert!(withdrawal.limit >= value);
    withdrawal.limit = withdrawal.limit - value;
    Withdrawal { owner: withdrawal.owner, limit: value }
}

public use fun withdrawal_join as Withdrawal.join;

public fun withdrawal_join<T: store>(
    withdrawal: &mut Withdrawal<T>,
    other: Withdrawal<T>,
) {
    assert!(withdrawal.owner == other.owner);
    assert!(std::u256::max_value!() - withdrawal.limit >= other.limit);
    let old_limit = withdrawal.limit;
    withdrawal.limit = withdrawal.limit + other.limit;
}
