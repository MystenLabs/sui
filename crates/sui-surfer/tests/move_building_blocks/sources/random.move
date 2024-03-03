// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module move_building_blocks::random {
    use sui::object::UID;
    use sui::object;
    use sui::tx_context::TxContext;
    use sui::transfer;
    use sui::random;


    // Test transactions that use the same shared object, sometimes with Random and sometimes without.

    struct SharedObject has key, store {
        id: UID,
        value: u64,
    }

    fun init(ctx: &mut TxContext) {
        let object = SharedObject {
            id: object::new(ctx),
            value: 123,
        };
        transfer::share_object(object);
    }

    entry fun mutate_shared_object(obj: &mut SharedObject, r: &random::Random, ctx: &mut TxContext) {
        let gen = random::new_generator(r, ctx);
        obj.value = random::generate_u64(&mut gen);
    }

    entry fun conditional_increment_shared_object(obj: &mut SharedObject) {
        if (obj.value < 10000) {
            obj.value += 1;
        }
    }


    // Test transactions that use Random without a shared object.
    entry fun generate_random_u64(r: &random::Random, ctx: &mut TxContext): u64 {
        let gen = random::new_generator(r, ctx);
        random::generate_u64(&mut gen)
    }
}
