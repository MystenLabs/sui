// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::convert::TryFrom;
use std::fmt::{Debug, Display, Formatter};
use std::sync::Arc;

use move_binary_format::CompiledModule;
use move_bytecode_utils::layout::TypeLayoutBuilder;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::annotated_value::{MoveStruct, MoveStructLayout, MoveTypeLayout, MoveValue};
use move_core_types::language_storage::StructTag;
use move_core_types::language_storage::TypeTag;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use serde_with::Bytes;

use crate::base_types::{FullObjectID, FullObjectRef, MoveObjectType, ObjectIDParseError};
use crate::coin::{Coin, CoinMetadata, TreasuryCap};
use crate::crypto::{default_hash, deterministic_random_account_key};
use crate::error::{ExecutionError, ExecutionErrorKind, UserInputError, UserInputResult};
use crate::error::{SuiError, SuiResult};
use crate::gas_coin::GAS;
use crate::is_system_package;
use crate::layout_resolver::LayoutResolver;
use crate::move_package::MovePackage;
use crate::{
    base_types::{
        ObjectDigest, ObjectID, ObjectRef, SequenceNumber, SuiAddress, TransactionDigest,
    },
    gas_coin::GasCoin,
};
use sui_protocol_config::ProtocolConfig;

use self::balance_traversal::BalanceTraversal;
use self::bounded_visitor::BoundedVisitor;

mod balance_traversal;
pub mod bounded_visitor;

pub const GAS_VALUE_FOR_TESTING: u64 = 300_000_000_000_000;
pub const OBJECT_START_VERSION: SequenceNumber = SequenceNumber::from_u64(1);

#[serde_as]
#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash)]
pub struct MoveObject {
    /// The type of this object. Immutable
    type_: MoveObjectType,
    /// DEPRECATED this field is no longer used to determine whether a tx can transfer this
    /// object. Instead, it is always calculated from the objects type when loaded in execution
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

    pub fn new_coin(coin_type: TypeTag, version: SequenceNumber, id: ObjectID, value: u64) -> Self {
        // unwrap safe because coins are always smaller than the max object size
        unsafe {
            Self::new_from_execution_with_limit(
                MoveObjectType::coin(coin_type),
                true,
                version,
                Coin::new(id, value).to_bcs_bytes(),
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
        if ID_END_INDEX > contents.len() {
            return Err(ObjectIDParseError::TryFromSliceError);
        }
        ObjectID::try_from(&contents[0..ID_END_INDEX])
    }

    /// Return the `value: u64` field of a `Coin<T>` type.
    /// Useful for reading the coin without deserializing the object into a Move value
    /// It is the caller's responsibility to check that `self` is a coin--this function
    /// may panic or do something unexpected otherwise.
    pub fn get_coin_value_unsafe(&self) -> u64 {
        debug_assert!(self.type_.is_coin());
        // 32 bytes for object ID, 8 for balance
        debug_assert!(self.contents.len() == 40);

        // unwrap safe because we checked that it is a coin
        u64::from_le_bytes(<[u8; 8]>::try_from(&self.contents[ID_END_INDEX..]).unwrap())
    }

    /// Update the `value: u64` field of a `Coin<T>` type.
    /// Useful for updating the coin without deserializing the object into a Move value
    /// It is the caller's responsibility to check that `self` is a coin--this function
    /// may panic or do something unexpected otherwise.
    pub fn set_coin_value_unsafe(&mut self, value: u64) {
        debug_assert!(self.type_.is_coin());
        // 32 bytes for object ID, 8 for balance
        debug_assert!(self.contents.len() == 40);

        self.contents.splice(ID_END_INDEX.., value.to_le_bytes());
    }

    /// Update the `timestamp_ms: u64` field of the `Clock` type.
    ///
    /// Panics if the object isn't a `Clock`.
    pub fn set_clock_timestamp_ms_unsafe(&mut self, timestamp_ms: u64) {
        assert!(self.is_clock());
        // 32 bytes for object ID, 8 for timestamp
        assert!(self.contents.len() == 40);

        self.contents
            .splice(ID_END_INDEX.., timestamp_ms.to_le_bytes());
    }

    pub fn is_coin(&self) -> bool {
        self.type_.is_coin()
    }

    pub fn is_staked_sui(&self) -> bool {
        self.type_.is_staked_sui()
    }

    pub fn is_clock(&self) -> bool {
        self.type_.is(&crate::clock::Clock::type_())
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
    pub fn get_layout(&self, resolver: &impl GetModule) -> Result<MoveStructLayout, SuiError> {
        Self::get_struct_layout_from_struct_tag(self.type_().clone().into(), resolver)
    }

    pub fn get_struct_layout_from_struct_tag(
        struct_tag: StructTag,
        resolver: &impl GetModule,
    ) -> Result<MoveStructLayout, SuiError> {
        let type_ = TypeTag::Struct(Box::new(struct_tag));
        let layout = TypeLayoutBuilder::build_with_types(&type_, resolver).map_err(|e| {
            SuiError::ObjectSerializationError {
                error: e.to_string(),
            }
        })?;
        match layout {
            MoveTypeLayout::Struct(l) => Ok(*l),
            _ => unreachable!(
                "We called build_with_types on Struct type, should get a struct layout"
            ),
        }
    }

    /// Convert `self` to the JSON representation dictated by `layout`.
    pub fn to_move_struct(&self, layout: &MoveStructLayout) -> Result<MoveStruct, SuiError> {
        BoundedVisitor::deserialize_struct(&self.contents, layout).map_err(|e| {
            SuiError::ObjectSerializationError {
                error: e.to_string(),
            }
        })
    }

    /// Convert `self` to the JSON representation dictated by `layout`.
    pub fn to_move_struct_with_resolver(
        &self,
        resolver: &impl GetModule,
    ) -> Result<MoveStruct, SuiError> {
        self.to_move_struct(&self.get_layout(resolver)?)
    }

    pub fn to_rust<'de, T: Deserialize<'de>>(&'de self) -> Option<T> {
        bcs::from_bytes(self.contents()).ok()
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
    pub fn get_total_sui(&self, layout_resolver: &mut dyn LayoutResolver) -> Result<u64, SuiError> {
        let balances = self.get_coin_balances(layout_resolver)?;
        Ok(balances.get(&GAS::type_tag()).copied().unwrap_or(0))
    }
}

// Helpers for extracting Coin<T> balances for all T
impl MoveObject {
    /// Get the total balances for all `Coin<T>` embedded in `self`.
    pub fn get_coin_balances(
        &self,
        layout_resolver: &mut dyn LayoutResolver,
    ) -> Result<BTreeMap<TypeTag, u64>, SuiError> {
        // Fast path without deserialization.
        if let Some(type_tag) = self.type_.coin_type_maybe() {
            let balance = self.get_coin_value_unsafe();
            Ok(if balance > 0 {
                BTreeMap::from([(type_tag.clone(), balance)])
            } else {
                BTreeMap::default()
            })
        } else {
            let layout = layout_resolver.get_annotated_layout(&self.type_().clone().into())?;

            let mut traversal = BalanceTraversal::default();
            MoveValue::visit_deserialize(&self.contents, &layout.into_layout(), &mut traversal)
                .map_err(|e| SuiError::ObjectSerializationError {
                    error: e.to_string(),
                })?;

            Ok(traversal.finish())
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
    Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash, JsonSchema, Ord, PartialOrd,
)]
#[cfg_attr(feature = "fuzzing", derive(proptest_derive::Arbitrary))]
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
    /// Object is sequenced via consensus. Ownership is managed by the configured authenticator.
    ///
    /// Note: wondering what happened to `V1`? `Shared` above was the V1 of consensus objects.
    ConsensusV2 {
        /// The version at which the object most recently became a consensus object.
        /// This serves the same function as `initial_shared_version`, except it may change
        /// if the object's Owner type changes.
        start_version: SequenceNumber,
        /// The authentication mode of the object
        authenticator: Box<Authenticator>,
    },
}

#[derive(
    Eq, PartialEq, Debug, Clone, Copy, Deserialize, Serialize, Hash, JsonSchema, Ord, PartialOrd,
)]
#[cfg_attr(feature = "fuzzing", derive(proptest_derive::Arbitrary))]
pub enum Authenticator {
    /// The contained SuiAddress exclusively has all permissions: read, write, delete, transfer
    SingleOwner(SuiAddress),
}

impl Authenticator {
    pub fn as_single_owner(&self) -> &SuiAddress {
        // NOTE: Existing callers are written assuming that only singly-owned
        // ConsensusV2 objects exist. If additional Authenticator variants are
        // added, do not simply panic here. Instead, change the return type of
        // this function and update callers accordingly.
        match self {
            Self::SingleOwner(address) => address,
        }
    }
}

impl Owner {
    // NOTE: only return address of AddressOwner, otherwise return error,
    // ObjectOwner's address is converted from object id, thus we will skip it.
    pub fn get_address_owner_address(&self) -> SuiResult<SuiAddress> {
        match self {
            Self::AddressOwner(address) => Ok(*address),
            Self::Shared { .. }
            | Self::Immutable
            | Self::ObjectOwner(_)
            | Self::ConsensusV2 { .. } => Err(SuiError::UnexpectedOwnerType),
        }
    }

    // NOTE: this function will return address of both AddressOwner and ObjectOwner,
    // address of ObjectOwner is converted from object id, even though the type is SuiAddress.
    pub fn get_owner_address(&self) -> SuiResult<SuiAddress> {
        match self {
            Self::AddressOwner(address) | Self::ObjectOwner(address) => Ok(*address),
            Self::Shared { .. } | Self::Immutable | Self::ConsensusV2 { .. } => {
                Err(SuiError::UnexpectedOwnerType)
            }
        }
    }

    // Returns initial_shared_version for Shared objects, and start_version for ConsensusV2 objects.
    pub fn start_version(&self) -> Option<SequenceNumber> {
        match self {
            Self::Shared {
                initial_shared_version,
            } => Some(*initial_shared_version),
            Self::ConsensusV2 { start_version, .. } => Some(*start_version),
            Self::Immutable | Self::AddressOwner(_) | Self::ObjectOwner(_) => None,
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

    pub fn is_consensus(&self) -> bool {
        matches!(self, Owner::Shared { .. } | Owner::ConsensusV2 { .. })
    }
}

impl PartialEq<ObjectID> for Owner {
    fn eq(&self, other: &ObjectID) -> bool {
        let other_id: SuiAddress = (*other).into();
        match self {
            Self::ObjectOwner(id) => id == &other_id,
            Self::AddressOwner(_)
            | Self::Shared { .. }
            | Self::Immutable
            | Self::ConsensusV2 { .. } => false,
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
            Self::Shared {
                initial_shared_version,
            } => {
                write!(f, "Shared( {} )", initial_shared_version.value())
            }
            Self::ConsensusV2 {
                start_version,
                authenticator,
            } => {
                write!(
                    f,
                    "ConsensusV2( {}, {} )",
                    start_version.value(),
                    authenticator
                )
            }
        }
    }
}

impl Display for Authenticator {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SingleOwner(address) => {
                write!(f, "SingleOwner({})", address)
            }
        }
    }
}

#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash)]
#[serde(rename = "Object")]
pub struct ObjectInner {
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

#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash)]
#[serde(from = "ObjectInner")]
pub struct Object(Arc<ObjectInner>);

impl From<ObjectInner> for Object {
    fn from(inner: ObjectInner) -> Self {
        Self(Arc::new(inner))
    }
}

impl Object {
    pub fn into_inner(self) -> ObjectInner {
        match Arc::try_unwrap(self.0) {
            Ok(inner) => inner,
            Err(inner_arc) => (*inner_arc).clone(),
        }
    }

    pub fn as_inner(&self) -> &ObjectInner {
        &self.0
    }

    pub fn owner(&self) -> &Owner {
        &self.0.owner
    }

    pub fn new_from_genesis(
        data: Data,
        owner: Owner,
        previous_transaction: TransactionDigest,
    ) -> Self {
        ObjectInner {
            data,
            owner,
            previous_transaction,
            storage_rebate: 0,
        }
        .into()
    }

    /// Create a new Move object
    pub fn new_move(o: MoveObject, owner: Owner, previous_transaction: TransactionDigest) -> Self {
        ObjectInner {
            data: Data::Move(o),
            owner,
            previous_transaction,
            storage_rebate: 0,
        }
        .into()
    }

    pub fn new_package_from_data(data: Data, previous_transaction: TransactionDigest) -> Self {
        ObjectInner {
            data,
            owner: Owner::Immutable,
            previous_transaction,
            storage_rebate: 0,
        }
        .into()
    }

    // Note: this will panic if `modules` is empty
    pub fn new_from_package(package: MovePackage, previous_transaction: TransactionDigest) -> Self {
        Self::new_package_from_data(Data::Package(package), previous_transaction)
    }

    pub fn new_package<'p>(
        modules: &[CompiledModule],
        previous_transaction: TransactionDigest,
        max_move_package_size: u64,
        move_binary_format_version: u32,
        dependencies: impl IntoIterator<Item = &'p MovePackage>,
    ) -> Result<Self, ExecutionError> {
        Ok(Self::new_package_from_data(
            Data::Package(MovePackage::new_initial(
                modules,
                max_move_package_size,
                move_binary_format_version,
                dependencies,
            )?),
            previous_transaction,
        ))
    }

    pub fn new_upgraded_package<'p>(
        previous_package: &MovePackage,
        new_package_id: ObjectID,
        modules: &[CompiledModule],
        previous_transaction: TransactionDigest,
        protocol_config: &ProtocolConfig,
        dependencies: impl IntoIterator<Item = &'p MovePackage>,
    ) -> Result<Self, ExecutionError> {
        Ok(Self::new_package_from_data(
            Data::Package(previous_package.new_upgraded(
                new_package_id,
                modules,
                protocol_config,
                dependencies,
            )?),
            previous_transaction,
        ))
    }

    pub fn new_package_for_testing(
        modules: &[CompiledModule],
        previous_transaction: TransactionDigest,
        dependencies: impl IntoIterator<Item = MovePackage>,
    ) -> Result<Self, ExecutionError> {
        let dependencies: Vec<_> = dependencies.into_iter().collect();
        let config = ProtocolConfig::get_for_max_version_UNSAFE();
        Self::new_package(
            modules,
            previous_transaction,
            config.max_move_package_size(),
            config.move_binary_format_version(),
            &dependencies,
        )
    }

    /// Create a system package which is not subject to size limits. Panics if the object ID is not
    /// a known system package.
    pub fn new_system_package(
        modules: &[CompiledModule],
        version: SequenceNumber,
        dependencies: Vec<ObjectID>,
        previous_transaction: TransactionDigest,
    ) -> Self {
        let ret = Self::new_package_from_data(
            Data::Package(MovePackage::new_system(version, modules, dependencies)),
            previous_transaction,
        );

        #[cfg(not(msim))]
        assert!(ret.is_system_package());

        ret
    }
}

impl std::ops::Deref for Object {
    type Target = ObjectInner;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Object {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::make_mut(&mut self.0)
    }
}

impl ObjectInner {
    /// Returns true if the object is a system package.
    pub fn is_system_package(&self) -> bool {
        self.is_package() && is_system_package(self.id())
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

    pub fn is_consensus(&self) -> bool {
        self.owner.is_consensus()
    }

    pub fn get_single_owner(&self) -> Option<SuiAddress> {
        self.owner.get_owner_address().ok()
    }

    // It's a common pattern to retrieve both the owner and object ID
    // together, if it's owned by a singler owner.
    pub fn get_owner_and_id(&self) -> Option<(Owner, ObjectID)> {
        Some((self.owner.clone(), self.id()))
    }

    /// Return true if this object is a Move package, false if it is a Move value
    pub fn is_package(&self) -> bool {
        matches!(&self.data, Data::Package(_))
    }

    pub fn compute_object_reference(&self) -> ObjectRef {
        (self.id(), self.version(), self.digest())
    }

    pub fn compute_full_object_reference(&self) -> FullObjectRef {
        (self.full_id(), self.version(), self.digest())
    }

    pub fn digest(&self) -> ObjectDigest {
        ObjectDigest::new(default_hash(self))
    }

    pub fn id(&self) -> ObjectID {
        use Data::*;

        match &self.data {
            Move(v) => v.id(),
            Package(m) => m.id(),
        }
    }

    pub fn full_id(&self) -> FullObjectID {
        let id = self.id();
        if let Some(start_version) = self.owner.start_version() {
            FullObjectID::Consensus((id, start_version))
        } else {
            FullObjectID::Fastpath(id)
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

    pub fn is_coin(&self) -> bool {
        if let Some(move_object) = self.data.try_as_move() {
            move_object.type_().is_coin()
        } else {
            false
        }
    }

    pub fn is_gas_coin(&self) -> bool {
        if let Some(move_object) = self.data.try_as_move() {
            move_object.type_().is_gas_coin()
        } else {
            false
        }
    }

    // TODO: use `MoveObj::get_balance_unsafe` instead.
    // context: https://github.com/MystenLabs/sui/pull/10679#discussion_r1165877816
    pub fn as_coin_maybe(&self) -> Option<Coin> {
        if let Some(move_object) = self.data.try_as_move() {
            let coin: Coin = bcs::from_bytes(move_object.contents()).ok()?;
            Some(coin)
        } else {
            None
        }
    }

    pub fn coin_type_maybe(&self) -> Option<TypeTag> {
        if let Some(move_object) = self.data.try_as_move() {
            move_object.type_().coin_type_maybe()
        } else {
            None
        }
    }

    /// Return the `value: u64` field of a `Coin<T>` type.
    /// Useful for reading the coin without deserializing the object into a Move value
    /// It is the caller's responsibility to check that `self` is a coin--this function
    /// may panic or do something unexpected otherwise.
    pub fn get_coin_value_unsafe(&self) -> u64 {
        self.data.try_as_move().unwrap().get_coin_value_unsafe()
    }

    /// Approximate size of the object in bytes. This is used for gas metering.
    /// This will be slightly different from the serialized size, but
    /// we also don't want to serialize the object just to get the size.
    /// This approximation should be good enough for gas metering.
    pub fn object_size_for_gas_metering(&self) -> usize {
        const DEFAULT_OWNER_SIZE: usize = 40;
        const TRANSACTION_DIGEST_SIZE: usize = 32;
        const STORAGE_REBATE_SIZE: usize = 8;

        let owner_size = match &self.owner {
            Owner::AddressOwner(_)
            | Owner::ObjectOwner(_)
            | Owner::Shared { .. }
            | Owner::Immutable => DEFAULT_OWNER_SIZE,
            Owner::ConsensusV2 { authenticator, .. } => {
                DEFAULT_OWNER_SIZE
                    + match authenticator.as_ref() {
                        Authenticator::SingleOwner(_) => 8, // marginal cost to store both SuiAddress and SequenceNumber
                    }
            }
        };
        let meta_data_size = owner_size + TRANSACTION_DIGEST_SIZE + STORAGE_REBATE_SIZE;
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
        resolver: &impl GetModule,
    ) -> Result<Option<MoveStructLayout>, SuiError> {
        match &self.data {
            Data::Move(m) => Ok(Some(m.get_layout(resolver)?)),
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

    pub fn to_rust<'de, T: Deserialize<'de>>(&'de self) -> Option<T> {
        self.data.try_as_move().and_then(|data| data.to_rust())
    }
}

// Testing-related APIs.
impl Object {
    /// Get the total amount of SUI embedded in `self`, including both Move objects and the storage rebate
    pub fn get_total_sui(&self, layout_resolver: &mut dyn LayoutResolver) -> Result<u64, SuiError> {
        Ok(self.storage_rebate
            + match &self.data {
                Data::Move(m) => m.get_total_sui(layout_resolver)?,
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
        ObjectInner {
            owner: Owner::Immutable,
            data,
            previous_transaction: TransactionDigest::genesis_marker(),
            storage_rebate: 0,
        }
        .into()
    }

    pub fn immutable_for_testing() -> Self {
        thread_local! {
            static IMMUTABLE_OBJECT_ID: ObjectID = ObjectID::random();
        }

        Self::immutable_with_id_for_testing(IMMUTABLE_OBJECT_ID.with(|id| *id))
    }

    /// Make a new random test shared object.
    pub fn shared_for_testing() -> Object {
        let id = ObjectID::random();
        let obj = MoveObject::new_gas_coin(OBJECT_START_VERSION, id, 10);
        let owner = Owner::Shared {
            initial_shared_version: obj.version(),
        };
        Object::new_move(obj, owner, TransactionDigest::genesis_marker())
    }

    pub fn with_id_owner_gas_for_testing(id: ObjectID, owner: SuiAddress, gas: u64) -> Self {
        let data = Data::Move(MoveObject {
            type_: GasCoin::type_().into(),
            has_public_transfer: true,
            version: OBJECT_START_VERSION,
            contents: GasCoin::new(id, gas).to_bcs_bytes(),
        });
        ObjectInner {
            owner: Owner::AddressOwner(owner),
            data,
            previous_transaction: TransactionDigest::genesis_marker(),
            storage_rebate: 0,
        }
        .into()
    }

    pub fn treasury_cap_for_testing(struct_tag: StructTag, treasury_cap: TreasuryCap) -> Self {
        let data = Data::Move(MoveObject {
            type_: TreasuryCap::type_(struct_tag).into(),
            has_public_transfer: true,
            version: OBJECT_START_VERSION,
            contents: bcs::to_bytes(&treasury_cap).expect("Failed to serialize"),
        });
        ObjectInner {
            owner: Owner::Immutable,
            data,
            previous_transaction: TransactionDigest::genesis_marker(),
            storage_rebate: 0,
        }
        .into()
    }

    pub fn coin_metadata_for_testing(struct_tag: StructTag, metadata: CoinMetadata) -> Self {
        let data = Data::Move(MoveObject {
            type_: CoinMetadata::type_(struct_tag).into(),
            has_public_transfer: true,
            version: OBJECT_START_VERSION,
            contents: bcs::to_bytes(&metadata).expect("Failed to serialize"),
        });
        ObjectInner {
            owner: Owner::Immutable,
            data,
            previous_transaction: TransactionDigest::genesis_marker(),
            storage_rebate: 0,
        }
        .into()
    }

    pub fn with_object_owner_for_testing(id: ObjectID, owner: ObjectID) -> Self {
        let data = Data::Move(MoveObject {
            type_: GasCoin::type_().into(),
            has_public_transfer: true,
            version: OBJECT_START_VERSION,
            contents: GasCoin::new(id, GAS_VALUE_FOR_TESTING).to_bcs_bytes(),
        });
        ObjectInner {
            owner: Owner::ObjectOwner(owner.into()),
            data,
            previous_transaction: TransactionDigest::genesis_marker(),
            storage_rebate: 0,
        }
        .into()
    }

    pub fn with_id_owner_for_testing(id: ObjectID, owner: SuiAddress) -> Self {
        // For testing, we provide sufficient gas by default.
        Self::with_id_owner_gas_for_testing(id, owner, GAS_VALUE_FOR_TESTING)
    }

    pub fn with_id_owner_version_for_testing(
        id: ObjectID,
        version: SequenceNumber,
        owner: Owner,
    ) -> Self {
        let data = Data::Move(MoveObject {
            type_: GasCoin::type_().into(),
            has_public_transfer: true,
            version,
            contents: GasCoin::new(id, GAS_VALUE_FOR_TESTING).to_bcs_bytes(),
        });
        ObjectInner {
            owner,
            data,
            previous_transaction: TransactionDigest::genesis_marker(),
            storage_rebate: 0,
        }
        .into()
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
            TransactionDigest::genesis_marker(),
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

    pub fn object(&self) -> UserInputResult<&Object> {
        match self {
            Self::Deleted(oref) => Err(UserInputError::ObjectDeleted { object_ref: *oref }),
            Self::NotExists(id) => Err(UserInputError::ObjectNotFound {
                object_id: *id,
                version: None,
            }),
            Self::Exists(_, o, _) => Ok(o),
        }
    }

    pub fn object_id(&self) -> ObjectID {
        match self {
            Self::Deleted(oref) => oref.0,
            Self::NotExists(id) => *id,
            Self::Exists(oref, _, _) => oref.0,
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

#[cfg(test)]
mod tests {
    use crate::object::{Object, Owner, OBJECT_START_VERSION};
    use crate::{
        base_types::{ObjectID, SuiAddress, TransactionDigest},
        gas_coin::GasCoin,
    };

    // Ensure that object digest computation and bcs serialized format are not inadvertently changed.
    #[test]
    fn test_object_digest_and_serialized_format() {
        let g =
            GasCoin::new_for_testing_with_id(ObjectID::ZERO, 123).to_object(OBJECT_START_VERSION);
        let o = Object::new_move(
            g,
            Owner::AddressOwner(SuiAddress::ZERO),
            TransactionDigest::ZERO,
        );
        let bytes = bcs::to_bytes(&o).unwrap();

        assert_eq!(
            bytes,
            [
                0, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 123, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ]
        );

        let objref = format!("{:?}", o.compute_object_reference());
        assert_eq!(objref, "(0x0000000000000000000000000000000000000000000000000000000000000000, SequenceNumber(1), o#59tZq65HVqZjUyNtD7BCGLTD87N5cpayYwEFrtwR4aMz)");
    }

    #[test]
    fn test_get_coin_value_unsafe() {
        fn test_for_value(v: u64) {
            let g = GasCoin::new_for_testing(v).to_object(OBJECT_START_VERSION);
            assert_eq!(g.get_coin_value_unsafe(), v);
            assert_eq!(GasCoin::try_from(&g).unwrap().value(), v);
        }

        test_for_value(0);
        test_for_value(1);
        test_for_value(8);
        test_for_value(9);
        test_for_value(u8::MAX as u64);
        test_for_value(u8::MAX as u64 + 1);
        test_for_value(u16::MAX as u64);
        test_for_value(u16::MAX as u64 + 1);
        test_for_value(u32::MAX as u64);
        test_for_value(u32::MAX as u64 + 1);
        test_for_value(u64::MAX);
    }

    #[test]
    fn test_set_coin_value_unsafe() {
        fn test_for_value(v: u64) {
            let mut g = GasCoin::new_for_testing(u64::MAX).to_object(OBJECT_START_VERSION);
            g.set_coin_value_unsafe(v);
            assert_eq!(g.get_coin_value_unsafe(), v);
            assert_eq!(GasCoin::try_from(&g).unwrap().value(), v);
            assert_eq!(g.version(), OBJECT_START_VERSION);
            assert_eq!(g.contents().len(), 40);
        }

        test_for_value(0);
        test_for_value(1);
        test_for_value(8);
        test_for_value(9);
        test_for_value(u8::MAX as u64);
        test_for_value(u8::MAX as u64 + 1);
        test_for_value(u16::MAX as u64);
        test_for_value(u16::MAX as u64 + 1);
        test_for_value(u32::MAX as u64);
        test_for_value(u32::MAX as u64 + 1);
        test_for_value(u64::MAX);
    }
}
