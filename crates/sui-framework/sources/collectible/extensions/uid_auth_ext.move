// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0


// TransferRequst { 0 }
// Commission { 1 } (price)
// RoyaltyPolicy { 2 } (price)
// Allowlist { 3 } (UID from extension)
// FinalApproval(stored: 3) => destroy the potato


// Kiosk(UID1, UID2, UID3) Item X
// ------
// Auction with UID1 Slow - nothing
// Auction with UID2 Winner -> takes X from Kiosk by UID
// Auction with UID3 Slow - nothing


module sui::entity_auth_extension {
    use sui::kiosk::{Self, Kiosk, KioskOwnerCap, TransferRequest, PurchaseCap};

    /// A custom key for the entity auth extension.
    struct EntityListing has store { entity: ID }

    /// A custom constraint that this extension registers in the Kiosk.
    struct Constraint<T: key + store> has store { entity: ID }

    /// A custom key to store extension constraints.
    /// If a Kiosk has this extension installed, it will be able to create
    /// `TransferRequest` with "constraints" attached.
    struct ExtensionConstraintKey<T: store> {}

    // public fun install<T: key + store>(
    //     self: &mut Kiosk,
    //     cap: &KioskOwnerCap,
    // ) {
    //     // make it a special call within the Kiosk so that only `creator` has access to
    //     df::add(kiosk::uid_mut(self), ExtensionConstraintKey<Constraint> {}, true);
    // }

    // public fun uninstall<T: key + store>(
    //     self: &mut Kiosk,
    //     cap: &KioskOwnerCap
    // ) {}

    public fun list<T: key + store>(
        self: &mut Kiosk,
        cap: &KioskOwnerCap,
        item_id: ID,
        entity_id: ID,
        min_amount: u64,
        ctx: &mut TxContext
    ) {
        df::add(
            kiosk::uid_mut(self),
            EntityListing { entity },
            kiosk::create_purchase_cap(self, cap, id, min_amount)
        )
    }

    /// Purchase an item from the `Kiosk` by performing an object authorization
    /// via the UID and passing in the payment.
    public fun purchase<T: key + store>(
        self: &mut Kiosk,
        entity: &UID,
        payment: Coin<SUI>
    ): (T, TransferRequest<T>) {
        let entity = object::uid_to_inner(&entity);
        let purchase_cap: PurchaseCap<T> = df::remove(
            kiosk::uid_mut(self),
            EntityListing { entity }
        );

        let (item, transfer_request) = kiosk::purchase_with_cap(self, purchase_cap, payment);
        kiosk::add_constraint(&mut transfer_request, Constraint { entity });
        (item, transfer_request)
    }

    public fun allow_entity_transfer<T: key + store>(cap: &TransferCap, req: EntityPurchaseRequest<T>): (ID, TransferRequest<T>) {
        let EntityPurchaseRequest { entity, request } = req;
        (entity, request)
    }

    public fun resolve_constraint<T: key + store>(cap: &TransferPolicyCap, constraint: Constraint): ID {
        let Constraint { entity } = constraint;
        entity
    }
}

#[test_only]
/// Conversation notes:
/// UID entity functionality
/// Rolling potato? Track number of passes of an object
/// No transfer caps - gas, multiple UIDs
///
module sui::kiosk_entity_list_ext {
    use sui::kiosk::{Self, Kiosk, KioskOwnerCap, TransferRequest, PurchaseCap};
    use sui::tx_context::TxContext;

    /// A custom key for the entity listing extension.
    struct EntityListing has store { entity: ID }

    /// List an item with `item_id` for a specific `entity`.
    /// Dummy Dam.
    /// Define custom listing option
    public fun list_for_entity<T: key + store>(
        self: &mut Kiosk,
        cap: &KioskOwnerCap,
        item_id: ID,
        entity: ID,
        ctx: &mut TxContext
    ) {
        let purchase_cap = kiosk::create_purchase_cap(self, cap, id, 1000);
        df::add(kiosk::uid_mut(self), EntityListing { entity }, purchase_cap);
    }

    /// Purchase an item as an entity by authorizing with a `UID`.
    /// Auction / Third party trading smth.
    /// Define purchase call
    public fun grab_as_entity<T: key + store>(self: &mut Kiosk, entity: &UID, coin: Coin<SUI>): (T, TransferRequest<T>) {
        let purchase_cap: PurchaseCap<T> = df::remove(kiosk::uid_mut(self), EntityListing { entity: object::uid_to_inner(&entity) });
        kiosk::purchase_with_cap(self, purchase_cap, coin)
    }

    /// Creator / Publisher.
    public fun allow_entity_transfer<T: key + store>(cap: &TransferCap, req: EntityPurchaseRequest<T>): (ID, TransferRequest<T>) {
        let EntityPurchaseRequest { entity, request } = req;
        (entity, request)
    }

    /// Define the policy rules
    public fun set_entity_policy<T: key + store>(cap: &TransferPolicyCap): EntityPolicy {

    }
}

// // example of an extension function in JS
// function entityExtension(item: string, payment: ObjectRef): [Transaction, TxInput] {
//    let tx = new Transaction();
//    let [ item, entity_req ] = tx.moveCall({ target: 0x2::kiosk_ext::entity ... });
//    // EXTENSIONS
//    let [ transfer_request ] = tx.moveCall({ ..... }, [ entity_req ]);
//    return [tx, transfer_request];
// }

//let tx = new Transaction();
//let [ item, entity_req ] = tx.moveCall({ target: 0x2::kiosk_ext::entity ... });

// EXTENSIONS
//let [ transfer_request ] = tx.moveCall({ ..... }, [ entity_req ]);

// discover royalty query
//
//tx.moveCall({ target: 0x2::royalty::pay, input: [ RoyaltyPolicy, TransferRequest ] });
//tx.transferObjects([item], 0x2);


#[test_only]
module sui::kiosk_transfer_caps_ext {
    use sui::dynamic_field as df;
    use sui::tx_context::TxContext;
    use sui::object::{Self, UID, ID};
    use sui::kiosk::{Self, Kiosk, KioskOwnerCap, PurchaseCap, TransferRequest};
    use sui::sui::SUI;
    use sui::coin;

    const ENotOwner: u64 = 0;
    const ENotExists: u64 = 1;
    const EWrongKiosk: u64 = 2;

    /// A transfer Cap which allows the bearer to access the
    struct TransferCap<phantom T: key + store> has key, store {
        id: UID,
        for: ID,
        kiosk_id: ID
    }

    /// Custom key to wrap this logic.
    struct CapKey has copy, store, drop { for: ID }

    /// Issue a `TransferCap` backed by the `PurchaseCap` with `min_price` set to `0`.
    public fun issue_transfer_cap<T: key + store>(
        self: &mut Kiosk, for: ID, cap: &KioskOwnerCap, ctx: &mut TxContext
    ): TransferCap<T> {
        assert!(kiosk::check_access(self, cap), ENotOwner);

        if (!df::exists_<CapKey>(kiosk::uid_mut(self), CapKey { for })) {
            let purchase_cap = kiosk::list_with_purchase_cap<T>(self, cap, for, 0, ctx);
            df::add(kiosk::uid_mut(self), CapKey { for }, purchase_cap);
        };

        TransferCap {
            id: object::new(ctx),
            kiosk_id: object::id(self),
            for
        }
    }

    public fun claim<T: key + store>(self: &mut Kiosk, transfer_cap: TransferCap<T>, ctx: &mut TxContext): (T, TransferRequest<T>) {
        let TransferCap { id, for, kiosk_id } = transfer_cap;
        let uid_mut = kiosk::uid_mut(self);
        object::delete(id);

        assert!(df::exists_<CapKey>(uid_mut, CapKey { for }), ENotExists);
        assert!(object::uid_to_inner(uid_mut) == kiosk_id, EWrongKiosk);

        let purchase_cap = df::remove<CapKey, PurchaseCap<T>>(uid_mut, CapKey { for });
        kiosk::purchase_with_cap(self, purchase_cap, coin::zero<SUI>(ctx))
    }

    /// Invalidate `TransferCap`s and make them unusable
    public fun invalidate_caps<T: key + store>(self: &mut Kiosk, for: ID, cap: &KioskOwnerCap) {
        assert!(kiosk::check_access(self, cap), ENotOwner);

        let uid_mut = kiosk::uid_mut(self);
        assert!(df::exists_<CapKey>(uid_mut, CapKey { for }), ENotExists);

        let purchase_cap = df::remove<CapKey, PurchaseCap<T>>(uid_mut, CapKey { for });
        kiosk::return_purchase_cap<T>(self, purchase_cap);
    }
}
