// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module adversarial_dynamic_fields::adversarial_dynamic_fields {
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;
    use sui::dynamic_field::{add, borrow};

    const NUM_DYNAMIC_FIELDS: u64 = 33;


    struct Obj has key, store {
        id: object::UID,
    }

    public fun add_dynamic_fields(obj: &mut Obj, n: u64) {
        let i = 0;
        while (i < n) {
            add<u64, u64>(&mut obj.id, (i as u64), (i as u64));
            i = i + 1;
        };
    }

    /// Initialize object to be used for dynamic field opers
    fun init(ctx: &mut TxContext) {
        let id = object::new(ctx);
        let x = Obj { id };
        add_dynamic_fields(&mut x, NUM_DYNAMIC_FIELDS);
        transfer::share_object(x);
    }
}
