// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module first_package::example {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    struct Sword has key, store {
        id: UID,
        magic: u64,
        strength: u64,
    }

    struct Forge has key {
        id: UID,
        swords_created: u64,
    }

    /// Module initializer to be executed when this module is published
    fun init(ctx: &mut TxContext) {
        let admin = Forge {
            id: object::new(ctx),
            swords_created: 0,
        };

        // transfer the forge object to the module/package publisher
        transfer::transfer(admin, tx_context::sender(ctx));
    }

    // === Accessors ===

    public fun magic(self: &Sword): u64 {
        self.magic
    }

    public fun strength(self: &Sword): u64 {
        self.strength
    }

    public fun swords_created(self: &Forge): u64 {
        self.swords_created
    }

    /// Constructor for creating swords
    public fun new_sword(
        forge: &mut Forge,
        magic: u64,
        strength: u64,
        ctx: &mut TxContext,
    ): Sword {
        forge.swords_created = forge.swords_created + 1;
        Sword {
            id: object::new(ctx),
            magic: magic,
            strength: strength,
        }
    }

    // === Tests ===
    #[test_only] use sui::test_scenario as ts;

    #[test_only] const ADMIN: address = @0xAD;
    #[test_only] const ALICE: address = @0xA;
    #[test_only] const BOB: address = @0xB;

    #[test]
    public fun test_module_init() {
        let ts = ts::begin(@0x0);

        // first transaction to emulate module initialization.
        {
            ts::next_tx(&mut ts, ADMIN);
            init(ts::ctx(&mut ts));
        };

        // second transaction to check if the forge has been created
        // and has initial value of zero swords created
        {
            ts::next_tx(&mut ts, ADMIN);

            // extract the Forge object
            let forge: Forge = ts::take_from_sender(&ts);

            // verify number of created swords
            assert!(swords_created(&forge) == 0, 1);

            // return the Forge object to the object pool
            ts::return_to_sender(&ts, forge);
        };

        ts::end(ts);
    }

    #[test]
    fun test_sword_transactions() {
        let ts = ts::begin(@0x0);

        // first transaction to emulate module initialization
        {
            ts::next_tx(&mut ts, ADMIN);
            init(ts::ctx(&mut ts));
        };

        // second transaction executed by admin to create the sword
        {
            ts::next_tx(&mut ts, ADMIN);
            let forge: Forge = ts::take_from_sender(&ts);
            // create the sword and transfer it to the initial owner
            let sword = new_sword(&mut forge, 42, 7, ts::ctx(&mut ts));
            transfer::public_transfer(sword, ALICE);
            ts::return_to_sender(&ts, forge);
        };

        // third transaction executed by the initial sword owner
        {
            ts::next_tx(&mut ts, ALICE);
            // extract the sword owned by the initial owner
            let sword: Sword = ts::take_from_sender(&ts);
            // transfer the sword to the final owner
            transfer::public_transfer(sword, BOB);
        };

        // fourth transaction executed by the final sword owner
        {
            ts::next_tx(&mut ts, BOB);
            // extract the sword owned by the final owner
            let sword: Sword = ts::take_from_sender(&ts);
            // verify that the sword has expected properties
            assert!(magic(&sword) == 42 && strength(&sword) == 7, 1);
            // return the sword to the object pool (it cannot be dropped)
            ts::return_to_sender(&ts, sword)
        };

        ts::end(ts);
    }
}
