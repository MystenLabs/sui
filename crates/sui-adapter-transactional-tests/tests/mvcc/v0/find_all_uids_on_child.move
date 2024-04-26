// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests finding UIDs for dynamic field access on a child object (non-input)

//# init --addresses test=0x0 --accounts A --protocol-version 16

//# publish

module test::m {
    use sui::dynamic_field as field;

    public struct Parent has key, store {
        id: UID,
    }

    public struct S has key, store {
        id: UID,
        other: UID,
        wrapped: Wrapped,
        many: vector<Wrapped>,
    }

    public struct Wrapped has key, store {
        id: UID,
        other: UID,
    }

    const KEY: u64 = 0;

    //////////////////////////////////////////////////////////////
    // new

    public fun new(ctx: &mut TxContext): Parent {
        let mut parent = Parent { id: object::new(ctx) };
        field::add(&mut parent.id, KEY, s(ctx));
        parent
    }

    fun s(ctx: &mut TxContext): S {
        let mut s = S {
            id: object::new(ctx),
            other: object::new(ctx),
            wrapped: wrapped(ctx),
            many: vector[wrapped(ctx), wrapped(ctx)],
        };
        field::add(&mut s.id, KEY, 0);
        field::add(&mut s.other, KEY, 0);
        s
    }

    fun wrapped(ctx: &mut TxContext): Wrapped {
        let mut w = Wrapped {
            id: object::new(ctx),
            other: object::new(ctx),
        };
        field::add(&mut w.id, KEY, 0);
        field::add(&mut w.other, KEY, 0);
        w
    }

    //////////////////////////////////////////////////////////////
    // set

    public fun set(parent: &mut Parent, value: u64) {
        set_s(field::borrow_mut(&mut parent.id, KEY), value);
    }


    fun set_s(s: &mut S, value: u64) {
        set_(&mut s.id, value);
        set_(&mut s.other, value);
        set_wrapped(&mut s.wrapped, value);
        set_wrapped(vector::borrow_mut(&mut s.many, 0), value);
        set_wrapped(vector::borrow_mut(&mut s.many, 1), value);
    }

    fun set_wrapped(w: &mut Wrapped, value: u64) {
        set_(&mut w.id, value);
        set_(&mut w.other, value);

    }

    fun set_(id: &mut UID, value: u64) {
        *field::borrow_mut(id, KEY) = value;
    }

    //////////////////////////////////////////////////////////////
    // remove

    public fun remove(parent: &mut Parent) {
        remove_s(field::borrow_mut(&mut parent.id, KEY));
    }

    fun remove_s(s: &mut S) {
        remove_(&mut s.id);
        remove_(&mut s.other);
        remove_wrapped(&mut s.wrapped);
        remove_wrapped(vector::borrow_mut(&mut s.many, 0));
        remove_wrapped(vector::borrow_mut(&mut s.many, 1));
    }

    fun remove_wrapped(w: &mut Wrapped) {
        remove_(&mut w.id);
        remove_(&mut w.other);
    }

    fun remove_(id: &mut UID) {
        field::remove<u64, u64>(id, KEY);
    }

    //////////////////////////////////////////////////////////////
    // check

    public fun check(parent: &Parent, expected: Option<u64>) {
        check_s(field::borrow(&parent.id, KEY), expected);
    }

    fun check_s(s: &S, expected: Option<u64>) {
        check_(&s.id, expected);
        check_(&s.other, expected);
        check_wrapped(&s.wrapped, expected);
        check_wrapped(vector::borrow(&s.many, 0), expected);
        check_wrapped(vector::borrow(&s.many, 1), expected);
    }

    fun check_wrapped(w: &Wrapped, expected: Option<u64>) {
        check_(&w.id, expected);
        check_(&w.other, expected);
    }

    fun check_(id: &UID, expected: Option<u64>) {
        if (option::is_some(&expected)) {
            let f = field::borrow(id, KEY);
            assert!(f == option::borrow(&expected), 0);
        } else {
            assert!(!field::exists_with_type<u64, u64>(id, KEY), 0);
        }
    }
}

//# programmable --sender A --inputs @A
//> 0: test::m::new();
//> TransferObjects([Result(0)], Input(0))

//# view-object 2,9

//# programmable --sender A --inputs object(2,9) 112
//> test::m::set(Input(0), Input(1))

//# view-object 2,9

//# programmable --sender A --inputs object(2,9) 112
//> test::m::remove(Input(0))

//# view-object 2,9


// dev-inspect with 'check' and correct values

//# programmable --sender A --inputs object(2,9)@2 vector[0] --dev-inspect
//> test::m::check(Input(0), Input(1))

//# programmable --sender A --inputs object(2,9)@3 vector[112] --dev-inspect
//> test::m::check(Input(0), Input(1))

//# programmable --sender A --inputs object(2,9)@4 vector[] --dev-inspect
//> test::m::check(Input(0), Input(1))


// dev-inspect with 'check' and _incorrect_ values

//# programmable --sender A --inputs object(2,9)@3 vector[0] --dev-inspect
//> test::m::check(Input(0), Input(1))

//# programmable --sender A --inputs object(2,9)@4 vector[112] --dev-inspect
//> test::m::check(Input(0), Input(1))

//# programmable --sender A --inputs object(2,9)@2 vector[] --dev-inspect
//> test::m::check(Input(0), Input(1))
