// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::account;

use std::type_name;
use sui::dynamic_field as field;
use sui::object::sui_account_root_address;

const ENotAccountOwner: u64 = 0;
const EInsufficientFunds: u64 = 1;

public struct AccountKey has copy, drop, store {
    address: address,
    ty: vector<u8>,
}

fun get_account_field_address<T>(address: address): address {
    let ty = type_name::get_with_original_ids<T>().into_string().into_bytes();
    let key = AccountKey { address, ty };
    return field::hash_type_and_key(sui_account_root_address(), key)
}

public struct Reservation<phantom T> has drop {
    owner: address,
    limit: u64,
}

fun decrement<T>(self: &mut Reservation<T>, amount: u64) {
    assert!(self.limit >= amount, EInsufficientFunds);
    self.limit = self.limit - amount;
}

/// The scheduling/execution layer ensures that a reserve() call is never permitted unless
/// there are sufficient funds. `Reservation<T>` thus serves as a proof that the account
/// cannot be overdrawn.
entry fun reserve<T>(owner: address, limit: u64, ctx: &TxContext): Reservation<T> {
    // TODO: handle sponsored transactions and (in the future) multi-agent transactions
    assert!(ctx.sender() == owner, ENotAccountOwner);
    return Reservation { owner, limit }
}

/// Withdraw from an account.
/// Requires a reservation of the appropriate type, with proof of enough funds to cover the
/// withdrawal.
///
/// `value` is an amount to be debited from the account. Therefore, modules who wish to
/// ensure conservation should use the following pattern, in which identical and offsetting
/// credits and debits are created at the same time:
/// 
/// fun withdraw(reservation: &mut Reservation<Foo>, amount: u64, ctx: &TxContext): Foo {
///   let debit = MergableFoo { value: amount };
///   let credit = Foo { value: amount };
///   account::withdraw_from_account(reservation, debit, amount, ctx.sender());
///   credit
/// }
public fun withdraw_from_account<T>(
    // reservation proves that the account has enough funds, and that the withdrawal
    // is authorized
    reservation: &mut Reservation<T>, 
    // debit is a typed wrapper around `amount`. It must contain the same value
    // as is stored in `amount`. iiuc we should be able to remove this duplication when
    // signatures are available.
    debit: T,
    amount: u64,
) {
    // Conservation: aborts if reservation is insufficient
    reservation.decrement(amount);
    let account_address = get_account_field_address<T>(reservation.owner);
    // Conservation:
    // - `debit` will be subtracted from the account
    // - No new reservations will be issued without taking into account the debit.
    split_from_account(debit, account_address);
}

/// Transfer a value to an account.
///
/// TODO: requires move verifier changes (analagous to the `transfer` checks) that ensures that
/// this can only be called from the module in which T is defined. Modules can implement secure
/// accounts without this by using a private type that cannot be constructed outside the module.
///
/// Because types must explicitly implement a conversion from their ordinary type to a mergable
/// type (i.e. one made only of types defined in mergable.move), there is no need for an analogue
/// to `public_transfer`.
public fun transfer_to_account<T>(deposit: T, recipient: address) {
    // Conservation: deposit is consumed here, and is guaranteed to be merged
    // into the recipient account.
    let account_address = get_account_field_address<T>(recipient);
    merge_to_account(deposit, account_address)
}

public struct Merge<T> {
    address: address,
    value: T,
}

public struct Split<T> {
    address: address,
    value: T,
}

fun merge_to_account<T>(value: T, recipient: address) {
    emit_account_event(Merge { address: recipient, value });
}

fun split_from_account<T>(value: T, holder: address) {
    emit_account_event(Split { address: holder, value });
}

/// TODO: this must abort if `value` contains any "naked" primitives - it must be built
/// solely from types defined in mergable.move
native fun emit_account_event<T>(value: T);