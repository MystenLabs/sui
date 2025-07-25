// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    balance::Balance,
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    collection_types::Bag,
    digests::TransactionDigest,
    dynamic_field::{serialize_dynamic_field, DynamicFieldID, DynamicFieldKey, DynamicFieldObject},
    error::{SuiError, SuiResult},
    object::{Object, Owner},
    storage::{ChildObjectResolver, ObjectStore},
    MoveTypeTagTrait, MoveTypeTagTraitGeneric, SUI_ACCUMULATOR_ROOT_ADDRESS,
    SUI_ACCUMULATOR_ROOT_OBJECT_ID, SUI_FRAMEWORK_ADDRESS, SUI_FRAMEWORK_PACKAGE_ID,
};
use move_core_types::{
    ident_str,
    identifier::IdentStr,
    language_storage::{StructTag, TypeTag},
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

pub const ACCUMULATOR_ROOT_MODULE: &IdentStr = ident_str!("accumulator");
pub const ACCUMULATOR_METADATA_MODULE: &IdentStr = ident_str!("accumulator_metadata");
pub const ACCUMULATOR_SETTLEMENT_MODULE: &IdentStr = ident_str!("accumulator_settlement");
pub const ACCUMULATOR_ROOT_CREATE_FUNC: &IdentStr = ident_str!("create");
pub const ACCUMULATOR_ROOT_SETTLE_U128_FUNC: &IdentStr = ident_str!("settle_u128");
pub const ACCUMULATOR_ROOT_SETTLEMENT_PROLOGUE_FUNC: &IdentStr = ident_str!("settlement_prologue");

const ACCUMULATOR_KEY_TYPE: &IdentStr = ident_str!("Key");
const ACCUMULATOR_OWNER_KEY_TYPE: &IdentStr = ident_str!("OwnerKey");
const ACCUMULATOR_OWNER_TYPE: &IdentStr = ident_str!("Owner");
const ACCUMULATOR_METADATA_KEY_TYPE: &IdentStr = ident_str!("MetadataKey");

const ACCUMULATOR_U128_TYPE: &IdentStr = ident_str!("U128");

pub fn get_accumulator_root_obj_initial_shared_version(
    object_store: &dyn ObjectStore,
) -> SuiResult<Option<SequenceNumber>> {
    Ok(object_store
        .get_object(&SUI_ACCUMULATOR_ROOT_OBJECT_ID)
        .map(|obj| match obj.owner {
            Owner::Shared {
                initial_shared_version,
            } => initial_shared_version,
            _ => unreachable!("Accumulator root object must be shared"),
        }))
}

/// Rust type for the Move type AccumulatorKey used to derive the dynamic field id for the
/// balance account object.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AccumulatorKey {
    pub owner: SuiAddress,
}

impl MoveTypeTagTraitGeneric for AccumulatorKey {
    fn get_type_tag(type_params: &[TypeTag]) -> TypeTag {
        TypeTag::Struct(Box::new(StructTag {
            address: SUI_FRAMEWORK_PACKAGE_ID.into(),
            module: ACCUMULATOR_ROOT_MODULE.to_owned(),
            name: ACCUMULATOR_KEY_TYPE.to_owned(),
            type_params: type_params.to_vec(),
        }))
    }
}

#[derive(Serialize, Deserialize)]
pub struct AccumulatorOwner {
    balances: Bag,
    owner: SuiAddress,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct MetadataKey(u8);

impl MoveTypeTagTraitGeneric for MetadataKey {
    fn get_type_tag(type_params: &[TypeTag]) -> TypeTag {
        TypeTag::Struct(Box::new(StructTag {
            address: SUI_FRAMEWORK_PACKAGE_ID.into(),
            module: ACCUMULATOR_METADATA_MODULE.to_owned(),
            name: ACCUMULATOR_METADATA_KEY_TYPE.to_owned(),
            type_params: type_params.to_vec(),
        }))
    }
}

#[derive(Serialize, Deserialize)]
pub struct AccumulatorMetadata {
    /// Any per-balance fields we wish to add in the future.
    fields: Bag,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct OwnerKey {
    owner: SuiAddress,
}

impl MoveTypeTagTrait for OwnerKey {
    fn get_type_tag() -> TypeTag {
        TypeTag::Struct(Box::new(StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: ACCUMULATOR_METADATA_MODULE.to_owned(),
            name: ACCUMULATOR_OWNER_KEY_TYPE.to_owned(),
            type_params: vec![],
        }))
    }
}

impl AccumulatorOwner {
    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: ACCUMULATOR_METADATA_MODULE.to_owned(),
            name: ACCUMULATOR_OWNER_TYPE.to_owned(),
            type_params: vec![],
        }
    }

    pub fn exists(
        child_object_resolver: &dyn ChildObjectResolver,
        version_bound: Option<SequenceNumber>,
        owner: SuiAddress,
    ) -> SuiResult<bool> {
        let key = OwnerKey { owner };

        DynamicFieldKey(
            SUI_ACCUMULATOR_ROOT_OBJECT_ID,
            key,
            OwnerKey::get_type_tag(),
        )
        .into_id()?
        .with_bound(version_bound.unwrap_or(SequenceNumber::MAX))
        .exists(child_object_resolver)
    }

    pub fn load(
        child_object_resolver: &dyn ChildObjectResolver,
        root_version: Option<SequenceNumber>,
        owner: SuiAddress,
    ) -> SuiResult<Option<Self>> {
        let key = OwnerKey { owner };
        DynamicFieldKey(
            SUI_ACCUMULATOR_ROOT_OBJECT_ID,
            key,
            OwnerKey::get_type_tag(),
        )
        .into_id()?
        .with_bound(root_version.unwrap_or(SequenceNumber::MAX))
        .load_object(child_object_resolver)?
        .map(|o| o.load_value::<Self>())
        .transpose()
    }

    pub fn metadata_exists(
        &self,
        child_object_resolver: &dyn ChildObjectResolver,
        version_bound: Option<SequenceNumber>,
        type_: &TypeTag,
    ) -> SuiResult<bool> {
        let key = MetadataKey::default();
        DynamicFieldKey(
            *self.balances.id.object_id(),
            key,
            MetadataKey::get_type_tag(&[type_.clone()]),
        )
        .into_id()?
        .with_bound(version_bound.unwrap_or(SequenceNumber::MAX))
        .exists(child_object_resolver)
    }

    pub fn load_metadata(
        &self,
        child_object_resolver: &dyn ChildObjectResolver,
        version_bound: Option<SequenceNumber>,
        type_: &TypeTag,
    ) -> SuiResult<Option<AccumulatorMetadata>> {
        let key = MetadataKey::default();
        DynamicFieldKey(
            *self.balances.id.object_id(),
            key,
            MetadataKey::get_type_tag(&[type_.clone()]),
        )
        .into_id()?
        .with_bound(version_bound.unwrap_or(SequenceNumber::MAX))
        .load_object(child_object_resolver)?
        .map(|o| o.load_value::<AccumulatorMetadata>())
        .transpose()
    }
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub enum AccumulatorValue {
    U128(U128),
}

#[derive(Default, Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct U128 {
    pub value: u128,
}

impl MoveTypeTagTrait for U128 {
    fn get_type_tag() -> TypeTag {
        TypeTag::Struct(Box::new(StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: ACCUMULATOR_ROOT_MODULE.to_owned(),
            name: ACCUMULATOR_U128_TYPE.to_owned(),
            type_params: vec![],
        }))
    }
}

impl AccumulatorValue {
    pub fn get_field_id(owner: SuiAddress, type_: &TypeTag) -> SuiResult<ObjectID> {
        if !Balance::is_balance_type(type_) {
            return Err(SuiError::TypeError {
                error: "only Balance<T> is supported".to_string(),
            });
        }

        let key = AccumulatorKey { owner };
        Ok(DynamicFieldKey(
            SUI_ACCUMULATOR_ROOT_OBJECT_ID,
            key,
            AccumulatorKey::get_type_tag(&[type_.clone()]),
        )
        .into_id()?
        .as_object_id())
    }

    pub fn exists(
        child_object_resolver: &dyn ChildObjectResolver,
        version_bound: Option<SequenceNumber>,
        owner: SuiAddress,
        type_: &TypeTag,
    ) -> SuiResult<bool> {
        if !Balance::is_balance_type(type_) {
            return Err(SuiError::TypeError {
                error: "only Balance<T> is supported".to_string(),
            });
        }

        let key = AccumulatorKey { owner };
        DynamicFieldKey(
            SUI_ACCUMULATOR_ROOT_OBJECT_ID,
            key,
            AccumulatorKey::get_type_tag(&[type_.clone()]),
        )
        .into_id()?
        .with_bound(version_bound.unwrap_or(SequenceNumber::MAX))
        .exists(child_object_resolver)
    }

    pub fn load_by_id<T>(
        child_object_resolver: &dyn ChildObjectResolver,
        version_bound: Option<SequenceNumber>,
        id: ObjectID,
    ) -> SuiResult<Option<T>>
    where
        T: Serialize + DeserializeOwned,
    {
        DynamicFieldID::<AccumulatorKey>::new(SUI_ACCUMULATOR_ROOT_OBJECT_ID, id)
            .with_bound(version_bound.unwrap_or(SequenceNumber::MAX))
            .load_object(child_object_resolver)?
            .map(|o| o.load_value::<T>())
            .transpose()
    }

    pub fn load(
        child_object_resolver: &dyn ChildObjectResolver,
        version_bound: Option<SequenceNumber>,
        owner: SuiAddress,
        type_: &TypeTag,
    ) -> SuiResult<Option<Self>> {
        if !Balance::is_balance_type(type_) {
            return Err(SuiError::TypeError {
                error: "only Balance<T> is supported".to_string(),
            });
        }

        let key = AccumulatorKey { owner };
        let key_type_tag = AccumulatorKey::get_type_tag(&[type_.clone()]);

        let Some(value) = DynamicFieldKey(SUI_ACCUMULATOR_ROOT_OBJECT_ID, key, key_type_tag)
            .into_id()?
            .with_bound(version_bound.unwrap_or(SequenceNumber::MAX))
            .load_object(child_object_resolver)?
            .map(|o| o.load_value::<U128>())
            .transpose()?
        else {
            return Ok(None);
        };

        Ok(Some(Self::U128(value)))
    }

    pub fn create_for_testing(owner: SuiAddress, type_tag: TypeTag, balance: u64) -> Object {
        let key = AccumulatorKey { owner };
        let value = U128 {
            value: balance as u128,
        };

        let field_key = DynamicFieldKey(
            SUI_ACCUMULATOR_ROOT_OBJECT_ID,
            key,
            AccumulatorKey::get_type_tag(&[type_tag.clone()]),
        );
        let field = field_key.into_field(value).unwrap();
        let move_object = field
            .into_move_object_unsafe_for_testing(SequenceNumber::new())
            .unwrap();

        Object::new_move(
            move_object,
            Owner::ObjectOwner(SUI_ACCUMULATOR_ROOT_ADDRESS.into()),
            TransactionDigest::genesis_marker(),
        )
    }
}

pub fn update_account_balance_for_testing(account_object: &mut Object, balance_change: i128) {
    let current_balance_field = DynamicFieldObject::<AccumulatorKey>::new(account_object.clone())
        .load_field::<U128>()
        .unwrap();

    let current_balance = current_balance_field.value.value;

    assert!(current_balance <= i128::MAX as u128);
    assert!(current_balance as i128 >= balance_change.abs());

    let new_balance = U128 {
        value: (current_balance as i128 + balance_change) as u128,
    };

    let new_field = serialize_dynamic_field(
        &current_balance_field.id,
        &current_balance_field.name,
        new_balance,
    )
    .unwrap();

    let move_object = account_object.data.try_as_move_mut().unwrap();
    move_object.set_contents_unsafe(new_field);
}
