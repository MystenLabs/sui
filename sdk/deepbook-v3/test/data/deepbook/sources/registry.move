// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Registry holds all created pools.
module deepbook::registry {
    // === Imports ===
    use std::type_name::{Self, TypeName};
    use sui::{bag::{Self, Bag}, versioned::{Self, Versioned}, vec_set::{Self, VecSet}};
    use deepbook::constants;

    // === Errors ===
    const EPoolAlreadyExists: u64 = 1;
    const EPoolDoesNotExist: u64 = 2;
    const EPackageVersionDisabled: u64 = 3;
    const EVersionNotDisabled: u64 = 4;
    const EVersionAlreadyDisabled: u64 = 5;
    const ECannotDisableCurrentVersion: u64 = 6;

    public struct REGISTRY has drop {}

    // === Structs ===
    /// DeepbookAdminCap is used to call admin functions.
    public struct DeepbookAdminCap has key, store {
        id: UID,
    }

    public struct Registry has key {
        id: UID,
        inner: Versioned,
    }

    public struct RegistryInner has store {
        disabled_versions: VecSet<u64>,
        pools: Bag,
        treasury_address: address,
    }

    public struct PoolKey has copy, drop, store {
        base: TypeName,
        quote: TypeName,
    }

    fun init(_: REGISTRY, ctx: &mut TxContext) {
        let registry_inner = RegistryInner {
            disabled_versions: vec_set::empty(),
            pools: bag::new(ctx),
            treasury_address: ctx.sender(),
        };
        let registry = Registry {
            id: object::new(ctx),
            inner: versioned::create(constants::current_version(), registry_inner, ctx),
        };
        transfer::share_object(registry);
        let admin = DeepbookAdminCap { id: object::new(ctx) };
        transfer::public_transfer(admin, ctx.sender());
    }

    // === Public Admin Functions ===
    /// Sets the treasury address where the pool creation fees are sent
    /// By default, the treasury address is the publisher of the deepbook package
    public fun set_treasury_address(
        self: &mut Registry,
        treasury_address: address,
        _cap: &DeepbookAdminCap,
    ) {
        let self = self.load_inner_mut();
        self.treasury_address = treasury_address;
    }

    /// Disables a package version
    /// Only Admin can disable a package version
    public fun disable_version(self: &mut Registry, version: u64, _cap: &DeepbookAdminCap) {
        let self = self.load_inner_mut();
        assert!(!self.disabled_versions.contains(&version), EVersionAlreadyDisabled);
        assert!(version != constants::current_version(), ECannotDisableCurrentVersion);
        self.disabled_versions.insert(version);
    }

    /// Enables a package version
    /// Only Admin can enable a package version
    public fun enable_version(self: &mut Registry, version: u64, _cap: &DeepbookAdminCap) {
        let self = self.load_inner_mut();
        assert!(self.disabled_versions.contains(&version), EVersionNotDisabled);
        self.disabled_versions.remove(&version);
    }

    // === Public-Package Functions ===
    public(package) fun load_inner_mut(self: &mut Registry): &mut RegistryInner {
        let inner: &mut RegistryInner = self.inner.load_value_mut();
        let package_version = constants::current_version();
        assert!(!inner.disabled_versions.contains(&package_version), EPackageVersionDisabled);

        inner
    }

    /// Register a new pool in the registry.
    /// Asserts if (Base, Quote) pool already exists or (Quote, Base) pool already exists.
    public(package) fun register_pool<BaseAsset, QuoteAsset>(self: &mut Registry, pool_id: ID) {
        let self = self.load_inner_mut();
        let key = PoolKey {
            base: type_name::get<QuoteAsset>(),
            quote: type_name::get<BaseAsset>(),
        };
        assert!(!self.pools.contains(key), EPoolAlreadyExists);

        let key = PoolKey {
            base: type_name::get<BaseAsset>(),
            quote: type_name::get<QuoteAsset>(),
        };
        assert!(!self.pools.contains(key), EPoolAlreadyExists);

        self.pools.add(key, pool_id);
    }

    /// Only admin can call this function
    public(package) fun unregister_pool<BaseAsset, QuoteAsset>(self: &mut Registry) {
        let self = self.load_inner_mut();
        let key = PoolKey {
            base: type_name::get<BaseAsset>(),
            quote: type_name::get<QuoteAsset>(),
        };
        assert!(self.pools.contains(key), EPoolDoesNotExist);
        self.pools.remove<PoolKey, ID>(key);
    }

    public(package) fun load_inner(self: &Registry): &RegistryInner {
        let inner: &RegistryInner = self.inner.load_value();
        let package_version = constants::current_version();
        assert!(!inner.disabled_versions.contains(&package_version), EPackageVersionDisabled);

        inner
    }

    /// Get the pool id for the given base and quote assets.
    public(package) fun get_pool_id<BaseAsset, QuoteAsset>(self: &Registry): ID {
        let self = self.load_inner();
        let key = PoolKey {
            base: type_name::get<BaseAsset>(),
            quote: type_name::get<QuoteAsset>(),
        };
        assert!(self.pools.contains(key), EPoolDoesNotExist);

        *self.pools.borrow<PoolKey, ID>(key)
    }

    /// Get the treasury address
    public(package) fun treasury_address(self: &Registry): address {
        let self = self.load_inner();
        self.treasury_address
    }

    public(package) fun get_disabled_versions(self: &Registry): VecSet<u64> {
        let self = self.load_inner();

        self.disabled_versions
    }

    // === Test Functions ===
    #[test_only]
    public fun test_registry(ctx: &mut TxContext): ID {
        let registry_inner = RegistryInner {
            disabled_versions: vec_set::empty(),
            pools: bag::new(ctx),
            treasury_address: ctx.sender(),
        };
        let registry = Registry {
            id: object::new(ctx),
            inner: versioned::create(constants::current_version(), registry_inner, ctx),
        };
        let id = object::id(&registry);
        transfer::share_object(registry);

        id
    }

    #[test_only]
    public fun get_admin_cap_for_testing(ctx: &mut TxContext): DeepbookAdminCap {
        DeepbookAdminCap { id: object::new(ctx) }
    }
}
