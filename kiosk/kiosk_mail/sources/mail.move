// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This module adds a `mail` module for kiosk.
/// 
/// This module allows creators to send kiosk-locked items to other users
/// directly, without prior transaction from a user.
/// 
/// The assurances here are that the item wrapped in the Mail object 
/// will be locked in a (personal or not) kiosk.
/// 
/// For the users receiving the mail, they can decide to either
/// 1. Lock it in their kiosk
/// 2. Return it back to the sender, getting the storage rebates themselves (good for spam mail).
/// 
/// We've also introduced versioning, however old versions won't stop functioning.
/// If we bump the version, new Mail objects will only be openable by the latest version.
/// Creators can choose which version they want to use, however older Mail objects can still be opened
/// by the latest version.
/// 
module kiosk_mail::mail {

    use sui::object::{UID, Self};
    use sui::tx_context::{Self, TxContext};
    use sui::package::{Self, Publisher};
    use sui::display::{Self, Display};
    use sui::transfer;
    use sui::transfer_policy::TransferPolicy;
    use sui::kiosk::{Self, Kiosk, KioskOwnerCap};

    use kiosk::personal_kiosk;

    /// Versioning
    /// That allows us to add different logic for future Mails (e.g. different rules)
    const CURRENT_VERSION: u8 = 0;

    /// Not a valid owner of the publisher object.
    const EInvalidPublisher: u64 = 1;
    /// Tries to claim a personal kiosk bound mail to a non-personal kiosk.
    const ENotPersonalKiosk: u64 = 2;
    /// Tries to claim a mail with an invalid version for the module.
    const EInvalidVersion: u64 = 3;

    /// The registry that holds the `publisher` object
    /// of the mail service, in order to create `Mail<T>` Display objects!
    struct MailRegistry has key {
        id: UID,
        publisher: Publisher
    }

    /// Base struct that holds the `item` that is being sent.
    struct Mail<T: key + store> has key {
        id: UID,
        requires_personal_kiosk: bool,
        sender: address,
        version: u8,
        item: T
    }

    /// OTW to claim publisher
    struct MAIL has drop {}

    #[lint_allow(freeze_wrapped)]
    /// Claim publisher object in the `MailboxRegistry`.
    /// Using this, we can create `Display` for `Mail<T>` as the publisher of `T`.
    /// It converts that to a frozen object so we can use in fast path.
    fun init(otw: MAIL, ctx: &mut TxContext) {
        let publisher = package::claim(otw, ctx);

        let registry = MailRegistry {
            id: object::new(ctx),
            publisher
        };

        transfer::freeze_object(registry);
    }

    /// 
    /// Create a Mail<T> Display
    /// 
    /// This allows a creator to: 
    /// 1. Mimic the internal type's Display, by referencing the wrapped item.
    /// 
    /// For instance, if a `T`'s field is "{image_url}", we'd use "{item.image_url}" for Mail<T>.
    /// 
    /// 2. Create a unique Display when we're using the mailing system for our objects.
    ///    We could do multiple things with this:
    ///    e.g. We could create different visuals, by adding params in our Mail<T> display (`{item.image_url}?in_mail=true`)
    /// 
    public fun create_display<T: key + store>(
        registry: &MailRegistry,
        publisher: &Publisher,
        ctx: &mut TxContext
    ): Display<Mail<T>> {
        assert!(package::from_module<T>(publisher), EInvalidPublisher);
        display::new<Mail<T>>(&registry.publisher, ctx)
    }

    /// Send a `Mail<T>` to a `target` address.
    public fun send<T: key + store>(
        item: T,
        _: &TransferPolicy<T>,
        requires_personal_kiosk: bool,
        target: address,
        ctx: &mut TxContext
    ) {
        let mail = Mail {
            id: object::new(ctx),
            requires_personal_kiosk,
            sender: tx_context::sender(ctx),
            item: item,
            version: CURRENT_VERSION
        };

        transfer::transfer(mail, target);
    }

    /// Claim a `T` out of a `Mail<T>` object
    public fun claim_direct<T: key + store>(
        mail: Mail<T>,
        kiosk: &mut Kiosk,
        cap: &KioskOwnerCap,
        policy: &TransferPolicy<T>,
        _ctx: &mut TxContext
    ) {
        assert_is_valid_version(&mail);
        let Mail {
            id, 
            requires_personal_kiosk,
            sender: _,
            version: _,
            item,
        } = mail;

        if (requires_personal_kiosk) {
            assert!(personal_kiosk::is_personal(kiosk), ENotPersonalKiosk);
        };

        object::delete(id);
        kiosk::lock(kiosk, cap, policy, item);
    }

    /// Returns `T` back to the sender of the `Mail<T>` object
    /// and deletes the Mail box. Good to get storage rebates 
    /// when handling spam mail.
    /// 
    /// We can safely return `T` without any kiosk-related operations,
    /// considering that the sender already had access to the item by value.
    public fun return_to_sender<T: key + store>(
        mail: Mail<T>,
        _ctx: &mut TxContext
    ) {
        assert_is_valid_version(&mail);

        let Mail {
            id, 
            requires_personal_kiosk: _,
            sender,
            version: _,
            item
        } = mail;

        object::delete(id);
        transfer::public_transfer(item, sender);
    }

    /// Check that the mail we're working with has the correct version.
    fun assert_is_valid_version<T: key + store>(mail: &Mail<T>) {
        assert!(CURRENT_VERSION >= mail.version, EInvalidVersion);
    }

    #[test_only]
    public fun init_for_testing<T: drop>(fake_otw: T, ctx: &mut TxContext) {
      let publisher = package::test_claim(fake_otw, ctx);

        let registry = MailRegistry {
            id: object::new(ctx),
            publisher: publisher
        };

        transfer::freeze_object(registry);
    }
}
