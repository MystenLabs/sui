// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::prover_tests {
    use sui::object::UID;

    struct Obj has key, store {
        id: UID
    }

    // ====================================================================
    // Object ownership
    // ====================================================================

    public fun simple_transfer(o: Obj, recipient: address) {
        sui::transfer::public_transfer(o, recipient);
    }

    spec simple_transfer {
        ensures sui::prover::owned_by(o, recipient);
        aborts_if false;
    }

    public fun simple_share(o: Obj) {
        sui::transfer::public_share_object(o)
    }

    spec simple_share {
        ensures sui::prover::shared(o);
        aborts_if sui::prover::owned(o);
    }

    public fun simple_freeze(o: Obj) {
        sui::transfer::public_freeze_object(o)
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
        sui::dynamic_field::add(&mut o.id, n1, v1);
        sui::dynamic_field::add(&mut o.id, n2, v2);
    }

    spec simple_field_add {
        aborts_if sui::prover::has_field(o, n1);
        aborts_if sui::prover::has_field(o, n2);
        ensures sui::prover::has_field(o, n1);
        ensures sui::prover::has_field(o, n2);
        ensures sui::prover::num_fields<Obj,u64>(o) == old(sui::prover::num_fields<Obj,u64>(o)) + 1;
        ensures sui::prover::num_fields<Obj,u8>(o) == old(sui::prover::num_fields<Obj,u8>(o)) + 1;
    }

    public fun simple_field_remove(o: &mut Obj, n1: u64, n2: u8) {
        sui::dynamic_field::remove<u64,u8>(&mut o.id, n1);
        sui::dynamic_field::remove<u8,u64>(&mut o.id, n2);
    }

    spec simple_field_remove {
        aborts_if !sui::prover::has_field(o, n1);
        aborts_if !sui::prover::has_field(o, n2);
        ensures !sui::prover::has_field(o, n1);
        ensures !sui::prover::has_field(o, n2);
        ensures sui::prover::num_fields<Obj,u64>(o) == old(sui::prover::num_fields<Obj,u64>(o)) - 1;
        ensures sui::prover::num_fields<Obj,u8>(o) == old(sui::prover::num_fields<Obj,u8>(o)) - 1;
    }
}
