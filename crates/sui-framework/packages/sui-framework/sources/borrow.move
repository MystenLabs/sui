// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A simple library that enables hot-potato-locked borrow mechanics.
///
/// With Programmable transactions, it is possible to borrow a value within
/// a transaction, use it and put back in the end. Hot-potato `Borrow` makes
/// sure the object is returned and was not swapped for another one.
module sui::borrow;

/// The `Borrow` does not match the `Referent`.
const EWrongBorrow: u64 = 0;
/// An attempt to swap the `Referent.value` with another object of the same type.
const EWrongValue: u64 = 1;

/// An object wrapping a `T` and providing the borrow API.
public struct Referent<T: key + store> has store {
    id: address,
    value: Option<T>,
}

/// A hot potato making sure the object is put back once borrowed.
public struct Borrow { ref: address, obj: ID }

/// Create a new `Referent` struct
public fun new<T: key + store>(value: T, ctx: &mut TxContext): Referent<T> {
    Referent {
        id: ctx.fresh_object_address(),
        value: option::some(value),
    }
}

/// Borrow the `T` from the `Referent`, receiving the `T` and a `Borrow`
/// hot potato.
public fun borrow<T: key + store>(self: &mut Referent<T>): (T, Borrow) {
    let value = self.value.extract();
    let id = object::id(&value);

    (
        value,
        Borrow {
            ref: self.id,
            obj: id,
        },
    )
}

/// Put an object and the `Borrow` hot potato back.
public fun put_back<T: key + store>(self: &mut Referent<T>, value: T, borrow: Borrow) {
    let Borrow { ref, obj } = borrow;

    assert!(object::id(&value) == obj, EWrongValue);
    assert!(self.id == ref, EWrongBorrow);
    self.value.fill(value);
}

/// Unpack the `Referent` struct and return the value.
public fun destroy<T: key + store>(self: Referent<T>): T {
    let Referent { id: _, value } = self;
    value.destroy_some()
}

#[test_only]
public struct Test has key, store {
    id: UID,
}

#[test]
fun test_borrow() {
    let ctx = &mut tx_context::dummy();
    let mut ref = new(Test { id: object::new(ctx) }, ctx);

    let (value, borrow) = ref.borrow();
    ref.put_back(value, borrow);

    let Test { id } = ref.destroy();
    id.delete();
}

#[test, expected_failure(abort_code = EWrongValue)]
/// The `value` is swapped with another instance of the type `T`.
fun test_object_swap() {
    let ctx = &mut tx_context::dummy();
    let mut ref_1 = new(Test { id: object::new(ctx) }, ctx);
    let mut ref_2 = new(Test { id: object::new(ctx) }, ctx);

    let (v_1, b_1) = ref_1.borrow();
    let (v_2, b_2) = ref_2.borrow();

    ref_1.put_back(v_2, b_1);
    ref_2.put_back(v_1, b_2);

    let Test { id } = ref_1.destroy();
    id.delete();

    let Test { id } = ref_2.destroy();
    id.delete();
}

#[test, expected_failure(abort_code = EWrongValue)]
/// The `value` is swapped with another instance of the type `T`.
fun test_borrow_swap() {
    let ctx = &mut sui::tx_context::dummy();
    let mut ref_1 = new(Test { id: object::new(ctx) }, ctx);
    let mut ref_2 = new(Test { id: object::new(ctx) }, ctx);

    let (v_1, b_1) = ref_1.borrow();
    let (v_2, b_2) = ref_2.borrow();

    ref_1.put_back(v_1, b_2);
    ref_2.put_back(v_2, b_1);

    let Test { id } = ref_1.destroy();
    id.delete();

    let Test { id } = ref_2.destroy();
    id.delete();
}

#[test, expected_failure(abort_code = EWrongBorrow)]
/// The both `borrow` and `value` are swapped with another `Referent`.
fun test_borrow_fail() {
    let ctx = &mut tx_context::dummy();
    let mut ref_1 = new(Test { id: object::new(ctx) }, ctx);
    let mut ref_2 = new(Test { id: object::new(ctx) }, ctx);

    let (v_1, b_1) = ref_1.borrow();
    let (v_2, b_2) = ref_2.borrow();

    ref_1.put_back(v_2, b_2);
    ref_2.put_back(v_1, b_1);

    let Test { id } = ref_1.destroy();
    id.delete();

    let Test { id } = ref_2.destroy();
    id.delete();
}
