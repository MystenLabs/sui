// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// This is a draft implementation of the `CoinMetadataV2`.
///
/// Minor changes:
/// - no more Url type, replaced with ascii::String
module sui::coin_metadata {
    use std::ascii;
    use std::type_name;
    use std::string::String;

    use sui::bag::Bag;
    use sui::balance::Supply;
    use sui::coin::{CoinMetadata, TreasuryCap};

    #[allow(unused_const)]
    /// The ID of the `MetadataRegistry` object.
    const REGISTRY: address = @0xC014;

    /// Capability was already claimed, and cannot be claimed again.
    const ECapabilityClaimedAlready: u64 = 0;

    /// Capability that manages the `CoinMetadata<T>` for CoinMetadata V2.
    public struct CoinMetadataCap<phantom T> has key, store { id: UID }

    /// A single registry for `CoinMetadata` V2.
    public struct MetadataRegistry has key {
        id: UID,
        storage: Bag
    }

    /// Second version of the `CoinMetadata` object, stored in the `MetadataRegistry`.
    public struct CoinMetadataV2 has store {
        decimals: u8,
        /// Name for the token.
        name: String,
        /// Symbol for the token.
        symbol: ascii::String,
        /// Description of the token.
        description: String,
        /// URL for the token logo.
        icon_url: Option<ascii::String>,
        /// Whether the Metadata is frozen and cannot be changed. To achieve this,
        /// the `CoinMetadataCap` must be consumed in `freeze_metadata` call.
        is_frozen: bool,
        /// The ID of the `TreasuryCap` for simpler discovery. Is not possible in
        /// initialization via `Supply` due to `Supply` not being an object.
        treasury_cap: Option<ID>,
    }

    /// Registers an `owned` or `shared` metadata
    ///
    /// This function deconstructs the `CoinMetadata` and recreates it as an
    /// owned object.
    public fun register_metadata<T>(
        registry: &mut MetadataRegistry,
        treasury_cap: &mut TreasuryCap<T>, // mark the treasury cap
        metadata: CoinMetadata<T>, // assuming CoinMetadata is shared
        ctx: &mut TxContext,
    ): CoinMetadataCap<T> {
        let mut v2 = copy_from_v1(&metadata);
        sui::test_utils::destroy(metadata);

        v2.treasury_cap.fill(object::id(treasury_cap));
        registry.storage.add(type_name::get<T>(), v2);
        CoinMetadataCap { id: object::new(ctx) }
    }

    /// Permissionless method to register a new metadata object. This method is
    /// used to register metadata as V2 even if the current version was frozen.
    ///
    /// Once the metadata is registered, the owner of the `TreasuryCap` can come
    /// and claim the ownership of the metadata, and either keep it frozen or
    /// unfreeze it and receive the `CoinMetadataCap`.
    public fun copy_immutable_metadata<T>(
        registry: &mut MetadataRegistry,
        metadata: &CoinMetadata<T>,
        _ctx: &mut TxContext,
    ) {
        registry.storage.add(type_name::get<T>(), copy_from_v1(metadata));
    }

    /// Claims the metadata capability for a given `TreasuryCap`. Can only be called
    /// once per `T` and will fail if the capability was already claimed. Should be
    /// used in the scenario where the `CoinMetadata` was previously registered via
    /// the `copy_immutable_metadata` method.
    public fun claim_metadata<T>(
        registry: &mut MetadataRegistry,
        treasury_cap: &mut TreasuryCap<T>,
        ctx: &mut TxContext,
    ): CoinMetadataCap<T> {
        let metadata: &mut CoinMetadataV2 = &mut registry.storage[type_name::get<T>()];
        assert!(metadata.treasury_cap.is_none(), ECapabilityClaimedAlready);
        metadata.treasury_cap.fill(object::id(treasury_cap));
        metadata.is_frozen = false;

        CoinMetadataCap { id: object::new(ctx) }
    }

    /// Enables currencies without a `TreasuryCap` to claim the metadata capability.
    /// This is useful for currencies that do not have a `TreasuryCap` and want to
    /// register their metadata. Something that wasn't possible in V1.
    ///
    /// Unfortunatelly, due to limitations module initialization, this method cannot
    /// be used in the module's `init` function, and should be called in a follow-up
    /// transaction.
    ///
    /// The `treasury_cap` is set to `0x0` to indicate that there is no treasury
    /// cap and prevent double claiming of the `CoinMetadataCap`. Malicious
    /// behaviour, such as claiming the metadata with existing `TreasuryCap` will
    /// result in `treasury_cap` not being discovered.
    public fun new_metadata_for_supply<T>(
        registry: &mut MetadataRegistry,
        _supply: &mut Supply<T>,
        decimals: u8,
        name: String,
        symbol: ascii::String,
        description: String,
        icon_url: Option<ascii::String>,
        ctx: &mut TxContext,
    ): CoinMetadataCap<T> {
        let metadata = CoinMetadataV2 {
            decimals,
            name,
            symbol,
            description,
            icon_url,
            is_frozen: false,
            treasury_cap: option::some(@0x0.to_id()),
        };

        registry.storage.add(type_name::get<T>(), metadata);
        CoinMetadataCap { id: object::new(ctx) }
    }

    /// Consume the `CoinMetadataCap` and effectively freeze the metadata.
    public fun freeze_metadata<T>(
        registry: &mut MetadataRegistry,
        cap: CoinMetadataCap<T>,
        _ctx: &mut TxContext,
    ) {
        let CoinMetadataCap { id } = cap;
        let metadata: &mut CoinMetadataV2 = &mut registry.storage[type_name::get<T>()];
        metadata.is_frozen = true;
        id.delete()
    }

    fun copy_from_v1<T>(metadata: &CoinMetadata<T>): CoinMetadataV2 {
        let (decimals, name, symbol, description, icon_url) = (
            metadata.get_decimals(),
            metadata.get_name(),
            metadata.get_symbol(),
            metadata.get_description(),
            metadata.get_icon_url(),
        );

        let icon_url = if (icon_url.is_some()) {
            option::some(icon_url.destroy_some().inner_url())
        } else {
            option::none()
        };

        CoinMetadataV2 {
            decimals,
            name,
            symbol,
            description,
            icon_url,
            is_frozen: false,
            treasury_cap: option::none(),
        }
    }
}
