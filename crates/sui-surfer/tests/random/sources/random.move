// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module random::random_test {
    use sui::object::UID;
    use sui::object;
    use sui::tx_context::TxContext;
    use sui::transfer;
    use sui::random;


    // Test transactions that use the same shared object, sometimes with Random and sometimes without.

    public struct SharedObject has key, store {
        id: UID,
        value: u256,
    }

    fun init(ctx: &mut TxContext) {
        let object = SharedObject {
            id: object::new(ctx),
            value: 123,
        };
        transfer::share_object(object);
    }

    // Update the shared object using Random.
    entry fun mutate_with_random(obj: &mut SharedObject, r: &random::Random, n: u8, ctx: &mut TxContext) {
        let mut gen = random::new_generator(r, ctx);
        let _b = random::generate_bytes(&mut gen, (n as u16));
        obj.value = random::generate_u256(&mut gen);
        assert!(obj.value > 0, 0); // very low probability
    }

    // Update the shared object without using Random.
    entry fun mutate_without(obj: &mut SharedObject) {
        obj.value = obj.value % 27;
    }


    // Test transactions that use Random without a shared object.
    entry fun generate(r: &random::Random, ctx: &mut TxContext): u64 {
        let _gen1 = random::new_generator(r, ctx);
        let _gen2 = random::new_generator(r, ctx);
        let mut gen3 = random::new_generator(r, ctx);
        random::generate_u64(&mut gen3)
    }
}
