/// Example of a game character with basic attributes, inventory, and
/// associated logic.
module Games::Hero {
    use Sui::Coin::{Self, Coin};
    use Sui::Event;
    use Sui::ID::{Self, ID, VersionedID};
    use Sui::Math;
    use Sui::GAS::GAS;
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};
    use Std::Option::{Self, Option};

    /// Our hero!
    struct Hero has key, store {
        id: VersionedID,
        /// Hit points. If they go to zero, the hero can't do anything
        hp: u64,
        /// Experience of the hero. Begins at zero
        experience: u64,
        /// The hero's minimal inventory
        sword: Option<Sword>,
    }

    /// The hero's trusty sword
    struct Sword has key, store {
        id: VersionedID,
        /// Constant set at creation. Acts as a multiplier on sword's strength.
        /// Swords with high magic are rarer (because they cost more).
        magic: u64,
        /// Sword grows in strength as we use it
        strength: u64,
    }

    /// For healing wounded heroes
    struct Potion has key, store {
        id: VersionedID,
        /// Effectivenss of the potion
        potency: u64
    }

    /// A creature that the hero can slay to level up
    struct Boar has key {
        id: VersionedID,
        /// Hit points before the boar is slain
        hp: u64,
        /// Strength of this particular boar
        strength: u64
    }

    /// Capability conveying the authority to create boars and potions
    struct GameAdmin has key {
        id: VersionedID,
        /// Total number of boars the admin has created
        boars_created: u64,
        /// Total number of potions the admin has created
        potions_created: u64
    }

    /// Event emitted each time a Hero slays a Boar
    struct BoarSlainEvent has copy, drop {
        /// Address of the user that slayed the boar
        slayer_address: address,
        /// ID of the Hero that slayed the boar
        hero: ID,
        /// ID of the now-deceased boar
        boar: ID,
    }

    /// Address of the admin account that receives payment for swords
    const ADMIN: address = @0xa5e6dbcf33730ace6ec8b400ff4788c1f150ff7e;
    /// Upper bound on player's HP
    const MAX_HP: u64 = 1000;
    /// Upper bound on how magical a sword can be
    const MAX_MAGIC: u64 = 10;
    /// Minimum amount you can pay for a sword
    const MIN_SWORD_COST: u64 = 100;

    // TODO: proper error codes
    /// The boar won the battle
    const EBOAR_WON: u64 = 0;
    /// The hero is too tired to fight
    const EHERO_TIRED: u64 = 1;
    /// Trying to initialize from a non-admin account
    const ENOT_ADMIN: u64 = 2;
    /// Not enough money to purchase the given item
    const EINSUFFICIENT_FUNDS: u64 = 3;
    /// Trying to remove a sword, but the hero does not have one
    const ENO_SWORD: u64 = 4;
    /// Assertion errors for testing
    const ASSERT_ERR: u64 = 5;

    // --- Initialization

    /// Create the `GameAdmin` capability and hand it off to the admin
    /// authenticator
    fun init(ctx: &mut TxContext) {
        let admin = admin();
        // ensure this is being initialized by the expected admin authenticator
        assert!(&TxContext::sender(ctx) == &admin, ENOT_ADMIN);
        Transfer::transfer(
            GameAdmin {
                id: TxContext::new_id(ctx),
                boars_created: 0,
                potions_created: 0
            },
            admin
        )
    }

    // --- Gameplay ---

    /// Slay the `boar` with the `hero`'s sword, get experience.
    /// Aborts if the hero has 0 HP or is not strong enough to slay the boar
    public fun slay(hero: &mut Hero, boar: Boar, ctx: &mut TxContext) {
        let Boar { id: boar_id, strength: boar_strength, hp } = boar;
        let hero_strength = hero_strength(hero);
        let boar_hp = hp;
        let hero_hp = hero.hp;
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
        if (Option::is_some(&hero.sword)) {
            level_up_sword(Option::borrow_mut(&mut hero.sword), 1)
        };
        // let the world know about the hero's triumph by emitting an event!
        Event::emit(BoarSlainEvent {
            slayer_address: TxContext::sender(ctx),
            hero: *ID::inner(&hero.id),
            boar: *ID::inner(&boar_id),
        });
        ID::delete(boar_id);

    }

    /// Strength of the hero when attacking
    public fun hero_strength(hero: &Hero): u64 {
        // a hero with zero HP is too tired to fight
        if (hero.hp == 0) {
            return 0
        };

        let sword_strength = if (Option::is_some(&hero.sword)) {
            sword_strength(Option::borrow(&hero.sword))
        } else {
            // hero can fight without a sword, but will not be very strong
            0
        };
        // hero is weaker if he has lower HP
        (hero.experience * hero.hp) + sword_strength
    }

    fun level_up_sword(sword: &mut Sword, amount: u64) {
        sword.strength = sword.strength + amount
    }

    /// Strength of a sword when attacking
    public fun sword_strength(sword: &Sword): u64 {
        sword.magic + sword.strength
    }

    // --- Inventory ---

    /// Heal the weary hero with a potion
    public fun heal(hero: &mut Hero, potion: Potion) {
        let Potion { id, potency } = potion;
        ID::delete(id);
        let new_hp = hero.hp + potency;
        // cap hero's HP at MAX_HP to avoid int overflows
        hero.hp = Math::min(new_hp, MAX_HP)
    }

    /// Add `new_sword` to the hero's inventory and return the old sword
    /// (if any)
    public fun equip_sword(hero: &mut Hero, new_sword: Sword): Option<Sword> {
        Option::swap_or_fill(&mut hero.sword, new_sword)
    }

    /// Disarm the hero by returning their sword.
    /// Aborts if the hero does not have a sword.
    public fun remove_sword(hero: &mut Hero): Sword {
        assert!(Option::is_some(&hero.sword), ENO_SWORD);
        Option::extract(&mut hero.sword)
    }

    // --- Object creation ---

    /// It all starts with the sword. Anyone can buy a sword, and proceeds go
    /// to the admin. Amount of magic in the sword depends on how much you pay
    /// for it.
    public fun create_sword(
        payment: Coin<GAS>,
        ctx: &mut TxContext
    ): Sword {
        let value = Coin::value(&payment);
        // ensure the user pays enough for the sword
        assert!(value >= MIN_SWORD_COST, EINSUFFICIENT_FUNDS);
        // pay the admin for ths sword
        Transfer::transfer(payment, admin());

        // magic of the sword is proportional to the amount you paid, up to
        // a max. one can only imbue a sword with so much magic
        let magic = (value - MIN_SWORD_COST) / MIN_SWORD_COST;
        Sword {
            id: TxContext::new_id(ctx),
            magic: Math::min(magic, MAX_MAGIC),
            strength: 1
        }
    }

    public fun acquire_hero(payment: Coin<GAS>, ctx: &mut TxContext) {
        let sword = create_sword(payment, ctx);
        let hero = create_hero(sword, ctx);
        Transfer::transfer(hero, TxContext::sender(ctx))
    }

    /// Anyone can create a hero if they have a sword. All heros start with the
    /// same attributes.
    public fun create_hero(sword: Sword, ctx: &mut TxContext): Hero {
        Hero {
            id: TxContext::new_id(ctx),
            hp: 100,
            experience: 0,
            sword: Option::some(sword),
        }
    }

    /// Admin can create a potion with the given `potency` for `recipient`
    public fun send_potion(
        potency: u64,
        player: address,
        admin: &mut GameAdmin,
        ctx: &mut TxContext
    ) {
        admin.potions_created = admin.potions_created + 1;
        // send potion to the designated player
        Transfer::transfer(
            Potion { id: TxContext::new_id(ctx), potency },
            player
        )
    }

    /// Admin can create a boar with the given attributes for `recipient`
    public fun send_boar(
        admin: &mut GameAdmin,
        hp: u64,
        strength: u64,
        player: address,
        ctx: &mut TxContext
    ) {
        admin.boars_created = admin.boars_created + 1;
        // send boars to the designated player
        Transfer::transfer(
            Boar { id: TxContext::new_id(ctx), hp, strength },
            player
        )
    }

    fun admin(): address {
        ADMIN
    }

    // --- Testing functions ---
    public fun assert_hero_strength(hero: &Hero, strength: u64, _: &mut TxContext) {
        assert!(hero_strength(hero) == strength, ASSERT_ERR);
    }

    #[test_only]
    public fun delete_hero_for_testing(hero: Hero) {
        let Hero { id, hp: _, experience: _, sword } = hero;
        ID::delete(id);
        let sword = Option::destroy_some(sword);
        let Sword { id, magic: _, strength: _ } = sword;
        ID::delete(id)
    }

    #[test_only]
    public fun delete_game_admin_for_testing(admin: GameAdmin) {
        let GameAdmin { id, boars_created: _, potions_created: _ } = admin;
        ID::delete(id);
    }

    #[test]
    public fun slay_boar_test() {
        use Sui::Coin;
        use Sui::TestScenario;

        let admin = ADMIN;
        let player = @0x0;

        let scenario = &mut TestScenario::begin(&admin);
        // Run the module initializers
        {
            init(TestScenario::ctx(scenario));
        };
        // Player purchases a hero with the coins
        TestScenario::next_tx(scenario, &player);
        {
            let coin = Coin::mint_for_testing(500, TestScenario::ctx(scenario));
            acquire_hero(coin, TestScenario::ctx(scenario));
        };
        // Admin sends a boar to the Player
        TestScenario::next_tx(scenario, &admin);
        {
            let admin_cap = TestScenario::remove_object<GameAdmin>(scenario);
            send_boar(&mut admin_cap, 10, 10, player, TestScenario::ctx(scenario));
            TestScenario::return_object(scenario, admin_cap)
        };
        // Player slays the boar!
        TestScenario::next_tx(scenario, &player);
        {
            let hero = TestScenario::remove_object<Hero>(scenario);
            let boar = TestScenario::remove_object<Boar>(scenario);
            slay(&mut hero, boar, TestScenario::ctx(scenario));
            TestScenario::return_object(scenario, hero)
        };
    }
}
