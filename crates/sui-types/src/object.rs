// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::convert::TryFrom;
use std::fmt::{Debug, Display, Formatter};
use std::mem::size_of;

use move_binary_format::CompiledModule;
use move_bytecode_utils::layout::TypeLayoutBuilder;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::language_storage::StructTag;
use move_core_types::language_storage::TypeTag;
use move_core_types::value::{MoveStruct, MoveStructLayout, MoveTypeLayout, MoveValue};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use serde_with::Bytes;

use crate::base_types::{MoveObjectType, ObjectIDParseError};
use crate::crypto::{deterministic_random_account_key, sha3_hash};
use crate::error::{ExecutionError, ExecutionErrorKind, UserInputError, UserInputResult};
use crate::error::{SuiError, SuiResult};
use crate::gas_coin::TOTAL_SUPPLY_MIST;
use crate::move_package::MovePackage;
use crate::{
    base_types::{
        ObjectDigest, ObjectID, ObjectRef, SequenceNumber, SuiAddress, TransactionDigest,
    },
    gas_coin::GasCoin,
};
use crate::{MOVE_STDLIB_OBJECT_ID, SUI_FRAMEWORK_OBJECT_ID};
use sui_protocol_config::ProtocolConfig;

pub const GAS_VALUE_FOR_TESTING: u64 = 1_000_000_u64;
pub const OBJECT_START_VERSION: SequenceNumber = SequenceNumber::from_u64(1);

#[serde_as]
#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash)]
pub struct MoveObject {
    /// The type of this object. Immutable
    type_: MoveObjectType,
    /// Determines if it is usable with the TransferObject command
    /// Derived from the type_
    has_public_transfer: bool,
    /// Number that increases each time a tx takes this object as a mutable input
    /// This is a lamport timestamp, not a sequentially increasing version
    version: SequenceNumber,
    /// BCS bytes of a Move struct value
    #[serde_as(as = "Bytes")]
    contents: Vec<u8>,
}

/// Index marking the end of the object's ID + the beginning of its version
pub const ID_END_INDEX: usize = ObjectID::LENGTH;

/// Different schemes for converting a Move value into a structured representation
#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash)]
pub struct ObjectFormatOptions {
    /// If true, include the type of each object as well as its fields; e.g.:
    /// `{ "fields": { "f": 20, "g": { "fields" { "h": true }, "type": "0x0::MyModule::MyNestedType" }, "type": "0x0::MyModule::MyType" }`
    ///  If false, include field names only; e.g.:
    /// `{ "f": 20, "g": { "h": true } }`
    include_types: bool,
}

impl ObjectFormatOptions {
    pub fn with_types() -> Self {
        ObjectFormatOptions {
            include_types: true,
        }
    }
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
        type_: MoveObjectType,
        has_public_transfer: bool,
        version: SequenceNumber,
        contents: Vec<u8>,
        protocol_config: &ProtocolConfig,
    ) -> Result<Self, ExecutionError> {
        Self::new_from_execution_with_limit(
            type_,
            has_public_transfer,
            version,
            contents,
            protocol_config.max_move_object_size(),
        )
    }

    /// # Safety
    /// This function should ONLY be called if has_public_transfer has been determined by the type_
    pub unsafe fn new_from_execution_with_limit(
        type_: MoveObjectType,
        has_public_transfer: bool,
        version: SequenceNumber,
        contents: Vec<u8>,
        max_move_object_size: u64,
    ) -> Result<Self, ExecutionError> {
        // coins should always have public transfer, as they always should have store.
        // Thus, type_ == GasCoin::type_() ==> has_public_transfer
        // TODO: think this can be generalized to is_coin
        debug_assert!(!type_.is_gas_coin() || has_public_transfer);
        if contents.len() as u64 > max_move_object_size {
            return Err(ExecutionError::from_kind(
                ExecutionErrorKind::MoveObjectTooBig {
                    object_size: contents.len() as u64,
                    max_object_size: max_move_object_size,
                },
            ));
        }
        Ok(Self {
            type_,
            has_public_transfer,
            version,
            contents,
        })
    }

    pub fn new_gas_coin(version: SequenceNumber, id: ObjectID, value: u64) -> Self {
        // unwrap safe because coins are always smaller than the max object size
        unsafe {
            Self::new_from_execution_with_limit(
                GasCoin::type_().into(),
                true,
                version,
                GasCoin::new(id, value).to_bcs_bytes(),
                256,
            )
            .unwrap()
        }
    }

    pub fn new_coin(
        coin_type: MoveObjectType,
        version: SequenceNumber,
        id: ObjectID,
        value: u64,
    ) -> Self {
        // unwrap safe because coins are always smaller than the max object size
        unsafe {
            Self::new_from_execution_with_limit(
                coin_type,
                true,
                version,
                GasCoin::new(id, value).to_bcs_bytes(),
                256,
            )
            .unwrap()
        }
    }

    pub fn type_(&self) -> &MoveObjectType {
        &self.type_
    }

    pub fn is_type(&self, s: &StructTag) -> bool {
        self.type_.is(s)
    }

    pub fn has_public_transfer(&self) -> bool {
        self.has_public_transfer
    }
    pub fn id(&self) -> ObjectID {
        Self::id_opt(&self.contents).unwrap()
    }

    pub fn id_opt(contents: &[u8]) -> Result<ObjectID, ObjectIDParseError> {
        // TODO: Ensure safe index to to parse ObjectID. https://github.com/MystenLabs/sui/issues/6278
        ObjectID::try_from(&contents[0..ID_END_INDEX])
    }

    pub fn is_coin(&self) -> bool {
        self.type_.is_coin()
    }

    pub fn version(&self) -> SequenceNumber {
        self.version
    }

    /// Contents of the object that are specific to its type--i.e., not its ID and version, which all objects have
    /// For example if the object was declared as `struct S has key { id: ID, f1: u64, f2: bool },
    /// this returns the slice containing `f1` and `f2`.
    #[cfg(test)]
    pub fn type_specific_contents(&self) -> &[u8] {
        &self.contents[ID_END_INDEX..]
    }

    /// Update the contents of this object but does not increment its version
    pub fn update_contents(
        &mut self,
        new_contents: Vec<u8>,
        protocol_config: &ProtocolConfig,
    ) -> Result<(), ExecutionError> {
        self.update_contents_with_limit(new_contents, protocol_config.max_move_object_size())
    }

    fn update_contents_with_limit(
        &mut self,
        new_contents: Vec<u8>,
        max_move_object_size: u64,
    ) -> Result<(), ExecutionError> {
        if new_contents.len() as u64 > max_move_object_size {
            return Err(ExecutionError::from_kind(
                ExecutionErrorKind::MoveObjectTooBig {
                    object_size: new_contents.len() as u64,
                    max_object_size: max_move_object_size,
                },
            ));
        }

        #[cfg(debug_assertions)]
        let old_id = self.id();
        self.contents = new_contents;

        // Update should not modify ID
        #[cfg(debug_assertions)]
        debug_assert_eq!(self.id(), old_id);

        Ok(())
    }

    /// Update a coin object without requiring the current ProtocolConfig.
    /// Asserts that the gas object is not unexpectedly large.
    pub fn update_coin_contents(&mut self, new_contents: Vec<u8>) {
        self.update_contents_with_limit(new_contents, 256).unwrap()
    }

    /// Sets the version of this object to a new value which is assumed to be higher (and checked to
    /// be higher in debug).
    pub fn increment_version_to(&mut self, next: SequenceNumber) {
        self.version.increment_to(next);
    }

    pub fn decrement_version_to(&mut self, prev: SequenceNumber) {
        self.version.decrement_to(prev);
    }

    pub fn contents(&self) -> &[u8] {
        &self.contents
    }

    pub fn into_contents(self) -> Vec<u8> {
        self.contents
    }

    pub fn into_type(self) -> MoveObjectType {
        self.type_
    }

    pub fn into_inner(self) -> (MoveObjectType, Vec<u8>) {
        (self.type_, self.contents)
    }

    /// Get a `MoveStructLayout` for `self`.
    /// The `resolver` value must contain the module that declares `self.type_` and the (transitive)
    /// dependencies of `self.type_` in order for this to succeed. Failure will result in an `ObjectSerializationError`
    pub fn get_layout(
        &self,
        format: ObjectFormatOptions,
        resolver: &impl GetModule,
    ) -> Result<MoveStructLayout, SuiError> {
        Self::get_layout_from_struct_tag(self.type_().clone().into(), format, resolver)
    }

    pub fn get_layout_from_struct_tag(
        struct_tag: StructTag,
        format: ObjectFormatOptions,
        resolver: &impl GetModule,
    ) -> Result<MoveStructLayout, SuiError> {
        let type_ = TypeTag::Struct(Box::new(struct_tag));
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
        let serialized_type_tag_size =
            bcs::serialized_size(&self.type_).expect("Serializing type tag should not fail");
        // + 1 for 'has_public_transfer'
        // + 8 for `version`
        self.contents.len() + serialized_type_tag_size + 1 + 8
    }

    /// Get the total amount of SUI embedded in `self`. Intended for testing purposes
    pub fn get_total_sui(&self, resolver: &impl GetModule) -> Result<u64, SuiError> {
        let layout = self.get_layout(ObjectFormatOptions::with_types(), resolver)?;
        let move_struct = self.to_move_struct(&layout)?;
        Ok(Self::get_total_sui_(&move_struct, 0))
    }

    /// Get all SUI in `s`, either directly or in its (transitive) fields. Intended for testing purposes
    fn get_total_sui_(s: &MoveStruct, acc: u64) -> u64 {
        match s {
            MoveStruct::WithTypes { type_, fields } => {
                if GasCoin::is_gas_balance(type_) {
                    match fields[0].1 {
                        MoveValue::U64(n) => acc + n,
                        _ => unreachable!(), // a Balance<SUI> object should have exactly one field, of type int
                    }
                } else {
                    fields.iter().fold(acc, |acc, (_, v)| match v {
                        MoveValue::Struct(s) => Self::get_total_sui_(s, acc),
                        _ => acc,
                    })
                }
            }
            _ => unreachable!(),
        }
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

    pub fn try_as_package_mut(&mut self) -> Option<&mut MovePackage> {
        use Data::*;
        match self {
            Move(_) => None,
            Package(p) => Some(p),
        }
    }

    pub fn try_into_package(self) -> Option<MovePackage> {
        use Data::*;
        match self {
            Move(_) => None,
            Package(p) => Some(p),
        }
    }

    pub fn type_(&self) -> Option<&MoveObjectType> {
        use Data::*;
        match self {
            Move(m) => Some(m.type_()),
            Package(_) => None,
        }
    }

    pub fn struct_tag(&self) -> Option<StructTag> {
        use Data::*;
        match self {
            Move(m) => Some(m.type_().clone().into()),
            Package(_) => None,
        }
    }

    pub fn id(&self) -> ObjectID {
        match self {
            Self::Move(v) => v.id(),
            Self::Package(m) => m.id(),
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
    Shared {
        /// The version at which the object became shared
        initial_shared_version: SequenceNumber,
    },
    /// Object is immutable, and hence ownership doesn't matter.
    Immutable,
}

impl Owner {
    pub fn get_owner_address(&self) -> SuiResult<SuiAddress> {
        match self {
            Self::AddressOwner(address) | Self::ObjectOwner(address) => Ok(*address),
            Self::Shared { .. } | Self::Immutable => Err(SuiError::UnexpectedOwnerType),
        }
    }

    pub fn is_immutable(&self) -> bool {
        matches!(self, Owner::Immutable)
    }

    pub fn is_address_owned(&self) -> bool {
        matches!(self, Owner::AddressOwner(_))
    }

    pub fn is_child_object(&self) -> bool {
        matches!(self, Owner::ObjectOwner(_))
    }

    pub fn is_shared(&self) -> bool {
        matches!(self, Owner::Shared { .. })
    }
}

impl PartialEq<SuiAddress> for Owner {
    fn eq(&self, other: &SuiAddress) -> bool {
        match self {
            Self::AddressOwner(address) => address == other,
            Self::ObjectOwner(_) | Self::Shared { .. } | Self::Immutable => false,
        }
    }
}

impl PartialEq<ObjectID> for Owner {
    fn eq(&self, other: &ObjectID) -> bool {
        let other_id: SuiAddress = (*other).into();
        match self {
            Self::ObjectOwner(id) => id == &other_id,
            Self::AddressOwner(_) | Self::Shared { .. } | Self::Immutable => false,
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
            Self::Shared { .. } => {
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

    /// Returns true if the object is a system package.
    pub fn is_system_package(&self) -> bool {
        let id = self.id();
        self.is_package() && (id == SUI_FRAMEWORK_OBJECT_ID || id == MOVE_STDLIB_OBJECT_ID)
    }

    /// Create a system package which is not subject to size limits. Panics if the object ID is not
    /// a known system package.
    pub fn new_system_package<'p>(
        modules: Vec<CompiledModule>,
        version: SequenceNumber,
        previous_transaction: TransactionDigest,
        dependencies: impl IntoIterator<Item = &'p MovePackage>,
    ) -> Result<Self, ExecutionError> {
        let ret = Self::new_package(
            modules,
            version,
            previous_transaction,
            // System objects are not subject to limits
            u64::MAX,
            dependencies,
        )?;
        assert!(ret.is_system_package());
        Ok(ret)
    }

    pub fn new_package_from_data(data: Data, previous_transaction: TransactionDigest) -> Self {
        Object {
            data,
            owner: Owner::Immutable,
            previous_transaction,
            storage_rebate: 0,
        }
    }

    // Note: this will panic if `modules` is empty
    pub fn new_package<'p>(
        modules: Vec<CompiledModule>,
        version: SequenceNumber,
        previous_transaction: TransactionDigest,
        max_move_package_size: u64,
        dependencies: impl IntoIterator<Item = &'p MovePackage>,
    ) -> Result<Self, ExecutionError> {
        Ok(Self::new_package_from_data(
            Data::Package(MovePackage::new_initial(
                version,
                modules,
                max_move_package_size,
                dependencies,
            )?),
            previous_transaction,
        ))
    }

    pub fn new_package_for_testing<'p>(
        modules: Vec<CompiledModule>,
        previous_transaction: TransactionDigest,
        dependencies: impl IntoIterator<Item = &'p MovePackage>,
    ) -> Result<Self, ExecutionError> {
        Self::new_package(
            modules,
            OBJECT_START_VERSION,
            previous_transaction,
            ProtocolConfig::get_for_max_version().max_move_package_size(),
            dependencies,
        )
    }

    pub fn is_immutable(&self) -> bool {
        self.owner.is_immutable()
    }

    pub fn is_address_owned(&self) -> bool {
        self.owner.is_address_owned()
    }

    pub fn is_child_object(&self) -> bool {
        self.owner.is_child_object()
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
            Move(o) => o.version(),
            Package(p) => p.version(),
        }
    }

    pub fn type_(&self) -> Option<&MoveObjectType> {
        self.data.type_()
    }

    pub fn struct_tag(&self) -> Option<StructTag> {
        self.data.struct_tag()
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
            Data::Package(p) => p.object_size_for_gas_metering(),
        };
        meta_data_size + data_size
    }

    /// Change the owner of `self` to `new_owner`.
    pub fn transfer(&mut self, new_owner: SuiAddress) {
        self.owner = Owner::AddressOwner(new_owner);
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
        let move_struct = self.data.struct_tag().ok_or_else(|| SuiError::TypeError {
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
}

// Testing-related APIs.
impl Object {
    /// Get the total amount of SUI embedded in `self`, including both Move objects and the storage rebate
    pub fn get_total_sui(&self, resolver: &impl GetModule) -> Result<u64, SuiError> {
        Ok(self.storage_rebate
            + match &self.data {
                Data::Move(m) => m.get_total_sui(resolver)?,
                Data::Package(_) => 0,
            })
    }

    pub fn immutable_with_id_for_testing(id: ObjectID) -> Self {
        let data = Data::Move(MoveObject {
            type_: GasCoin::type_().into(),
            has_public_transfer: true,
            version: OBJECT_START_VERSION,
            contents: GasCoin::new(id, GAS_VALUE_FOR_TESTING).to_bcs_bytes(),
        });
        Self {
            owner: Owner::Immutable,
            data,
            previous_transaction: TransactionDigest::genesis(),
            storage_rebate: 0,
        }
    }

    /// make a test shared object.
    pub fn shared_for_testing() -> Object {
        thread_local! {
            static SHARED_OBJECT_ID: ObjectID = ObjectID::random();
        }

        let obj =
            MoveObject::new_gas_coin(OBJECT_START_VERSION, SHARED_OBJECT_ID.with(|id| *id), 10);
        let owner = Owner::Shared {
            initial_shared_version: obj.version(),
        };
        Object::new_move(obj, owner, TransactionDigest::genesis())
    }

    pub fn with_id_owner_gas_for_testing(id: ObjectID, owner: SuiAddress, gas: u64) -> Self {
        let data = Data::Move(MoveObject {
            type_: GasCoin::type_().into(),
            has_public_transfer: true,
            version: OBJECT_START_VERSION,
            contents: GasCoin::new(id, gas).to_bcs_bytes(),
        });
        Self {
            owner: Owner::AddressOwner(owner),
            data,
            previous_transaction: TransactionDigest::genesis(),
            storage_rebate: 0,
        }
    }

    pub fn with_object_owner_for_testing(id: ObjectID, owner: ObjectID) -> Self {
        let data = Data::Move(MoveObject {
            type_: GasCoin::type_().into(),
            has_public_transfer: true,
            version: OBJECT_START_VERSION,
            contents: GasCoin::new(id, GAS_VALUE_FOR_TESTING).to_bcs_bytes(),
        });
        Self {
            owner: Owner::ObjectOwner(owner.into()),
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
            type_: GasCoin::type_().into(),
            has_public_transfer: true,
            version,
            contents: GasCoin::new(id, GAS_VALUE_FOR_TESTING).to_bcs_bytes(),
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

    /// Generate a new gas coin worth `value` with a random object ID and owner
    /// For testing purposes only
    pub fn new_gas_with_balance_and_owner_for_testing(value: u64, owner: SuiAddress) -> Self {
        let obj = MoveObject::new_gas_coin(OBJECT_START_VERSION, ObjectID::random(), value);
        Object::new_move(
            obj,
            Owner::AddressOwner(owner),
            TransactionDigest::genesis(),
        )
    }

    /// Generate a new gas coin object with default balance and random owner.
    pub fn new_gas_for_testing() -> Self {
        let gas_object_id = ObjectID::random();
        let (owner, _) = deterministic_random_account_key();
        Object::with_id_owner_for_testing(gas_object_id, owner)
    }
}

/// Make a few test gas objects (all with the same random owner).
pub fn generate_test_gas_objects() -> Vec<Object> {
    thread_local! {
        static GAS_OBJECTS: Vec<Object> = (0..50)
            .map(|_| {
                let gas_object_id = ObjectID::random();
                let (owner, _) = deterministic_random_account_key();
                Object::with_id_owner_for_testing(gas_object_id, owner)
            })
            .collect();
    }

    GAS_OBJECTS.with(|v| v.clone())
}

/// Make a few test gas objects (all with the same owner).
pub fn generate_test_gas_objects_with_owner(count: usize, owner: SuiAddress) -> Vec<Object> {
    (0..count)
        .map(|_i| {
            let gas_object_id = ObjectID::random();
            Object::with_id_owner_gas_for_testing(gas_object_id, owner, GAS_VALUE_FOR_TESTING)
        })
        .collect()
}

/// Make a few test gas objects (all with the same owner) with TOTAL_SUPPLY_MIST / count balance
pub fn generate_max_test_gas_objects_with_owner(count: u64, owner: SuiAddress) -> Vec<Object> {
    let coin_size = TOTAL_SUPPLY_MIST / count;
    (0..count)
        .map(|_i| {
            let gas_object_id = ObjectID::random();
            Object::with_id_owner_gas_for_testing(gas_object_id, owner, coin_size)
        })
        .collect()
}

/// Make a few test gas objects with specific owners.
pub fn generate_test_gas_objects_with_owner_list<O>(owners: O) -> Vec<Object>
where
    O: IntoIterator<Item = SuiAddress>,
{
    owners
        .into_iter()
        .enumerate()
        .map(|(_, owner)| {
            let gas_object_id = ObjectID::random();
            Object::with_id_owner_for_testing(gas_object_id, owner)
        })
        .collect()
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
    pub fn into_object(self) -> UserInputResult<Object> {
        match self {
            Self::Deleted(oref) => Err(UserInputError::ObjectDeleted { object_ref: oref }),
            Self::NotExists(id) => Err(UserInputError::ObjectNotFound {
                object_id: id,
                version: None,
            }),
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

impl Display for ObjectRead {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Deleted(oref) => {
                write!(f, "ObjectRead::Deleted ({:?})", oref)
            }
            Self::NotExists(id) => {
                write!(f, "ObjectRead::NotExists ({:?})", id)
            }
            Self::Exists(oref, _, _) => {
                write!(f, "ObjectRead::Exists ({:?})", oref)
            }
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "status", content = "details")]
pub enum PastObjectRead {
    /// The object does not exist
    ObjectNotExists(ObjectID),
    /// The object is found to be deleted with this version
    ObjectDeleted(ObjectRef),
    /// The object exists and is found with this version
    VersionFound(ObjectRef, Object, Option<MoveStructLayout>),
    /// The object exists but not found with this version
    VersionNotFound(ObjectID, SequenceNumber),
    /// The asked object version is higher than the latest
    VersionTooHigh {
        object_id: ObjectID,
        asked_version: SequenceNumber,
        latest_version: SequenceNumber,
    },
}

impl PastObjectRead {
    /// Returns the object value if there is any, otherwise an Err
    pub fn into_object(self) -> UserInputResult<Object> {
        match self {
            Self::ObjectDeleted(oref) => Err(UserInputError::ObjectDeleted { object_ref: oref }),
            Self::ObjectNotExists(id) => Err(UserInputError::ObjectNotFound {
                object_id: id,
                version: None,
            }),
            Self::VersionFound(_, o, _) => Ok(o),
            Self::VersionNotFound(object_id, version) => Err(UserInputError::ObjectNotFound {
                object_id,
                version: Some(version),
            }),
            Self::VersionTooHigh {
                object_id,
                asked_version,
                latest_version,
            } => Err(UserInputError::ObjectSequenceNumberTooHigh {
                object_id,
                asked_version,
                latest_version,
            }),
        }
    }
}

impl Display for PastObjectRead {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ObjectDeleted(oref) => {
                write!(f, "PastObjectRead::ObjectDeleted ({:?})", oref)
            }
            Self::ObjectNotExists(id) => {
                write!(f, "PastObjectRead::ObjectNotExists ({:?})", id)
            }
            Self::VersionFound(oref, _, _) => {
                write!(f, "PastObjectRead::VersionFound ({:?})", oref)
            }
            Self::VersionNotFound(object_id, version) => {
                write!(
                    f,
                    "PastObjectRead::VersionNotFound ({:?}, asked sequence number {:?})",
                    object_id, version
                )
            }
            Self::VersionTooHigh {
                object_id,
                asked_version,
                latest_version,
            } => {
                write!(f, "PastObjectRead::VersionTooHigh ({:?}, asked sequence number {:?}, latest sequence number {:?})", object_id, asked_version, latest_version)
            }
        }
    }
}
