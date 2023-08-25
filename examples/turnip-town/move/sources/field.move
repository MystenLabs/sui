/// # Field
///
/// Defines the `Field` type, which is responsible for one instance of
/// the game.  `Field`s are stored centrally, in a `Game` instance,
/// and a `Deed` is created that gives its owner access to that
/// `Field` through the `Game`.
///
/// Although `Field` is not an object, an `ID` is generated for it, to
/// identify it in the `Game`.  This `ID` is also stored in the `Deed`.
///
/// Any field owner can sow or harvest turnips from the field, but
/// simulating growth and watering can only be done via the `Game`
/// which decides the cadence of simulation and how much water to
/// dispense.
module turnip_town::field {
    use std::option::{Self, Option};
    use std::vector;
    use sui::math;
    use sui::object::{Self, ID, UID};
    use sui::tx_context::TxContext;

    use turnip_town::turnip::{Self, Turnip};

    friend turnip_town::game;

    struct Field has store {
        slots: vector<Option<Turnip>>,
        water: u64,
    }

    /// Deed of ownership for a particular field in the game.
    struct Deed has key, store {
        id: UID,
        field: ID,
    }

    /// Trying to plant in a non-existent slot.
    const EOutOfBounds: u64 = 0;

    /// Slot is already occupied.
    const EAlreadyFilled: u64 = 1;

    /// Slot does not contain a turnip.
    const ENotFilled: u64 = 2;

    /// Field being destroyed contains a turnip.
    const ENotEmpty: u64 = 3;

    const WIDTH: u64 = 4;
    const HEIGHT: u64 = 4;

    public fun deed_field(deed: &Deed): ID {
        deed.field
    }

    /// Plant a fresh turnip at position (i, j) in `field`.
    ///
    /// Fails if the position is out of bounds or there is already a
    /// turnip there.
    public fun sow(field: &mut Field, i: u64, j: u64, ctx: &mut TxContext) {
        assert!(i < WIDTH && j < HEIGHT, EOutOfBounds);

        let ix = i + j * HEIGHT;
        let slot = vector::borrow_mut(&mut field.slots, ix);

        assert!(option::is_none(slot), EAlreadyFilled);
        option::fill(slot, turnip::fresh(ctx));
    }

    /// Harvest the turnip at position (i, j).
    ///
    /// Fails if the position is out of bounds, if no turnip exists
    /// there or the turnip was too small to harvest.
    public fun harvest(field: &mut Field, i: u64, j: u64): Turnip {
        assert!(i < WIDTH && j < HEIGHT, EOutOfBounds);

        let ix = i + j * HEIGHT;
        let slot = vector::borrow_mut(&mut field.slots, ix);

        assert!(option::is_some(slot), ENotFilled);
        let turnip = option::extract(slot);
        turnip::assert_harvest(&turnip);
        turnip
    }

    /* Protected Functions ****************************************************/

    /// Create a brand new field.  Protected to prevent `Field`s being
    /// created but not attached to a game.
    public(friend) fun mint(ctx: &mut TxContext): (Deed, Field) {
        let uid = object::new(ctx);
        let field = object::uid_to_inner(&uid);
        object::delete(uid);

        let slots = vector[];
        let total = WIDTH * HEIGHT;
        while (vector::length(&slots) < total) {
            vector::push_back(&mut slots, option::none());
        };

        (
            Deed { id: object::new(ctx), field },
            Field { slots, water: 0 },
        )
    }

    /// Destroy an empty field.  Protected to prevent `Field` being
    /// destroyed without its associated `Deed` being destroyed.
    ///
    /// Fails if there are any turnips left in the `field`.
    public(friend) fun burn_field(field: Field) {
        let Field { slots, water: _ } = field;

        while (!vector::is_empty(&slots)) {
            let turnip = vector::pop_back(&mut slots);
            assert!(option::is_none(&turnip), ENotEmpty);
            option::destroy_none(turnip);
        };

        vector::destroy_empty(slots)
    }

    /// Destroy the deed for a field.  Protected to prevent `Deed`
    /// being destroyed without its associated `Field` being
    /// destroyed.
    public(friend) fun burn_deed(deed: Deed) {
        let Deed { id, field: _ } = deed;
        object::delete(id);
    }

    /// Add water to the field.  Protected so the game can control how
    /// much water is given.
    public(friend) fun water(field: &mut Field, amount: u64) {
        field.water = field.water + amount;
    }

    /// Simulates turnips growing.  Protected as only the game module
    /// can control when simulation occurs.
    public(friend) fun simulate_growth(field: &mut Field, is_sunny: bool) {
        let (total_size, count) = count_turnips(field);

        // Not enough water to maintain freshness
        if (field.water < total_size) {
            field.water = 0;
            debit_field_freshness(field);
        } else {
            field.water = field.water - total_size;
            credit_field_freshness(field);
        };

        if (is_sunny) {
            let total_growth = math::min(20 * count, field.water);
            grow_turnips(field, total_growth / count / 2);
            field.water = field.water - total_growth;
        };

        if (field.water > 0) {
            debit_field_freshness(field);
        };

        clean_up_field(field);
    }

    /* Private Functions ***************************************************/
    /* (Helpers for `simulate_growth`) */

    fun count_turnips(field: &Field): (u64, u64) {
        let slots = &field.slots;
        let len = vector::length(slots);

        let (i, size, count) = (0, 0, 0);
        while (i < len) {
            let slot = vector::borrow(slots, i);
            if (option::is_some(slot)) {
                let turnip = option::borrow(slot);
                size = size + turnip::size(turnip);
                count = count + 1;
            };
            i = i + 1;
        };

        (size, count)
    }

    fun grow_turnips(field: &mut Field, growth: u64) {
        let slots = &mut field.slots;
        let len = vector::length(slots);

        let i = 0;
        while (i < len) {
            let slot = vector::borrow_mut(slots, i);
            if (option::is_some(slot)) {
                let turnip = option::borrow_mut(slot);
                turnip::grow(turnip, growth);
            };
            i = i + 1;
        };
    }

    fun debit_field_freshness(field: &mut Field) {
        let slots = &mut field.slots;
        let len = vector::length(slots);

        let i = 0;
        while (i < len) {
            let slot = vector::borrow_mut(slots, i);
            if (option::is_some(slot)) {
                let turnip = option::borrow_mut(slot);
                turnip::debit_freshness(turnip);
            };
            i = i + 1;
        };

    }

    fun credit_field_freshness(field: &mut Field) {
        let slots = &mut field.slots;
        let len = vector::length(slots);

        let i = 0;
        while (i < len) {
            let slot = vector::borrow_mut(slots, i);
            if (option::is_some(slot)) {
                let turnip = option::borrow_mut(slot);
                turnip::credit_freshness(turnip);
            };
            i = i + 1;
        };
    }

    fun clean_up_field(field: &mut Field) {
        let slots = &mut field.slots;
        let len = vector::length(slots);

        let i = 0;
        while (i < len) {
            let slot = vector::borrow_mut(slots, i);
            if (option::is_some(slot)) {
                if (!turnip::is_fresh(option::borrow(slot))) {
                    turnip::burn(option::extract(slot))
                }
            };
            i = i + 1;
        };
    }

    /* Tests ******************************************************************/
    use sui::test_scenario as ts;

    #[test_only]
    fun borrow_mut(field: &mut Field, i: u64, j: u64): &mut Turnip {
        option::borrow_mut(vector::borrow_mut(&mut field.slots, i + j * WIDTH))
    }

    #[test]
    fun test_burn() {
        let ts = ts::begin(@0xA);
        let (deed, field) = mint(ts::ctx(&mut ts));

        burn_field(field);
        burn_deed(deed);
        ts::end(ts);
    }

    #[test]
    #[expected_failure(abort_code = ENotEmpty)]
    fun test_burn_failure() {
        let ts = ts::begin(@0xA);
        let (deed, field) = mint(ts::ctx(&mut ts));

        // Sow a turnip, now the field is not empty.
        sow(&mut field, 0, 0, ts::ctx(&mut ts));

        burn_field(field);
        burn_deed(deed);
        ts::end(ts);
    }

    #[test]
    fun test_sow_and_harvest() {
        let ts = ts::begin(@0xA);
        let (deed, field) = mint(ts::ctx(&mut ts));

        sow(&mut field, 0, 0, ts::ctx(&mut ts));

        // Update the sown turnip to be big enough to harvest.
        turnip::prepare_for_harvest(borrow_mut(&mut field, 0, 0));

        let turnip = harvest(&mut field, 0, 0);
        turnip::burn(turnip);

        burn_field(field);
        burn_deed(deed);
        ts::end(ts);
    }

    #[test]
    #[expected_failure(abort_code = EOutOfBounds)]
    fun test_sow_out_of_bounds() {
        let ts = ts::begin(@0xA);
        let (_deed, field) = mint(ts::ctx(&mut ts));

        sow(&mut field, WIDTH + 1, 0, ts::ctx(&mut ts));
        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = EAlreadyFilled)]
    fun test_sow_overlap() {
        let ts = ts::begin(@0xA);
        let (_deed, field) = mint(ts::ctx(&mut ts));

        sow(&mut field, 0, 0, ts::ctx(&mut ts));
        sow(&mut field, 0, 0, ts::ctx(&mut ts));
        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = EOutOfBounds)]
    fun test_harvest_out_of_bounds() {
        let ts = ts::begin(@0xA);
        let (_deed, field) = mint(ts::ctx(&mut ts));

        let _turnip = harvest(&mut field, WIDTH + 1, 0);
        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = turnip::ETooSmall)]
    fun test_harvest_too_small() {
        let ts = ts::begin(@0xA);
        let (_deed, field) = mint(ts::ctx(&mut ts));

        sow(&mut field, 0, 0, ts::ctx(&mut ts));
        let _turnip = harvest(&mut field, 0, 0);
        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = ENotFilled)]
    fun test_harvest_non_existent() {
        let ts = ts::begin(@0xA);
        let (_deed, field) = mint(ts::ctx(&mut ts));

        let _turnip = harvest(&mut field, 0, 0);
        abort 1337
    }

    #[test]
    fun test_simulation_growth() {
        let ts = ts::begin(@0xA);
        let (deed, field) = mint(ts::ctx(&mut ts));

        sow(&mut field, 0, 0, ts::ctx(&mut ts));

        // Update the sown turnip to be big enough to harvest (even
        // without growing)
        let turnip = borrow_mut(&mut field, 0, 0);
        turnip::prepare_for_harvest(turnip);
        let size = turnip::size(turnip);

        let is_sunny = true;
        water(&mut field, size + 10);
        simulate_growth(&mut field, is_sunny);

        // All the water should be used up.
        assert!(field.water == 0, 0);

        let turnip = harvest(&mut field, 0, 0);

        // Turnip grows by half its excess water usage.
        assert!(turnip::size(&turnip) == size + 5, 0);

        turnip::burn(turnip);
        burn_field(field);
        burn_deed(deed);
        ts::end(ts);
    }

    #[test]
    fun test_simulation_dry() {
        let ts = ts::begin(@0xA);
        let (deed, field) = mint(ts::ctx(&mut ts));

        sow(&mut field, 0, 0, ts::ctx(&mut ts));

        // Update the sown turnip to be big enough to harvest (even
        // without growing)
        let turnip = borrow_mut(&mut field, 0, 0);
        turnip::prepare_for_harvest(turnip);
        let size = turnip::size(turnip);

        // Not enough water to maintain freshness
        let is_sunny = true;
        water(&mut field, size - 1);
        simulate_growth(&mut field, is_sunny);

        // All the water should be used up.
        assert!(field.water == 0, 0);

        let turnip = harvest(&mut field, 0, 0);

        // Turnip does not grow, and it gets less fresh
        assert!(turnip::size(&turnip) == size, 0);
        assert!(turnip::freshness(&turnip) == 50_00, 0);

        turnip::burn(turnip);
        burn_field(field);
        burn_deed(deed);
        ts::end(ts);
    }

    #[test]
    fun test_simulation_rot() {
        let ts = ts::begin(@0xA);
        let (deed, field) = mint(ts::ctx(&mut ts));

        sow(&mut field, 0, 0, ts::ctx(&mut ts));

        // Update the sown turnip to be big enough to harvest (even
        // without growing)
        let turnip = borrow_mut(&mut field, 0, 0);
        turnip::prepare_for_harvest(turnip);
        let size = turnip::size(turnip);

        // Not enough water to maintain freshness
        let is_sunny = true;
        water(&mut field, size - 1);
        simulate_growth(&mut field, is_sunny);

        // All the water should be used up.
        assert!(field.water == 0, 0);

        let turnip = harvest(&mut field, 0, 0);

        // Turnip does not grow, and it gets less fresh
        assert!(turnip::size(&turnip) == size, 0);
        assert!(turnip::freshness(&turnip) == 50_00, 0);

        turnip::burn(turnip);
        burn_field(field);
        burn_deed(deed);
        ts::end(ts);
    }

    #[test]
    fun test_simulation_not_sunny() {
        let ts = ts::begin(@0xA);
        let (deed, field) = mint(ts::ctx(&mut ts));

        sow(&mut field, 0, 0, ts::ctx(&mut ts));

        // Update the sown turnip to be big enough to harvest (even
        // without growing)
        let turnip = borrow_mut(&mut field, 0, 0);
        turnip::prepare_for_harvest(turnip);
        let size = turnip::size(turnip);

        // If the weather is not sunny, then nothing grows
        let is_sunny = false;
        water(&mut field, size + 10);
        simulate_growth(&mut field, is_sunny);

        // The water that would have been used for growth remain.
        assert!(field.water == 10, 0);

        let turnip = harvest(&mut field, 0, 0);

        // Turnip does not grow, and gets less fresh
        assert!(turnip::size(&turnip) == size, 0);
        assert!(turnip::freshness(&turnip) == 50_00, 0);

        turnip::burn(turnip);
        burn_field(field);
        burn_deed(deed);
        ts::end(ts);
    }

    #[test]
    fun test_simulation_multi() {
        let ts = ts::begin(@0xA);
        let (deed, field) = mint(ts::ctx(&mut ts));

        sow(&mut field, 0, 0, ts::ctx(&mut ts));
        sow(&mut field, 1, 0, ts::ctx(&mut ts));

        // Update the sown turnips to be big enough to harvest (even
        // without growing)
        turnip::prepare_for_harvest(borrow_mut(&mut field, 0, 0));
        turnip::prepare_for_harvest(borrow_mut(&mut field, 1, 0));

        // Make the second turnip bigger, for variety
        turnip::prepare_for_harvest(borrow_mut(&mut field, 1, 0));

        let s0 = turnip::size(borrow_mut(&mut field, 0, 0));
        let s1 = turnip::size(borrow_mut(&mut field, 1, 0));

        let is_sunny = true;
        water(&mut field, s0 + s1 + 10);
        simulate_growth(&mut field, is_sunny);

        // All the water should be used up.
        assert!(field.water == 0, 0);

        let t0 = harvest(&mut field, 0, 0);
        let t1 = harvest(&mut field, 1, 0);

        // Turnips only grow by 2, because there are 5 units of water
        // for each, and we grow by half that, rounded down.
        assert!(turnip::size(&t0) == s0 + 2, 0);
        assert!(turnip::size(&t1) == s1 + 2, 0);

        turnip::burn(t0);
        turnip::burn(t1);

        burn_field(field);
        burn_deed(deed);
        ts::end(ts);
    }

    #[test]
    fun test_simulation_cleanup() {
        let ts = ts::begin(@0xA);
        let (deed, field) = mint(ts::ctx(&mut ts));

        sow(&mut field, 0, 0, ts::ctx(&mut ts));
        turnip::prepare_for_harvest(borrow_mut(&mut field, 0, 0));

        let is_sunny = true;
        let expected_freshness = 50_00;
        while (expected_freshness != 0) {
            simulate_growth(&mut field, is_sunny);
            let turnip = borrow_mut(&mut field, 0, 0);
            assert!(expected_freshness == turnip::freshness(turnip), 0);
            expected_freshness = expected_freshness / 2;
        };

        // The turnip was cleaned up once its freshness reached zero.
        simulate_growth(&mut field, is_sunny);
        assert!(expected_freshness == 0, 0);
        assert!(option::is_none(vector::borrow(&mut field.slots, 0)), 0);

        burn_field(field);
        burn_deed(deed);
        ts::end(ts);
    }
}
