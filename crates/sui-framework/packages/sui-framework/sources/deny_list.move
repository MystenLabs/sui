// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Defines the `DenyList` type. The `DenyList` shared object is used to restrict access to
/// instances of certain core types from being used as inputs by specified addresses in the deny
/// list.
module sui::deny_list {
    use sui::config::{Self, Config};
    use sui::dynamic_field as field;
    use sui::table::{Self, Table};
    use sui::bag::{Self, Bag};
    use sui::vec_set::{Self, VecSet};

    /// Trying to create a deny list object when not called by the system address.
    const ENotSystemAddress: u64 = 0;
    /// The specified address to be removed is not already in the deny list.
    const ENotDenied: u64 = 1;
    /// The specified address cannot be added to the deny list.
    const EInvalidAddress: u64 = 1;

    /// The index into the deny list vector for the `sui::coin::Coin` type.
    const COIN_INDEX: u64 = 0;

    /// These addresses are reserved and cannot be added to the deny list.
    /// The addresses listed are well known package and object addresses. So it would be
    /// meaningless to add them to the deny list.
    const RESERVED: vector<address> = vector[
        @0x0,
        @0x1,
        @0x2,
        @0x3,
        @0x4,
        @0x5,
        @0x6,
        @0x7,
        @0x8,
        @0x9,
        @0xA,
        @0xB,
        @0xC,
        @0xD,
        @0xE,
        @0xF,
        @0x403,
        @0xDEE9,
    ];

    /// A shared object that stores the addresses that are blocked for a given core type.
    public struct DenyList has key {
        id: UID,
        /// The individual deny lists.
        lists: Bag,
    }


    // === V2 ===

    public struct ConfigWriteCap() has drop;

    public struct ConfigKey has copy, drop, store {
        per_type_index: u64,
    }

    const DL_V2_MARKER: vector<u8> = b"::marker";
    const DL_V2_ADDRESSES: vector<u8> = b"::address::";
    const DL_V2_GLOBAL_PAUSE: vector<u8> = b"::global_pause";

    public(package) fun v2_add(
        deny_list: &mut DenyList,
        per_type_index: u64,
        ty: vector<u8>,
        addr: address,
        ctx: &mut TxContext,
    ) {
        let per_type_config = deny_list.borrow_per_type_config_mut(per_type_index);
        maybe_create_deny_list_v2_marker(per_type_config, ty, ctx);
        let setting_name = deny_list_v2_address_setting_name(ty, addr);
        let next_epoch_entry = per_type_config.entry!<_, vector<u8>, bool>(
            &mut ConfigWriteCap(),
            setting_name,
            |_deny_list, _cap, _ctx| true,
            ctx,
        );
        *next_epoch_entry = true;
    }

    public(package) fun v2_remove(
        deny_list: &mut DenyList,
        per_type_index: u64,
        ty: vector<u8>,
        addr: address,
        ctx: &mut TxContext,
    ) {
        let per_type_config = deny_list.borrow_per_type_config_mut(per_type_index);
        maybe_create_deny_list_v2_marker(per_type_config, ty, ctx);
        let setting_name = deny_list_v2_address_setting_name(ty, addr);
        let next_epoch_entry = per_type_config.entry!<_, vector<u8>, bool>(
            &mut ConfigWriteCap(),
            setting_name,
            |_deny_list, _cap, _ctx| false,
            ctx,
        );
        *next_epoch_entry = false;
    }

    public(package) fun v2_most_recent_contains(
        deny_list: &DenyList,
        per_type_index: u64,
        ty: vector<u8>,
        addr: address,
        _ctx: &TxContext,
    ): bool {
        let per_type_config = deny_list.borrow_per_type_config(per_type_index);
        let setting_name = deny_list_v2_address_setting_name(ty, addr);
        if (!per_type_config.exists_with_type<_, _, bool>(setting_name)) return false;
        *per_type_config.borrow_most_recent(setting_name)
    }

    // public(package) fun v2_per_type_contains(
    //     per_type_index: u64,
    //     ty: vector<u8>,
    //     addr: address,
    // ): bool {
    //    // TODO can read from the config directly once the ID is set
    // }

    public(package) fun v2_enable_global_pause(
        deny_list: &mut DenyList,
        per_type_index: u64,
        ty: vector<u8>,
        ctx: &mut TxContext,
    ) {
        let per_type_config = deny_list.borrow_per_type_config_mut(per_type_index);
        maybe_create_deny_list_v2_marker(per_type_config, ty, ctx);
        let setting_name = deny_list_v2_global_pause_setting_name(ty);
        let next_epoch_entry = per_type_config.entry!<_, vector<u8>, bool>(
            &mut ConfigWriteCap(),
            setting_name,
            |_deny_list, _cap, _ctx| true,
            ctx,
        );
        *next_epoch_entry = true;
    }

    public(package) fun v2_disable_global_pause(
        deny_list: &mut DenyList,
        per_type_index: u64,
        ty: vector<u8>,
        ctx: &mut TxContext,
    ) {
        let per_type_config = deny_list.borrow_per_type_config_mut(per_type_index);
        maybe_create_deny_list_v2_marker(per_type_config, ty, ctx);
        let setting_name = deny_list_v2_global_pause_setting_name(ty);
        let next_epoch_entry = per_type_config.entry!<_, vector<u8>, bool>(
            &mut ConfigWriteCap(),
            setting_name,
            |_deny_list, _cap, _ctx| false,
            ctx,
        );
        *next_epoch_entry = false;
    }

    public(package) fun v2_most_recent_is_global_pause_enabled(
        deny_list: &DenyList,
        per_type_index: u64,
        ty: vector<u8>,
        _ctx: &TxContext,
    ): bool {
        let per_type_config = deny_list.borrow_per_type_config(per_type_index);
        let setting_name = deny_list_v2_global_pause_setting_name(ty);
        if (!per_type_config.exists_with_type<_, _, bool>(setting_name)) return false;
        *per_type_config.borrow_most_recent(setting_name)
    }

    // public(package) fun v2_per_type_is_global_pause_enabled(
    //     per_type_index: u64,
    //     ty: vector<u8>,
    // ): bool {
    //    // TODO can read from the config directly once the ID is set
    // }

    fun maybe_create_deny_list_v2_marker(
        per_type_config: &mut Config<ConfigWriteCap>,
        ty: vector<u8>,
        ctx: &mut TxContext,
    ) {
        let setting_name = deny_list_v2_marker_setting_name(ty);
        if (per_type_config.exists_with_type<_, vector<u8>, bool>(setting_name)) return;
        let cap = &mut ConfigWriteCap();
        per_type_config.new_for_epoch<_, vector<u8>, bool>(cap, setting_name, true, ctx);
    }

    // b"{type}::marker"
    fun deny_list_v2_marker_setting_name(ty: vector<u8>): vector<u8> {
        let mut setting_name = ty;
        setting_name.append(DL_V2_MARKER);
        setting_name
    }

    // b"{type}::address::{bcs_bytes(index)}"
    fun deny_list_v2_address_setting_name(ty: vector<u8>, addr: address): vector<u8> {
        let mut setting_name = ty;
        setting_name.append(DL_V2_ADDRESSES);
        setting_name.append(sui::hex::encode(sui::address::to_bytes(addr)));
        setting_name
    }


    // b"{type}::global_pause"
    fun deny_list_v2_global_pause_setting_name(ty: vector<u8>): vector<u8> {
        let mut setting_name = ty;
        setting_name.append(DL_V2_GLOBAL_PAUSE);
        setting_name
    }

    public(package) fun add_per_type_config(
        deny_list: &mut DenyList,
        per_type_index: u64,
        ctx: &mut TxContext,
    ) {
        let config = config::new(&mut ConfigWriteCap(), ctx);
        let key = ConfigKey { per_type_index };
        let id = object::id(&config);
        field::add(&mut deny_list.id, key, id);
        let (field, _) = field::field_info<ConfigKey>(&deny_list.id, key);
        field::add_child_object(field.to_address(), config);
    }

    public(package) fun borrow_per_type_config(
        deny_list: &DenyList,
        per_type_index: u64,
    ): &Config<ConfigWriteCap> {
        let key = ConfigKey { per_type_index };
        let (field, value_id) = field::field_info<ConfigKey>(&deny_list.id, key);
        field::borrow_child_object<Config<ConfigWriteCap>>(field, value_id)
    }

    public(package) fun borrow_per_type_config_mut(
        deny_list: &mut DenyList,
        per_type_index: u64,
    ): &mut Config<ConfigWriteCap> {
        let key = ConfigKey { per_type_index };
        let (field, value_id) = field::field_info_mut<ConfigKey>(&mut deny_list.id, key);
        field::borrow_child_object_mut<Config<ConfigWriteCap>>(field, value_id)
    }

    // === V1 ===

    /// Stores the addresses that are denied for a given core type.
    public struct PerTypeList has key, store {
        id: UID,
        /// Number of object types that have been banned for a given address.
        /// Used to quickly skip checks for most addresses.
        denied_count: Table<address, u64>,
        /// Set of addresses that are banned for a given type.
        /// For example with `sui::coin::Coin`: If addresses A and B are banned from using
        /// "0...0123::my_coin::MY_COIN", this will be "0...0123::my_coin::MY_COIN" -> {A, B}.
        denied_addresses: Table<vector<u8>, VecSet<address>>,
    }

    /// Adds the given address to the deny list of the specified type, preventing it
    /// from interacting with instances of that type as an input to a transaction. For coins,
    /// the type specified is the type of the coin, not the coin type itself. For example,
    /// "00...0123::my_coin::MY_COIN" would be the type, not "00...02::coin::Coin".
    public(package) fun v1_add(
        deny_list: &mut DenyList,
        per_type_index: u64,
        `type`: vector<u8>,
        addr: address,
    ) {
        let reserved = RESERVED;
        assert!(!reserved.contains(&addr), EInvalidAddress);
        let bag_entry: &mut PerTypeList = &mut deny_list.lists[per_type_index];
        bag_entry.v1_per_type_list_add(`type`, addr)
    }

    fun v1_per_type_list_add(
        list: &mut PerTypeList,
        `type`: vector<u8>,
        addr: address,
    ) {
        if (!list.denied_addresses.contains(`type`)) {
            list.denied_addresses.add(`type`, vec_set::empty());
        };
        let denied_addresses = &mut list.denied_addresses[`type`];
        let already_denied = denied_addresses.contains(&addr);
        if (already_denied) return;

        denied_addresses.insert(addr);
        if (!list.denied_count.contains(addr)) {
            list.denied_count.add(addr, 0);
        };
        let denied_count = &mut list.denied_count[addr];
        *denied_count = *denied_count + 1;
    }

    /// Removes a previously denied address from the list.
    /// Aborts with `ENotDenied` if the address is not on the list.
    public(package) fun v1_remove(
        deny_list: &mut DenyList,
        per_type_index: u64,
        `type`: vector<u8>,
        addr: address,
    ) {
        let reserved = RESERVED;
        assert!(!reserved.contains(&addr), EInvalidAddress);
        let bag_entry: &mut PerTypeList = &mut deny_list.lists[per_type_index];
        bag_entry.v1_per_type_list_remove(`type`, addr)
    }

    fun v1_per_type_list_remove(
        list: &mut PerTypeList,
        `type`: vector<u8>,
        addr: address,
    ) {
        let denied_addresses = &mut list.denied_addresses[`type`];
        assert!(denied_addresses.contains(&addr), ENotDenied);
        denied_addresses.remove(&addr);
        let denied_count = &mut list.denied_count[addr];
        *denied_count = *denied_count - 1;
        if (*denied_count == 0) {
            list.denied_count.remove(addr);
        }
    }

    /// Returns true iff the given address is denied for the given type.
    public(package) fun v1_contains(
        deny_list: &DenyList,
        per_type_index: u64,
        `type`: vector<u8>,
        addr: address,
    ): bool {
        let reserved = RESERVED;
        if (reserved.contains(&addr)) return false;
        let bag_entry: &PerTypeList = &deny_list.lists[per_type_index];
        bag_entry.v1_per_type_list_contains(`type`, addr)
    }

    fun v1_per_type_list_contains(
        list: &PerTypeList,
        `type`: vector<u8>,
        addr: address,
    ): bool {
        if (!list.denied_count.contains(addr)) return false;

        let denied_count = &list.denied_count[addr];
        if (*denied_count == 0) return false;

        if (!list.denied_addresses.contains(`type`)) return false;

        let denied_addresses = &list.denied_addresses[`type`];
        denied_addresses.contains(&addr)
    }

    #[allow(unused_function)]
    /// Creation of the deny list object is restricted to the system address
    /// via a system transaction.
    fun create(ctx: &mut TxContext) {
        assert!(ctx.sender() == @0x0, ENotSystemAddress);

        let mut lists = bag::new(ctx);
        lists.add(COIN_INDEX, per_type_list(ctx));
        let deny_list_object = DenyList {
            id: object::sui_deny_list_object_id(),
            lists,
        };
        transfer::share_object(deny_list_object);
    }

    fun per_type_list(ctx: &mut TxContext): PerTypeList {
        PerTypeList {
            id: object::new(ctx),
            denied_count: table::new(ctx),
            denied_addresses: table::new(ctx),
        }
    }

    #[test_only]
    public fun reserved_addresses(): vector<address> {
        RESERVED
    }

    #[test_only]
    public fun create_for_test(ctx: &mut TxContext) {
        create(ctx);
    }

    #[test_only]
    /// Creates and returns a new DenyList object for testing purposes. It
    /// doesn't matter which object ID the list has in this kind of test.
    public fun new_for_testing(ctx: &mut TxContext): DenyList {
        let mut lists = bag::new(ctx);
        lists.add(COIN_INDEX, per_type_list(ctx));
        DenyList {
            id: object::new(ctx),
            lists,
        }
    }
}
