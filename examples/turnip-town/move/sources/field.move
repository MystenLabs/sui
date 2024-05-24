// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Defines the `Field` type, which represents a single instance of the game.
/// Fields are installed as Kiosk Apps on the player's Kiosk. The player that
/// owns the Kiosk is said to own the installed `Field`.
///
/// Only the owner is able to sow turnips in the field, and take water from
/// their field (which can be used anywhere, including other players' fields).
/// Any player can visit a field to water turnips and harvest them, but
/// harvested turnips always go to the field owner's kiosk.
module turnip_town::field {
    use turnip_town::turnip::{Self, Turnip};
    use turnip_town::water::Water;

    // === Types ===

    public struct Field has store {
        slots: vector<Slot>,
    }

    /// A slot is a single position in the field.
    public struct Slot has store {
        /// The turnip growing at this position in the field.
        turnip: Option<Turnip>,

        /// The water left over at this position, this is consumed over time.
        water: u64,

        /// The last epoch this slot was simulated at.
        last_updated: u64,
    }

    // === Constants ===

    /// Field width (number of slots)
    const WIDTH: u64 = 4;

    /// Field height (number of slots)
    const HEIGHT: u64 = 4;

    // === Errors ===

    /// Trying to plant in a non-existent slot.
    const EOutOfBounds: u64 = 0;

    /// Slot is already occupied.
    const EAlreadyFilled: u64 = 1;

    /// Slot does not contain a turnip.
    const ENotFilled: u64 = 2;

    /// Field being destroyed contains a turnip.
    const ENotEmpty: u64 = 3;

    /// Turnip is too small to harvest
    const ETooSmall: u64 = 4;

    // === Protected Functions ===

    /// Create a brand new field.
    public(package) fun new(ctx: &TxContext): Field {
        let mut slots = vector[];
        let total = WIDTH * HEIGHT;
        while (slots.length() < total) {
            slots.push_back(Slot {
                turnip: option::none(),
                water: 0,
                last_updated: ctx.epoch(),
            });
        };

        Field { slots }
    }

    /// Destroy a field with turnips potentially in it, as long as they could
    /// not be harvested.
    public(package) fun burn(mut field: Field, ctx: &TxContext) {
        field.simulate(ctx);

        let Field { mut slots } = field;

        while (!vector::is_empty(&slots)) {
            let Slot { turnip, water: _, last_updated: _ } = slots.pop_back();
            if (turnip.is_some()) {
                let turnip = turnip.destroy_some();
                assert!(!turnip.can_harvest(), ENotEmpty);
                turnip.consume();
            } else {
                turnip.destroy_none();
            }
        };

        vector::destroy_empty(slots)
    }

    /// Plant a fresh turnip at position (i, j) in `field`.
    ///
    /// Fails if the position is out of bounds or there is already a turnip
    /// there.
    public(package) fun sow(
        field: &mut Field,
        i: u64,
        j: u64,
        ctx: &mut TxContext,
    ) {
        let slot = field.slot_mut(i, j, ctx);

        assert!(slot.turnip.is_none(), EAlreadyFilled);
        slot.turnip.fill(turnip::fresh(ctx));
        slot.last_updated = ctx.epoch();
    }

    /// Add water at position (i, j) in `field`.
    ///
    /// Fails if the postion is out-of-bounds. It is valid to water a slot
    /// without a turnip.
    public(package) fun water(
        field: &mut Field,
        i: u64,
        j: u64,
        water: Water,
        ctx: &TxContext,
    ) {
        let slot = field.slot_mut(i, j, ctx);
        slot.water = slot.water + water.value();
    }

    /// Harvest the turnip at position (i, j).
    ///
    /// Fails if the position is out of bounds, if no turnip exists there or the
    /// turnip was too small to harvest.
    public(package) fun harvest(
        field: &mut Field,
        i: u64,
        j: u64,
        ctx: &TxContext,
    ): Turnip {
        let slot = field.slot_mut(i, j, ctx);

        assert!(slot.turnip.is_some(), ENotFilled);
        let turnip = slot.turnip.extract();

        assert!(turnip.can_harvest(), ETooSmall);
        turnip
    }

    /// Bring all the slots in the field up-to-date with the current epoch.
    public fun simulate(field: &mut Field, ctx: &TxContext) {
        let mut j = 0;
        while (j < HEIGHT) {
            let mut i = 0;
            while (i < WIDTH) {
                // Calling slot_mut has the effect of bringing the slot
                // up-to-date before returning a reference to it (which is
                // immediately discarded).
                let _ = field.slot_mut(i, j, ctx);
                i = i + 1;
            };
            j = j + 1;
        }
    }

    // === Private Functions ===

    /// Return the slot at position (i, j), up-to-date as of the epoch in `ctx`.
    ///
    ///  Fails if (i, j) is out-of-bounds.
    fun slot_mut(field: &mut Field, i: u64, j: u64, ctx: &TxContext): &mut Slot {
        assert!(i < WIDTH && j < HEIGHT, EOutOfBounds);

        let epoch = ctx.epoch();
        let ix = i + j * WIDTH;
        let slot = &mut field.slots[ix];
        let days = epoch - slot.last_updated;
        slot.last_updated = epoch;

        if (slot.turnip.is_some()) {
            let turnip = slot.turnip.borrow_mut();
            turnip.simulate(&mut slot.water, days);
            if (!turnip.is_fresh()) {
                slot.turnip.extract().consume();
            }
        };

        slot
    }

    // === Test Helpers ===

    #[test_only]
    #[syntax(index)]
    /// General access to slots in the field, but only exposed for tests.
    public fun borrow(field: &Field, i: u64, j: u64): &Turnip {
        field.slots[i + j *  WIDTH].turnip.borrow()
    }

    #[test_only]
    #[syntax(index)]
    /// General access to slots in the field, but only exposed for tests.
    public fun borrow_mut(field: &mut Field, i: u64, j: u64): &mut Turnip {
        field.slots[i + j *  WIDTH].turnip.borrow_mut()
    }

    #[test_only]
    public fun is_empty(field: &mut Field, i: u64, j: u64): bool {
        field.slots[i + j *  WIDTH].turnip.is_none()
    }

    #[test_only]
    /// Clean-up the field even if it contains turnips in it.
    public fun destroy_for_test(field: Field) {
        let Field { mut slots } = field;

        while (!vector::is_empty(&slots)) {
            let Slot { turnip, water: _, last_updated: _ } = slots.pop_back();
            if (turnip.is_some()) {
                let turnip = turnip.destroy_some();
                turnip.consume();
            } else {
                turnip.destroy_none();
            }
        };

        vector::destroy_empty(slots)
    }
}
