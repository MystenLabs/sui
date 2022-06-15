// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module my_first_package::m1 {
    use sui::id::VersionedID;
    use sui::tx_context::TxContext;

    struct Sword has key, store {
        id: VersionedID,
        magic: u64,
        strength: u64,
    }

    struct Forge has key, store {
        id: VersionedID,
        swords_created: u64,
    }

    // module initializer to be executed when this module is published
    fun init(ctx: &mut TxContext) {
        use sui::transfer;
        use sui::tx_context;
        let admin = Forge {
            id: tx_context::new_id(ctx),
            swords_created: 0,
        };
        // transfer the forge object to the module/package publisher
        // (presumably the game admin)
        transfer::transfer(admin, tx_context::sender(ctx));
    }

    public fun swords_created(self: &Forge): u64 {
        self.swords_created
    }

    public fun magic(self: &Sword): u64 {
        self.magic
    }

    public fun strength(self: &Sword): u64 {
        self.strength
    }

    public entry fun sword_create(forge: &mut Forge, magic: u64, strength: u64, recipient: address, ctx: &mut TxContext) {
        use sui::transfer;
        use sui::tx_context;
        // create a sword
        let sword = Sword {
            id: tx_context::new_id(ctx),
            magic: magic,
            strength: strength,
        };
        // transfer the sword
        transfer::transfer(sword, recipient);
        forge.swords_created = forge.swords_created + 1;
    }

    public entry fun sword_transfer(sword: Sword, recipient: address) {
        use sui::transfer;
        // transfer the sword
        transfer::transfer(sword, recipient);
    }

    #[test]
    public fun test_module_init() {
        use sui::test_scenario;

        // create test address representing game admin
        let admin = @0xBABE;

        // first transaction to emulate module initialization
        let scenario = &mut test_scenario::begin(&admin);
        {
            init(test_scenario::ctx(scenario));
        };
        // second transaction to check if the forge has been created
        // and has initial value of zero swords created
        test_scenario::next_tx(scenario, &admin);
        {
            // extract the Forge object
            let forge = test_scenario::take_owned<Forge>(scenario);
            // verify number of created swords
            assert!(swords_created(&forge) == 0, 1);
            // return the Forge object to the object pool
            test_scenario::return_owned(scenario, forge)
        }
    }

    #[test]
    fun test_sword_transactions() {
        use sui::test_scenario;

        // create test addresses representing users
        let admin = @0xBABE;
        let initial_owner = @0xCAFE;
        let final_owner = @0xFACE;

        // first transaction to emulate module initialization
        let scenario = &mut test_scenario::begin(&admin);
        {
            init(test_scenario::ctx(scenario));
        };
        // second transaction executed by admin to create the sword
        test_scenario::next_tx(scenario, &admin);
        {
            let forge = test_scenario::take_owned<Forge>(scenario);
            // create the sword and transfer it to the initial owner
            sword_create(&mut forge, 42, 7, initial_owner, test_scenario::ctx(scenario));
            test_scenario::return_owned(scenario, forge)
        };
        // third transaction executed by the initial sword owner
        test_scenario::next_tx(scenario, &initial_owner);
        {
            // extract the sword owned by the initial owner
            let sword = test_scenario::take_owned<Sword>(scenario);
            // transfer the sword to the final owner
            sword_transfer(sword, final_owner);
        };
        // fourth transaction executed by the final sword owner
        test_scenario::next_tx(scenario, &final_owner);
        {

            // extract the sword owned by the final owner
            let sword = test_scenario::take_owned<Sword>(scenario);
            // verify that the sword has expected properties
            assert!(magic(&sword) == 42 && strength(&sword) == 7, 1);
            // return the sword to the object pool (it cannot be simply "dropped")
            test_scenario::return_owned(scenario, sword)
        }
    }


    #[test]
    public fun test_sword_create() {
        use sui::transfer;
        use sui::tx_context;

        // create a dummy TxContext for testing
        let ctx = tx_context::dummy();

        // create a sword
        let sword = Sword {
            id: tx_context::new_id(&mut ctx),
            magic: 42,
            strength: 7,
        };

        // check if accessor functions return correct values
        assert!(magic(&sword) == 42 && strength(&sword) == 7, 1);

        // create a dummy address and transfer the sword
        let dummy_address = @0xCAFE;
        transfer::transfer(sword, dummy_address);
    }

}
