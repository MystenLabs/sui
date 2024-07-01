// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Acts as an entrypoint to Turnip Town, as a Kiosk app.
///
/// Other modules in this package expose public(package) functions, which this
/// module calls into after performing the appropriate authorization checks.
module turnip_town::game {
    use sui::kiosk::{Kiosk, KioskOwnerCap};
    use sui::kiosk_extension as app;
    use sui::transfer_policy::TransferPolicy;
    use turnip_town::field::{Self, Field};
    use turnip_town::turnip::Turnip;
    use turnip_town::water::{Self, Water, Well};

    // === Types ===

    /// Kiosk App witness -- doubles as a dynamic field key for holding the game
    /// state.
    public struct EXT() has drop;

    /// Key for storing the game state in the Kiosk App's bag.
    public struct KEY() has copy, drop, store;

    public struct Game has store {
        field: Field,
        well: Well,
    }

    // === Constants ===

    /// Kiosk App permisions
    ///
    /// place: To support placing harvested turnips into the kiosk.
    const PERMISSIONS: u128 = 1;

    // === Errors ===

    /// Game is already installed.
    const EAlreadyInstalled: u64 = 0;

    /// Game is not installed on this Kiosk.
    const ENotInstalled: u64 = 1;

    /// Action can only be performed by the kiosk owner.
    const ENotAuthorized: u64 = 2;

    // === Public Functions ===

    /// Install Turnip Town as a Kiosk App (adds the extension and sets up a new
    /// game state). Each kiosk can host at most one game instance.
    public fun add(kiosk: &mut Kiosk, cap: &KioskOwnerCap, ctx: &mut TxContext) {
        assert!(kiosk.has_access(cap), ENotAuthorized);
        assert!(!app::is_installed<EXT>(kiosk), EAlreadyInstalled);
        app::add(EXT(), kiosk, cap, PERMISSIONS, ctx);
        let bag = app::storage_mut(EXT(), kiosk);
        bag.add(KEY(), Game {
            field: field::new(ctx),
            well: water::well(ctx),
        });
    }

    /// Uninstall Turnip Town as a Kiosk App. The field must be empty (any
    /// eligible turnips harvested) for this operation to succeed.
    public fun remove(kiosk: &mut Kiosk, cap: &KioskOwnerCap, ctx: &TxContext) {
        assert!(kiosk.has_access(cap), ENotAuthorized);
        assert!(app::is_installed<EXT>(kiosk), ENotInstalled);
        let Game { field, well: _ } = app::storage_mut(EXT(), kiosk).remove(KEY());
        field.burn(ctx);
    }

    /// Sow a seed at slot `(i, j)` of the field in the game installed on
    /// `kiosk`. This is an authorized action, so can only be performed by the
    /// owner of the kiosk, and only works if the kiosk has the game installed,
    /// and the slot does not already contain a turnip.
    public fun sow(
        kiosk: &mut Kiosk,
        cap: &KioskOwnerCap,
        i: u64,
        j: u64,
        ctx: &mut TxContext,
    ) {
        assert!(kiosk.has_access(cap), ENotAuthorized);
        game_mut(kiosk).field.sow(i, j, ctx)
    }

    /// Fetch water from the well of the game installed on `kiosk`. This is an
    /// authorized action, so can only be performed by the owner of the kiosk,
    /// and only works if the kiosk has the game installed.
    ///
    /// Access to water is limited to a fixed quantity in each epoch. Attempts
    /// to access more water than is available will fail.
    public fun fetch_water(
        kiosk: &mut Kiosk,
        cap: &KioskOwnerCap,
        amount: u64,
        ctx: &TxContext,
    ): Water {
        assert!(kiosk.has_access(cap), ENotAuthorized);
        game_mut(kiosk).well.fetch(amount, ctx)
    }

    /// Water the turnip at cell `(i, j)` on the field in the game installed on
    /// `kiosk`. This operation can only be performed on kiosks where the game
    /// has been installed, and where the field contains a turnip at the given
    /// position.
    public fun water(
        kiosk: &mut Kiosk,
        i: u64,
        j: u64,
        water: Water,
        ctx: &TxContext,
    ) {
        game_mut(kiosk).field.water(i, j, water, ctx)
    }

    /// Harvest a turnip growing at position `(i, j)` on the field in the game
    /// installed on `kiosk`. This action can only be performed on kiosks where
    /// the game has been installed.
    ///
    /// The harvested Kiosk is placed in the owning field (regardless of who
    /// harvested it), and its ID is returned.
    public fun harvest(
        kiosk: &mut Kiosk,
        policy: &TransferPolicy<Turnip>,
        i: u64,
        j: u64,
        ctx: &TxContext,
    ): ID {
        let turnip = game_mut(kiosk).field.harvest(i, j, ctx);
        let id = object::id(&turnip);
        app::place(EXT(), kiosk, turnip, policy);
        id
    }

    /// Run the simulation across all turnips in the field of the game installed
    /// on `kiosk`, so that its state is accurate up to the current epoch.
    ///
    /// This action can be performed by anyone.
    public fun simulate(kiosk: &mut Kiosk, ctx: &TxContext) {
        game_mut(kiosk).field.simulate(ctx)
    }

    // === Private Functions ===

    fun game_mut(kiosk: &mut Kiosk): &mut Game {
        assert!(app::is_installed<EXT>(kiosk), ENotInstalled);
        &mut app::storage_mut(EXT(), kiosk)[KEY()]
    }
}
