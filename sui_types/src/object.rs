// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::binary_views::BinaryIndexedView;
use move_bytecode_utils::layout::TypeLayoutBuilder;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::language_storage::TypeTag;
use move_core_types::value::{MoveStruct, MoveStructLayout, MoveTypeLayout};
use move_disassembler::disassembler::Disassembler;
use move_ir_types::location::Spanned;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use serde_json::{json, Value};
use serde_with::{serde_as, Bytes};
use std::collections::BTreeMap;
use std::convert::{TryFrom, TryInto};
use std::fmt::{Debug, Display, Formatter};

use move_binary_format::CompiledModule;
use move_core_types::language_storage::StructTag;

use crate::error::SuiError;
use crate::{
    base_types::{
        sha3_hash, BcsSignable, ObjectDigest, ObjectID, ObjectRef, SequenceNumber, SuiAddress,
        TransactionDigest,
    },
    gas_coin::GasCoin,
};

pub const GAS_VALUE_FOR_TESTING: u64 = 100000_u64;
pub const OBJECT_START_VERSION: SequenceNumber = SequenceNumber::from_u64(1);

#[serde_as]
#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash)]
pub struct MoveObject {
    pub type_: StructTag,
    #[serde_as(as = "Bytes")]
    contents: Vec<u8>,
    read_only: bool,
}

/// Byte encoding of a 64 byte unsigned integer in BCS
type BcsU64 = [u8; 8];
/// Index marking the end of the object's ID + the beginning of its version
const ID_END_INDEX: usize = ObjectID::LENGTH;
/// Index marking the end of the object's version + the beginning of type-specific data
const VERSION_END_INDEX: usize = ID_END_INDEX + 8;

/// Different schemes for converting a Move value into a structured representation
#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash)]
pub struct ObjectFormatOptions {
    /// If true, include the type of each object as well as its fields; e.g.:
    /// `{ "fields": { "f": 20, "g": { "fields" { "h": true }, "type": "0x0::MyModule::MyNestedType" }, "type": "0x0::MyModule::MyType" }`
    ///  If false, include field names only; e.g.:
    /// `{ "f": 20, "g": { "h": true } }`
    include_types: bool,
}

impl MoveObject {
    pub fn new(type_: StructTag, contents: Vec<u8>) -> Self {
        Self {
            type_,
            contents,
            read_only: false,
        }
    }

    pub fn id(&self) -> ObjectID {
        ObjectID::try_from(&self.contents[0..ID_END_INDEX]).unwrap()
    }

    pub fn version(&self) -> SequenceNumber {
        SequenceNumber::from(u64::from_le_bytes(*self.version_bytes()))
    }

    /// Contents of the object that are specific to its type--i.e., not its ID and version, which all objects have
    /// For example if the object was declared as `struct S has key { id: ID, f1: u64, f2: bool },
    /// this returns the slice containing `f1` and `f2`.
    pub fn type_specific_contents(&self) -> &[u8] {
        &self.contents[VERSION_END_INDEX..]
    }

    pub fn id_version_contents(&self) -> &[u8] {
        &self.contents[..VERSION_END_INDEX]
    }

    /// Update the contents of this object and increment its version
    pub fn update_contents(&mut self, new_contents: Vec<u8>) {
        #[cfg(debug_assertions)]
        let old_id = self.id();
        #[cfg(debug_assertions)]
        let old_version = self.version();

        self.contents = new_contents;

        #[cfg(debug_assertions)]
        {
            // caller should never overwrite ID or version
            debug_assert_eq!(self.id(), old_id);
            debug_assert_eq!(self.version(), old_version);
        }

        self.increment_version();
    }

    /// Increase the version of this object by one
    pub fn increment_version(&mut self) {
        let new_version = self.version().increment();
        // TODO: better bit tricks are probably possible here. for now, just do the obvious thing
        self.version_bytes_mut()
            .copy_from_slice(bcs::to_bytes(&new_version).unwrap().as_slice());
    }

    fn version_bytes(&self) -> &BcsU64 {
        self.contents[ID_END_INDEX..VERSION_END_INDEX]
            .try_into()
            .unwrap()
    }

    fn version_bytes_mut(&mut self) -> &mut [u8] {
        &mut self.contents[ID_END_INDEX..VERSION_END_INDEX]
    }

    pub fn contents(&self) -> &[u8] {
        &self.contents
    }

    pub fn into_contents(self) -> Vec<u8> {
        self.contents
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub fn freeze(&mut self) {
        self.read_only = true;
    }

    /// Get a `MoveStructLayout` for `self`.
    /// The `resolver` value must contain the module that declares `self.type_` and the (transitive)
    /// dependencies of `self.type_` in order for this to succeed. Failure will result in an `ObjectSerializationError`
    pub fn get_layout(
        &self,
        format: ObjectFormatOptions,
        resolver: &impl GetModule,
    ) -> Result<MoveStructLayout, SuiError> {
        let type_ = TypeTag::Struct(self.type_.clone());
        let layout = if format.include_types {
            TypeLayoutBuilder::build_with_types(&type_, resolver)
        } else {
            TypeLayoutBuilder::build_with_fields(&type_, resolver)
        }
        .map_err(|_e| SuiError::ObjectSerializationError)?;
        match layout {
            MoveTypeLayout::Struct(l) => Ok(l),
            _ => unreachable!(
                "We called build_with_types on Struct type, should get a struct layout"
            ),
        }
    }

    /// Convert `self` to the JSON representation dictated by `layout`.
    pub fn to_json(&self, layout: &MoveStructLayout) -> Result<Value, SuiError> {
        let move_value = MoveStruct::simple_deserialize(&self.contents, layout)
            .map_err(|_e| SuiError::ObjectSerializationError)?;
        serde_json::to_value(&move_value).map_err(|_e| SuiError::ObjectSerializationError)
    }
}

// TODO: Make MovePackage a NewType so that we can implement functions on it.
// serde_bytes::ByteBuf is an analog of Vec<u8> with built-in fast serialization.
pub type MovePackage = BTreeMap<String, ByteBuf>;

#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash)]
#[allow(clippy::large_enum_variant)]
pub enum Data {
    /// An object whose governing logic lives in a published Move module
    Move(MoveObject),
    /// Map from each module name to raw serialized Move module bytes
    Package(MovePackage),
    // ... FastX "native" types go here
}

impl Data {
    pub fn try_as_move(&self) -> Option<&MoveObject> {
        use Data::*;
        match self {
            Move(m) => Some(m),
            Package(_) => None,
        }
    }

    pub fn try_as_move_mut(&mut self) -> Option<&mut MoveObject> {
        use Data::*;
        match self {
            Move(m) => Some(m),
            Package(_) => None,
        }
    }

    pub fn try_as_package(&self) -> Option<&MovePackage> {
        use Data::*;
        match self {
            Move(_) => None,
            Package(p) => Some(p),
        }
    }

    pub fn type_(&self) -> Option<&StructTag> {
        use Data::*;
        match self {
            Move(m) => Some(&m.type_),
            Package(_) => None,
        }
    }

    /// Convert `self` to the JSON representation dictated by `format`.
    /// If `self` is a Move value, the `resolver` value must contain the module that declares `self.type_` and the (transitive)
    /// dependencies of `self.type_` in order for this to succeed. Failure will result in an `ObjectSerializationError`
    pub fn to_json_with_resolver(
        &self,
        format: ObjectFormatOptions,
        resolver: &impl GetModule,
    ) -> Result<Value, SuiError> {
        let layout = match self {
            Data::Move(m) => Some(m.get_layout(format, resolver)?),
            Data::Package(_) => None,
        };
        self.to_json(&layout)
    }

    /// Convert `self` to the JSON representation dictated by `format`.
    /// If `self` is a Move value, the `resolver` value must contain the module that declares `self.type_` and the (transitive)
    /// dependencies of `self.type_` in order for this to succeed. Failure will result in an `ObjectSerializationError`
    pub fn to_json(&self, layout: &Option<MoveStructLayout>) -> Result<Value, SuiError> {
        use Data::*;
        match self {
            Move(m) => match layout {
                Some(l) => m.to_json(l),
                None => Err(SuiError::ObjectSerializationError),
            },
            Package(p) => {
                let mut disassembled = serde_json::Map::new();
                for (name, bytecode) in p {
                    let module = CompiledModule::deserialize(bytecode)
                        .expect("Adapter publish flow ensures that this bytecode deserializes");
                    let view = BinaryIndexedView::Module(&module);
                    let d = Disassembler::from_view(view, Spanned::unsafe_no_loc(()).loc)
                        .map_err(|_e| SuiError::ObjectSerializationError)?;
                    let bytecode_str = d
                        .disassemble()
                        .map_err(|_e| SuiError::ObjectSerializationError)?;
                    disassembled.insert(name.to_string(), Value::String(bytecode_str));
                }
                Ok(Value::Object(disassembled))
            }
        }
    }
}

#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash)]
pub struct Object {
    /// The meat of the object
    pub data: Data,
    /// The owner address that unlocks this object (eg. hashes of public key, or object id)
    pub owner: SuiAddress,
    /// The digest of the order that created or last mutated this object
    pub previous_transaction: TransactionDigest,
}

impl BcsSignable for Object {}

impl Object {
    /// Create a new Move object
    pub fn new_move(
        o: MoveObject,
        owner: SuiAddress,
        previous_transaction: TransactionDigest,
    ) -> Self {
        Object {
            data: Data::Move(o),
            owner,
            previous_transaction,
        }
    }

    pub fn new_package(
        modules: Vec<CompiledModule>,
        owner: SuiAddress,
        previous_transaction: TransactionDigest,
    ) -> Self {
        let serialized: MovePackage = modules
            .into_iter()
            .map(|module| {
                let mut bytes = Vec::new();
                module.serialize(&mut bytes).unwrap();
                (module.self_id().name().to_string(), ByteBuf::from(bytes))
            })
            .collect();
        Object {
            data: Data::Package(serialized),
            owner,
            previous_transaction,
        }
    }

    pub fn is_read_only(&self) -> bool {
        match &self.data {
            Data::Move(m) => m.is_read_only(),
            Data::Package(_) => true,
        }
    }

    /// Return true if this object is a Move package, false if it is a Move value
    pub fn is_package(&self) -> bool {
        matches!(&self.data, Data::Package(_))
    }

    pub fn to_object_reference(&self) -> ObjectRef {
        (self.id(), self.version(), self.digest())
    }

    pub fn id(&self) -> ObjectID {
        use Data::*;

        match &self.data {
            Move(v) => v.id(),
            Package(m) => {
                // All modules in the same package must have the same address.
                // TODO: Use byte trick to get ID directly without deserialization.
                ObjectID::from(
                    *CompiledModule::deserialize(m.values().next().unwrap())
                        .unwrap()
                        .self_id()
                        .address(),
                )
            }
        }
    }

    pub fn version(&self) -> SequenceNumber {
        use Data::*;

        match &self.data {
            Move(v) => v.version(),
            Package(_) => SequenceNumber::from(1), // modules are immutable, version is always 1
        }
    }

    pub fn type_(&self) -> Option<&StructTag> {
        self.data.type_()
    }

    pub fn digest(&self) -> ObjectDigest {
        ObjectDigest::new(sha3_hash(self))
    }

    /// Change the owner of `self` to `new_owner`
    pub fn transfer(&mut self, new_owner: SuiAddress) {
        // TODO: these should be raised SuiError's instead of panic's
        assert!(!self.is_read_only(), "Cannot transfer an immutable object");
        match &mut self.data {
            Data::Move(m) => {
                assert!(
                    m.type_ == GasCoin::type_(),
                    "Invalid transfer: only transfer of GasCoin is supported"
                );

                self.owner = new_owner;
                m.increment_version();
            }
            Data::Package(_) => panic!("Cannot transfer a module object"),
        }
    }

    pub fn with_id_owner_gas_for_testing(
        id: ObjectID,
        version: SequenceNumber,
        owner: SuiAddress,
        gas: u64,
    ) -> Self {
        let data = Data::Move(MoveObject {
            type_: GasCoin::type_(),
            contents: GasCoin::new(id, version, gas).to_bcs_bytes(),
            read_only: false,
        });
        Self {
            owner,
            data,
            previous_transaction: TransactionDigest::genesis(),
        }
    }

    pub fn with_id_owner_for_testing(id: ObjectID, owner: SuiAddress) -> Self {
        // For testing, we provide sufficient gas by default.
        Self::with_id_owner_gas_for_testing(id, SequenceNumber::new(), owner, GAS_VALUE_FOR_TESTING)
    }

    /// Create Coin object for use in Move object operation
    pub fn with_id_owner_gas_coin_object_for_testing(
        id: ObjectID,
        version: SequenceNumber,
        owner: SuiAddress,
        value: u64,
    ) -> Self {
        let obj = GasCoin::new(id, version, value);

        let data = Data::Move(MoveObject {
            type_: GasCoin::type_(),
            contents: bcs::to_bytes(&obj).unwrap(),
            read_only: false,
        });
        Self {
            owner,
            data,
            previous_transaction: TransactionDigest::genesis(),
        }
    }

    /// Get a `MoveStructLayout` for `self`.
    /// The `resolver` value must contain the module that declares `self.type_` and the (transitive)
    /// dependencies of `self.type_` in order for this to succeed. Failure will result in an `ObjectSerializationError`
    pub fn get_layout(
        &self,
        format: ObjectFormatOptions,
        resolver: &impl GetModule,
    ) -> Result<Option<MoveStructLayout>, SuiError> {
        match &self.data {
            Data::Move(m) => Ok(Some(m.get_layout(format, resolver)?)),
            Data::Package(_) => Ok(None),
        }
    }

    /// Convert `self` to the JSON representation dictated by `format`.
    /// If `self` is a Move value, the `resolver` value must contain the module that declares `self.type_` and the (transitive)
    /// dependencies of `self.type_` in order for this to succeed. Failure will result in an `ObjectSerializationError`
    pub fn to_json(&self, layout: &Option<MoveStructLayout>) -> Result<Value, SuiError> {
        let contents = self.data.to_json(layout)?;
        let owner =
            serde_json::to_value(&self.owner).map_err(|_e| SuiError::ObjectSerializationError)?;
        let previous_transaction = serde_json::to_value(&self.previous_transaction)
            .map_err(|_e| SuiError::ObjectSerializationError)?;
        Ok(json!({ "contents": contents, "owner": owner, "tx_digest": previous_transaction }))
    }
}

#[allow(clippy::large_enum_variant)]
pub enum ObjectRead {
    NotExists(ObjectID),
    Exists(ObjectRef, Object, Option<MoveStructLayout>),
    Deleted(ObjectRef),
}

impl ObjectRead {
    /// Returns a reference to the object if there is any, otherwise an Err if
    /// the object does not exist or is deleted.
    pub fn object(&self) -> Result<&Object, SuiError> {
        match &self {
            Self::Deleted(oref) => Err(SuiError::ObjectDeleted { object_ref: *oref }),
            Self::NotExists(id) => Err(SuiError::ObjectNotFound { object_id: *id }),
            Self::Exists(_, o, _) => Ok(o),
        }
    }

    /// Returns the layout of the object if it was requested in the read, None if it was not requested or does not have a layout
    /// Returns an Err if the object does not exist or is deleted.
    pub fn layout(&self) -> Result<&Option<MoveStructLayout>, SuiError> {
        match &self {
            Self::Deleted(oref) => Err(SuiError::ObjectDeleted { object_ref: *oref }),
            Self::NotExists(id) => Err(SuiError::ObjectNotFound { object_id: *id }),
            Self::Exists(_, _, layout) => Ok(layout),
        }
    }

    /// Returns the object ref if there is an object, otherwise an Err if
    /// the object does not exist or is deleted.
    pub fn reference(&self) -> Result<ObjectRef, SuiError> {
        match &self {
            Self::Deleted(oref) => Err(SuiError::ObjectDeleted { object_ref: *oref }),
            Self::NotExists(id) => Err(SuiError::ObjectNotFound { object_id: *id }),
            Self::Exists(oref, _, _) => Ok(*oref),
        }
    }
}

impl Display for Object {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let type_string = self
            .data
            .type_()
            .map_or("Type Unwrap Failed".to_owned(), |type_| {
                format!("{}", type_)
            });

        write!(
            f,
            "Owner: {:?}\nVersion: {:?}\nID: {:?}\nReadonly: {:?}\nType: {}",
            self.owner,
            self.version().value(),
            self.id(),
            self.is_read_only(),
            type_string
        )
    }
}

impl Default for ObjectFormatOptions {
    fn default() -> Self {
        ObjectFormatOptions {
            include_types: true,
        }
    }
}
