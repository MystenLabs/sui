// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::prover_tests {
    use sui::object::UID;
    use sui::dynamic_field;

    struct Obj has key, store {
        id: UID,
    }

    // ====================================================================
    // Object ownership
    // ====================================================================

    public fun simple_transfer(o: Obj, recipient: address) {
        sui::transfer::transfer(o, recipient);
    }

    spec simple_transfer {
        ensures sui::prover::owned_by(o, recipient);
        aborts_if false;
    }

    public fun simple_share(o: Obj) {
        sui::transfer::share_object(o)
    }

    spec simple_share {
        ensures sui::prover::shared(o);
        aborts_if sui::prover::owned(o);
    }

    public fun simple_freeze(o: Obj) {
        sui::transfer::freeze_object(o)
    }

    spec simple_freeze {
        ensures sui::prover::immutable(o);
        aborts_if false;
    }

    public fun simple_delete(o: Obj) {
        let Obj { id } = o;
        sui::object::delete(id);
    }

    spec simple_delete {
        aborts_if false;
        ensures !sui::prover::owned(o) && !sui::prover::shared(o) && !sui::prover::immutable(o);
    }

    // ====================================================================
    // Dynamic fields
    // ====================================================================

    public fun simple_field_add(o: &mut Obj, n1: u64, v1: u8, n2: u8, v2: u64) {
        dynamic_field::add(&mut o.id, n1, v1);
        dynamic_field::add(&mut o.id, n2, v2);
    }

    spec simple_field_add {
        aborts_if dynamic_field::spec_has_field(o, n1);
        aborts_if dynamic_field::spec_has_field(o, n2);

        ensures dynamic_field::spec_has_field(o, n1);
        ensures dynamic_field::spec_has_field(o, n2);
        ensures dynamic_field::spec_num_fields<Obj, u64>(o) ==
            old(dynamic_field::spec_num_fields<Obj, u64>(o)) + 1;
        ensures dynamic_field::spec_num_fields<Obj, u8>(o) ==
            old(dynamic_field::spec_num_fields<Obj, u8>(o)) + 1;

        ensures dynamic_field::spec_has_field_with_type<Obj, u64, u8>(o, n1);
        ensures dynamic_field::spec_has_field_with_type<Obj, u8, u64>(o, n2);
        ensures dynamic_field::spec_num_fields_with_type<Obj, u64, u8>(o) ==
            old(dynamic_field::spec_num_fields_with_type<Obj, u64, u8>(o)) + 1;
        ensures dynamic_field::spec_num_fields_with_type<Obj, u8, u64>(o) ==
            old(dynamic_field::spec_num_fields_with_type<Obj, u8, u64>(o)) + 1;
    }

    public fun simple_field_remove(o: &mut Obj, n1: u64, n2: u8) {
        sui::dynamic_field::remove<u64, u8>(&mut o.id, n1);
        sui::dynamic_field::remove<u8, u64>(&mut o.id, n2);
    }

    spec simple_field_remove {
        aborts_if !dynamic_field::spec_has_field(o, n1);
        aborts_if !dynamic_field::spec_has_field(o, n2);
        aborts_if !dynamic_field::spec_has_field_with_type<Obj, u64, u8>(o, n1);
        aborts_if !dynamic_field::spec_has_field_with_type<Obj, u8, u64>(o, n2);

        ensures !dynamic_field::spec_has_field(o, n1);
        ensures !dynamic_field::spec_has_field(o, n2);
        ensures dynamic_field::spec_num_fields<Obj, u64>(o) ==
            old(dynamic_field::spec_num_fields<Obj, u64>(o)) - 1;
        ensures dynamic_field::spec_num_fields<Obj, u8>(o) ==
            old(dynamic_field::spec_num_fields<Obj, u8>(o)) - 1;

        ensures !dynamic_field::spec_has_field_with_type<Obj, u64, u8>(o, n1);
        ensures !dynamic_field::spec_has_field_with_type<Obj, u8, u64>(o, n2);
        ensures dynamic_field::spec_num_fields_with_type<Obj, u64, u8>(o) ==
            old(dynamic_field::spec_num_fields_with_type<Obj, u64, u8>(o)) - 1;
        ensures dynamic_field::spec_num_fields_with_type<Obj, u8, u64>(o) ==
            old(dynamic_field::spec_num_fields_with_type<Obj, u8, u64>(o)) - 1;
    }

    public fun simple_field_borrow(o: &mut Obj, n1: u64, n2: u8): (u8, u64) {
        let r1 = dynamic_field::borrow<u64, u8>(&o.id, n1);
        let r2 = dynamic_field::borrow<u8, u64>(&o.id, n2);
        (*r1, *r2)
    }

    spec simple_field_borrow {
        aborts_if !dynamic_field::spec_has_field(o, n1);
        aborts_if !dynamic_field::spec_has_field(o, n2);
        aborts_if !dynamic_field::spec_has_field_with_type<Obj, u64, u8>(o, n1);
        aborts_if !dynamic_field::spec_has_field_with_type<Obj, u8, u64>(o, n2);

        ensures result_1 == dynamic_field::spec_get_value<Obj, u64, u8>(o, n1);
        ensures result_2 == dynamic_field::spec_get_value<Obj, u8, u64>(o, n2);
    }


    public fun simple_field_add_and_borrow(o: &mut Obj, n1: u64, v1: u8, n2: u8, v2: u64): (u8, u64) {
        dynamic_field::add(&mut o.id, n1, v1);
        dynamic_field::add(&mut o.id, n2, v2);
        let r1 = dynamic_field::borrow<u64, u8>(&o.id, n1);
        let r2 = dynamic_field::borrow<u8, u64>(&o.id, n2);
        (*r1, *r2)
    }
    spec simple_field_add_and_borrow {
        aborts_if dynamic_field::spec_has_field(o, n1);
        aborts_if dynamic_field::spec_has_field(o, n2);

        ensures dynamic_field::spec_has_field(o, n1);
        ensures dynamic_field::spec_has_field(o, n2);
        ensures dynamic_field::spec_num_fields<Obj, u64>(o) ==
            old(dynamic_field::spec_num_fields<Obj, u64>(o)) + 1;
        ensures dynamic_field::spec_num_fields<Obj, u8>(o) ==
            old(dynamic_field::spec_num_fields<Obj, u8>(o)) + 1;

        ensures dynamic_field::spec_has_field_with_type<Obj, u64, u8>(o, n1);
        ensures dynamic_field::spec_has_field_with_type<Obj, u8, u64>(o, n2);
        ensures dynamic_field::spec_num_fields_with_type<Obj, u64, u8>(o) ==
            old(dynamic_field::spec_num_fields_with_type<Obj, u64, u8>(o)) + 1;
        ensures dynamic_field::spec_num_fields_with_type<Obj, u8, u64>(o) ==
            old(dynamic_field::spec_num_fields_with_type<Obj, u8, u64>(o)) + 1;

        ensures result_1 == v1;
        ensures result_2 == v2;
    }

    public fun simple_field_borrow_mut(o: &mut Obj, n1: u64, v1: u8, n2: u8, v2: u64) {
        let r1 = dynamic_field::borrow_mut(&mut o.id, n1);
        *r1 = v1;

        let r2 = dynamic_field::borrow_mut(&mut o.id, n2);
        *r2 = v2;
    }
    spec simple_field_borrow_mut {
        aborts_if !dynamic_field::spec_has_field(o, n1);
        aborts_if !dynamic_field::spec_has_field(o, n2);
        aborts_if !dynamic_field::spec_has_field_with_type<Obj, u64, u8>(o, n1);
        aborts_if !dynamic_field::spec_has_field_with_type<Obj, u8, u64>(o, n2);
        // TODO(mengxu): currently incomplete about the borrowed value
    }
}
