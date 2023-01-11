// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module frenemies::registry {
    use std::string::{Self, String};
    use std::option::{Self, Option};
    use sui::object::{Self, UID};
    use sui::table::{Self, Table};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    friend frenemies::frenemies;

    /// Shared registry ensuring global uniqueness of names + tracking total participants
    struct Registry has key {
        id: UID,
        players: Table<Name, address>,
    }

    /// Wrapper around String that enforces a max name length
    struct Name has copy, drop, store {
        name: String
    }

    /// This name exceeds the max length
    const ENameTooLong: u64 = 0;

    /// This name has already been registered by a different player
    const ENameAlreadyRegistered: u64 = 1;

    /// The maximum length of a username
    const MAX_NAME_SIZE: u64 = 64;

    fun init(ctx: &mut TxContext) {
        transfer::share_object(Registry {
            id: object::new(ctx),
            players: table::new(ctx),
        })
    }

    /// Only callable from scorecard
    public(friend) fun register(self: &mut Registry, name: String, ctx: &TxContext): Name {
        assert!(string::length(&name) <= MAX_NAME_SIZE, ENameTooLong);

        let name = Name { name };
        let players = &mut self.players;
        assert!(!table::contains(players, name), ENameAlreadyRegistered);
        table::add(players, name, tx_context::sender(ctx));
        name
    }


    /// Return the address of the player with `name`, if any
    public fun player_address(self: &Registry, name: String): Option<address> {
        let name = Name { name };
        let players = &self.players;
        if (table::contains(players, name)) {
            option::some(*table::borrow(players, name))
        } else {
            option::none()
        }
    }

    /// Return `true` if `name` is registered
    public fun is_registered(self: &Registry, name: String): bool {
        table::contains(&self.players, Name { name })
    }

    /// Return the number of players that have registered
    public fun num_players(self: &Registry): u64 {
        table::length(&self.players)
    }

    #[test_only]
    public fun init_for_testing(ctx: &mut TxContext) {
        init(ctx)
    }

    #[test_only]
    public fun name_for_testing(name: String): Name {
        Name { name }
    }

    #[test]
    fun e2e() {
        use sui::test_scenario as ts;

        let scenario = ts::begin(@0xA);
        let s = &mut scenario;
        init(ts::ctx(s));
        ts::next_tx(s, @0xA);
        let registry = ts::take_shared<Registry>(s);
        let name = string::utf8(b"alice");

        assert!(!is_registered(&registry, name), 0);
        assert!(player_address(&registry, name) == option::none(), 0);
        assert!(num_players(&registry) == 0, 0);
        register(&mut registry, name, ts::ctx(s));
        assert!(is_registered(&registry, name), 0);
        assert!(player_address(&registry, name) == option::some(@0xA), 0);
        assert!(num_players(&registry) == 1, 0);

        ts::return_shared(registry);
        ts::end(scenario);
    }

    #[expected_failure(abort_code = frenemies::registry::ENameAlreadyRegistered)]
    #[test]
    fun double_register() {
        use sui::test_scenario as ts;

        let scenario = ts::begin(@0xA);
        let s = &mut scenario;
        init(ts::ctx(s));
        ts::next_tx(s, @0xA);
        let registry = ts::take_shared<Registry>(s);
        let name = string::utf8(b"alice");

        register(&mut registry, name, ts::ctx(s));
        register(&mut registry, name, ts::ctx(s)); // should fail here

        ts::return_shared(registry);
        ts::end(scenario);
    }

    #[expected_failure(abort_code = frenemies::registry::ENameTooLong)]
    #[test]
    fun register_big_name() {
        use sui::test_scenario as ts;

        let scenario = ts::begin(@0xA);
        let s = &mut scenario;
        init(ts::ctx(s));
        ts::next_tx(s, @0xA);
        let registry = ts::take_shared<Registry>(s);
        let name = string::utf8(b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");

        register(&mut registry, name, ts::ctx(s)); // should fail here

        ts::return_shared(registry);
        ts::end(scenario);
    }
}
