// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{MoveObjectType, ObjectDigest, SuiAddress};
use crate::crypto::DefaultHash;
use crate::error::{SuiError, SuiErrorKind, SuiResult};
use crate::id::UID;
use crate::object::{MoveObject, Object};
use crate::storage::{ChildObjectResolver, ObjectStore};
use crate::sui_serde::Readable;
use crate::sui_serde::SuiTypeTag;
use crate::{MoveTypeTagTrait, ObjectID, SUI_FRAMEWORK_ADDRESS, SequenceNumber};
use fastcrypto::encoding::Base64;
use fastcrypto::hash::HashFunction;
use move_core_types::annotated_value::{MoveStruct, MoveValue};
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::{StructTag, TypeTag};
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use serde_with::DisplayFromStr;
use serde_with::serde_as;
use shared_crypto::intent::HashingIntentScope;
use std::fmt;
use std::fmt::{Display, Formatter};

pub mod visitor;

pub const DYNAMIC_FIELD_MODULE_NAME: &IdentStr = ident_str!("dynamic_field");
pub const DYNAMIC_FIELD_FIELD_STRUCT_NAME: &IdentStr = ident_str!("Field");

const DYNAMIC_OBJECT_FIELD_MODULE_NAME: &IdentStr = ident_str!("dynamic_object_field");
const DYNAMIC_OBJECT_FIELD_WRAPPER_STRUCT_NAME: &IdentStr = ident_str!("Wrapper");

/// Rust version of the Move sui::dynamic_field::Field type
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Field<N, V> {
    pub id: UID,
    pub name: N,
    pub value: V,
}

/// Rust version of the Move sui::dynamic_object_field::Wrapper type
#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
pub struct DOFWrapper<N> {
    pub name: N,
}

impl<N> MoveTypeTagTrait for DOFWrapper<N>
where
    N: MoveTypeTagTrait,
{
    fn get_type_tag() -> TypeTag {
        TypeTag::Struct(Box::new(DynamicFieldInfo::dynamic_object_field_wrapper(
            N::get_type_tag(),
        )))
    }
}

#[serde_as]
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DynamicFieldInfo {
    pub name: DynamicFieldName,
    #[serde_as(as = "Readable<Base64, _>")]
    pub bcs_name: Vec<u8>,
    pub type_: DynamicFieldType,
    pub object_type: String,
    pub object_id: ObjectID,
    pub version: SequenceNumber,
    pub digest: ObjectDigest,
}

#[serde_as]
#[derive(Clone, Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DynamicFieldName {
    #[schemars(with = "String")]
    #[serde_as(as = "Readable<SuiTypeTag, _>")]
    pub type_: TypeTag,
    // Bincode does not like serde_json::Value, rocksdb will not insert the value without serializing value as string.
    // TODO: investigate if this can be removed after switch to BCS.
    #[schemars(with = "Value")]
    #[serde_as(as = "Readable<_, DisplayFromStr>")]
    pub value: Value,
}

impl Display for DynamicFieldName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.type_, self.value)
    }
}

#[derive(
    Copy, Clone, Serialize, Deserialize, JsonSchema, Ord, PartialOrd, Eq, PartialEq, Debug,
)]
pub enum DynamicFieldType {
    #[serde(rename_all = "camelCase")]
    DynamicField,
    DynamicObject,
}

impl Display for DynamicFieldType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DynamicFieldType::DynamicField => write!(f, "DynamicField"),
            DynamicFieldType::DynamicObject => write!(f, "DynamicObject"),
        }
    }
}

impl DynamicFieldInfo {
    pub fn is_dynamic_field(tag: &StructTag) -> bool {
        tag.address == SUI_FRAMEWORK_ADDRESS
            && tag.module.as_ident_str() == DYNAMIC_FIELD_MODULE_NAME
            && tag.name.as_ident_str() == DYNAMIC_FIELD_FIELD_STRUCT_NAME
    }

    pub fn is_dynamic_object_field_wrapper(tag: &StructTag) -> bool {
        tag.address == SUI_FRAMEWORK_ADDRESS
            && tag.module.as_ident_str() == DYNAMIC_OBJECT_FIELD_MODULE_NAME
            && tag.name.as_ident_str() == DYNAMIC_OBJECT_FIELD_WRAPPER_STRUCT_NAME
    }

    pub fn dynamic_field_type(key: TypeTag, value: TypeTag) -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            name: DYNAMIC_FIELD_FIELD_STRUCT_NAME.to_owned(),
            module: DYNAMIC_FIELD_MODULE_NAME.to_owned(),
            type_params: vec![key, value],
        }
    }

    pub fn dynamic_object_field_wrapper(key: TypeTag) -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: DYNAMIC_OBJECT_FIELD_MODULE_NAME.to_owned(),
            name: DYNAMIC_OBJECT_FIELD_WRAPPER_STRUCT_NAME.to_owned(),
            type_params: vec![key],
        }
    }

    pub fn try_extract_field_name(tag: &StructTag, type_: &DynamicFieldType) -> SuiResult<TypeTag> {
        match (type_, tag.type_params.first()) {
            (DynamicFieldType::DynamicField, Some(name_type)) => Ok(name_type.clone()),
            (DynamicFieldType::DynamicObject, Some(TypeTag::Struct(s))) => Ok(s
                .type_params
                .first()
                .ok_or_else(|| SuiErrorKind::ObjectDeserializationError {
                    error: format!("Error extracting dynamic object name from object: {tag}"),
                })?
                .clone()),
            _ => Err(SuiErrorKind::ObjectDeserializationError {
                error: format!("Error extracting dynamic object name from object: {tag}"),
            }
            .into()),
        }
    }

    pub fn try_extract_field_value(tag: &StructTag) -> SuiResult<TypeTag> {
        match tag.type_params.last() {
            Some(value_type) => Ok(value_type.clone()),
            None => Err(SuiErrorKind::ObjectDeserializationError {
                error: format!("Error extracting dynamic object value from object: {tag}"),
            }
            .into()),
        }
    }

    pub fn parse_move_object(
        move_struct: &MoveStruct,
    ) -> SuiResult<(MoveValue, DynamicFieldType, ObjectID)> {
        let name = extract_field_from_move_struct(move_struct, "name").ok_or_else(|| {
            SuiErrorKind::ObjectDeserializationError {
                error: "Cannot extract [name] field from sui::dynamic_field::Field".to_string(),
            }
        })?;

        let value = extract_field_from_move_struct(move_struct, "value").ok_or_else(|| {
            SuiErrorKind::ObjectDeserializationError {
                error: "Cannot extract [value] field from sui::dynamic_field::Field".to_string(),
            }
        })?;

        Ok(if is_dynamic_object(move_struct) {
            let name = match name {
                MoveValue::Struct(name_struct) => {
                    extract_field_from_move_struct(name_struct, "name")
                }
                _ => None,
            }
            .ok_or_else(|| SuiErrorKind::ObjectDeserializationError {
                error: "Cannot extract [name] field from sui::dynamic_object_field::Wrapper."
                    .to_string(),
            })?;
            // ID extracted from the wrapper object
            let object_id = extract_id_value(value).ok_or_else(|| {
                SuiErrorKind::ObjectDeserializationError {
                    error: format!(
                        "Cannot extract dynamic object's object id from \
                        sui::dynamic_field::Field, {value:?}"
                    ),
                }
            })?;
            (name.clone(), DynamicFieldType::DynamicObject, object_id)
        } else {
            // ID of the Field object
            let object_id = extract_object_id(move_struct).ok_or_else(|| {
                SuiErrorKind::ObjectDeserializationError {
                    error: format!(
                        "Cannot extract dynamic object's object id from \
                        sui::dynamic_field::Field, {move_struct:?}",
                    ),
                }
            })?;
            (name.clone(), DynamicFieldType::DynamicField, object_id)
        })
    }
}

pub fn extract_field_from_move_struct<'a>(
    move_struct: &'a MoveStruct,
    field_name: &str,
) -> Option<&'a MoveValue> {
    move_struct.fields.iter().find_map(|(id, value)| {
        if id.to_string() == field_name {
            Some(value)
        } else {
            None
        }
    })
}

fn extract_object_id(value: &MoveStruct) -> Option<ObjectID> {
    // id:UID is the first value in an object
    let uid_value = &value.fields.first()?.1;

    // id is the first value in UID
    let id_value = match uid_value {
        MoveValue::Struct(MoveStruct { fields, .. }) => &fields.first()?.1,
        _ => return None,
    };
    extract_id_value(id_value)
}

pub fn extract_id_value(id_value: &MoveValue) -> Option<ObjectID> {
    // the id struct has a single bytes field
    let id_bytes_value = match id_value {
        MoveValue::Struct(MoveStruct { fields, .. }) => &fields.first()?.1,
        _ => return None,
    };
    // the bytes field should be an address
    match id_bytes_value {
        MoveValue::Address(addr) => Some(ObjectID::from(*addr)),
        _ => None,
    }
}

pub fn is_dynamic_object(move_struct: &MoveStruct) -> bool {
    matches!(
        &move_struct.type_.type_params[0],
        TypeTag::Struct(tag) if DynamicFieldInfo::is_dynamic_object_field_wrapper(tag)
    )
}

pub fn derive_dynamic_field_id<T>(
    parent: T,
    key_type_tag: &TypeTag,
    key_bytes: &[u8],
) -> Result<ObjectID, bcs::Error>
where
    T: Into<SuiAddress>,
{
    let parent: SuiAddress = parent.into();
    let k_tag_bytes = bcs::to_bytes(key_type_tag)?;
    tracing::trace!(
        "Deriving dynamic field ID for parent={:?}, key={:?}, key_type_tag={}",
        parent,
        key_bytes,
        key_type_tag.to_canonical_display(true),
    );

    // hash(parent || len(key) || key || key_type_tag)
    let mut hasher = DefaultHash::default();
    hasher.update([HashingIntentScope::ChildObjectId as u8]);
    hasher.update(parent);
    hasher.update(key_bytes.len().to_le_bytes());
    hasher.update(key_bytes);
    hasher.update(k_tag_bytes);
    let hash = hasher.finalize();

    // truncate into an ObjectID and return
    // OK to access slice because digest should never be shorter than ObjectID::LENGTH.
    let id = ObjectID::try_from(&hash.as_ref()[0..ObjectID::LENGTH]).unwrap();
    tracing::trace!("derive_dynamic_field_id result: {:?}", id);
    Ok(id)
}

pub fn serialize_dynamic_field<K, V>(id: &UID, name: &K, value: V) -> Result<Vec<u8>, SuiError>
where
    K: Serialize + Clone,
    V: Serialize,
{
    let field = Field::<K, V> {
        id: id.clone(),
        name: name.clone(),
        value,
    };

    bcs::to_bytes(&field).map_err(|err| {
        SuiErrorKind::ObjectSerializationError {
            error: err.to_string(),
        }
        .into()
    })
}

/// Given a parent object ID (e.g. a table), and a `key`, retrieve the corresponding dynamic field object
/// from the `object_store`. The key type `K` must implement `MoveTypeTagTrait` which has an associated
/// function that returns the Move type tag.
/// Note that this function returns the Field object itself, not the value in the field.
pub fn get_dynamic_field_object_from_store<K>(
    object_store: &dyn ObjectStore,
    parent_id: ObjectID,
    key: &K,
) -> Result<Object, SuiError>
where
    K: MoveTypeTagTrait + Serialize + DeserializeOwned + fmt::Debug + Clone,
{
    Ok(DynamicFieldKey(parent_id, key.clone(), K::get_type_tag())
        .into_unbounded_id()?
        .expect_object(key, object_store)?
        .into_object())
}

/// Similar to `get_dynamic_field_object_from_store`, but returns the value in the field instead of
/// the Field object itself.
pub fn get_dynamic_field_from_store<K, V>(
    object_store: &dyn ObjectStore,
    parent_id: ObjectID,
    key: &K,
) -> Result<V, SuiError>
where
    K: MoveTypeTagTrait + Serialize + DeserializeOwned + fmt::Debug + Clone,
    V: Serialize + DeserializeOwned,
{
    DynamicFieldKey(parent_id, key.clone(), K::get_type_tag())
        .into_unbounded_id()?
        .expect_object(key, object_store)?
        .load_value::<V>()
}

/// A chainable API for getting dynamic fields.
///
/// This allows you to start with either:
/// - a parent ID + key
/// - or pre-hashed child ID,
///
/// and then allows you to read the field
/// - consistently (with a version bound)
/// - or inconsistently (no version bound)
///
/// And take the object that was read from the store and:
/// - return the raw object
/// - or deserialize the field value from the object
/// - or just check if it exists
///
/// By chaining these together, we can have all the options above without
/// a cross-product of functions for each case.
///
/// DynamicFieldKey represents the inputs into a dynamic field id computation (hash).
/// Use it to start a lookup if you know the parent and key.
pub struct DynamicFieldKey<ParentID, K>(pub ParentID, pub K, pub TypeTag);

impl<ParentID, K> DynamicFieldKey<ParentID, K>
where
    ParentID: Into<SuiAddress> + Into<ObjectID> + Copy,
    K: Serialize + fmt::Debug,
{
    /// Get the computed ID of the dynamic field.
    pub fn object_id(&self) -> Result<ObjectID, SuiError> {
        derive_dynamic_field_id(self.0, &self.2, &bcs::to_bytes(&self.1).unwrap())
            .map_err(|e| SuiErrorKind::DynamicFieldReadError(e.to_string()).into())
    }

    /// Convert the key into a UnboundedDynamicFieldID, which can be used to load the latest
    /// version of the field object.
    pub fn into_unbounded_id(self) -> Result<UnboundedDynamicFieldID<K>, SuiError> {
        let id = self.object_id()?;
        Ok(UnboundedDynamicFieldID::<K>::new(self.0.into(), id))
    }

    /// Convert the key into a BoundedDynamicFieldID, which can be used to load the field object
    /// with a version bound for consistent reads.
    pub fn into_id_with_bound(
        self,
        parent_version: SequenceNumber,
    ) -> Result<BoundedDynamicFieldID<K>, SuiError> {
        let id = self.object_id()?;
        Ok(BoundedDynamicFieldID::<K>::new(
            self.0.into(),
            id,
            parent_version,
        ))
    }

    /// Convert the key into a DynamicField, which contains the `Field<K, V>` object,
    /// that is, the value that would be stored in the field object.
    ///
    /// Used to compute the contents of a field object without writing and then reading it back.
    pub fn into_field<V>(self, value: V) -> Result<DynamicField<K, V>, SuiError>
    where
        V: Serialize + DeserializeOwned + MoveTypeTagTrait,
    {
        let id = self.object_id()?;
        let field = Field::<K, V> {
            id: UID::new(id),
            name: self.1,
            value,
        };
        let type_tag = TypeTag::Struct(Box::new(DynamicFieldInfo::dynamic_field_type(
            self.2,
            V::get_type_tag(),
        )));
        Ok(DynamicField(field, type_tag))
    }
}

/// A DynamicField is a `Field<K, V>` object, that is, the value that would be stored in the field object.
pub struct DynamicField<K, V>(Field<K, V>, TypeTag);

impl<K, V> DynamicField<K, V>
where
    K: Serialize,
    V: Serialize,
{
    /// Convert the internal `Field<K, V>` object into a Move object.
    /// Use this to create a Move object in memory without reading one from the store.
    ///
    /// IMPORTANT: Do not call except in tests to avoid possible conservation bugs.
    /// (For instance, you can simply create a Move object that contains minted SUI. If
    /// you then write this object to the db, it will break conservation.)
    pub fn into_move_object_unsafe_for_testing(
        self,
        version: SequenceNumber,
    ) -> Result<MoveObject, SuiError> {
        let field = self.0;
        let type_tag = self.1;
        let TypeTag::Struct(struct_tag) = type_tag else {
            unreachable!()
        };
        // TODO(address-balances): more efficient type repr
        let move_object_type = MoveObjectType::from(*struct_tag);

        let field_bytes =
            bcs::to_bytes(&field).map_err(|e| SuiErrorKind::ObjectSerializationError {
                error: e.to_string(),
            })?;
        Ok(unsafe {
            MoveObject::new_from_execution_with_limit(
                move_object_type,
                false, // A dynamic field is never transferable, public or otherwise.
                version,
                field_bytes,
                512,
            )
        }?)
    }

    /// Get the internal `Field<K, V>` object.
    pub fn into_inner(self) -> Field<K, V> {
        self.0
    }
}

/// A UnboundedDynamicFieldID contains the material needed to load an a dynamic field from
/// the store.
///
/// Can be obtained from a DynamicFieldKey, or created directly if you know the
/// parent and child IDs but do not know the key.
pub struct UnboundedDynamicFieldID<K: Serialize>(
    pub ObjectID, // parent
    pub ObjectID, // child
    std::marker::PhantomData<K>,
);

impl<K> UnboundedDynamicFieldID<K>
where
    K: Serialize + std::fmt::Debug,
{
    /// Create a UnboundedDynamicFieldID from a parent and child ID.
    pub fn new(parent: ObjectID, id: ObjectID) -> Self {
        Self(parent, id, std::marker::PhantomData)
    }

    /// Load the field object from the store.
    pub fn load_object(self, object_store: &dyn ObjectStore) -> Option<DynamicFieldObject<K>> {
        object_store
            .get_object(&self.1)
            .map(DynamicFieldObject::<K>::new)
    }

    /// Load the field object from the store.
    /// If the field does not exist, return an error.
    pub fn expect_object(
        self,
        key: &K,
        object_store: &dyn ObjectStore,
    ) -> Result<DynamicFieldObject<K>, SuiError> {
        let parent = self.0;
        let id = self.1;
        self.load_object(object_store).ok_or_else(|| {
            {
                SuiErrorKind::DynamicFieldReadError(format!(
                    "Dynamic field with key={:?} and ID={:?} not found on parent {:?}",
                    key, id, parent
                ))
            }
            .into()
        })
    }

    /// Check if the field object exists in the store.
    pub fn exists(self, object_store: &dyn ObjectStore) -> bool {
        self.load_object(object_store).is_some()
    }

    /// Convert an UnboundedDynamicFieldID into a BoundedDynamicFieldID, which can then
    /// be used to do a consistent lookup of the field.
    pub fn with_bound(self, parent_version: SequenceNumber) -> BoundedDynamicFieldID<K> {
        BoundedDynamicFieldID::new(self.0, self.1, parent_version)
    }

    /// Get the child ID.
    pub fn as_object_id(self) -> ObjectID {
        self.1
    }
}

/// A BoundedDynamicFieldID contains the material needed to load an a dynamic field from
/// the store, along with a parent version bound. The returned field will be the highest
/// version of the field object that has a version less than or equal to the bound.
///
/// Can be obtained from a DynamicFieldID by calling `with_bound`, or created directly
/// if you know the parent and child IDs and the parent version bound.
pub struct BoundedDynamicFieldID<K: Serialize>(
    pub ObjectID,       // parent
    pub ObjectID,       // child
    pub SequenceNumber, // parent version
    std::marker::PhantomData<K>,
);

impl<K> BoundedDynamicFieldID<K>
where
    K: Serialize,
{
    /// Create a BoundedDynamicFieldID from a parent and child ID and a parent version.
    pub fn new(parent_id: ObjectID, child_id: ObjectID, parent_version: SequenceNumber) -> Self {
        Self(
            parent_id,
            child_id,
            parent_version,
            std::marker::PhantomData,
        )
    }

    /// Load the field object from the store.
    /// If the field does not exist, return None.
    pub fn load_object(
        self,
        child_object_resolver: &dyn ChildObjectResolver,
    ) -> Result<Option<DynamicFieldObject<K>>, SuiError> {
        child_object_resolver
            .read_child_object(&self.0, &self.1, self.2)
            .map(|r| r.map(DynamicFieldObject::<K>::new))
    }

    /// Check if the field object exists in the store.
    pub fn exists(self, child_object_resolver: &dyn ChildObjectResolver) -> Result<bool, SuiError> {
        self.load_object(child_object_resolver).map(|r| r.is_some())
    }
}

/// A DynamicFieldObject is a wrapper around an Object that contains a `Field<K, V>` object.
pub struct DynamicFieldObject<K>(pub Object, std::marker::PhantomData<K>);

impl<K> DynamicFieldObject<K> {
    /// Create a DynamicFieldObject directly from an Object.
    pub fn new(object: Object) -> Self {
        Self(object, std::marker::PhantomData)
    }

    /// Get the underlying Object.
    pub fn into_object(self) -> Object {
        self.0
    }

    pub fn as_object(&self) -> &Object {
        &self.0
    }
}

impl<K> DynamicFieldObject<K>
where
    K: Serialize + DeserializeOwned,
{
    /// Deserialize the field value from the object. Requires that the value type is known.
    pub fn load_value<V>(self) -> Result<V, SuiError>
    where
        V: Serialize + DeserializeOwned,
    {
        self.load_field::<V>().map(|f| f.value)
    }

    /// Deserialize the field value from the object. Requires that the value type is known.
    pub fn load_field<V>(self) -> Result<Field<K, V>, SuiError>
    where
        V: Serialize + DeserializeOwned,
    {
        let object = self.0;
        let move_object = object.data.try_as_move().ok_or_else(|| {
            SuiErrorKind::DynamicFieldReadError(format!(
                "Dynamic field {:?} is not a Move object",
                object.id()
            ))
        })?;
        bcs::from_bytes::<Field<K, V>>(move_object.contents())
            .map_err(|err| SuiErrorKind::DynamicFieldReadError(err.to_string()).into())
    }
}
