/// # Game
///
/// The `game` module is the entrypoint for Turnip Town.  On publish,
/// it creates a central table of all active game instances, and
/// protects access to a game instance (a `Field`) via its `Deed`.
///
/// `simulate_weather` is an admin-only operation that the game admin
/// uses to progress the simulation of the game on a given field
/// (identified by its ID).
module turnip_town::game {
    use sui::object::{Self, ID, UID};
    use sui::table::{Self, Table};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    use turnip_town::admin::{Self, AdminCap};
    use turnip_town::field::{Self, Deed, Field};
    use turnip_town::turnip::Turnip;

    struct Game has key, store {
        id: UID,
        fields: Table<ID, Field>,
    }

    /// No corresponding field for the deed.
    const ENoSuchField: u64 = 0;

    /// How much water a field gets when watered by a player.
    const WATER_INCREMENT: u64 = 1000;

    fun init(ctx: &mut TxContext) {
        let game = Game {
            id: object::new(ctx),
            fields: table::new(ctx),
        };

        transfer::public_transfer(
            admin::mint(object::id(&game), ctx),
            tx_context::sender(ctx),
        );

        transfer::share_object(game);
    }

    /// Create a new field to the `game`.  The field starts off empty
    /// (no plants, no water).  Returns the deed that gives a player
    /// control of that field.
    public fun new(game: &mut Game, ctx: &mut TxContext): Deed {
        let (deed, field) = field::mint(ctx);

        table::add(
            &mut game.fields,
            field::deed_field(&deed),
            field,
        );

        deed
    }

    /// Destroy the field owned by `deed`.
    ///
    /// Fails if that field is not empty, or if the field has somehow
    /// already been deleted.
    public fun burn(deed: Deed, game: &mut Game) {
        let fid = field::deed_field(&deed);
        assert!(table::contains(&game.fields, fid), ENoSuchField);

        let field = table::remove(&mut game.fields, fid);
        field::burn_field(field);
        field::burn_deed(deed);
    }

    /// Sow a turnip at position (i, j) of the field owned by `deed`
    /// in `game`.
    ///
    /// Fails if the field does not exist for this `deed`, or there is
    /// already a turnip at that position.
    public fun sow(
        deed: &Deed,
        game: &mut Game,
        i: u64,
        j: u64,
        ctx: &mut TxContext,
    ) {
        let fid = field::deed_field(deed);
        assert!(table::contains(&game.fields, fid), ENoSuchField);

        let field = table::borrow_mut(&mut game.fields, fid);
        field::sow(field, i, j, ctx);
    }

    /// Water the field owned by `deed`.
    ///
    /// Fails if the field does not exist for this `deed`.
    public fun water(deed: &Deed, game: &mut Game) {
        let fid = field::deed_field(deed);
        assert!(table::contains(&game.fields, fid), ENoSuchField);

        let field = table::borrow_mut(&mut game.fields, fid);
        field::water(field, WATER_INCREMENT);
    }

    /// Harvest a turnip at position (i, j) of the field owned by
    /// `deed`.
    ///
    /// Fails if the field for this `deed` does not exist, there is no
    /// turnip to harvest at this position or that turnip is too small
    /// to harvest.
    public fun harvest(deed: &Deed, game: &mut Game, i: u64, j: u64): Turnip {
        let fid = field::deed_field(deed);
        assert!(table::contains(&game.fields, fid), ENoSuchField);

        let field = table::borrow_mut(&mut game.fields, fid);
        field::harvest(field, i, j)
    }

    /// Admin-only operation for the game service to simulate weather.
    ///
    /// Rain is simulated by adding `rain_amount` to the field's water
    /// supply. The water supply is then shared among all turnips in
    /// the field.
    ///
    /// Turnips gain 5% freshness (up to a max of 100%) for each
    /// simulation tick they have enough water for.
    ///
    /// Every turnip needs water equal to its size to remain fresh.
    /// If there is not enough water to keep all turnips fresh, then
    /// freshness halves (turnips dry).
    ///
    /// If it `is_sunny` and there is water left over, it is
    /// distributed evenly between the turnips, to grow them.  Every 2
    /// units of water increases turnip size by 1, up to 10 size
    /// units.
    ///
    /// If there is water left over, then freshness also halves
    /// (turnips rot).
    ///
    /// If freshness drops to zero, the turnip has died and will be
    /// removed.
    ///
    /// Fails if the field (represented by its ID in the game's table)
    /// does not exist.
    public fun simulate_weather(
        admin: &AdminCap,
        game: &mut Game,
        rain_amount: u64,
        is_sunny: bool,
        fid: ID,
    ) {
        admin::authorize(admin, object::id(game));
        assert!(table::contains(&game.fields, fid), ENoSuchField);

        let field = table::borrow_mut(&mut game.fields, fid);
        field::water(field, rain_amount);
        field::simulate_growth(field, is_sunny);
    }
}
