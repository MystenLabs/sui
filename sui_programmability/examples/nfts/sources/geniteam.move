// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module nfts::geniteam {
    use nfts::bag::{Self, Bag};
    use nfts::collection::{Self, Collection};
    use sui::object::{Self, UID};
    use sui::typed_id::{Self, TypedID};
    use sui::tx_context::{Self, TxContext};
    use std::option::{Self, Option};
    use sui::transfer;
    use std::ascii::{Self, String};
    use std::vector;

    /// Trying to add more than 1 farm to a Player
    const ETooManyFarms: u64 = 1;

    /// Monster collection not owned by farm
    const EMonsterCollectionNotOwnedByFarm: u64 = 2;

    /// Inventory not owned by player
    const EInventoryNotOwnedByPlayer: u64 = 3;

    /// Invalid cosmetic slot
    const EInvalidCosmeticsSlot: u64 = 4;

    struct Player has key {
        id: UID,
        player_name: String,
        water_runes_count: u64,
        fire_runes_count: u64,
        wind_runes_count: u64,
        earth_runes_count: u64,

        // Owned Farm
        owned_farm: Option<TypedID<Farm>>,

        // Inventory of unassigned cosmetics.
        // A cosmetic can be either a FarmCosmetic or a MonsterCosmetic.
        // Since they can be of different types, we use Bag instead of Collection.
        inventory: TypedID<Bag>,
    }

    struct Farm has key {
        id: UID,
        farm_name: String,
        farm_img_index: u64,
        level: u64,
        current_xp: u64,
        total_monster_slots: u64,
        occupied_monster_slots: u64,

        // Collection of Pet monsters owned by this Farm
        pet_monsters: TypedID<Collection<Monster>>,

        // Applied cosmetic at this slot
        applied_farm_cosmetic_0:  Option<TypedID<FarmCosmetic>>,
        // Applied cosmetic at this slot
        applied_farm_cosmetic_1:  Option<TypedID<FarmCosmetic>>,
    }

    struct Monster has key, store {
        id: UID,
        monster_name: String,
        monster_img_index: u64,
        breed: u8,
        monster_affinity: u8,
        monster_description: String,
        monster_level: u64,
        monster_xp: u64,
        hunger_level: u64,
        affection_level: u64,
        buddy_level: u8,
        display: String,

        // Applied cosmetic at this slot
        applied_monster_cosmetic_0: Option<TypedID<MonsterCosmetic>>,
        // Applied cosmetic at this slot
        applied_monster_cosmetic_1: Option<TypedID<MonsterCosmetic>>,

    }

    struct FarmCosmetic has key, store{
        id: UID,
        cosmetic_type: u8,
        display: String,
    }

    struct MonsterCosmetic has key, store {
        id: UID,
        cosmetic_type: u8,
        display: String,
    }

    // ============================ Entry functions ============================

    /// Create a player and transfer it to the transaction sender
    public entry fun create_player(
        player_name: vector<u8>, ctx: &mut TxContext
    ) {
        // Create player simply and transfer to caller
        let player = new_player(player_name, ctx);
        transfer::transfer(player, tx_context::sender(ctx))
    }

    /// Create a Farm and add it to the Player
    public entry fun create_farm(
        player: &mut Player, farm_img_index: u64, farm_name: vector<u8>,
        total_monster_slots: u64, ctx: &mut TxContext
    ) {
        // We only allow one farm for now
        assert!(option::is_none(&player.owned_farm), ETooManyFarms);

        let farm = new_farm(farm_name, farm_img_index, total_monster_slots, ctx);
        let farm_id = typed_id::new(&farm);

        // Transfer ownership of farm to player
        transfer::transfer_to_object(farm, player);

        // Store the farm
        option::fill(&mut player.owned_farm, farm_id)
    }

    /// Create a Monster and add it to the Farm's collection of Monsters, which
    /// is unbounded
    public entry fun create_monster(_player: &mut Player,
                              farm: &mut Farm,
                              pet_monsters: &mut Collection<Monster>,
                              monster_name: vector<u8>,
                              monster_img_index: u64,
                              breed: u8,
                              monster_affinity: u8,
                              monster_description: vector<u8>,
                              display: vector<u8>,
                              ctx: &mut TxContext
    ) {
        let monster = new_monster(
            monster_name,
            monster_img_index,
            breed,
            monster_affinity,
            monster_description,
            display,
            ctx
        );

        // Check if this is the right collection
        assert!(
            typed_id::equals_object(&farm.pet_monsters, pet_monsters),
            EMonsterCollectionNotOwnedByFarm,
        );

        // TODO: Decouple adding monster to farm from creating a monster.
        // Add it to the collection
        collection::add(pet_monsters, monster, ctx);
    }

    /// Create Farm cosmetic owned by player and add to its inventory
    public fun create_farm_cosmetics(
        player: &mut Player, inventory: &mut Bag, cosmetic_type: u8,
        display: vector<u8>, ctx: &mut TxContext
    ) {
        // Check if this is the right collection
        assert!(
            typed_id::equals_object(&player.inventory, inventory),
            EInventoryNotOwnedByPlayer,
        );

        // Create the farm cosmetic object
        let farm_cosmetic = FarmCosmetic {
            id: object::new(ctx),
            cosmetic_type,
            display: ascii::string(display)
            };

        // Add it to the player's inventory
        bag::add(inventory, farm_cosmetic, ctx);
    }

    /// Create Monster cosmetic owned by player and add to its inventory
    public fun create_monster_cosmetics(
        player: &mut Player, inventory: &mut Bag, cosmetic_type: u8,
        display: vector<u8>, ctx: &mut TxContext
    ) {
        // Check if this is the right collection
        assert!(
            typed_id::equals_object(&player.inventory, inventory),
            EInventoryNotOwnedByPlayer,
        );

        // Create the farm cosmetic object
        let monster_cosmetic = MonsterCosmetic {
            id: object::new(ctx),
            cosmetic_type,
            display: ascii::string(display)
            };

        // Add it to the player's inventory
        bag::add(inventory, monster_cosmetic, ctx);
    }

    /// Update the attributes of a player
    public fun update_player(
        player: &mut Player,
        water_runes_count: u64,
        fire_runes_count: u64,
        wind_runes_count: u64,
        earth_runes_count: u64,
    ) {
        player.water_runes_count = water_runes_count;
        player.fire_runes_count = fire_runes_count;
        player.wind_runes_count = wind_runes_count;
        player.earth_runes_count = earth_runes_count
    }

    /// Update the attributes of a monster
    public fun update_monster_stats(
        _player: &mut Player,
        _farm: &mut Farm,
        _pet_monsters: &mut Collection<Monster>,
        self: &mut Monster,
        monster_affinity: u8,
        monster_level: u64,
        hunger_level: u64,
        affection_level: u64,
        buddy_level: u8,
        display: vector<u8>,
    ) {
        self.monster_affinity = monster_affinity;
        self.monster_level = monster_level;
        self.hunger_level = hunger_level;
        self.affection_level = affection_level;
        self.buddy_level = buddy_level;
        if (vector::length<u8>(&display) != 0) {
            self.display = ascii::string(display);
        }
    }


    /// Update the attributes of the farm
    public fun update_farm_stats(
        _player: &mut Player, farm: &mut Farm, level: u64, current_xp: u64,
    ) {
        farm.current_xp = current_xp;
        farm.level = level;
    }

    /// Apply the cosmetic to the Farm from the inventory
    public fun update_farm_cosmetics(
        _player: &mut Player, farm: &mut Farm, _inventory: &mut Bag,
        farm_cosmetic: FarmCosmetic, cosmetic_slot_id: u64
    ) {
        // Only 2 slots allowed
        assert!(cosmetic_slot_id <= 1 , EInvalidCosmeticsSlot);

        // Transfer ownership of cosmetic to this farm
        let child_ref = typed_id::new(&farm_cosmetic);
        transfer::transfer_to_object(farm_cosmetic, farm);

        // Assign by slot
        if (cosmetic_slot_id == 0) {
            // Store the cosmetic
            option::fill(&mut farm.applied_farm_cosmetic_0, child_ref)
        } else {
            // Store the cosmetic
            option::fill(&mut farm.applied_farm_cosmetic_1, child_ref)
        };
    }

    /// Apply the cosmetics to the Monster from the inventory
    public fun update_monster_cosmetics(
        _player: &mut Player, _farm: &mut Farm, monster: &mut Monster,
        _inventory: &mut Bag, monster_cosmetic: MonsterCosmetic,
        _pet_monsters: &mut Collection<Monster>, cosmetic_slot_id: u64,
    ) {
        // Only 2 slots allowed
        assert!(cosmetic_slot_id <= 1 , EInvalidCosmeticsSlot);

        // Transfer ownership of cosmetic to this monster
        let child_ref = typed_id::new(&monster_cosmetic);
        transfer::transfer_to_object(monster_cosmetic, monster);

        // Assign by slot
        if (cosmetic_slot_id == 0) {
            // Store the cosmetic
            option::fill(&mut monster.applied_monster_cosmetic_0, child_ref)
        } else {
            // Store the cosmetic
            option::fill(&mut monster.applied_monster_cosmetic_1, child_ref)
        };
    }

    // ============== Constructors. These create new Sui objects. ==============

    // Constructs a new basic Player object
    fun new_player(
        player_name: vector<u8>, ctx: &mut TxContext
    ): Player {
        // Create a new id for player.
        let id = object::new(ctx);

        // Create inventory collection.
        let inventory = bag::new(ctx);

        // Transfer ownership of inventory to player.
        let inventory_id = typed_id::new(&inventory);
        bag::transfer_to_object_id(inventory, &mut id);

        let player = Player {
            id,
            player_name: ascii::string(player_name),
            water_runes_count: 0,
            fire_runes_count: 0,
            wind_runes_count: 0,
            earth_runes_count: 0,
            owned_farm: option::none(),
            inventory: inventory_id
        };

        player
    }

    // Constructs a new basic Farm object
    fun new_farm(
        farm_name: vector<u8>, farm_img_index: u64, total_monster_slots: u64,
        ctx: &mut TxContext
    ): Farm {
        // Create a new id for farm.
        let id = object::new(ctx);

        // Create pet monsters collection.
        let pet_monsters = collection::new<Monster>(ctx);

        // Transfer ownership of pet monsters to farm.
        let pet_monsters_id = typed_id::new(&pet_monsters);
        collection::transfer_to_object_id(pet_monsters, &mut id);


        let farm = Farm {
            id,
            farm_name: ascii::string(farm_name),
            total_monster_slots,
            farm_img_index,
            level: 0,
            current_xp: 0,
            occupied_monster_slots: 0,
            pet_monsters: pet_monsters_id,
            applied_farm_cosmetic_0: option::none(),
            applied_farm_cosmetic_1: option::none(),
        };

        farm
    }

    // Constructs a new basic Monster object
    fun new_monster(
        monster_name: vector<u8>,
        monster_img_index: u64,
        breed: u8,
        monster_affinity: u8,
        monster_description: vector<u8>,
        display: vector<u8>,
        ctx: &mut TxContext
    ): Monster {

        Monster {
            id: object::new(ctx),
            monster_name: ascii::string(monster_name),
            monster_img_index,
            breed,
            monster_affinity,
            monster_description: ascii::string(monster_description),
            monster_level: 0,
            monster_xp: 0,
            hunger_level: 0,
            affection_level: 0,
            buddy_level: 0,
            display: ascii::string(display),
            applied_monster_cosmetic_0: option::none(),
            applied_monster_cosmetic_1: option::none(),
        }
    }
}
