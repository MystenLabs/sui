// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Example of a game character with basic attributes, inventory, and
/// associated logic.
module games::hero {
    use sui::coin::Coin;
    use sui::event;
    use sui::sui::SUI;

    /// Our hero!
    public struct Hero has key, store {
        id: UID,
        /// Hit points. If they go to zero, the hero can't do anything
        hp: u64,
        /// Experience of the hero. Begins at zero
        experience: u64,
        /// The hero's minimal inventory
        sword: Option<Sword>,
        /// An ID of the game user is playing
        game_id: ID,
    }

    /// The hero's trusty sword
    public struct Sword has key, store {
        id: UID,
        /// Constant set at creation. Acts as a multiplier on sword's strength.
        /// Swords with high magic are rarer (because they cost more).
        magic: u64,
        /// Sword grows in strength as we use it
        strength: u64,
        /// An ID of the game
        game_id: ID,
    }

    /// For healing wounded heroes
    public struct Potion has key, store {
        id: UID,
        /// Effectiveness of the potion
        potency: u64,
        /// An ID of the game
        game_id: ID,
    }

    /// A creature that the hero can slay to level up
    public struct Boar has key {
        id: UID,
        /// Hit points before the boar is slain
        hp: u64,
        /// Strength of this particular boar
        strength: u64,
        /// An ID of the game
        game_id: ID,
    }

    /// An immutable object that contains information about the
    /// game admin. Created only once in the module initializer,
    /// hence it cannot be recreated or falsified.
    public struct GameInfo has key {
        id: UID,
        admin: address
    }

    /// Capability conveying the authority to create boars and potions
    public struct GameAdmin has key {
        id: UID,
        /// Total number of boars the admin has created
        boars_created: u64,
        /// Total number of potions the admin has created
        potions_created: u64,
        /// ID of the game where current user is an admin
        game_id: ID,
    }

    /// Event emitted each time a Hero slays a Boar
    public struct BoarSlainEvent has copy, drop {
        /// Address of the user that slayed the boar
        slayer_address: address,
        /// ID of the Hero that slayed the boar
        hero: ID,
        /// ID of the now-deceased boar
        boar: ID,
        /// ID of the game where event happened
        game_id: ID,
    }

    /// Upper bound on player's HP
    const MAX_HP: u64 = 1000;
    /// Upper bound on how magical a sword can be
    const MAX_MAGIC: u64 = 10;
    /// Minimum amount you can pay for a sword
    const MIN_SWORD_COST: u64 = 100;

    // TODO: proper error codes
    /// The boar won the battle
    const EBOAR_WON: u64 = 0;
    /// Not enough money to purchase the given item
    const EINSUFFICIENT_FUNDS: u64 = 3;
    /// Trying to remove a sword, but the hero does not have one
    const ENO_SWORD: u64 = 4;
    /// Assertion errors for testing
    const ASSERT_ERR: u64 = 5;

    // --- Initialization

    #[allow(unused_function)]
    /// On module publish, sender creates a new game. But once it is published,
    /// anyone create a new game with a `new_game` function.
    fun init(ctx: &mut TxContext) {
        create(ctx);
    }

    /// Anyone can create run their own game, all game objects will be
    /// linked to this game.
    public entry fun new_game(ctx: &mut TxContext) {
        create(ctx);
    }

    /// Create a new game. Separated to bypass public entry vs init requirements.
    fun create(ctx: &mut TxContext) {
        let sender = ctx.sender();
        let id = object::new(ctx);
        let game_id = id.to_inner();

        transfer::freeze_object(GameInfo {
            id,
            admin: sender,
        });

        transfer::transfer(
            GameAdmin {
                game_id,
                id: object::new(ctx),
                boars_created: 0,
                potions_created: 0,
            },
            sender
        )
    }

    // --- Gameplay ---

    /// Slay the `boar` with the `hero`'s sword, get experience.
    /// Aborts if the hero has 0 HP or is not strong enough to slay the boar
    public entry fun slay(
        game: &GameInfo, hero: &mut Hero, boar: Boar, ctx: &TxContext
    ) {
        game.check_id(hero.game_id);
        game.check_id(boar.game_id);
        let Boar { id: boar_id, strength: boar_strength, hp, game_id: _ } = boar;
        let hero_strength = hero_strength(hero);
        let mut boar_hp = hp;
        let mut hero_hp = hero.hp;
        // attack the boar with the sword until its HP goes to zero
        while (boar_hp > hero_strength) {
            // first, the hero attacks
            boar_hp = boar_hp - hero_strength;
            // then, the boar gets a turn to attack. if the boar would kill
            // the hero, abort--we can't let the boar win!
            assert!(hero_hp >= boar_strength , EBOAR_WON);
            hero_hp = hero_hp - boar_strength;

        };
        // hero takes their licks
        hero.hp = hero_hp;
        // hero gains experience proportional to the boar, sword grows in
        // strength by one (if hero is using a sword)
        hero.experience = hero.experience + hp;
        if (hero.sword.is_some()) {
            hero.sword.borrow_mut().level_up(1)
        };
        // let the world know about the hero's triumph by emitting an event!
        event::emit(BoarSlainEvent {
            slayer_address: ctx.sender(),
            hero: hero.id.to_inner(),
            boar: boar_id.to_inner(),
            game_id: id(game)
        });
        object::delete(boar_id);
    }

    public use fun hero_strength as Hero.strength;

    /// Strength of the hero when attacking
    public fun hero_strength(hero: &Hero): u64 {
        // a hero with zero HP is too tired to fight
        if (hero.hp == 0) {
            return 0
        };

        let sword_strength = if (hero.sword.is_some()) {
            hero.sword.borrow().strength()
        } else {
            // hero can fight without a sword, but will not be very strong
            0
        };
        // hero is weaker if he has lower HP
        (hero.experience * hero.hp) + sword_strength
    }

    use fun level_up_sword as Sword.level_up;

    fun level_up_sword(sword: &mut Sword, amount: u64) {
        sword.strength = sword.strength + amount
    }

    public use fun sword_strength as Sword.strength;

    /// Strength of a sword when attacking
    public fun sword_strength(sword: &Sword): u64 {
        sword.magic + sword.strength
    }

    // --- Inventory ---

    /// Heal the weary hero with a potion
    public fun heal(hero: &mut Hero, potion: Potion) {
        assert!(hero.game_id == potion.game_id, 403);
        let Potion { id, potency, game_id: _ } = potion;
        object::delete(id);
        let new_hp = hero.hp + potency;
        // cap hero's HP at MAX_HP to avoid int overflows
        hero.hp = new_hp.min(MAX_HP)
    }

    /// Add `new_sword` to the hero's inventory and return the old sword
    /// (if any)
    public fun equip_sword(hero: &mut Hero, new_sword: Sword): Option<Sword> {
        hero.sword.swap_or_fill(new_sword)
    }

    /// Disarm the hero by returning their sword.
    /// Aborts if the hero does not have a sword.
    public fun remove_sword(hero: &mut Hero): Sword {
        assert!(hero.sword.is_some(), ENO_SWORD);
        hero.sword.extract()
    }

    // --- Object creation ---

    /// It all starts with the sword. Anyone can buy a sword, and proceeds go
    /// to the admin. Amount of magic in the sword depends on how much you pay
    /// for it.
    public fun create_sword(
        game: &GameInfo,
        payment: Coin<SUI>,
        ctx: &mut TxContext
    ): Sword {
        let value = payment.value();
        // ensure the user pays enough for the sword
        assert!(value >= MIN_SWORD_COST, EINSUFFICIENT_FUNDS);
        // pay the admin for this sword
        transfer::public_transfer(payment, game.admin);

        // magic of the sword is proportional to the amount you paid, up to
        // a max. one can only imbue a sword with so much magic
        let magic = (value - MIN_SWORD_COST) / MIN_SWORD_COST;
        Sword {
            id: object::new(ctx),
            magic: magic.min(MAX_MAGIC),
            strength: 1,
            game_id: id(game)
        }
    }

    public entry fun acquire_hero(
        game: &GameInfo, payment: Coin<SUI>, ctx: &mut TxContext
    ) {
        let sword = game.create_sword(payment, ctx);
        let hero = game.create_hero(sword, ctx);
        transfer::public_transfer(hero, ctx.sender())
    }

    /// Anyone can create a hero if they have a sword. All heroes start with the
    /// same attributes.
    public fun create_hero(
        game: &GameInfo, sword: Sword, ctx: &mut TxContext
    ): Hero {
        game.check_id(sword.game_id);
        Hero {
            id: object::new(ctx),
            hp: 100,
            experience: 0,
            sword: option::some(sword),
            game_id: id(game)
        }
    }

    /// Admin can create a potion with the given `potency` for `recipient`
    public entry fun send_potion(
        game: &GameInfo,
        potency: u64,
        player: address,
        admin: &mut GameAdmin,
        ctx: &mut TxContext
    ) {
        game.check_id(admin.game_id);
        admin.potions_created = admin.potions_created + 1;
        // send potion to the designated player
        transfer::public_transfer(
            Potion { id: object::new(ctx), potency, game_id: id(game) },
            player
        )
    }

    /// Admin can create a boar with the given attributes for `recipient`
    public entry fun send_boar(
        game: &GameInfo,
        admin: &mut GameAdmin,
        hp: u64,
        strength: u64,
        player: address,
        ctx: &mut TxContext
    ) {
        game.check_id(admin.game_id);
        admin.boars_created = admin.boars_created + 1;
        // send boars to the designated player
        transfer::transfer(
            Boar { id: object::new(ctx), hp, strength, game_id: id(game) },
            player
        )
    }

    // --- Game integrity / Links checks ---

    public fun check_id(game_info: &GameInfo, id: ID) {
        assert!(game_info.id() == id, 403); // TODO: error code
    }

    public fun id(game_info: &GameInfo): ID {
        object::id(game_info)
    }

    // --- Testing functions ---
    public fun assert_hero_strength(hero: &Hero, strength: u64) {
        assert!(hero.strength() == strength, ASSERT_ERR);
    }

    #[test_only]
    public fun delete_hero_for_testing(hero: Hero) {
        let Hero { id, hp: _, experience: _, sword, game_id: _ } = hero;
        object::delete(id);
        let sword = sword.destroy_some();
        let Sword { id, magic: _, strength: _, game_id: _ } = sword;
        object::delete(id)
    }

    #[test_only]
    public fun delete_game_admin_for_testing(admin: GameAdmin) {
        let GameAdmin { id, boars_created: _, potions_created: _, game_id: _ } = admin;
        object::delete(id);
    }

    #[test]
    fun slay_boar_test() {
        use sui::test_scenario;
        use sui::coin;

        let admin = @0xAD014;
        let player = @0x0;

        let mut scenario_val = test_scenario::begin(admin);
        let scenario = &mut scenario_val;
        // Run the module initializers
        scenario.next_tx(admin);
        {
            init(scenario.ctx());
        };
        // Player purchases a hero with the coins
        scenario.next_tx(player);
        {
            let game = scenario.take_immutable<GameInfo>();
            let game_ref = &game;
            let coin = coin::mint_for_testing(500, scenario.ctx());
            acquire_hero(game_ref, coin, scenario.ctx());
            test_scenario::return_immutable(game);
        };
        // Admin sends a boar to the Player
        scenario.next_tx(admin);
        {
            let game = scenario.take_immutable<GameInfo>();
            let game_ref = &game;
            let mut admin_cap = scenario.take_from_sender<GameAdmin>();
            send_boar(game_ref, &mut admin_cap, 10, 10, player, scenario.ctx());
            scenario.return_to_sender(admin_cap);
            test_scenario::return_immutable(game);
        };
        // Player slays the boar!
        scenario.next_tx(player);
        {
            let game = scenario.take_immutable<GameInfo>();
            let game_ref = &game;
            let mut hero = scenario.take_from_sender<Hero>();
            let boar = scenario.take_from_sender<Boar>();
            slay(game_ref, &mut hero, boar, scenario.ctx());
            scenario.return_to_sender(hero);
            test_scenario::return_immutable(game);
        };
        scenario_val.end();
    }
}
