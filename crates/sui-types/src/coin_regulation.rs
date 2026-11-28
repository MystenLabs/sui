// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Rust bindings for `sui::coin_regulation`.
//!
//! Per-type and per-address state are dynamic fields directly on the `FreezeList` (0xcf), so each
//! field ID is a pure function of the coin type, or of the coin type and address. Execution reads
//! them bounded by the FreezeList version assigned to the transaction; signing reads the latest.

use crate::base_types::{ObjectID, SequenceNumber, SuiAddress};
use crate::collection_types::VecSet;
use crate::dynamic_field::DynamicFieldKey;
use crate::error::SuiResult;
use crate::storage::{ObjectStore, RuntimeObjectResolver};
use crate::{MoveTypeTagTrait, SUI_FRAMEWORK_ADDRESS, SUI_FREEZE_LIST_OBJECT_ID};
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::{StructTag, TypeTag};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub const COIN_REGULATION_MODULE_NAME: &IdentStr = ident_str!("coin_regulation");
const COIN_TYPE_KEY_STRUCT_NAME: &IdentStr = ident_str!("CoinTypeKey");
const FROZEN_ADDRESS_KEY_STRUCT_NAME: &IdentStr = ident_str!("FrozenAddressKey");

/// Mirror of the Move struct `sui::coin_regulation::CoinTypeKey<T>`, an empty struct, which the
/// compiler gives one `bool` field.
#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct CoinTypeKey {
    dummy_field: bool,
}

/// Mirror of the Move struct `sui::coin_regulation::CoinTypeState`.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct CoinTypeState {
    pub paused: bool,
    pub num_frozen_addresses: u64,
}

/// Mirror of the Move struct `sui::coin_regulation::FrozenAddressKey<T>`.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct FrozenAddressKey {
    pub frozen_address: SuiAddress,
}

/// Mirror of the Move struct `sui::coin_regulation::FrozenAddressState`.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct FrozenAddressState {
    pub seize_auths: VecSet<ObjectID>,
    pub frozen_epoch: u64,
    pub unfreeze_lock: UnfreezeLock,
}

/// Mirror of the Move enum `sui::coin_regulation::UnfreezeLock`.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, Eq, PartialEq)]
pub enum UnfreezeLock {
    None,
    UntilEpoch(u64),
    Permanent,
}

fn coin_regulation_struct(name: &IdentStr, type_params: Vec<TypeTag>) -> StructTag {
    StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: COIN_REGULATION_MODULE_NAME.to_owned(),
        name: name.to_owned(),
        type_params,
    }
}

impl MoveTypeTagTrait for CoinTypeState {
    fn get_type_tag() -> TypeTag {
        TypeTag::Struct(Box::new(coin_regulation_struct(
            ident_str!("CoinTypeState"),
            vec![],
        )))
    }
}

impl MoveTypeTagTrait for FrozenAddressState {
    fn get_type_tag() -> TypeTag {
        TypeTag::Struct(Box::new(coin_regulation_struct(
            ident_str!("FrozenAddressState"),
            vec![],
        )))
    }
}

/// The `type_name::with_defining_ids` string of `coin_type`, as errors name coin types.
pub fn coin_type_name(coin_type: &TypeTag) -> String {
    coin_type.to_canonical_string(/* with_prefix */ false)
}

pub type FreezeListField<K> = DynamicFieldKey<ObjectID, K>;

/// The FreezeList field keyed by `key`, whose type is `name<coin_type>`. The keys are phantom in
/// the coin type, so it appears only in the key's type tag.
fn freeze_list_field<K>(name: &IdentStr, coin_type: &TypeTag, key: K) -> FreezeListField<K> {
    let key_type = coin_regulation_struct(name, vec![coin_type.clone()]);
    DynamicFieldKey(
        SUI_FREEZE_LIST_OBJECT_ID,
        key,
        TypeTag::Struct(Box::new(key_type)),
    )
}

impl CoinTypeKey {
    pub fn field(coin_type: &TypeTag) -> FreezeListField<Self> {
        freeze_list_field(COIN_TYPE_KEY_STRUCT_NAME, coin_type, Self::default())
    }

    pub fn field_id(coin_type: &TypeTag) -> SuiResult<ObjectID> {
        Self::field(coin_type).object_id()
    }
}

impl FrozenAddressKey {
    pub fn field(coin_type: &TypeTag, frozen_address: SuiAddress) -> FreezeListField<Self> {
        freeze_list_field(
            FROZEN_ADDRESS_KEY_STRUCT_NAME,
            coin_type,
            Self { frozen_address },
        )
    }

    pub fn field_id(coin_type: &TypeTag, frozen_address: SuiAddress) -> SuiResult<ObjectID> {
        Self::field(coin_type, frozen_address).object_id()
    }
}

/// Reads `field` as of FreezeList version `freeze_list_version`.
fn load_bounded<K, V>(
    resolver: &dyn RuntimeObjectResolver,
    freeze_list_version: SequenceNumber,
    field: FreezeListField<K>,
) -> SuiResult<Option<V>>
where
    K: Serialize + DeserializeOwned + std::fmt::Debug,
    V: Serialize + DeserializeOwned,
{
    field
        .into_id_with_bound(freeze_list_version)?
        .load_object(resolver)?
        .map(|field| field.load_value::<V>())
        .transpose()
}

/// `None` means `coin_type` is not regulated by the FreezeList.
pub fn get_coin_type_state(
    resolver: &dyn RuntimeObjectResolver,
    freeze_list_version: SequenceNumber,
    coin_type: &TypeTag,
) -> SuiResult<Option<CoinTypeState>> {
    load_bounded(resolver, freeze_list_version, CoinTypeKey::field(coin_type))
}

/// Whether `coin_type` has a `CoinTypeKey` as of FreezeList version `freeze_list_version`.
///
/// A coin type gets one when it migrates from its `DenyCapV2` or its `ComplianceCap` first freezes
/// or pauses it. From then on the FreezeList alone regulates it, and its legacy deny list is ignored.
pub fn has_coin_type_state(
    resolver: &dyn RuntimeObjectResolver,
    freeze_list_version: SequenceNumber,
    coin_type: &TypeTag,
) -> SuiResult<bool> {
    CoinTypeKey::field(coin_type)
        .into_id_with_bound(freeze_list_version)?
        .exists(resolver)
}

/// Whether `addr` is frozen for `coin_type` as of FreezeList version `freeze_list_version`.
pub fn is_address_frozen(
    resolver: &dyn RuntimeObjectResolver,
    freeze_list_version: SequenceNumber,
    coin_type: &TypeTag,
    addr: SuiAddress,
) -> SuiResult<bool> {
    FrozenAddressKey::field(coin_type, addr)
        .into_id_with_bound(freeze_list_version)?
        .exists(resolver)
}

pub fn has_latest_coin_type_state(store: &dyn ObjectStore, coin_type: &TypeTag) -> SuiResult<bool> {
    Ok(CoinTypeKey::field(coin_type)
        .into_unbounded_id()?
        .load_object(store)
        .is_some())
}

#[cfg(test)]
#[path = "unit_tests/coin_regulation_tests.rs"]
mod coin_regulation_tests;
