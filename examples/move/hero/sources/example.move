// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Example of a game character with basic attributes, inventory, and
/// associated logic.
module hero::example;

use sui::balance::{Self, Balance};
use sui::coin::{Self, Coin};
use sui::event;
use sui::sui::SUI;

/// Our hero!
public struct Hero has key, store {
    id: UID,
    /// Game this hero is playing in.
    game_id: ID,
    /// Hit points. If they go to zero, the hero can't do anything
    health: u64,
    /// Experience of the hero. Begins at zero
    experience: u64,
    /// The hero's minimal inventory
    sword: Option<Sword>,
}

/// The hero's trusty sword
public struct Sword has key, store {
    id: UID,
    /// Game this sword is from.
    game_id: ID,
    /// Constant set at creation. Acts as a multiplier on sword's strength.
    /// Swords with high magic are rarer (because they cost more).
    magic: u64,
    /// Sword grows in strength as we use it
    strength: u64,
}

/// For healing wounded heroes
public struct Potion has key, store {
    id: UID,
    /// Game this potion is from.
    game_id: ID,
    /// Effectiveness of the potion
    potency: u64,
}

/// A creature that the hero can slay to level up
public struct Boar has key, store {
    id: UID,
    /// Game this boar is from.
    game_id: ID,
    /// Hit points before the boar is slain
    health: u64,
    /// Strength of this particular boar
    strength: u64,
}

/// Contains information about the game managed by a given `admin`.  Holds
/// payments for player actions for the admin to collect.
public struct Game has key {
    id: UID,
    payments: Balance<SUI>,
}

/// Capability conveying the authority to create boars and potions, and take
/// payments.
public struct Admin has key, store {
    id: UID,
    /// ID of the game this admin manages
    game_id: ID,
    /// Total number of boars the admin has created
    boars_created: u64,
    /// Total number of potions the admin has created
    potions_created: u64,
}

/// Event emitted each time a Hero slays a Boar
public struct BoarSlainEvent has copy, drop {
    /// Address of the user that slayed the boar
    slayer_address: address,
    /// ID of the now-deceased boar
    boar: ID,
    /// ID of the Hero that slayed the boar
    hero: ID,
    /// ID of the game where event happened
    game_id: ID,
}

// === Constants ===

/// Upper bound on player's HP
const MAX_HP: u64 = 1000;

/// Upper bound on how magical a sword can be
const MAX_MAGIC: u64 = 10;

/// Minimum amount you can pay for a sword
const MIN_SWORD_COST: u64 = 100;

// === Error Codes ===

/// Objects are from differing game instances.
const EWrongGame: u64 = 0;

/// The boar won the battle
const EBoarWon: u64 = 1;

/// The hero is too tired to fight
const EHeroTired: u64 = 2;

/// Trying to initialize from a non-admin account
const ENotAdmin: u64 = 3;

/// Not enough money to purchase the given item
const EInsufficientFunds: u64 = 5;

/// Trying to equip a sword but the hero already has one
const EAlreadyEquipped: u64 = 6;

/// Trying to remove a sword, but the hero does not have one
const ENotEquipped: u64 = 7;

// === Player Object creation ===

/// It all starts with the sword. Anyone can buy a sword, and proceeds are
/// stored in the `Game`. Amount of magic in the sword depends on how much
/// you pay for it.
public fun new_sword(game: &mut Game, payment: Coin<SUI>, ctx: &mut TxContext): Sword {
    let value = payment.value();
    // ensure the user pays enough for the sword
    assert!(value >= MIN_SWORD_COST, EInsufficientFunds);

    // pay the game for this sword
    coin::put(&mut game.payments, payment);

    // magic of the sword is proportional to the amount you paid, up to
    // a max. one can only imbue a sword with so much magic
    let magic = (value - MIN_SWORD_COST) / MIN_SWORD_COST;
    Sword {
        id: object::new(ctx),
        magic: magic.min(MAX_MAGIC),
        strength: 1,
        game_id: object::id(game),
    }
}

/// Anyone can create a hero if they have a sword. All heroes start with the
/// same attributes.
public fun new_hero(sword: Sword, ctx: &mut TxContext): Hero {
    Hero {
        id: object::new(ctx),
        game_id: sword.game_id,
        health: 100,
        experience: 0,
        sword: option::some(sword),
    }
}

// === Admin Object creation ===

/// Create a new `Game` (shared) and an `Admin` (returned) to run it. Anyone
/// can run a game, all objects spawned by the game will be associated with
/// it.
public fun new_game(ctx: &mut TxContext): Admin {
    let game = Game {
        id: object::new(ctx),
        payments: balance::zero(),
    };

    let admin = Admin {
        id: object::new(ctx),
        game_id: object::id(&game),
        boars_created: 0,
        potions_created: 0,
    };

    transfer::share_object(game);
    admin
}

/// Admin can create a potion with the given `potency` for `recipient`
public fun new_potion(admin: &mut Admin, potency: u64, ctx: &mut TxContext): Potion {
    admin.potions_created = admin.potions_created + 1;
    Potion { id: object::new(ctx), potency, game_id: admin.game_id }
}

/// Admin can create a boar with the given attributes
public fun new_boar(admin: &mut Admin, health: u64, strength: u64, ctx: &mut TxContext): Boar {
    admin.boars_created = admin.boars_created + 1;
    Boar { id: object::new(ctx), health, strength, game_id: admin.game_id }
}

// === Gameplay ===

/// Slay the `boar` with the `hero`'s sword, get experience.
/// Aborts if the hero has 0 HP or is not strong enough to slay the boar
public fun slay(hero: &mut Hero, boar: Boar, ctx: &TxContext) {
    assert!(hero.game_id == boar.game_id, EWrongGame);

    let Boar {
        id: boar_id,
        strength: boar_strength,
        health: mut boar_health,
        game_id: _,
    } = boar;

    // Hero gains experience proportional to the boar's health.
    let experience = boar_health;

    // Attack the boar with the sword until its HP goes to zero.
    loop {
        let hero_strength = hero.hero_strength();

        // First, the hero attacks.
        if (boar_health < hero_strength) {
            break
        } else {
            boar_health = boar_health - hero_strength;
        };

        // Then, the boar gets a turn to attack.  If the boar would kill the
        // hero, abort -- we can't let the boar win!
        assert!(hero.health >= boar_strength, EBoarWon);
        hero.health = hero.health - boar_strength;
    };

    // Boar slain, level up the hero, and their sword if they have one.
    hero.experience = hero.experience + experience;
    if (hero.sword.is_some()) {
        hero.sword.borrow_mut().level_up_sword(1)
    };

    // Let the world know about the hero's triumph by emitting an event!
    event::emit(BoarSlainEvent {
        slayer_address: ctx.sender(),
        hero: object::id(hero),
        boar: boar_id.uid_to_inner(),
        game_id: hero.game_id,
    });
    boar_id.delete();
}

/// Strength of the hero when attacking -- aborts if the hero cannot fight.
public fun hero_strength(hero: &Hero): u64 {
    // A hero with zero HP is too tired to fight.
    assert!(hero.health > 0, EHeroTired);

    // Hero can fight without a sword, but will not be very strong.
    let sword_strength = if (hero.sword.is_some()) {
        hero.sword.borrow().sword_strength()
    } else {
        0
    };

    // Hero is weaker if they have lower HP.
    (hero.experience * hero.health) + sword_strength
}

fun level_up_sword(sword: &mut Sword, amount: u64) {
    sword.strength = sword.strength + amount
}

/// Strength of a sword when attacking.
public fun sword_strength(sword: &Sword): u64 {
    sword.magic + sword.strength
}

// === Inventory ===

/// Heal the weary hero with a potion.
public fun heal(hero: &mut Hero, potion: Potion) {
    let Potion { id, potency, game_id } = potion;
    id.delete();

    assert!(hero.game_id == game_id, EWrongGame);

    // cap hero's HP at MAX_HP to avoid int overflows
    hero.health = (hero.health + potency).min(MAX_HP)
}

/// Add `new_sword` to the hero's inventory and return the old sword
/// (if any)
public fun equip(hero: &mut Hero, sword: Sword) {
    assert!(hero.sword.is_none(), EAlreadyEquipped);
    hero.sword.fill(sword);
}

/// Disarm the hero by returning their sword.
/// Aborts if the hero does not have a sword.
public fun unequip(hero: &mut Hero): Sword {
    assert!(hero.sword.is_some(), ENotEquipped);
    option::extract(&mut hero.sword)
}

// === Payments ===

/// The owner of the `Admin` object can extract payment from the `Game`.
public fun take_payment(admin: &Admin, game: &mut Game, ctx: &mut TxContext): Coin<SUI> {
    assert!(admin.game_id == object::id(game), ENotAdmin);
    coin::from_balance(game.payments.withdraw_all(), ctx)
}

// === Tests ===
#[test_only]
use sui::test_scenario as ts;

#[test]
fun slay_boar_test() {
    let mut ts = ts::begin(@0x0);
    let admin = @0xAD;
    let alice = @0xA;
    let bob = @0xb;

    // Create the game and admin.
    setup_game(admin, &mut ts);

    // Player purchases a hero with the coins
    acquire_hero(alice, MIN_SWORD_COST, &mut ts);

    // Admin sends a boar to the player
    {
        ts.next_tx(admin);
        let game: Game = ts.take_shared();
        let mut cap: Admin = ts.take_from_sender();
        let boar: Boar = cap.new_boar(9, 9, ts.ctx());
        transfer::public_transfer(boar, alice);
        ts.return_to_sender(cap);
        ts::return_shared(game);
    };

    // Player slays the boar!
    {
        ts.next_tx(alice);
        let mut hero: Hero = ts.take_from_sender();
        let boar: Boar = ts.take_from_sender();
        hero.slay(boar, ts.ctx());
        ts.return_to_sender(hero);
    };

    // Admin sends a potion to the player
    {
        ts.next_tx(admin);
        let mut cap: Admin = ts.take_from_sender();
        let potion: Potion = cap.new_potion(1, ts.ctx());
        transfer::public_transfer(potion, alice);
        ts.return_to_sender(cap);
    };

    // Player restores some health with the potion
    {
        ts.next_tx(alice);
        let mut hero: Hero = ts.take_from_sender();
        let potion: Potion = ts.take_from_sender();

        let potency = potion.potency;
        let before = hero.health;
        hero.heal(potion);
        assert!(hero.health == before + potency, 0);

        ts.return_to_sender(hero);
    };

    // Another player acquires a hero
    acquire_hero(bob, MIN_SWORD_COST + 42, &mut ts);

    // Admin takes payment for the swords sold so far
    {
        ts.next_tx(admin);
        let mut game: Game = ts.take_shared();
        let cap: Admin = ts.take_from_sender();

        let payment = cap.take_payment(&mut game, ts.ctx());
        assert!(payment.value() == MIN_SWORD_COST * 2 + 42, 0);

        transfer::public_transfer(payment, admin);
        ts.return_to_sender(cap);
        ts::return_shared(game);
    };

    ts.end();
}

#[test]
#[expected_failure(abort_code = EWrongGame)]
fun test_wrong_game() {
    let mut ts = ts::begin(@0x0);
    let admin0 = @0xAD0;
    let admin1 = @0xAD1;
    let player = @0xA;

    // Create the first game
    setup_game(admin0, &mut ts);

    // Remember its ID
    let g0 = {
        ts.next_tx(@0x0);
        ts::most_recent_id_shared<Game>().destroy_some()
    };

    // Create the second game
    setup_game(admin1, &mut ts);

    // Create a hero in the first game
    {
        ts.next_tx(player);
        let mut game: Game = ts.take_shared_by_id(g0);
        let coin = coin::mint_for_testing(MIN_SWORD_COST, ts.ctx());
        let sword = game.new_sword(coin, ts.ctx());
        let hero = sword.new_hero(ts.ctx());
        transfer::public_transfer(hero, player);
        ts::return_shared(game);
    };

    // ...get a potion from the second game
    {
        ts.next_tx(admin1);
        let mut cap: Admin = ts.take_from_sender();
        let potion: Potion = cap.new_potion(1, ts.ctx());
        transfer::public_transfer(potion, player);
        ts.return_to_sender(cap);
    };

    // Try to use the potion with the hero.
    {
        ts.next_tx(player);
        let mut hero: Hero = ts.take_from_sender();
        let potion: Potion = ts.take_from_sender();
        hero.heal(potion);
    };

    abort 1337
}

#[test]
#[expected_failure(abort_code = EHeroTired)]
fun test_hero_tired() {
    let mut ts = ts::begin(@0x0);
    let admin = @0xAD;
    let player = @0xA;

    // Create the game and admin.
    setup_game(admin, &mut ts);

    // Player purchases a hero with the coins
    acquire_hero(player, MIN_SWORD_COST, &mut ts);

    // Admin sends a boar to the player
    {
        ts.next_tx(admin);
        let game: Game = ts.take_shared();
        let mut cap: Admin = ts.take_from_sender();
        let boar: Boar = cap.new_boar(9, 9, ts.ctx());
        transfer::public_transfer(boar, player);
        ts.return_to_sender(cap);
        ts::return_shared(game);
    };

    // Hero is somehow tired (modified for test set-up)
    {
        ts.next_tx(player);
        let mut hero: Hero = ts.take_from_sender();
        let boar: Boar = ts.take_from_sender();
        hero.health = 0;
        hero.slay(boar, ts.ctx());
    };

    abort 1337
}

#[test]
#[expected_failure(abort_code = EBoarWon)]
fun test_boar_win() {
    let mut ts = ts::begin(@0x0);
    let admin = @0xAD;
    let player = @0xA;

    // Create the game and admin.
    setup_game(admin, &mut ts);

    // Player purchases a hero with the coins
    acquire_hero(player, MIN_SWORD_COST, &mut ts);

    // Admin sends a boar to the player
    {
        ts.next_tx(admin);
        let game: Game = ts.take_shared();
        let mut cap: Admin = ts.take_from_sender();
        let boar: Boar = cap.new_boar(9, 9, ts.ctx());
        transfer::public_transfer(boar, player);
        ts.return_to_sender(cap);
        ts::return_shared(game);
    };

    // Hero is very weak (modified for test set-up)
    {
        ts.next_tx(player);
        let mut hero: Hero = ts.take_from_sender();
        let boar: Boar = ts.take_from_sender();
        hero.health = 1;
        hero.slay(boar, ts.ctx());
    };

    abort 1337
}

#[test]
#[expected_failure(abort_code = EInsufficientFunds)]
fun test_insufficient_funds() {
    let mut ts = ts::begin(@0x0);
    let admin = @0xAD;
    let player = @0xA;

    // Create the game and admin.
    setup_game(admin, &mut ts);

    // Player offers too small a coin for the sword
    acquire_hero(player, MIN_SWORD_COST - 1, &mut ts);

    abort 1337
}

#[test]
#[expected_failure(abort_code = ENotEquipped)]
fun test_unequip_empty() {
    let admin = @0xAD;
    let player = @0xA;

    let mut ts = ts::begin(admin);
    let _admin = new_game(ts.ctx());

    ts.next_tx(player);
    let mut game: Game = ts.take_shared();
    let coin = coin::mint_for_testing(MIN_SWORD_COST, ts.ctx());

    let sword = game.new_sword(coin, ts.ctx());
    let mut hero = sword.new_hero(ts.ctx());

    let _s0 = hero.unequip();
    let _s1 = hero.unequip(); // Doesn't exist

    abort 1337
}

#[test]
#[expected_failure(abort_code = EAlreadyEquipped)]
fun test_equip_already_equipped() {
    let admin = @0xAD;
    let player = @0xA;

    let mut ts = ts::begin(admin);
    let _admin = new_game(ts.ctx());

    ts.next_tx(player);
    let mut game: Game = ts.take_shared();
    let c0 = coin::mint_for_testing(MIN_SWORD_COST, ts.ctx());
    let s0 = game.new_sword(c0, ts.ctx());

    let c1 = coin::mint_for_testing(MIN_SWORD_COST, ts.ctx());
    let s1 = game.new_sword(c1, ts.ctx());

    let mut hero = s0.new_hero(ts.ctx());
    hero.equip(s1);

    abort 1337
}

#[test_only]
fun setup_game(admin: address, ts: &mut ts::Scenario) {
    ts.next_tx(admin);
    let cap = new_game(ts.ctx());
    transfer::public_transfer(cap, admin);
}

#[test_only]
fun acquire_hero(player: address, payment: u64, ts: &mut ts::Scenario) {
    ts.next_tx(player);
    let mut game: Game = ts.take_shared();
    let coin = coin::mint_for_testing(payment, ts.ctx());
    let sword = game.new_sword(coin, ts.ctx());
    let hero = sword.new_hero(ts.ctx());
    transfer::public_transfer(hero, player);
    ts::return_shared(game);
}
