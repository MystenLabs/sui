// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Kiosk Custody Marketplace
///
/// Description:
/// A Marketplace design, where sellers entrust their Kiosk to the
/// marketplace; Marketplace provides the discovery mechanism for the
/// sellers (and the interface). Marketplace does not control the Kiosks
/// (there's no implementation for that) but may decide which listings
/// to show on the website - eg only verified collections.
///
/// Details:
/// - The marketplace is a custodian of the Kiosk, and they
/// charge the fee for the discovery service. The marketplace allows
/// Kiosks owners to leave at any time (with a fee).
///
/// - Although the marketplace is a custodian of the Kiosk, the user
/// is free to use all the features of the Kiosk except for the `withdraw`.
/// If a Kiosk balance change is detected, the transaction aborts; all other
/// operations are allowed.
///
/// Caveats:
/// - Marketplace publisher keeps all owner Caps, and may submit an upgrade
/// to take Kiosks in possession. Although this is the issue with all
/// custodian models, wonder if there's an easy way to mitigate this. Perhaps,
/// moving the authorization (MarketplaceKioskCap -> KioskOwnerCap) module
/// into another package and burning the UpgradeCap to make upgrades impossible.
///
/// TODOs:
/// - Add the MarketplaceManagerCap which allows withdrawing the fees.
/// - Add some functionality to update fees.
/// - Generalize the solution so that someone could lock any Kiosk this way (locking
/// profits, but allowing to use the rest of the Kiosk's function as usual)
///
/// Notes (intended fixes for Kiosk):
/// #1 - `kiosk` module has no way to read Kiosk.id from the `KioskOwnerCap`
module kiosk::kiosk_custody_marketplace {
    use std::option::none;
    use sui::balance::{Self, Balance};
    use sui::coin::{Self, Coin};
    use sui::dynamic_field as df;
    use sui::event;
    use sui::kiosk::{Self, Kiosk, KioskOwnerCap};
    use sui::object::{Self, ID, UID};
    use sui::sui::SUI;
    use sui::tx_context::TxContext;
    use sui::transfer;

    /// Trying to participate with a Kiosk which is not owned by the user.
    const ENotOwner: u64 = 0;
    /// Trying to use a `MarketplaceKioskCap` for a different Kiosk.
    const EWrongKiosk: u64 = 1;
    /// Profits were withdrawn from the Kiosk ignoring the fees.
    const EAmountChanged: u64 = 2;

    /// The Marketplace object - holds the trusted `KioskOwnerCap`s,
    /// and the collected fees from trades.
    struct Marketplace has key {
        id: UID,
        fee_bp: u32,
        balance: Balance<SUI>,
    }

    /// A Capabalility which gives access to the `KioskOwnerCap` for
    /// the Marketplace participants.
    struct MarketplaceKioskCap has key, store {
        id: UID,
        kiosk_id: ID,
    }

    /// A custom dynamic field key to store the `KioskOwnerCap`.
    struct OwnerKey has store, copy, drop { kiosk_id: ID }

    /// Hot potato which makes sure that the `KioskOwnerCap` is returned
    /// and that the Kiosk balance is not changed.
    struct Borrow { kiosk_id: ID, kiosk_profits: u64 }

    // === Events ===

    /// A new Kiosk has joined the Marketplace. Useful for discovery.
    struct KioskParticipated has store, copy, drop {
        kiosk_id: ID,
    }

    /// A Kiosk has left the Marketplace.
    struct KioskLeft has store, copy, drop {
        kiosk_id: ID,
    }

    // === In and Out ===

    /// Become a participant of the Marketplace, give up the `KioskOwnerCap`
    /// and get a `MarketplaceKioskCap` which gives access the `KioskOwnerCap`.
    public fun participate(
        self: &mut Marketplace,
        // TODO: see note #1
        kiosk: &mut Kiosk,
        kiosk_cap: KioskOwnerCap,
        ctx: &mut TxContext
    ): MarketplaceKioskCap {
        let kiosk_id = object::id(kiosk);
        assert!(kiosk::has_access(kiosk, &kiosk_cap), ENotOwner);
        df::add(&mut self.id, OwnerKey { kiosk_id }, kiosk_cap);
        event::emit(KioskParticipated { kiosk_id });

        MarketplaceKioskCap {
            kiosk_id,
            id: object::new(ctx),
        }
    }

    /// Leave the Marketplace, get back the `KioskOwnerCap` and pay the exit fee.
    public fun leave(
        self: &mut Marketplace,
        kiosk: &mut Kiosk,
        mkt_cap: MarketplaceKioskCap,
        ctx: &mut TxContext,
    ): (KioskOwnerCap, Coin<SUI>) {
        let MarketplaceKioskCap { id, kiosk_id } = mkt_cap;
        let kiosk_cap = df::remove(&mut self.id, OwnerKey { kiosk_id });

        event::emit(KioskLeft { kiosk_id });
        object::delete(id);

        let profits = kiosk::withdraw(kiosk, &kiosk_cap, none(), ctx);
        collect_fee(self, &mut profits);
        (kiosk_cap, profits)
    }

    // === Using the Kiosk ===

    /// Withdraw all profits from the `Kiosk` and take the markeptlace fee.
    public fun withdraw_profits(
        self: &mut Marketplace,
        kiosk: &mut Kiosk,
        mkt_cap: &MarketplaceKioskCap,
        ctx: &mut TxContext,
    ): Coin<SUI> {
        let kiosk_id = mkt_cap.kiosk_id;
        let kiosk_cap = df::borrow(&mut self.id, OwnerKey { kiosk_id });

        assert!(kiosk_id == object::id(kiosk), EWrongKiosk);
        assert!(kiosk::has_access(kiosk, kiosk_cap), ENotOwner);

        let profits = kiosk::withdraw(kiosk, kiosk_cap, none(), ctx);
        collect_fee(self, &mut profits);
        profits
    }

    /// Borrow the `KioskOwnerCap` from the Marketplace and give the user
    /// full access to their Kiosk with one limitation - they cannot `withdraw`
    /// the profits.
    public fun borrow_cap(
        self: &mut Marketplace,
        kiosk: &Kiosk,
        mkt_cap: &MarketplaceKioskCap,
    ): (KioskOwnerCap, Borrow) {
        let kiosk_id = object::id(kiosk);
        let kiosk_profits = kiosk::profits_amount(kiosk);
        assert!(kiosk_id == mkt_cap.kiosk_id, EWrongKiosk);

        (
            df::remove(&mut self.id, OwnerKey { kiosk_id }),
            Borrow { kiosk_profits, kiosk_id },
        )
    }

    /// Return the `KioskOwnerCap` to the Marketplace and make sure that the
    /// Kiosk balance is not changed.
    public fun return_cap(
        self: &mut Marketplace,
        kiosk: &Kiosk,
        kiosk_cap: KioskOwnerCap,
        borrow: Borrow,
    ) {
        let Borrow { kiosk_profits, kiosk_id } = borrow;
        assert!(kiosk::has_access(kiosk, &kiosk_cap), ENotOwner);
        assert!(kiosk_id == object::id(kiosk), EWrongKiosk);
        assert!(kiosk_profits == kiosk::profits_amount(kiosk), EAmountChanged);

        df::add(&mut self.id, OwnerKey { kiosk_id }, kiosk_cap);
    }

    // === Init + Display ===

    /// The OTW to claim the `Publisher`.
    struct KIOSK_CUSTODY_MARKETPLACE has drop {}

    /// Share the Markeptlace object and claim the `Publisher`.
    fun init(otw: KIOSK_CUSTODY_MARKETPLACE, ctx: &mut TxContext) {
        sui::package::claim_and_keep(otw, ctx);
        transfer::share_object(Marketplace {
            id: object::new(ctx),
            fee_bp: 1000, // 10%
            balance: balance::zero(),
        })
    }

    // === Utility methods ===

    /// Collect fees from the profits Coin and put them to the Marketplace balance.
    fun collect_fee(self: &mut Marketplace, profits: &mut Coin<SUI>) {
        // calculate the marketplace fee
        let amount = coin::value(profits);
        let fee_amt = (((amount as u128) * (self.fee_bp as u128) / 10_000) as u64);

        // leave the fee in the marketplace
        let fee = balance::split(coin::balance_mut(profits), fee_amt);
        balance::join(&mut self.balance, fee);
    }
}
