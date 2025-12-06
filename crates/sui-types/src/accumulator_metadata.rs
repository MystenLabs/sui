// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    MoveTypeTagTrait, MoveTypeTagTraitGeneric, SUI_ACCUMULATOR_ROOT_OBJECT_ID,
    SUI_FRAMEWORK_ADDRESS, SUI_FRAMEWORK_PACKAGE_ID,
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    collection_types::Bag,
    dynamic_field::{DynamicFieldKey, DynamicFieldObject},
    error::SuiResult,
    object::Object,
    storage::ChildObjectResolver,
};
use move_core_types::{
    ident_str,
    identifier::IdentStr,
    language_storage::{StructTag, TypeTag},
};
use serde::{Deserialize, Serialize};

pub const ACCUMULATOR_METADATA_MODULE: &IdentStr = ident_str!("accumulator_metadata");
pub const ACCUMULATOR_OWNER_KEY_TYPE: &IdentStr = ident_str!("OwnerKey");
pub const ACCUMULATOR_OWNER_TYPE: &IdentStr = ident_str!("Owner");
pub const ACCUMULATOR_METADATA_KEY_TYPE: &IdentStr = ident_str!("MetadataKey");
pub const ACCUMULATOR_METADATA_TYPE: &IdentStr = ident_str!("Metadata");

#[derive(Serialize, Deserialize)]
pub struct AccumulatorOwner {
    pub balances: Bag,
    pub owner: SuiAddress,
}

impl MoveTypeTagTrait for AccumulatorOwner {
    fn get_type_tag() -> TypeTag {
        TypeTag::Struct(Box::new(StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: ACCUMULATOR_METADATA_MODULE.to_owned(),
            name: ACCUMULATOR_OWNER_TYPE.to_owned(),
            type_params: vec![],
        }))
    }
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

impl MoveTypeTagTraitGeneric for AccumulatorMetadata {
    fn get_type_tag(type_params: &[TypeTag]) -> TypeTag {
        TypeTag::Struct(Box::new(StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: ACCUMULATOR_METADATA_MODULE.to_owned(),
            name: ACCUMULATOR_METADATA_TYPE.to_owned(),
            type_params: type_params.to_vec(),
        }))
    }
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

    pub fn get_object_id(owner: SuiAddress) -> SuiResult<ObjectID> {
        let key = OwnerKey { owner };
        DynamicFieldKey(
            SUI_ACCUMULATOR_ROOT_OBJECT_ID,
            key,
            OwnerKey::get_type_tag(),
        )
        .object_id()
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
        .into_id_with_bound(version_bound.unwrap_or(SequenceNumber::MAX))?
        .exists(child_object_resolver)
    }

    pub fn load_object(
        child_object_resolver: &dyn ChildObjectResolver,
        root_version: Option<SequenceNumber>,
        owner: SuiAddress,
    ) -> SuiResult<Option<Object>> {
        let key = OwnerKey { owner };
        Ok(DynamicFieldKey(
            SUI_ACCUMULATOR_ROOT_OBJECT_ID,
            key,
            OwnerKey::get_type_tag(),
        )
        .into_id_with_bound(root_version.unwrap_or(SequenceNumber::MAX))?
        .load_object(child_object_resolver)?
        .map(|o| o.into_object()))
    }

    pub fn from_object(object: Object) -> SuiResult<Self> {
        DynamicFieldObject::<OwnerKey>::new(object).load_value::<Self>()
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
        .into_id_with_bound(root_version.unwrap_or(SequenceNumber::MAX))?
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
            MetadataKey::get_type_tag(std::slice::from_ref(type_)),
        )
        .into_id_with_bound(version_bound.unwrap_or(SequenceNumber::MAX))?
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
            MetadataKey::get_type_tag(std::slice::from_ref(type_)),
        )
        .into_id_with_bound(version_bound.unwrap_or(SequenceNumber::MAX))?
        .load_object(child_object_resolver)?
        .map(|o| o.load_value::<AccumulatorMetadata>())
        .transpose()
    }
}
