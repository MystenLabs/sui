// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::versioned_type {
    use sui::object::{UID, ID};
    use sui::tx_context::TxContext;
    use sui::object;
    use sui::dynamic_field;

    const EInvalidUpgrade: u64 = 0;

    /// A wrapper type that supports versioning of the inner type.
    /// The inner type is a dynamic field of the Versioned object, and is keyed using version.
    struct Versioned has key, store {
        id: UID,
        version: u64,
    }

    /// Represents a hot potato object generated when we take out the dynamic field.
    /// This is to make sure that we always put a new value back.
    struct VersionChangeCap {
        versioned_id: ID,
        old_version: u64,
    }

    public fun create<T: store>(init_version: u64, init_value: T, ctx: &mut TxContext): Versioned {
        let self = Versioned {
            id: object::new(ctx),
            version: init_version,
        };
        dynamic_field::add(&mut self.id, init_version, init_value);
        self
    }

    public fun version(self: &Versioned): u64 {
        self.version
    }

    public fun load_value<T: store>(self: &Versioned): &T {
        dynamic_field::borrow(&self.id, self.version)
    }

    public fun load_value_mut<T: store>(self: &mut Versioned): &mut T {
        dynamic_field::borrow_mut(&mut self.id, self.version)
    }

    public fun remove_value<T: store>(self: &mut Versioned): (T, VersionChangeCap) {
        (
            dynamic_field::remove(&mut self.id, self.version),
            VersionChangeCap {
                versioned_id: object::id(self),
                old_version: self.version,
            }
        )
    }

    public fun add_value<T: store>(self: &mut Versioned, new_version: u64, new_value: T, cap: VersionChangeCap) {
        let VersionChangeCap { versioned_id, old_version } = cap;
        assert!(versioned_id == object::id(self), EInvalidUpgrade);
        assert!(old_version != new_version, EInvalidUpgrade);
        dynamic_field::add(&mut self.id, new_version, new_value);
    }
}
