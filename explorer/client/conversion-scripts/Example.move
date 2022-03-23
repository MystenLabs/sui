/// Example of a game character with basic attributes, inventory, and
/// associated logic.
module Examples::Hero {
    use Examples::TrustedCoin::EXAMPLE;
    use FastX::Address::{Self, Address};
    use FastX::Coin::{Self, Coin};
    use FastX::Event;
    use FastX::ID::{Self, ID, IDBytes};
    use FastX::Math;
    use FastX::Transfer;
    use FastX::TxContext::{Self, TxContext};
    use Std::Option::{Self, Option};

    /// Our hero!
    struct Hero has key, store {
        id: ID,
        /// Hit points. If they go to zero, the hero can't do anything
        hp: u64,
        /// Experience of the hero. Begins at zero
        experience: u64,
        /// The hero's minimal inventory
        sword: Option<Sword>,
    }

    /// The hero's trusty sword
    struct Sword has key, store {
        id: ID,
        /// Constant set at creation. Acts as a multiplier on sword's strength.
        /// Swords with high magic are rarer (because they cost more).
        magic: u64,
        /// Sword grows in strength as we use it
        strength: u64,
    }

    /// For healing wounded heroes
    struct Potion has key, store {
        id: ID,
        /// Effectivenss of the potion
        potency: u64
    }

    /// A creature that the hero can slay to level up
    struct Boar has key {
        id: ID,
        /// Hit points before the boar is slain
        hp: u64,
        /// Strength of this particular boar
        strength: u64
    }

    /// Capability conveying the authority to create boars and potions
    struct GameAdmin has key {
        id: ID,
        /// Total number of boars the admin has created
        boars_created: u64,
        /// Total number of potions the admin has created
        potions_created: u64
    }

    /// Event emitted each time a Hero slays a Boar
    struct BoarSlainEvent has copy, drop {
        /// Address of the user that slayed the boar
        slayer_address: Address,
        /// ID of the Hero that slayed the boar
        hero: IDBytes,
        /// ID of the now-deceased boar
        boar: IDBytes,
    }

    /// Address of the admin account that receives payment for swords
    const ADMIN: vector<u8> = vector[189, 215, 127, 86, 129, 189, 1, 4, 90, 106, 17, 10, 123, 200, 40, 18, 34, 173, 240, 91, 213, 72, 183, 249, 213, 210, 39, 181, 105, 254, 59, 163];
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
        assert!(&TxContext::get_signer_address(ctx) == &admin, ENOT_ADMIN);
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
            slayer_address: TxContext::get_signer_address(ctx),
            hero: *ID::get_inner(&hero.id),
            boar: *ID::get_inner(&boar_id),
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
        payment: Coin<EXAMPLE>,
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

    public fun acquire_hero(payment: Coin<EXAMPLE>, ctx: &mut TxContext) {
        let sword = create_sword(payment, ctx);
        let hero = create_hero(sword, ctx);
        Transfer::transfer(hero, TxContext::get_signer_address(ctx))
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
        player: Address,
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
        player: vector<u8>,
        ctx: &mut TxContext
    ) {
        admin.boars_created = admin.boars_created + 1;
        // send boars to the designated player
        Transfer::transfer(
            Boar { id: TxContext::new_id(ctx), hp, strength },
            Address::new(player)
        )
    }

    fun admin(): Address {
        Address::new(ADMIN)
    }

    // --- Testing functions ---
    fun assert_hero_strength(hero: &Hero, strength: u64, _: &mut TxContext) {
        assert!(hero_strength(hero) == strength, ASSERT_ERR);
    }

}
