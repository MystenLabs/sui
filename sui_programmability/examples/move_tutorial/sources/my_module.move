// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module my_first_package::my_module {
    // Part 1: imports
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    // Part 2: struct definitions
    struct Sword has key, store {
        id: UID,
        magic: u64,
        strength: u64,
    }

    struct Forge has key {
        id: UID,
        swords_created: u64,
    }

    #[allow(unused_function)]
    // Part 3: module initializer to be executed when this module is published
    fun init(ctx: &mut TxContext) {
        let admin = Forge {
            id: object::new(ctx),
            swords_created: 0,
        };
        // transfer the forge object to the module/package publisher
        transfer::transfer(admin, tx_context::sender(ctx));
    }

    // Part 4: accessors required to read the struct attributes
    public fun magic(self: &Sword): u64 {
        self.magic
    }

    public fun strength(self: &Sword): u64 {
        self.strength
    }

    public fun swords_created(self: &Forge): u64 {
        self.swords_created
    }

    // Part 5: entry functions to create and transfer swords
    public entry fun sword_create(forge: &mut Forge, magic: u64, strength: u64, recipient: address, ctx: &mut TxContext) {
        // create a sword
        let sword = Sword {
            id: object::new(ctx),
            magic: magic,
            strength: strength,
        };
        // transfer the sword
        transfer::public_transfer(sword, recipient);
        forge.swords_created = forge.swords_created + 1;
    }

    // Part 6: tests
    #[test]
    public fun test_module_init() {
        use sui::test_scenario;

        // create test address representing game admin
        let admin = @0xBABE;

        // first transaction to emulate module initialization
        let scenario_val = test_scenario::begin(admin);
        let scenario = &mut scenario_val;
        {
            init(test_scenario::ctx(scenario));
        };
        // second transaction to check if the forge has been created
        // and has initial value of zero swords created
        test_scenario::next_tx(scenario, admin);
        {
            // extract the Forge object
            let forge = test_scenario::take_from_sender<Forge>(scenario);
            // verify number of created swords
            assert!(swords_created(&forge) == 0, 1);
            // return the Forge object to the object pool
            test_scenario::return_to_sender(scenario, forge);
        };
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_sword_transactions() {
        use sui::test_scenario;

        // create test addresses representing users
        let admin = @0xBABE;
        let initial_owner = @0xCAFE;
        let final_owner = @0xFACE;

        // first transaction to emulate module initialization
        let scenario_val = test_scenario::begin(admin);
        let scenario = &mut scenario_val;
        {
            init(test_scenario::ctx(scenario));
        };
        // second transaction executed by admin to create the sword
        test_scenario::next_tx(scenario, admin);
        {
            let forge = test_scenario::take_from_sender<Forge>(scenario);
            // create the sword and transfer it to the initial owner
            sword_create(&mut forge, 42, 7, initial_owner, test_scenario::ctx(scenario));
            test_scenario::return_to_sender(scenario, forge)
        };
        // third transaction executed by the initial sword owner
        test_scenario::next_tx(scenario, initial_owner);
        {
            // extract the sword owned by the initial owner
            let sword = test_scenario::take_from_sender<Sword>(scenario);
            // transfer the sword to the final owner
            transfer::public_transfer(sword, final_owner);
        };
        // fourth transaction executed by the final sword owner
        test_scenario::next_tx(scenario, final_owner);
        {

            // extract the sword owned by the final owner
            let sword = test_scenario::take_from_sender<Sword>(scenario);
            // verify that the sword has expected properties
            assert!(magic(&sword) == 42 && strength(&sword) == 7, 1);
            // return the sword to the object pool (it cannot be simply "dropped")
            test_scenario::return_to_sender(scenario, sword)
        };
        test_scenario::end(scenario_val);
    }


    #[test]
    public fun test_sword_create() {
        // create a dummy TxContext for testing
        let ctx = tx_context::dummy();

        // create a sword
        let sword = Sword {
            id: object::new(&mut ctx),
            magic: 42,
            strength: 7,
        };

        // check if accessor functions return correct values
        assert!(magic(&sword) == 42 && strength(&sword) == 7, 1);

        // create a dummy address and transfer the sword
        let dummy_address = @0xCAFE;
        transfer::public_transfer(sword, dummy_address);
    }
}
