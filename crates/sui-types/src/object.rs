// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::convert::{TryFrom, TryInto};
use std::fmt::{Debug, Display, Formatter};
use std::mem::size_of;

use move_binary_format::CompiledModule;
use move_bytecode_utils::layout::TypeLayoutBuilder;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::language_storage::StructTag;
use move_core_types::language_storage::TypeTag;
use move_core_types::value::{MoveStruct, MoveStructLayout, MoveTypeLayout};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use serde_with::Bytes;

use crate::crypto::{sha3_hash, BcsSignable};
use crate::error::{ExecutionError, ExecutionErrorKind};
use crate::error::{SuiError, SuiResult};
use crate::move_package::MovePackage;
use crate::{
    base_types::{
        ObjectDigest, ObjectID, ObjectRef, SequenceNumber, SuiAddress, TransactionDigest,
    },
    gas_coin::GasCoin,
};

pub const GAS_VALUE_FOR_TESTING: u64 = 100000_u64;
pub const OBJECT_START_VERSION: SequenceNumber = SequenceNumber::from_u64(1);

#[serde_as]
#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash)]
pub struct MoveObject {
    pub type_: StructTag,
    /// Determines if it is usable with the TransferObject
    /// Derived from the type_
    has_public_transfer: bool,
    #[serde_as(as = "Bytes")]
    contents: Vec<u8>,
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
    /// Creates a new Move object of type `type_` with BCS encoded bytes in `contents`
    /// `has_public_transfer` is determined by the abilities of the `type_`, but resolving
    /// the abilities requires the compiled modules of the `type_: StructTag`.
    /// In other words, `has_public_transfer` will be the same for all objects of the same `type_`.
    ///
    /// # Safety
    ///
    /// This function should ONLY be called if has_public_transfer has been determined by the type_.
    /// Yes, this is a bit of an abuse of the `unsafe` marker, but bad things will happen if this
    /// is inconsistent
    pub unsafe fn new_from_execution(
        type_: StructTag,
        has_public_transfer: bool,
        contents: Vec<u8>,
    ) -> Self {
        // coins should always have public transfer, as they always should have store.
        // Thus, type_ == GasCoin::type_() ==> has_public_transfer
        debug_assert!(type_ != GasCoin::type_() || has_public_transfer);
        Self {
            type_,
            has_public_transfer,
            contents,
        }
    }

    pub fn new_gas_coin(contents: Vec<u8>) -> Self {
        unsafe { Self::new_from_execution(GasCoin::type_(), true, contents) }
    }

    pub fn has_public_transfer(&self) -> bool {
        self.has_public_transfer
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
    pub fn update_contents_and_increment_version(&mut self, new_contents: Vec<u8>) {
        self.update_contents_without_version_change(new_contents);
        self.increment_version();
    }

    /// Update the contents of this object but does not increment its version
    pub fn update_contents_without_version_change(&mut self, new_contents: Vec<u8>) {
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

    /// Get a `MoveStructLayout` for `self`.
    /// The `resolver` value must contain the module that declares `self.type_` and the (transitive)
    /// dependencies of `self.type_` in order for this to succeed. Failure will result in an `ObjectSerializationError`
    pub fn get_layout(
        &self,
        format: ObjectFormatOptions,
        resolver: &impl GetModule,
    ) -> Result<MoveStructLayout, SuiError> {
        Self::get_layout_from_struct_tag(self.type_.clone(), format, resolver)
    }

    pub fn get_layout_from_struct_tag(
        struct_tag: StructTag,
        format: ObjectFormatOptions,
        resolver: &impl GetModule,
    ) -> Result<MoveStructLayout, SuiError> {
        let type_ = TypeTag::Struct(struct_tag);
        let layout = if format.include_types {
            TypeLayoutBuilder::build_with_types(&type_, resolver)
        } else {
            TypeLayoutBuilder::build_with_fields(&type_, resolver)
        }
        .map_err(|e| SuiError::ObjectSerializationError {
            error: e.to_string(),
        })?;
        match layout {
            MoveTypeLayout::Struct(l) => Ok(l),
            _ => unreachable!(
                "We called build_with_types on Struct type, should get a struct layout"
            ),
        }
    }

    /// Convert `self` to the JSON representation dictated by `layout`.
    pub fn to_move_struct(&self, layout: &MoveStructLayout) -> Result<MoveStruct, SuiError> {
        MoveStruct::simple_deserialize(&self.contents, layout).map_err(|e| {
            SuiError::ObjectSerializationError {
                error: e.to_string(),
            }
        })
    }

    /// Convert `self` to the JSON representation dictated by `layout`.
    pub fn to_move_struct_with_resolver(
        &self,
        format: ObjectFormatOptions,
        resolver: &impl GetModule,
    ) -> Result<MoveStruct, SuiError> {
        self.to_move_struct(&self.get_layout(format, resolver)?)
    }

    /// Approximate size of the object in bytes. This is used for gas metering.
    /// For the type tag field, we serialize it on the spot to get the accurate size.
    /// This should not be very expensive since the type tag is usually simple, and
    /// we only do this once per object being mutated.
    pub fn object_size_for_gas_metering(&self) -> usize {
        let seriealized_type_tag =
            bcs::to_bytes(&self.type_).expect("Serializing type tag should not fail");
        // + 1 for 'has_public_transfer'
        self.contents.len() + seriealized_type_tag.len() + 1
    }
}

#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash)]
#[allow(clippy::large_enum_variant)]
pub enum Data {
    /// An object whose governing logic lives in a published Move module
    Move(MoveObject),
    /// Map from each module name to raw serialized Move module bytes
    Package(MovePackage),
    // ... Sui "native" types go here
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
}

#[derive(
    Eq, PartialEq, Debug, Clone, Copy, Deserialize, Serialize, Hash, JsonSchema, Ord, PartialOrd,
)]
pub enum Owner {
    /// Object is exclusively owned by a single address, and is mutable.
    AddressOwner(SuiAddress),
    /// Object is exclusively owned by a single object, and is mutable.
    /// The object ID is converted to SuiAddress as SuiAddress is universal.
    ObjectOwner(SuiAddress),
    /// Object is shared, can be used by any address, and is mutable.
    Shared,
    /// Object is immutable, and hence ownership doesn't matter.
    Immutable,
}

impl Owner {
    pub fn get_owner_address(&self) -> SuiResult<SuiAddress> {
        match self {
            Self::AddressOwner(address) | Self::ObjectOwner(address) => Ok(*address),
            Self::Shared | Self::Immutable => Err(SuiError::UnexpectedOwnerType),
        }
    }

    pub fn is_immutable(&self) -> bool {
        self == &Owner::Immutable
    }

    pub fn is_owned(&self) -> bool {
        match self {
            Owner::AddressOwner(_) | Owner::ObjectOwner(_) => true,
            Owner::Shared | Owner::Immutable => false,
        }
    }

    pub fn is_shared(&self) -> bool {
        matches!(self, Owner::Shared)
    }
}

impl PartialEq<SuiAddress> for Owner {
    fn eq(&self, other: &SuiAddress) -> bool {
        match self {
            Self::AddressOwner(address) => address == other,
            Self::ObjectOwner(_) | Self::Shared | Self::Immutable => false,
        }
    }
}

impl PartialEq<ObjectID> for Owner {
    fn eq(&self, other: &ObjectID) -> bool {
        let other_id: SuiAddress = (*other).into();
        match self {
            Self::ObjectOwner(id) => id == &other_id,
            Self::AddressOwner(_) | Self::Shared | Self::Immutable => false,
        }
    }
}

impl Display for Owner {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AddressOwner(address) => {
                write!(f, "Account Address ( {} )", address)
            }
            Self::ObjectOwner(address) => {
                write!(f, "Object ID: ( {} )", address)
            }
            Self::Immutable => {
                write!(f, "Immutable")
            }
            Self::Shared => {
                write!(f, "Shared")
            }
        }
    }
}

#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash)]
pub struct Object {
    /// The meat of the object
    pub data: Data,
    /// The owner that unlocks this object
    pub owner: Owner,
    /// The digest of the transaction that created or last mutated this object
    pub previous_transaction: TransactionDigest,
    /// The amount of SUI we would rebate if this object gets deleted.
    /// This number is re-calculated each time the object is mutated based on
    /// the present storage gas price.
    pub storage_rebate: u64,
}

impl BcsSignable for Object {}

impl Object {
    /// Create a new Move object
    pub fn new_move(o: MoveObject, owner: Owner, previous_transaction: TransactionDigest) -> Self {
        Object {
            data: Data::Move(o),
            owner,
            previous_transaction,
            storage_rebate: 0,
        }
    }

    // Note: this will panic if `modules` is empty
    pub fn new_package(
        modules: Vec<CompiledModule>,
        previous_transaction: TransactionDigest,
    ) -> Self {
        Object {
            data: Data::Package(MovePackage::from_iter(modules)),
            owner: Owner::Immutable,
            previous_transaction,
            storage_rebate: 0,
        }
    }

    pub fn is_immutable(&self) -> bool {
        self.owner.is_immutable()
    }

    pub fn is_owned(&self) -> bool {
        self.owner.is_owned()
    }

    pub fn is_shared(&self) -> bool {
        self.owner.is_shared()
    }

    pub fn get_single_owner(&self) -> Option<SuiAddress> {
        self.owner.get_owner_address().ok()
    }

    // It's a common pattern to retrieve both the owner and object ID
    // together, if it's owned by a singler owner.
    pub fn get_owner_and_id(&self) -> Option<(Owner, ObjectID)> {
        Some((self.owner, self.id()))
    }

    /// Return true if this object is a Move package, false if it is a Move value
    pub fn is_package(&self) -> bool {
        matches!(&self.data, Data::Package(_))
    }

    pub fn compute_object_reference(&self) -> ObjectRef {
        (self.id(), self.version(), self.digest())
    }

    pub fn id(&self) -> ObjectID {
        use Data::*;

        match &self.data {
            Move(v) => v.id(),
            Package(m) => m.id(),
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

    /// Approximate size of the object in bytes. This is used for gas metering.
    /// This will be slgihtly different from the serialized size, but
    /// we also don't want to serialize the object just to get the size.
    /// This approximation should be good enough for gas metering.
    pub fn object_size_for_gas_metering(&self) -> usize {
        let meta_data_size = size_of::<Owner>() + size_of::<TransactionDigest>() + size_of::<u64>();
        let data_size = match &self.data {
            Data::Move(m) => m.object_size_for_gas_metering(),
            Data::Package(p) => p
                .serialized_module_map()
                .iter()
                .map(|(name, module)| name.len() + module.len())
                .sum(),
        };
        meta_data_size + data_size
    }

    /// Change the owner of `self` to `new_owner`. This function does not increase the version
    /// number of the object.
    pub fn transfer_without_version_change(
        &mut self,
        new_owner: SuiAddress,
    ) -> Result<(), ExecutionError> {
        self.ensure_public_transfer_eligible()?;
        self.owner = Owner::AddressOwner(new_owner);
        Ok(())
    }

    /// Change the owner of `self` to `new_owner`. This function will increment the version
    /// number of the object after transfer.
    pub fn transfer_and_increment_version(
        &mut self,
        new_owner: SuiAddress,
    ) -> Result<(), ExecutionError> {
        self.transfer_without_version_change(new_owner)?;
        let data = self.data.try_as_move_mut().unwrap();
        data.increment_version();
        Ok(())
    }

    pub fn immutable_with_id_for_testing(id: ObjectID) -> Self {
        let data = Data::Move(MoveObject {
            type_: GasCoin::type_(),
            has_public_transfer: true,
            contents: GasCoin::new(id, SequenceNumber::new(), GAS_VALUE_FOR_TESTING).to_bcs_bytes(),
        });
        Self {
            owner: Owner::Immutable,
            data,
            previous_transaction: TransactionDigest::genesis(),
            storage_rebate: 0,
        }
    }

    pub fn with_id_owner_gas_for_testing(id: ObjectID, owner: SuiAddress, gas: u64) -> Self {
        let data = Data::Move(MoveObject {
            type_: GasCoin::type_(),
            has_public_transfer: true,
            contents: GasCoin::new(id, SequenceNumber::new(), gas).to_bcs_bytes(),
        });
        Self {
            owner: Owner::AddressOwner(owner),
            data,
            previous_transaction: TransactionDigest::genesis(),
            storage_rebate: 0,
        }
    }

    pub fn with_id_owner_for_testing(id: ObjectID, owner: SuiAddress) -> Self {
        // For testing, we provide sufficient gas by default.
        Self::with_id_owner_gas_for_testing(id, owner, GAS_VALUE_FOR_TESTING)
    }

    pub fn with_id_owner_version_for_testing(
        id: ObjectID,
        version: SequenceNumber,
        owner: SuiAddress,
    ) -> Self {
        let data = Data::Move(MoveObject {
            type_: GasCoin::type_(),
            has_public_transfer: true,
            contents: GasCoin::new(id, version, GAS_VALUE_FOR_TESTING).to_bcs_bytes(),
        });
        Self {
            owner: Owner::AddressOwner(owner),
            data,
            previous_transaction: TransactionDigest::genesis(),
            storage_rebate: 0,
        }
    }

    pub fn with_owner_for_testing(owner: SuiAddress) -> Self {
        Self::with_id_owner_for_testing(ObjectID::random(), owner)
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

    /// Treat the object type as a Move struct with one type parameter,
    /// like this: `S<T>`.
    /// Returns the inner parameter type `T`.
    pub fn get_move_template_type(&self) -> SuiResult<TypeTag> {
        let move_struct = self.data.type_().ok_or_else(|| SuiError::TypeError {
            error: "Object must be a Move object".to_owned(),
        })?;
        fp_ensure!(
            move_struct.type_params.len() == 1,
            SuiError::TypeError {
                error: "Move object struct must have one type parameter".to_owned()
            }
        );
        // Index access safe due to checks above.
        let type_tag = move_struct.type_params[0].clone();
        Ok(type_tag)
    }

    pub fn ensure_public_transfer_eligible(&self) -> Result<(), ExecutionError> {
        if !matches!(self.owner, Owner::AddressOwner(_)) {
            return Err(ExecutionErrorKind::TransferUnowned.into());
        }
        let has_public_transfer = match &self.data {
            Data::Move(m) => m.has_public_transfer(),
            Data::Package(_) => false,
        };
        if !has_public_transfer {
            return Err(ExecutionErrorKind::TransferObjectWithoutPublicTransfer.into());
        }
        Ok(())
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "status", content = "details")]
pub enum ObjectRead {
    NotExists(ObjectID),
    Exists(ObjectRef, Object, Option<MoveStructLayout>),
    Deleted(ObjectRef),
}

impl ObjectRead {
    /// Returns the object value if there is any, otherwise an Err if
    /// the object does not exist or is deleted.
    pub fn into_object(self) -> Result<Object, SuiError> {
        match self {
            Self::Deleted(oref) => Err(SuiError::ObjectDeleted { object_ref: oref }),
            Self::NotExists(id) => Err(SuiError::ObjectNotFound { object_id: id }),
            Self::Exists(_, o, _) => Ok(o),
        }
    }
}

impl Default for ObjectFormatOptions {
    fn default() -> Self {
        ObjectFormatOptions {
            include_types: true,
        }
    }
}
