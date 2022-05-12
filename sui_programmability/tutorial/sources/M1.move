// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module MyFirstPackage::M1 {
    use Sui::ID::VersionedID;
    use Sui::TxContext::TxContext;

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
        use Sui::Transfer;
        use Sui::TxContext;
        let admin = Forge {
            id: TxContext::new_id(ctx),
            swords_created: 0,
        };
        // transfer the forge object to the module/package publisher
        // (presumably the game admin)
        Transfer::transfer(admin, TxContext::sender(ctx));
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

    public(script) fun sword_create(forge: &mut Forge, magic: u64, strength: u64, recipient: address, ctx: &mut TxContext) {
        use Sui::Transfer;
        use Sui::TxContext;
        // create a sword
        let sword = Sword {
            id: TxContext::new_id(ctx),
            magic: magic,
            strength: strength,
        };
        // transfer the sword
        Transfer::transfer(sword, recipient);
        forge.swords_created = forge.swords_created + 1;
    }

    public(script) fun sword_transfer(sword: Sword, recipient: address, _ctx: &mut TxContext) {
        use Sui::Transfer;
        // transfer the sword
        Transfer::transfer(sword, recipient);
    }

    #[test]
    public fun test_module_init() {
        use Sui::TestScenario;

        // create test address representing game admin
        let admin = @0xBABE;

        // first transaction to emulate module initialization
        let scenario = &mut TestScenario::begin(&admin);
        {
            init(TestScenario::ctx(scenario));
        };
        // second transaction to check if the forge has been created
        // and has initial value of zero swords created
        TestScenario::next_tx(scenario, &admin);
        {
            // extract the Forge object
            let forge = TestScenario::take_owned<Forge>(scenario);
            // verify number of created swords
            assert!(swords_created(&forge) == 0, 1);
            // return the Forge object to the object pool
            TestScenario::return_owned(scenario, forge)
        }
    }

    #[test]
    public(script) fun test_sword_transactions() {
        use Sui::TestScenario;

        // create test addresses representing users
        let admin = @0xBABE;
        let initial_owner = @0xCAFE;
        let final_owner = @0xFACE;

        // first transaction to emulate module initialization
        let scenario = &mut TestScenario::begin(&admin);
        {
            init(TestScenario::ctx(scenario));
        };
        // second transaction executed by admin to create the sword
        TestScenario::next_tx(scenario, &admin);
        {
            let forge = TestScenario::take_owned<Forge>(scenario);
            // create the sword and transfer it to the initial owner
            sword_create(&mut forge, 42, 7, initial_owner, TestScenario::ctx(scenario));
            TestScenario::return_owned(scenario, forge)
        };
        // third transaction executed by the initial sword owner
        TestScenario::next_tx(scenario, &initial_owner);
        {
            // extract the sword owned by the initial owner
            let sword = TestScenario::take_owned<Sword>(scenario);
            // transfer the sword to the final owner
            sword_transfer(sword, final_owner, TestScenario::ctx(scenario));
        };
        // fourth transaction executed by the final sword owner
        TestScenario::next_tx(scenario, &final_owner);
        {

            // extract the sword owned by the final owner
            let sword = TestScenario::take_owned<Sword>(scenario);
            // verify that the sword has expected properties
            assert!(magic(&sword) == 42 && strength(&sword) == 7, 1);
            // return the sword to the object pool (it cannot be simply "dropped")
            TestScenario::return_owned(scenario, sword)
        }
    }


    #[test]
    public fun test_sword_create() {
        use Sui::Transfer;
        use Sui::TxContext;

        // create a dummy TxContext for testing
        let ctx = TxContext::dummy();

        // create a sword
        let sword = Sword {
            id: TxContext::new_id(&mut ctx),
            magic: 42,
            strength: 7,
        };

        // check if accessor functions return correct values
        assert!(magic(&sword) == 42 && strength(&sword) == 7, 1);

        // create a dummy address and transfer the sword
        let dummy_address = @0xCAFE;
        Transfer::transfer(sword, dummy_address);
    }

}
