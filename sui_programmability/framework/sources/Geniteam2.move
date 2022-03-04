module Sui::Geniteam2 {
    use Sui::ID::{Self, ID, VersionedID};
    use Sui::TxContext::{Self, TxContext};
    use Sui::Transfer;
    use Std::ASCII::{Self, String};
    use Std::Option::{Self, Option};
    use Std::Vector::Self;

    /// Trying to add more than `total_monster_slots` monsters to a Farm
    const ETOO_MANY_MONSTERS: u64 = 0;
    /// Can't find a monster with the given ID
    const EMONSTER_NOT_FOUND: u64 = 1;

    struct Player has key {
        id: VersionedID,
        player_name: String,
        farm: Farm,
        water_runes_count: u64,
        fire_runes_count: u64,
        wind_runes_count: u64,
        earth_runes_count: u64,
        owned_farm_cosmetics: vector<FarmCosmetic>,
        owned_monster_cosmetics: vector<MonsterCosmetic>
    }

    struct Farm has key, store {
        id: VersionedID,
        farm_name: String,
        farm_img_id: u64,
        level: u64,
        current_xp: u64,
        total_monster_slots: u64,
        occupied_monster_slots: u64,
        farm_cosmetic_slot1: Option<FarmCosmetic>,
        farm_cosmetic_slot2: Option<FarmCosmetic>,
        pet_monsters: vector<Monster>,
    }

    struct Monster has key, store {
        id: VersionedID,
        monster_name: String,
        monster_img_id: u64,
        breed: u8,
        monster_affinity: u8,
        monster_description: String,
        monster_level: u64,
        monster_xp: u64,
        hunger_level: u64,
        affection_level: u64,
        buddy_level: u8,
        monster_cosmetic_slot1: Option<MonsterCosmetic>,
        monster_cosmetic_slot2: Option<MonsterCosmetic>,
    }

    struct FarmCosmetic has store, drop {
        cosmetic_type: u8,
        cosmetic_id: u64
    }

    struct MonsterCosmetic has store, drop {
        cosmetic_type: u8,
        cosmetic_id: u64
    }

    // === Constructors. These create new Sui objects. ===

    public fun create_player_(
        player_name: vector<u8>, farm: Farm, ctx: &mut TxContext
    ): Player {
        Player {
            id: TxContext::new_id(ctx),
            player_name: ASCII::string(player_name),
            farm,
            water_runes_count: 0,
            fire_runes_count: 0,
            wind_runes_count: 0,
            earth_runes_count: 0,
            owned_farm_cosmetics: Vector::empty(),
            owned_monster_cosmetics: Vector::empty()
        }
    }

    public fun create_farm_(
        farm_name: vector<u8>, farm_img_id: u64, total_monster_slots: u64, ctx: &mut TxContext
    ): Farm {
        Farm {
            id: TxContext::new_id(ctx),
            farm_name: ASCII::string(farm_name),
            farm_img_id,
            level: 0,
            current_xp: 0,
            total_monster_slots,
            occupied_monster_slots: 0,
            farm_cosmetic_slot1: Option::none(),
            farm_cosmetic_slot2: Option::none(),
            pet_monsters: Vector::empty(),
        }
    }

    public fun create_monster_(
        monster_name: vector<u8>,
        monster_img_id: u64,
        breed: u8,
        monster_affinity: u8,
        monster_description: vector<u8>,
        ctx: &mut TxContext
    ): Monster {

        Monster {
            id: TxContext::new_id(ctx),
            monster_name: ASCII::string(monster_name),
            monster_img_id,
            breed,
            monster_affinity,
            monster_description: ASCII::string(monster_description),
            monster_level: 0,
            monster_xp: 0,
            hunger_level: 0,
            affection_level: 0,
            buddy_level: 0,
            monster_cosmetic_slot1: Option::none(),
            monster_cosmetic_slot2: Option::none(),
        }
    }

    // === Mutators. These equip objects with other objects and update the attributes of objects. ===

    /// Remove a monster from a farm.
    /// Aborts if the monster with the given ID is not found
    public fun remove_monster_(self: &mut Player, monster_id: &ID): Monster {
        let farm = &mut self.farm;
        let monsters = &mut farm.pet_monsters;
        let num_monsters = Vector::length(monsters);
        let i = 0;
        while (i < num_monsters) {
            let m = Vector::borrow(monsters, i);
            if (ID::id(m) == monster_id) {
                break
            };
            i = i + 1;
        };
        assert!(i != num_monsters, EMONSTER_NOT_FOUND);
        farm.occupied_monster_slots = farm.occupied_monster_slots - 1;
        Vector::remove(monsters, i)
    }

    // === Entrypoints. Each of these functions can be called from a Sui transaction, whereas functions above cannot. ===

    /// Create a player and transfer it to the transaction sender
    public fun create_player(
        player_name: vector<u8>, farm_name: vector<u8>, farm_img_id: u64, total_monster_slots: u64, ctx: &mut TxContext
    ) {
        let farm = create_farm_(farm_name, farm_img_id, total_monster_slots, ctx);
        let player = create_player_(player_name, farm, ctx);
        Transfer::transfer(player, TxContext::sender(ctx))
    }

    /// Update the attributes of a player
    public fun update_player(
        self: &mut Player,
        water_runes_count: u64,
        fire_runes_count: u64,
        wind_runes_count: u64,
        earth_runes_count: u64,
        _ctx: &mut TxContext
    ) {
        self.water_runes_count = water_runes_count;
        self.fire_runes_count = fire_runes_count;
        self.wind_runes_count = wind_runes_count;
        self.earth_runes_count = earth_runes_count
    }

    /// Create a monster and transfer it to the transaction sender
    public fun create_monster(
        monster_name: vector<u8>,
        monster_img_id: u64,
        breed: u8,
        monster_affinity: u8,
        monster_description: vector<u8>,
        ctx: &mut TxContext
    ) {
        let monster = create_monster_(
            monster_name,
            monster_img_id,
            breed,
            monster_affinity,
            monster_description,
            ctx
        );
        Transfer::transfer(monster, TxContext::sender(ctx))
    }

    /// Add a monster to a farm
    public fun add_monster(self: &mut Player, monster: Monster, _ctx: &mut TxContext) {
        Vector::push_back(&mut self.farm.pet_monsters, monster);
        self.farm.occupied_monster_slots = self.farm.occupied_monster_slots + 1;
        assert!(self.farm.occupied_monster_slots <= self.farm.total_monster_slots, ETOO_MANY_MONSTERS)
    }

    /// Remove a monster from a farm amd transfer it to the transaction sender
    public fun remove_monster(self: &mut Player, monster_id: vector<u8>, ctx: &mut TxContext) {
        // TODO: monster_id should be probably be `address`, but leaving this as-is to avoid breaking Geniteam
        let monster = remove_monster_(self, &ID::new_from_bytes(monster_id));
        Transfer::transfer(monster, TxContext::sender(ctx))
    }

    /// Update the attributes of a player's farm
    public fun update_farm(self: &mut Player, level: u64, _ctx: &mut TxContext) {
        self.farm.level = level;
    }

    /// Add cosmetics to a farm's first slot
    public fun update_farm_cosmetic_slot1(
        self: &mut Player, cosmetic_type: u8, cosmetic_id: u64, _ctx: &mut TxContext
    ) {
        self.farm.farm_cosmetic_slot1 = Option::some(FarmCosmetic { cosmetic_type, cosmetic_id })
    }

     /// Add cosmetics to a farm's second slot
    public fun update_farm_cosmetic_slot2(
        self: &mut Player, cosmetic_type: u8, cosmetic_id: u64, _ctx: &mut TxContext
    ) {
        self.farm.farm_cosmetic_slot2 = Option::some(FarmCosmetic { cosmetic_type, cosmetic_id })
    }

    /// Update the attributes of a monster
    public fun update_monster(
        self: &mut Monster,
        monster_level: u64,
        hunger_level: u64,
        affection_level: u64,
        buddy_level: u8,
        _ctx: &mut TxContext
    ) {
        self.monster_level = monster_level;
        self.hunger_level = hunger_level;
        self.affection_level = affection_level;
        self.buddy_level = buddy_level;
    }

    /// Add cosmetics to a monster's second slot
    public fun update_monster_cosmetic_slot1(
        self: &mut Monster, cosmetic_type: u8, cosmetic_id: u64, _ctx: &mut TxContext
    ) {
        self.monster_cosmetic_slot1 = Option::some(MonsterCosmetic { cosmetic_type, cosmetic_id })
    }

    /// Add cosmetics to a monster's first slot
    public fun update_monster_cosmetic_slot2(
        self: &mut Monster, cosmetic_type: u8, cosmetic_id: u64, _ctx: &mut TxContext
    ) {
        self.monster_cosmetic_slot2 = Option::some(MonsterCosmetic { cosmetic_type, cosmetic_id })
    }
}