// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context as _, anyhow};
use async_graphql::{Context, Enum, Object, SimpleObject, connection::Connection};
use move_core_types::language_storage::StructTag;
use sui_types::{
    SUI_FRAMEWORK_ADDRESS, TypeTag,
    base_types::SuiAddress as NativeAddress,
    coin::{
        COIN_METADATA_STRUCT_NAME, COIN_MODULE_NAME, CoinMetadata as NativeMetadata,
        RegulatedCoinMetadata, TreasuryCap,
    },
    coin_registry::{
        Currency as NativeCurrency, RegulatedState as NativeRegulated, SupplyState as NativeSupply,
    },
    gas_coin::{GAS, TOTAL_SUPPLY_MIST},
    object::Owner as NativeOwner,
};
use tokio::sync::OnceCell;

use crate::{
    api::scalars::{
        base64::Base64, big_int::BigInt, sui_address::SuiAddress, type_filter::TypeInput,
        uint53::UInt53,
    },
    error::{RpcError, upcast},
    scope::Scope,
};

use super::{
    balance::{self, Balance},
    dynamic_field::{DynamicField, DynamicFieldName},
    move_object::MoveObject,
    move_value::MoveValue,
    object::{self, CLive, CVersion, Object, VersionFilter},
    object_filter::{ObjectFilter, ObjectFilterValidator as OFValidator},
    owner::Owner,
    transaction::{CTransaction, Transaction, filter::TransactionFilter},
};

pub(crate) struct CoinMetadata {
    pub(crate) super_: MoveObject,

    contents: OnceCell<Option<MetadataContents>>,
}

/// Whether the currency is regulated or not.
#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub(crate) enum RegulatedState {
    /// A `DenyCap` or a `RegulatedCoinMetadata` exists for this currency.
    Regulated,

    /// The currency was created without a deny list.
    Unregulated,
}

#[derive(SimpleObject, Default)]
pub(crate) struct RegulatedFields {
    /// Whether the currency is regulated or not. `null` indicates that the regulatory status is unknown.
    regulated_state: Option<RegulatedState>,

    /// Whether the `DenyCap` can be used to enable a global pause that behaves as if all addresses were added to the deny list. `null` indicates that it is not known whether the currency can be paused or not. This field is only populated on currencies held in the Coin Registry. To determine whether a legacy currency can be paused, check the contents of its `DenyCap`, if it can be found.
    allow_global_pause: Option<bool>,

    /// If the currency is regulated, this object represents the capability to modify the deny list. If a capability is known but wrapped, its address can be fetched but other fields will not be accessible.
    deny_cap: Option<MoveObject>,
}

/// Future behavior of a currency's supply.
#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub(crate) enum SupplyState {
    /// The supply can only decrease.
    BurnOnly,

    /// The supply can neither increase nor decrease.
    Fixed,
}

#[derive(SimpleObject, Default)]
pub(crate) struct SupplyFields {
    /// Future behavior of the supply. `null` indicates that the future behavior of the supply is not known because the currency's treasury still exists.
    supply_state: Option<SupplyState>,

    /// The overall balance of coins issued.
    supply: Option<BigInt>,
}

struct MetadataContents {
    coin_type: TypeTag,
    native: NativeContents,
}

enum NativeContents {
    Metadata(NativeMetadata),
    Registry(NativeCurrency),
}

/// An object representing metadata about a coin type.
#[Object]
impl CoinMetadata {
    /// The CoinMetadata's ID.
    pub(crate) async fn address(&self, ctx: &Context<'_>) -> Result<SuiAddress, RpcError> {
        self.super_.address(ctx).await
    }

    /// The version of this object that this content comes from.
    pub(crate) async fn version(&self, ctx: &Context<'_>) -> Result<Option<UInt53>, RpcError> {
        self.super_.version(ctx).await
    }

    /// 32-byte hash that identifies the object's contents, encoded in Base58.
    pub(crate) async fn digest(&self, ctx: &Context<'_>) -> Result<Option<String>, RpcError> {
        self.super_.digest(ctx).await
    }

    /// Fetch the total balance for coins with marker type `coinType` (e.g. `0x2::sui::SUI`), owned by this address.
    ///
    /// If the address does not own any coins of that type, a balance of zero is returned.
    pub(crate) async fn balance(
        &self,
        ctx: &Context<'_>,
        coin_type: TypeInput,
    ) -> Result<Option<Balance>, RpcError<balance::Error>> {
        self.super_.balance(ctx, coin_type).await
    }

    /// Total balance across coins owned by this address, grouped by coin type.
    pub(crate) async fn balances(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<balance::Cursor>,
        last: Option<u64>,
        before: Option<balance::Cursor>,
    ) -> Result<Option<Connection<String, Balance>>, RpcError<balance::Error>> {
        self.super_.balances(ctx, first, after, last, before).await
    }

    /// The structured representation of the object's contents.
    pub(crate) async fn contents(&self, ctx: &Context<'_>) -> Result<Option<MoveValue>, RpcError> {
        self.super_.contents(ctx).await
    }

    /// Number of decimal places the coin uses.
    pub(crate) async fn decimals(&self, ctx: &Context<'_>) -> Result<Option<u8>, RpcError> {
        let Some(contents) = self.metadata_contents(ctx).await? else {
            return Ok(None);
        };

        Ok(Some(match &contents.native {
            NativeContents::Metadata(metadata) => metadata.decimals,
            NativeContents::Registry(currency) => currency.decimals,
        }))
    }

    /// The domain explicitly configured as the default SuiNS name for this address.
    pub(crate) async fn default_suins_name(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<String>, RpcError> {
        self.super_.default_suins_name(ctx).await
    }

    /// Description of the coin.
    pub(crate) async fn description(&self, ctx: &Context<'_>) -> Result<Option<&str>, RpcError> {
        let Some(contents) = self.metadata_contents(ctx).await? else {
            return Ok(None);
        };

        Ok(Some(match &contents.native {
            NativeContents::Metadata(metadata) => &metadata.description,
            NativeContents::Registry(currency) => &currency.description,
        }))
    }

    /// Access a dynamic field on an object using its type and BCS-encoded name.
    ///
    /// Returns `null` if a dynamic field with that name could not be found attached to this object.
    pub(crate) async fn dynamic_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>, RpcError> {
        self.super_.dynamic_field(ctx, name).await
    }

    /// Dynamic fields owned by this object.
    ///
    /// Dynamic fields on wrapped objects can be accessed using `Address.dynamicFields`.
    pub(crate) async fn dynamic_fields(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CLive>,
        last: Option<u64>,
        before: Option<CLive>,
    ) -> Result<Option<Connection<String, DynamicField>>, RpcError<object::Error>> {
        self.super_
            .dynamic_fields(ctx, first, after, last, before)
            .await
    }

    /// Access a dynamic object field on an object using its type and BCS-encoded name.
    ///
    /// Returns `null` if a dynamic object field with that name could not be found attached to this object.
    pub(crate) async fn dynamic_object_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>, RpcError> {
        self.super_.dynamic_object_field(ctx, name).await
    }

    /// Whether this object can be transfered using the `TransferObjects` Programmable Transaction Command or `sui::transfer::public_transfer`.
    ///
    /// Both these operations require the object to have both the `key` and `store` abilities.
    pub(crate) async fn has_public_transfer(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<bool>, RpcError> {
        self.super_.has_public_transfer(ctx).await
    }

    /// URL for the coin logo.
    pub(crate) async fn icon_url(&self, ctx: &Context<'_>) -> Result<Option<&str>, RpcError> {
        let Some(contents) = self.metadata_contents(ctx).await? else {
            return Ok(None);
        };

        Ok(match &contents.native {
            NativeContents::Metadata(metadata) => metadata.icon_url.as_deref(),
            NativeContents::Registry(currency) => Some(&currency.icon_url),
        })
    }

    /// Access dynamic fields on an object using their types and BCS-encoded names.
    ///
    /// Returns a list of dynamic fields that is guaranteed to be the same length as `keys`. If a dynamic field in `keys` could not be found in the store, its corresponding entry in the result will be `null`.
    pub(crate) async fn multi_get_dynamic_fields(
        &self,
        ctx: &Context<'_>,
        keys: Vec<DynamicFieldName>,
    ) -> Result<Vec<Option<DynamicField>>, RpcError> {
        self.super_.multi_get_dynamic_fields(ctx, keys).await
    }

    /// Access dynamic object fields on an object using their types and BCS-encoded names.
    ///
    /// Returns a list of dynamic object fields that is guaranteed to be the same length as `keys`. If a dynamic object field in `keys` could not be found in the store, its corresponding entry in the result will be `null`.
    pub(crate) async fn multi_get_dynamic_object_fields(
        &self,
        ctx: &Context<'_>,
        keys: Vec<DynamicFieldName>,
    ) -> Result<Vec<Option<DynamicField>>, RpcError> {
        self.super_.multi_get_dynamic_object_fields(ctx, keys).await
    }

    /// The Base64-encoded BCS serialize of this object, as a `MoveObject`.
    pub(crate) async fn move_object_bcs(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Base64>, RpcError> {
        self.super_.move_object_bcs(ctx).await
    }

    /// Fetch the total balances keyed by coin types (e.g. `0x2::sui::SUI`) owned by this address.
    ///
    /// If the address does not own any coins of a given type, a balance of zero is returned for that type.
    pub(crate) async fn multi_get_balances(
        &self,
        ctx: &Context<'_>,
        keys: Vec<TypeInput>,
    ) -> Result<Option<Vec<Balance>>, RpcError<balance::Error>> {
        self.super_.multi_get_balances(ctx, keys).await
    }

    /// Name for the coin.
    pub(crate) async fn name(&self, ctx: &Context<'_>) -> Result<Option<&str>, RpcError> {
        let Some(contents) = self.metadata_contents(ctx).await? else {
            return Ok(None);
        };

        Ok(Some(match &contents.native {
            NativeContents::Metadata(metadata) => &metadata.name,
            NativeContents::Registry(currency) => &currency.name,
        }))
    }

    /// Fetch the object with the same ID, at a different version, root version bound, or checkpoint.
    pub(crate) async fn object_at(
        &self,
        ctx: &Context<'_>,
        version: Option<UInt53>,
        root_version: Option<UInt53>,
        checkpoint: Option<UInt53>,
    ) -> Result<Option<Object>, RpcError<object::Error>> {
        self.super_
            .object_at(ctx, version, root_version, checkpoint)
            .await
    }

    /// The Base64-encoded BCS serialization of this object, as an `Object`.
    pub(crate) async fn object_bcs(&self, ctx: &Context<'_>) -> Result<Option<Base64>, RpcError> {
        self.super_.object_bcs(ctx).await
    }

    /// Paginate all versions of this object after this one.
    pub(crate) async fn object_versions_after(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CVersion>,
        last: Option<u64>,
        before: Option<CVersion>,
        filter: Option<VersionFilter>,
    ) -> Result<Option<Connection<String, Object>>, RpcError> {
        self.super_
            .object_versions_after(ctx, first, after, last, before, filter)
            .await
    }

    /// Paginate all versions of this object before this one.
    pub(crate) async fn object_versions_before(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CVersion>,
        last: Option<u64>,
        before: Option<CVersion>,
        filter: Option<VersionFilter>,
    ) -> Result<Option<Connection<String, Object>>, RpcError> {
        self.super_
            .object_versions_before(ctx, first, after, last, before, filter)
            .await
    }

    /// Objects owned by this object, optionally filtered by type.
    pub(crate) async fn objects(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CLive>,
        last: Option<u64>,
        before: Option<CLive>,
        #[graphql(validator(custom = "OFValidator::allows_empty()"))] filter: Option<ObjectFilter>,
    ) -> Result<Option<Connection<String, MoveObject>>, RpcError<object::Error>> {
        self.super_
            .objects(ctx, first, after, last, before, filter)
            .await
    }

    /// The object's owner kind.
    pub(crate) async fn owner(&self, ctx: &Context<'_>) -> Result<Option<Owner>, RpcError> {
        self.super_.owner(ctx).await
    }

    /// The transaction that created this version of the object.
    pub(crate) async fn previous_transaction(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Transaction>, RpcError> {
        self.super_.previous_transaction(ctx).await
    }

    /// The SUI returned to the sponsor or sender of the transaction that modifies or deletes this object.
    pub(crate) async fn storage_rebate(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<BigInt>, RpcError> {
        self.super_.storage_rebate(ctx).await
    }

    /// The transactions that sent objects to this object.
    pub(crate) async fn received_transactions(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CTransaction>,
        last: Option<u64>,
        before: Option<CTransaction>,
        filter: Option<TransactionFilter>,
    ) -> Result<Option<Connection<String, Transaction>>, RpcError> {
        self.super_
            .received_transactions(ctx, first, after, last, before, filter)
            .await
    }

    #[graphql(flatten)]
    pub(crate) async fn regulated_fields(
        &self,
        ctx: &Context<'_>,
    ) -> Result<RegulatedFields, RpcError<object::Error>> {
        let Some(contents) = self.metadata_contents(ctx).await.map_err(upcast)? else {
            return Ok(RegulatedFields::default());
        };

        let scope = &self.super_.super_.super_.scope;

        // If the currency's metadata is stored in the Coin Registry, and its regulation state is
        // known, return it.
        match &contents.native {
            NativeContents::Registry(NativeCurrency {
                regulated: NativeRegulated::Unregulated,
                ..
            }) => {
                return Ok(RegulatedFields {
                    regulated_state: Some(RegulatedState::Unregulated),
                    allow_global_pause: Some(false),
                    deny_cap: None,
                });
            }

            NativeContents::Registry(NativeCurrency {
                regulated:
                    NativeRegulated::Regulated {
                        cap,
                        allow_global_pause,
                        variant: _,
                    },
                ..
            }) => {
                return Ok(RegulatedFields {
                    regulated_state: Some(RegulatedState::Regulated),
                    allow_global_pause: *allow_global_pause,
                    deny_cap: Some(MoveObject::from_super(Object::with_address(
                        scope.without_root_version(),
                        (*cap).into(),
                    ))),
                });
            }

            _ => {}
        }

        // ...otherwise fall back to looking up the RegulatedCoinMetadata singleton object (for
        // coin registry currencies where the regulated state is not known and for legacy currencies).
        let TypeTag::Struct(coin_type) = &contents.coin_type else {
            return Ok(RegulatedFields::default());
        };

        if GAS::is_gas(coin_type.as_ref()) {
            return Ok(RegulatedFields {
                regulated_state: Some(RegulatedState::Unregulated),
                allow_global_pause: Some(false),
                deny_cap: None,
            });
        }

        let type_ = RegulatedCoinMetadata::type_(*coin_type.clone());
        let Some(object) = Object::singleton(ctx, scope.without_root_version(), type_).await?
        else {
            // If there is no RegulatedCoinMetadata object, the coin is unregulated.
            return Ok(RegulatedFields {
                regulated_state: Some(RegulatedState::Unregulated),
                allow_global_pause: Some(false),
                deny_cap: None,
            });
        };

        let Some(contents) = object.contents(ctx).await.map_err(upcast)? else {
            // We were able to find a regulated coin metadata object but couldn't load its
            // contents -- it's definitely regulated, but we don't have more details.
            return Ok(RegulatedFields {
                regulated_state: Some(RegulatedState::Regulated),
                allow_global_pause: None,
                deny_cap: None,
            });
        };

        let move_object = contents
            .data
            .try_as_move()
            .context("Query by type returned a package")?;

        let metadata: RegulatedCoinMetadata = bcs::from_bytes(move_object.contents())
            .context("Failed to deserialize RegulatedCoinMetadata")?;

        Ok(RegulatedFields {
            regulated_state: Some(RegulatedState::Regulated),
            allow_global_pause: None,
            deny_cap: Some(MoveObject::from_super(Object::with_address(
                scope.without_root_version(),
                metadata.deny_cap_object.bytes.into(),
            ))),
        })
    }

    #[graphql(flatten)]
    pub(crate) async fn supply_fields(
        &self,
        ctx: &Context<'_>,
    ) -> Result<SupplyFields, RpcError<object::Error>> {
        let Some(contents) = self.metadata_contents(ctx).await.map_err(upcast)? else {
            return Ok(SupplyFields::default());
        };

        // If the currency's metadata is stored in the Coin Registry, and its supply is known,
        // return it.
        match &contents.native {
            NativeContents::Registry(NativeCurrency {
                supply: Some(NativeSupply::Fixed(s)),
                ..
            }) => {
                return Ok(SupplyFields {
                    supply_state: Some(SupplyState::Fixed),
                    supply: Some(BigInt::from(*s)),
                });
            }

            NativeContents::Registry(NativeCurrency {
                supply: Some(NativeSupply::BurnOnly(s)),
                ..
            }) => {
                return Ok(SupplyFields {
                    supply_state: Some(SupplyState::BurnOnly),
                    supply: Some(BigInt::from(*s)),
                });
            }

            _ => {}
        }

        // ...otherwise fall back to looking up the TreasuryCap singleton object (for coin registry
        // currencies where the supply state is not known and for legacy currencies).
        let TypeTag::Struct(coin_type) = &contents.coin_type else {
            return Ok(SupplyFields::default());
        };

        if GAS::is_gas(coin_type.as_ref()) {
            return Ok(SupplyFields {
                supply_state: Some(SupplyState::Fixed),
                supply: Some(BigInt::from(TOTAL_SUPPLY_MIST)),
            });
        }

        let type_ = TreasuryCap::type_(*coin_type.clone());
        let scope = self.super_.super_.super_.scope.without_root_version();
        let Some(object) = Object::singleton(ctx, scope, type_).await? else {
            return Ok(SupplyFields::default());
        };

        let Some(contents) = object.contents(ctx).await.map_err(upcast)? else {
            return Ok(SupplyFields::default());
        };

        let move_object = contents
            .data
            .try_as_move()
            .context("Query by type returned a package")?;

        let treasury_cap: TreasuryCap =
            bcs::from_bytes(move_object.contents()).context("Failed to deserialize TreasuryCap")?;

        // Treat the supply as fixed if the TreasuryCap is immutable, or owned by the zero address.
        let supply_state = if matches!(
            contents.owner(),
            NativeOwner::Immutable
                | NativeOwner::AddressOwner(NativeAddress::ZERO)
                | NativeOwner::ConsensusAddressOwner {
                    owner: NativeAddress::ZERO,
                    ..
                }
        ) {
            Some(SupplyState::Fixed)
        } else {
            None
        };

        Ok(SupplyFields {
            supply_state,
            supply: Some(BigInt::from(treasury_cap.total_supply.value)),
        })
    }

    /// Symbol for the coin.
    pub(crate) async fn symbol(&self, ctx: &Context<'_>) -> Result<Option<&str>, RpcError> {
        let Some(contents) = self.metadata_contents(ctx).await? else {
            return Ok(None);
        };

        Ok(Some(match &contents.native {
            NativeContents::Metadata(metadata) => &metadata.symbol,
            NativeContents::Registry(currency) => &currency.symbol,
        }))
    }
}

impl CoinMetadata {
    /// Create a CoinMetadata from a `MoveObject`, assuming (but not checking) that it is a
    /// CoinMetadata.
    pub(crate) fn from_super(super_: MoveObject) -> Self {
        Self {
            super_,
            contents: OnceCell::new(),
        }
    }

    /// Create a CoinMetadata from a `MoveObject`, after checking whether it is a CoinMetadata.
    pub(crate) async fn from_move_object(
        move_object: &MoveObject,
        ctx: &Context<'_>,
    ) -> Result<Option<Self>, RpcError> {
        let Some(native) = move_object.native(ctx).await?.as_ref() else {
            return Ok(None);
        };

        if !native.type_().is_coin_metadata() && !native.type_().is_currency() {
            return Ok(None);
        }

        Ok(Some(Self::from_super(move_object.clone())))
    }

    /// Find a CoinMetadata object by the coin type it represents.
    ///
    /// First checks if the currency is represented in the Coin Registry, and otherwise looks for a
    /// live singleton `CoinMetadata` object.
    ///
    /// Returns `None` if the currency is not in the Coin Registry and there is no live
    /// `CoinMetadata` object for the given coin type (it may have been deleted, wrapped, or never
    /// created).
    pub(crate) async fn by_coin_type(
        ctx: &Context<'_>,
        scope: Scope,
        coin_type: TypeTag,
    ) -> Result<Option<Self>, RpcError<object::Error>> {
        let currency_id: SuiAddress = NativeCurrency::derive_object_id(coin_type.clone())
            .context("Failed to derive Currency ID")?
            .into();

        if let Some(object) = Object::latest(ctx, scope.clone(), currency_id)
            .await
            .map_err(upcast)?
        {
            return Ok(Some(Self::from_super(MoveObject::from_super(object))));
        }

        let type_ = StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: COIN_MODULE_NAME.to_owned(),
            name: COIN_METADATA_STRUCT_NAME.to_owned(),
            type_params: vec![coin_type],
        };

        Ok(Object::singleton(ctx, scope, type_)
            .await?
            .map(|obj| Self::from_super(MoveObject::from_super(obj))))
    }

    /// Get the native CoinMetadata data, loading it lazily if needed.
    async fn metadata_contents(
        &self,
        ctx: &Context<'_>,
    ) -> Result<&Option<MetadataContents>, RpcError> {
        self.contents
            .get_or_try_init(async || {
                let Some(native_move) = self.super_.native(ctx).await?.as_ref() else {
                    return Ok(None);
                };

                let coin_type = || {
                    StructTag::from(native_move.type_().clone())
                        .type_params
                        .pop()
                        .context("No coin type parameter")
                };

                Ok(Some(if native_move.type_().is_currency() {
                    MetadataContents {
                        coin_type: coin_type()?,
                        native: NativeContents::Registry(
                            bcs::from_bytes(native_move.contents())
                                .context("Failed to deserialize Currency")?,
                        ),
                    }
                } else if native_move.type_().is_coin_metadata() {
                    MetadataContents {
                        coin_type: coin_type()?,
                        native: NativeContents::Metadata(
                            bcs::from_bytes(native_move.contents())
                                .context("Failed to deserialize CoinMetadata")?,
                        ),
                    }
                } else {
                    return Err(anyhow!("Not a CoinMetadata").into());
                }))
            })
            .await
    }
}
