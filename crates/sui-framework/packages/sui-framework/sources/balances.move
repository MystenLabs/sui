module sui::balances;

use std::type_name::{Self, TypeName};

use sui::balance::Balance;
use sui::dynamic_field;
use sui::coin::Coin;
use sui::vec_set::{Self, VecSet};

#[error]
const EInvalidDestroy: vector<u8> = b"Cannot destroy non-empty Balances";
#[error]
const EBalanceTypeNotFound: vector<u8> = b"This Balances object does not have a balance of the given type";
#[error]
const EInsufficientBalance: vector<u8> = b"This Balances object does not have a sufficient balance in the given type";

/// Heterogenous collection of `Balance`s stored via dynamic fields.
/// Supports splitting, joining, and other iteration operations that would not be possible in pure Move.
public struct Balances has key, store {
    id: UID,
    // TODO: need sorted collection here
    types: VecSet<TypeName>,
}

public struct Amount has copy, drop {
    typ: TypeName,
    amount: u64
}

public fun new(ctx: &mut TxContext): Balances {
    Balances { id: object::new(ctx), types: vec_set::empty() }
}

/// Merge the balances in `other` into `bals`
public fun join(bals: &mut Balances, other: Balances) {
    let Balances { id, types } = other;
    types.into_keys().do!(|t| bals.types.insert(t));
    id.delete()
}

/// Join Balance dynamic fields of `from` into `to`
native fun join_balances(to: UID, from: &mut UID);

/// Create a new `Balances` object by splitting out the amount specified in `amounts
public fun split(bals: &mut Balances, amounts: vector<Amount>): Balances {
    let types = &bals.types;
    amounts.do_ref!(|amount| assert!(types.contains(&amount.typ), EBalanceTypeNotFound));
    split_balances(&mut bals.id, amounts)
}

native fun split_balances(bals: &mut UID, amounts: vector<Amount>): Balances;

public fun insert_coin<T>(bals: &mut Balances, c: Coin<T>) {
    bals.insert_balance(c.into_balance())
}

public fun insert_balance<T>(bals: &mut Balances, b: Balance<T>) {
    let t = type_name::get<T>();
    // TODO: use insertion sort here
    bals.types.insert(t);
    dynamic_field::add(&mut bals.id, t, b)
}

public fun split_balance<T>(bals: &mut Balances, amount: u64): Balance<T> {
    let t = type_name::get<T>();
    assert!(dynamic_field::exists_(&bals.id, t), EBalanceTypeNotFound);
    let mut bal = dynamic_field::borrow_mut<TypeName, Balance<T>>(&mut bals.id, t);
    assert!(bal.value() >= amount, EInsufficientBalance);
    bal.split(amount)
}

// TODO: could provide native function that tolerated non-emptiness if balances are all zero
/// Destroy an empty collection of Balances
public fun destroy_empty(bals: Balances) {
    let Balances { id, types } = bals;
    assert!(types.is_empty(), EInvalidDestroy);
    id.delete()
}

