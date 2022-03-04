module Sui::Geniteam {
    use Sui::Collection;
    use Sui::ID::{Self, VersionedID, ID};
    use Sui::TxContext::{Self, TxContext};
    use Std::Option::{Self, Option};
    use Sui::Transfer;
    use Std::ASCII::{Self, String};

    /// Trying to add more than `total_monster_slots` monsters to a Farm
    const ETOO_MANY_MONSTERS: u64 = 0;
    /// Can't find a monster with the given VersionedID
    const EMONSTER_NOT_FOUND: u64 = 1;

    /// Trying to add more than 1 farm to a Player
    const ETOO_MANY_FARMS: u64 = 2;

    /// Cosmetic not owned by the Player
    const ECOSMETIC_NOT_OWNED_BY_PLAYER: u64 = 3;

    /// Too many Cosmetics for the slot
    const ETOO_MANY_COSMETICS: u64 = 4;

    /// Monster collection not owned by farm
    const EMONSTER_COLLECTION_NOT_OWNED_BY_FARM: u64 = 5;

    /// Farm cosmetics inventory not owned by player
    const EFARM_COSMETICS_INVENTORY_NOT_OWNED_BY_PLAYER: u64 = 6;

    /// Monster cosmetics inventory not owned by player
    const EMONSTER_COSMETICS_INVENTORY_NOT_OWNED_BY_PLAYER: u64 = 7;

    /// Invalid cosmetic slot
    const EINVALID_COSMETICS_SLOT: u64 = 8;

    struct Player has key {
        id: VersionedID,

        player_name: String,
        
        water_runes_count: u64,
        fire_runes_count: u64,
        wind_runes_count: u64,
        earth_runes_count: u64,
        
        // ID of the Collection of owned farm: Farm
        owned_farms_id: Option<ID>,

        // Inventory of unassigned items: Inventory
        // Collection of owned farm cosmetics: FarmCosmetic
        farm_inventory_cosmetics_id: ID,
        // Collection of owned monster cosmetics: MonsterCosmetic
        monster_inventory_cosmetics_id: ID,
    }
    struct Farm has key, store {
        id: VersionedID,
        farm_name: String,
        farm_img_index: u64,
        level: u64,
        current_xp: u64,
        total_monster_slots: u64,
        occupied_monster_slots: u64,

        // Collection of Pet monsters owned by this Farm
        pet_monsters_id: ID,

        // ID of the applied cosmetic at this slot
        applied_farm_cosmetic_0_id:  Option<ID>,
        // ID of the applied cosmetic at this slot
        applied_farm_cosmetic_1_id:  Option<ID>,        
    }

    struct Monster has key, store {
        id: VersionedID,
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


        // ID of the applied cosmetic at this slot
        applied_monster_cosmetic_0_id: Option<ID>,
        // ID of the applied cosmetic at this slot
        applied_monster_cosmetic_1_id: Option<ID>,    

    }

    struct FarmCosmetic has key, store {
        id: VersionedID,
        cosmetic_type: u8,
    }

    struct MonsterCosmetic has key, store {
        id: VersionedID,
        cosmetic_type: u8,
    }


    // === Constructors. These create new Sui objects. ===

    // Creates a basic player object
    public fun create_player_(
        player_name: vector<u8>, ctx: &mut TxContext
    ): Player {
        let player = Player {
            id: TxContext::new_id(ctx),
            player_name: ASCII::string(player_name),
            water_runes_count: 0,
            fire_runes_count: 0,
            wind_runes_count: 0,
            earth_runes_count: 0,

            owned_farms_id: Option::none(),
            farm_inventory_cosmetics_id: ID::new(@0x0),
            monster_inventory_cosmetics_id: ID::new(@0x0)
        };

        // Create the collections
        //let owned_farms_c = Collection::new(&mut ctx);
        let inventory_farm_cosmetics_c = Collection::new(ctx);
        let inventory_monster_cosmetics_c = Collection::new(ctx);

        // Set the fields
        
        // Destroy the old id


        player.farm_inventory_cosmetics_id = *ID::id(&inventory_farm_cosmetics_c);
        player.monster_inventory_cosmetics_id = *ID::id(&inventory_monster_cosmetics_c);

        // Transfer ownership to the player
        //Transfer::transfer_to_object(owned_farms_c, player);
        Transfer::transfer_to_object(inventory_farm_cosmetics_c, &mut player);
        Transfer::transfer_to_object(inventory_monster_cosmetics_c, &mut player);

        player
    }

    // Creates a basic farm object
    public fun create_farm_(
        farm_name: vector<u8>, farm_img_index: u64, total_monster_slots: u64, ctx: &mut TxContext
    ): Farm {
        let farm = Farm {
            id: TxContext::new_id(ctx),
            farm_name: ASCII::string(farm_name),
            farm_img_index,
            level: 0,
            current_xp: 0,
            total_monster_slots,
            occupied_monster_slots: 0,
            pet_monsters_id: ID::new(@0x0),
            applied_farm_cosmetic_0_id: Option::none(),
            applied_farm_cosmetic_1_id: Option::none(),
        };


        // Create the collections
        let pet_monsters_c = Collection::new(ctx);

        // Set the fields
        farm.pet_monsters_id = *ID::id(&pet_monsters_c);

        // Transfer ownership to the farm
        Transfer::transfer_to_object(pet_monsters_c, &mut farm);

        farm
    }

    // Creates a basic Monster object
    public fun create_monster_(
        monster_name: vector<u8>,
        monster_img_index: u64,
        breed: u8,
        monster_affinity: u8,
        monster_description: vector<u8>,
        ctx: &mut TxContext
    ): Monster {

        Monster {
            id: TxContext::new_id(ctx),
            monster_name: ASCII::string(monster_name),
            monster_img_index,
            breed,
            monster_affinity,
            monster_description: ASCII::string(monster_description),
            monster_level: 0,
            monster_xp: 0,
            hunger_level: 0,
            affection_level: 0,
            buddy_level: 0,
            applied_monster_cosmetic_0_id: Option::none(),
            applied_monster_cosmetic_1_id: Option::none(),
        }
    }

    // ================================================================================= Entry functions ================================================================================= 
    // Constructors

    /// Create a player and transfer it to the transaction sender
    public fun create_player(
        player_name: vector<u8>, ctx: &mut TxContext
    ) {
        // Create player simply and transfer to caller
        let player = create_player_(player_name, ctx);
        Transfer::transfer(player, TxContext::sender(ctx))
    }

    /// Create a Farm and add it to the Player's collection of farms, which must be max 1
    public fun create_farm(player: &mut Player, farm_img_index: u64,
                            farm_name: vector<u8>, total_monster_slots: u64, ctx: &mut TxContext
    ) {
        // We only allow one farm for now
        assert!(Option::is_none(&player.owned_farms_id), ETOO_MANY_FARMS);

        let farm = create_farm_(farm_name, farm_img_index, total_monster_slots, ctx);

        // Store the ID of the farm
        player.owned_farms_id = Option::some(*ID::id(&farm));

        // Transfer ownership ofr farm to player
        Transfer::transfer_to_object(farm, player);
    }

    /// Create a Monster and add it to the Farm's collection of Monsters, which is unbounded
    public fun create_monster(_player: &mut Player,
                                farm: &mut Farm,
                                pet_monsters_c: &mut Collection:: Collection,
                                monster_name: vector<u8>,
                                monster_img_index: u64,
                                breed: u8,
                                monster_affinity: u8,
                                monster_description: vector<u8>,
                                ctx: &mut TxContext
    ) {

        let monster = create_monster_(
            monster_name,
            monster_img_index,
            breed,
            monster_affinity,
            monster_description,
            ctx
        );

        // Check if this is the right collection
        assert!(*&farm.pet_monsters_id == *ID::id(pet_monsters_c), EMONSTER_COLLECTION_NOT_OWNED_BY_FARM);


        // Add it to the collection
        Collection::add(pet_monsters_c, monster);
    }

    /// Create Monster cosmetic owned by player and add to its inventory
    public fun create_farm_cosmetics(player: &mut Player, farm_cosmetics_inventory_c: &mut Collection::Collection, cosmetic_type: u8, ctx: &mut TxContext) {
        
        // Check if this is the right collection
        assert!(*&player.farm_inventory_cosmetics_id
                    == *ID::id(farm_cosmetics_inventory_c), EFARM_COSMETICS_INVENTORY_NOT_OWNED_BY_PLAYER);
        
        
        // Create the farm cosmetic object
        let farm_cosmetic = FarmCosmetic {id: TxContext::new_id(ctx), cosmetic_type };

        // Add it to the player's inventory
        Collection::add(farm_cosmetics_inventory_c, farm_cosmetic);
    }
    /// Create Monster cosmetic owned by player and add to its inventory
    public fun create_monster_cosmetics(player: &mut Player, monster_cosmetics_inventory_c: &mut Collection::Collection, cosmetic_type: u8, ctx: &mut TxContext) {
        
        // Check if this is the right collection
        assert!(*&player.monster_inventory_cosmetics_id
                    == *ID::id(monster_cosmetics_inventory_c), EMONSTER_COSMETICS_INVENTORY_NOT_OWNED_BY_PLAYER);
        
        
        // Create the farm cosmetic object
        let monster_cosmetic = MonsterCosmetic {id: TxContext::new_id(ctx), cosmetic_type};

        // Add it to the player's inventory
        Collection::add(monster_cosmetics_inventory_c, monster_cosmetic);
    }

    /// Update the attributes of a player
    public fun update_player(
        player: &mut Player,
        water_runes_count: u64,
        fire_runes_count: u64,
        wind_runes_count: u64,
        earth_runes_count: u64,
        _ctx: &mut TxContext
    ) {
        player.water_runes_count = water_runes_count;
        player.fire_runes_count = fire_runes_count;
        player.wind_runes_count = wind_runes_count;
        player.earth_runes_count = earth_runes_count
    }

    /// Analog of Update_Farm
    /// Need to pass in Player for ownership checks
    public fun update_farm_stats(_player: &mut Player, farm: &mut Farm, level: u64, current_xp: u64, _ctx: &mut TxContext) {
        farm.current_xp = current_xp;
        farm.level = level;
    }

    /// Apply the cosmetics to the Farm from the inventory
    public fun update_farm_cosmetics(_player: &mut Player, farm: &mut Farm, _farm_cosmetic_inventory: &mut Collection::Collection,
                                        farm_cosmetic: FarmCosmetic,  cosmetic_slot_id: u64, _ctx: &mut TxContext) {

        // Only 2 slots allowed
        assert!(cosmetic_slot_id <= 1 , EINVALID_COSMETICS_SLOT);

        // Assign by slot
        if (cosmetic_slot_id == 0) {
            // Check that the slot has no items
            assert!(Option::is_none(&farm.applied_farm_cosmetic_0_id), ETOO_MANY_COSMETICS);

            // Fill the slot ID
            farm.applied_farm_cosmetic_0_id = Option::some(*ID::id(&farm_cosmetic));
        } else {
            // Check that the slot has no items
            assert!(Option::is_none(&farm.applied_farm_cosmetic_1_id), ETOO_MANY_COSMETICS);

            // Fill the slot ID
            farm.applied_farm_cosmetic_1_id = Option::some(*ID::id(&farm_cosmetic));
        };

        // Transfer owner in this farm
        Transfer::transfer_to_object(farm_cosmetic, farm);
    }

    /// Apply the cosmetics to the Monster from the inventory
    public fun update_monster_cosmetics(_player: &mut Player, _farm: &mut Farm, monster: &mut Monster, _monster_cosmetic_inventory: &mut Collection::Collection, 
                                    monster_cosmetic: MonsterCosmetic, cosmetic_slot_id: u64,  _ctx: &mut TxContext) {

        // Only 2 slots allowed
        assert!(cosmetic_slot_id <= 1 , EINVALID_COSMETICS_SLOT);

        // Assign by slot
        if (cosmetic_slot_id == 0) {
            // Check that the slot has no items
            assert!(Option::is_none(&monster.applied_monster_cosmetic_0_id), ETOO_MANY_COSMETICS);

            // Fill the slot ID
            monster.applied_monster_cosmetic_0_id = Option::some(*ID::id(&monster_cosmetic));
        } else {
            // Check that the slot has no items
            assert!(Option::is_none(&monster.applied_monster_cosmetic_1_id), ETOO_MANY_COSMETICS);

            // Fill the slot ID
            monster.applied_monster_cosmetic_1_id = Option::some(*ID::id(&monster_cosmetic));
        };

        // Transfer owner in this monster
        Transfer::transfer_to_object(monster_cosmetic, monster);

    }
}
