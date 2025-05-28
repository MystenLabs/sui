// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Module for conversions between sui-core types and sui-sdk types
//!
//! For now this module makes heavy use of the `bcs_convert_impl` macro to implement the `From` trait
//! for converting between core and external sdk types, relying on the fact that the BCS format of
//! these types are strictly identical. As time goes on we'll slowly hand implement these impls
//! directly to avoid going through the BCS machinery.

use fastcrypto::traits::ToFromBytes;
use sui_sdk_types::*;
use tap::Pipe;

#[derive(Debug)]
pub struct SdkTypeConversionError(String);

impl std::fmt::Display for SdkTypeConversionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SdkTypeConversionError {}

impl From<TypeParseError> for SdkTypeConversionError {
    fn from(value: TypeParseError) -> Self {
        Self(value.to_string())
    }
}

impl From<anyhow::Error> for SdkTypeConversionError {
    fn from(value: anyhow::Error) -> Self {
        Self(value.to_string())
    }
}

impl From<bcs::Error> for SdkTypeConversionError {
    fn from(value: bcs::Error) -> Self {
        Self(value.to_string())
    }
}

macro_rules! bcs_convert_impl {
    ($core:ty, $external:ty) => {
        impl TryFrom<$core> for $external {
            type Error = bcs::Error;

            fn try_from(value: $core) -> Result<Self, Self::Error> {
                let bytes = bcs::to_bytes(&value)?;
                bcs::from_bytes(&bytes)
            }
        }

        impl TryFrom<$external> for $core {
            type Error = bcs::Error;

            fn try_from(value: $external) -> Result<Self, Self::Error> {
                let bytes = bcs::to_bytes(&value)?;
                bcs::from_bytes(&bytes)
            }
        }
    };
}

bcs_convert_impl!(crate::object::Object, Object);
bcs_convert_impl!(crate::transaction::TransactionData, Transaction);
bcs_convert_impl!(crate::effects::TransactionEffects, TransactionEffects);
bcs_convert_impl!(
    crate::messages_checkpoint::CheckpointSummary,
    CheckpointSummary
);
bcs_convert_impl!(
    crate::messages_checkpoint::CertifiedCheckpointSummary,
    SignedCheckpointSummary
);
bcs_convert_impl!(
    crate::messages_checkpoint::CheckpointContents,
    CheckpointContents
);
bcs_convert_impl!(
    crate::full_checkpoint_content::CheckpointData,
    CheckpointData
);
bcs_convert_impl!(crate::signature::GenericSignature, UserSignature);
bcs_convert_impl!(crate::effects::TransactionEvents, TransactionEvents);
bcs_convert_impl!(crate::transaction::Command, Command);
bcs_convert_impl!(crate::transaction::TransactionKind, TransactionKind);
bcs_convert_impl!(crate::move_package::MovePackage, MovePackage);

impl<const T: bool> From<crate::crypto::AuthorityQuorumSignInfo<T>>
    for ValidatorAggregatedSignature
{
    fn from(value: crate::crypto::AuthorityQuorumSignInfo<T>) -> Self {
        let crate::crypto::AuthorityQuorumSignInfo {
            epoch,
            signature,
            signers_map,
        } = value;

        Self {
            epoch,
            signature: Bls12381Signature::from_bytes(signature.as_ref()).unwrap(),
            bitmap: signers_map,
        }
    }
}

impl<const T: bool> From<ValidatorAggregatedSignature>
    for crate::crypto::AuthorityQuorumSignInfo<T>
{
    fn from(value: ValidatorAggregatedSignature) -> Self {
        let ValidatorAggregatedSignature {
            epoch,
            signature,
            bitmap,
        } = value;

        Self {
            epoch,
            signature: crate::crypto::AggregateAuthoritySignature::from_bytes(signature.as_bytes())
                .unwrap(),
            signers_map: bitmap,
        }
    }
}

impl From<crate::object::Owner> for Owner {
    fn from(value: crate::object::Owner) -> Self {
        match value {
            crate::object::Owner::AddressOwner(address) => Self::Address(address.into()),
            crate::object::Owner::ObjectOwner(object_id) => Self::Object(object_id.into()),
            crate::object::Owner::Shared {
                initial_shared_version,
            } => Self::Shared(initial_shared_version.value()),
            crate::object::Owner::Immutable => Self::Immutable,
            // TODO: Corresponding types need to be added to sui-sdk-types.
            crate::object::Owner::ConsensusV2 {
                start_version: _,
                authenticator: _,
            } => todo!(),
        }
    }
}

impl From<Owner> for crate::object::Owner {
    fn from(value: Owner) -> Self {
        match value {
            Owner::Address(address) => crate::object::Owner::AddressOwner(address.into()),
            Owner::Object(object_id) => crate::object::Owner::ObjectOwner(object_id.into()),
            Owner::Shared(initial_shared_version) => crate::object::Owner::Shared {
                initial_shared_version: initial_shared_version.into(),
            },
            Owner::Immutable => crate::object::Owner::Immutable,
            Owner::ConsensusAddress { .. } => todo!(),
        }
    }
}

impl From<crate::base_types::SuiAddress> for Address {
    fn from(value: crate::base_types::SuiAddress) -> Self {
        Self::new(value.to_inner())
    }
}

impl From<Address> for crate::base_types::SuiAddress {
    fn from(value: Address) -> Self {
        crate::base_types::ObjectID::new(value.into_inner()).into()
    }
}

impl From<crate::base_types::ObjectID> for ObjectId {
    fn from(value: crate::base_types::ObjectID) -> Self {
        Self::new(value.into_bytes())
    }
}

impl From<ObjectId> for crate::base_types::ObjectID {
    fn from(value: ObjectId) -> Self {
        Self::new(value.into_inner())
    }
}

impl From<crate::base_types::SuiAddress> for ObjectId {
    fn from(value: crate::base_types::SuiAddress) -> Self {
        Self::new(value.to_inner())
    }
}

impl From<ObjectId> for crate::base_types::SuiAddress {
    fn from(value: ObjectId) -> Self {
        crate::base_types::ObjectID::new(value.into_inner()).into()
    }
}

impl TryFrom<crate::transaction::SenderSignedData> for SignedTransaction {
    type Error = SdkTypeConversionError;

    fn try_from(value: crate::transaction::SenderSignedData) -> Result<Self, Self::Error> {
        let crate::transaction::SenderSignedTransaction {
            intent_message,
            tx_signatures,
        } = value.into_inner();

        Self {
            transaction: intent_message.value.try_into()?,
            signatures: tx_signatures
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        }
        .pipe(Ok)
    }
}

impl TryFrom<SignedTransaction> for crate::transaction::SenderSignedData {
    type Error = SdkTypeConversionError;

    fn try_from(value: SignedTransaction) -> Result<Self, Self::Error> {
        let SignedTransaction {
            transaction,
            signatures,
        } = value;

        Self::new(
            transaction.try_into()?,
            signatures
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        )
        .pipe(Ok)
    }
}

impl TryFrom<crate::transaction::Transaction> for SignedTransaction {
    type Error = SdkTypeConversionError;

    fn try_from(value: crate::transaction::Transaction) -> Result<Self, Self::Error> {
        value.into_data().try_into()
    }
}

impl TryFrom<SignedTransaction> for crate::transaction::Transaction {
    type Error = SdkTypeConversionError;

    fn try_from(value: SignedTransaction) -> Result<Self, Self::Error> {
        Ok(Self::new(value.try_into()?))
    }
}

pub fn type_tag_core_to_sdk(
    value: move_core_types::language_storage::TypeTag,
) -> Result<TypeTag, SdkTypeConversionError> {
    match value {
        move_core_types::language_storage::TypeTag::Bool => TypeTag::Bool,
        move_core_types::language_storage::TypeTag::U8 => TypeTag::U8,
        move_core_types::language_storage::TypeTag::U64 => TypeTag::U64,
        move_core_types::language_storage::TypeTag::U128 => TypeTag::U128,
        move_core_types::language_storage::TypeTag::Address => TypeTag::Address,
        move_core_types::language_storage::TypeTag::Signer => TypeTag::Signer,
        move_core_types::language_storage::TypeTag::Vector(type_tag) => {
            TypeTag::Vector(Box::new(type_tag_core_to_sdk(*type_tag)?))
        }
        move_core_types::language_storage::TypeTag::Struct(struct_tag) => {
            TypeTag::Struct(Box::new(struct_tag_core_to_sdk(*struct_tag)?))
        }
        move_core_types::language_storage::TypeTag::U16 => TypeTag::U16,
        move_core_types::language_storage::TypeTag::U32 => TypeTag::U32,
        move_core_types::language_storage::TypeTag::U256 => TypeTag::U256,
    }
    .pipe(Ok)
}

pub fn struct_tag_core_to_sdk(
    value: move_core_types::language_storage::StructTag,
) -> Result<StructTag, SdkTypeConversionError> {
    let move_core_types::language_storage::StructTag {
        address,
        module,
        name,
        type_params,
    } = value;

    let address = Address::new(address.into_bytes());
    let module = Identifier::new(module.as_str())?;
    let name = Identifier::new(name.as_str())?;
    let type_params = type_params
        .into_iter()
        .map(type_tag_core_to_sdk)
        .collect::<Result<_, _>>()?;
    StructTag {
        address,
        module,
        name,
        type_params,
    }
    .pipe(Ok)
}

pub fn type_tag_sdk_to_core(
    value: TypeTag,
) -> Result<move_core_types::language_storage::TypeTag, SdkTypeConversionError> {
    match value {
        TypeTag::Bool => move_core_types::language_storage::TypeTag::Bool,
        TypeTag::U8 => move_core_types::language_storage::TypeTag::U8,
        TypeTag::U64 => move_core_types::language_storage::TypeTag::U64,
        TypeTag::U128 => move_core_types::language_storage::TypeTag::U128,
        TypeTag::Address => move_core_types::language_storage::TypeTag::Address,
        TypeTag::Signer => move_core_types::language_storage::TypeTag::Signer,
        TypeTag::Vector(type_tag) => move_core_types::language_storage::TypeTag::Vector(Box::new(
            type_tag_sdk_to_core(*type_tag)?,
        )),
        TypeTag::Struct(struct_tag) => move_core_types::language_storage::TypeTag::Struct(
            Box::new(struct_tag_sdk_to_core(*struct_tag)?),
        ),
        TypeTag::U16 => move_core_types::language_storage::TypeTag::U16,
        TypeTag::U32 => move_core_types::language_storage::TypeTag::U32,
        TypeTag::U256 => move_core_types::language_storage::TypeTag::U256,
    }
    .pipe(Ok)
}

pub fn struct_tag_sdk_to_core(
    value: StructTag,
) -> Result<move_core_types::language_storage::StructTag, SdkTypeConversionError> {
    let StructTag {
        address,
        module,
        name,
        type_params,
    } = value;

    let address = move_core_types::account_address::AccountAddress::new(address.into_inner());
    let module = move_core_types::identifier::Identifier::new(module.into_inner())?;
    let name = move_core_types::identifier::Identifier::new(name.into_inner())?;
    let type_params = type_params
        .into_iter()
        .map(type_tag_sdk_to_core)
        .collect::<Result<_, _>>()?;
    move_core_types::language_storage::StructTag {
        address,
        module,
        name,
        type_params,
    }
    .pipe(Ok)
}

impl From<crate::messages_checkpoint::CheckpointDigest> for CheckpointDigest {
    fn from(value: crate::messages_checkpoint::CheckpointDigest) -> Self {
        Self::new(value.into_inner())
    }
}

impl From<CheckpointDigest> for crate::messages_checkpoint::CheckpointDigest {
    fn from(value: CheckpointDigest) -> Self {
        Self::new(value.into_inner())
    }
}

impl From<crate::digests::TransactionDigest> for TransactionDigest {
    fn from(value: crate::digests::TransactionDigest) -> Self {
        Self::new(value.into_inner())
    }
}

impl From<TransactionDigest> for crate::digests::TransactionDigest {
    fn from(value: TransactionDigest) -> Self {
        Self::new(value.into_inner())
    }
}

impl From<crate::digests::ObjectDigest> for ObjectDigest {
    fn from(value: crate::digests::ObjectDigest) -> Self {
        Self::new(value.into_inner())
    }
}

impl From<ObjectDigest> for crate::digests::ObjectDigest {
    fn from(value: ObjectDigest) -> Self {
        Self::new(value.into_inner())
    }
}

impl From<crate::committee::Committee> for ValidatorCommittee {
    fn from(value: crate::committee::Committee) -> Self {
        Self {
            epoch: value.epoch(),
            members: value
                .voting_rights
                .into_iter()
                .map(|(name, stake)| ValidatorCommitteeMember {
                    public_key: name.into(),
                    stake,
                })
                .collect(),
        }
    }
}

impl From<ValidatorCommittee> for crate::committee::Committee {
    fn from(value: ValidatorCommittee) -> Self {
        let ValidatorCommittee { epoch, members } = value;

        Self::new(
            epoch,
            members
                .into_iter()
                .map(|member| (member.public_key.into(), member.stake))
                .collect(),
        )
    }
}

impl From<crate::crypto::AuthorityPublicKeyBytes> for Bls12381PublicKey {
    fn from(value: crate::crypto::AuthorityPublicKeyBytes) -> Self {
        Self::new(value.0)
    }
}

impl From<Bls12381PublicKey> for crate::crypto::AuthorityPublicKeyBytes {
    fn from(value: Bls12381PublicKey) -> Self {
        Self::new(value.into_inner())
    }
}

impl From<UnchangedSharedKind> for crate::effects::UnchangedSharedKind {
    fn from(value: UnchangedSharedKind) -> Self {
        match value {
            UnchangedSharedKind::ReadOnlyRoot { version, digest } => {
                Self::ReadOnlyRoot((version.into(), digest.into()))
            }
            UnchangedSharedKind::MutateDeleted { version } => {
                Self::MutateConsensusStreamEnded(version.into())
            }
            UnchangedSharedKind::ReadDeleted { version } => {
                Self::ReadConsensusStreamEnded(version.into())
            }
            UnchangedSharedKind::Canceled { version } => Self::Cancelled(version.into()),
            UnchangedSharedKind::PerEpochConfig => Self::PerEpochConfig,
        }
    }
}

impl From<crate::effects::UnchangedSharedKind> for UnchangedSharedKind {
    fn from(value: crate::effects::UnchangedSharedKind) -> Self {
        match value {
            crate::effects::UnchangedSharedKind::ReadOnlyRoot((version, digest)) => {
                Self::ReadOnlyRoot {
                    version: version.into(),
                    digest: digest.into(),
                }
            }
            crate::effects::UnchangedSharedKind::MutateConsensusStreamEnded(version) => {
                Self::MutateDeleted {
                    version: version.into(),
                }
            }
            crate::effects::UnchangedSharedKind::ReadConsensusStreamEnded(version) => {
                Self::ReadDeleted {
                    version: version.into(),
                }
            }
            crate::effects::UnchangedSharedKind::Cancelled(version) => Self::Canceled {
                version: version.into(),
            },
            crate::effects::UnchangedSharedKind::PerEpochConfig => Self::PerEpochConfig,
        }
    }
}

impl From<crate::transaction::TransactionExpiration> for TransactionExpiration {
    fn from(value: crate::transaction::TransactionExpiration) -> Self {
        match value {
            crate::transaction::TransactionExpiration::None => Self::None,
            crate::transaction::TransactionExpiration::Epoch(epoch) => Self::Epoch(epoch),
        }
    }
}

impl From<TransactionExpiration> for crate::transaction::TransactionExpiration {
    fn from(value: TransactionExpiration) -> Self {
        match value {
            TransactionExpiration::None => Self::None,
            TransactionExpiration::Epoch(epoch) => Self::Epoch(epoch),
        }
    }
}

impl From<crate::execution_status::TypeArgumentError> for TypeArgumentError {
    fn from(value: crate::execution_status::TypeArgumentError) -> Self {
        match value {
            crate::execution_status::TypeArgumentError::TypeNotFound => Self::TypeNotFound,
            crate::execution_status::TypeArgumentError::ConstraintNotSatisfied => {
                Self::ConstraintNotSatisfied
            }
        }
    }
}

impl From<crate::execution_status::PackageUpgradeError> for PackageUpgradeError {
    fn from(value: crate::execution_status::PackageUpgradeError) -> Self {
        match value {
            crate::execution_status::PackageUpgradeError::UnableToFetchPackage { package_id } => {
                Self::UnableToFetchPackage {
                    package_id: package_id.into(),
                }
            }
            crate::execution_status::PackageUpgradeError::NotAPackage { object_id } => {
                Self::NotAPackage {
                    object_id: object_id.into(),
                }
            }
            crate::execution_status::PackageUpgradeError::IncompatibleUpgrade => {
                Self::IncompatibleUpgrade
            }
            crate::execution_status::PackageUpgradeError::DigestDoesNotMatch { digest } => {
                Self::DigestDoesNotMatch {
                    digest: Digest::from_bytes(digest).unwrap(),
                }
            }
            crate::execution_status::PackageUpgradeError::UnknownUpgradePolicy { policy } => {
                Self::UnknownUpgradePolicy { policy }
            }
            crate::execution_status::PackageUpgradeError::PackageIDDoesNotMatch {
                package_id,
                ticket_id,
            } => Self::PackageIdDoesNotMatch {
                package_id: package_id.into(),
                ticket_id: ticket_id.into(),
            },
        }
    }
}

impl From<crate::execution_status::CommandArgumentError> for CommandArgumentError {
    fn from(value: crate::execution_status::CommandArgumentError) -> Self {
        match value {
            crate::execution_status::CommandArgumentError::TypeMismatch => Self::TypeMismatch,
            crate::execution_status::CommandArgumentError::InvalidBCSBytes => Self::InvalidBcsBytes,
            crate::execution_status::CommandArgumentError::InvalidUsageOfPureArg => Self::InvalidUsageOfPureArgument,
            crate::execution_status::CommandArgumentError::InvalidArgumentToPrivateEntryFunction => Self::InvalidArgumentToPrivateEntryFunction,
            crate::execution_status::CommandArgumentError::IndexOutOfBounds { idx } => Self::IndexOutOfBounds { index: idx },
            crate::execution_status::CommandArgumentError::SecondaryIndexOutOfBounds { result_idx, secondary_idx } => Self::SecondaryIndexOutOfBounds { result: result_idx, subresult: secondary_idx },
            crate::execution_status::CommandArgumentError::InvalidResultArity { result_idx } => Self::InvalidResultArity { result: result_idx },
            crate::execution_status::CommandArgumentError::InvalidGasCoinUsage => Self::InvalidGasCoinUsage,
            crate::execution_status::CommandArgumentError::InvalidValueUsage => Self::InvalidValueUsage,
            crate::execution_status::CommandArgumentError::InvalidObjectByValue => Self::InvalidObjectByValue,
            crate::execution_status::CommandArgumentError::InvalidObjectByMutRef => Self::InvalidObjectByMutRef,
            crate::execution_status::CommandArgumentError::SharedObjectOperationNotAllowed => Self::SharedObjectOperationNotAllowed,
            crate::execution_status::CommandArgumentError::InvalidArgumentArity => Self::InvalidArgumentArity,
        }
    }
}

impl From<crate::execution_status::ExecutionFailureStatus> for ExecutionError {
    fn from(value: crate::execution_status::ExecutionFailureStatus) -> Self {
        match value {
            crate::execution_status::ExecutionFailureStatus::InsufficientGas => Self::InsufficientGas,
            crate::execution_status::ExecutionFailureStatus::InvalidGasObject => Self::InvalidGasObject,
            crate::execution_status::ExecutionFailureStatus::InvariantViolation => Self::InvariantViolation,
            crate::execution_status::ExecutionFailureStatus::FeatureNotYetSupported => Self::FeatureNotYetSupported,
            crate::execution_status::ExecutionFailureStatus::MoveObjectTooBig { object_size, max_object_size } => Self::ObjectTooBig { object_size, max_object_size },
            crate::execution_status::ExecutionFailureStatus::MovePackageTooBig { object_size, max_object_size } => Self::PackageTooBig { object_size, max_object_size },
            crate::execution_status::ExecutionFailureStatus::CircularObjectOwnership { object } => Self::CircularObjectOwnership { object: object.into() },
            crate::execution_status::ExecutionFailureStatus::InsufficientCoinBalance => Self::InsufficientCoinBalance,
            crate::execution_status::ExecutionFailureStatus::CoinBalanceOverflow => Self::CoinBalanceOverflow,
            crate::execution_status::ExecutionFailureStatus::PublishErrorNonZeroAddress => Self::PublishErrorNonZeroAddress,
            crate::execution_status::ExecutionFailureStatus::SuiMoveVerificationError => Self::SuiMoveVerificationError,
            crate::execution_status::ExecutionFailureStatus::MovePrimitiveRuntimeError(move_location_opt) => Self::MovePrimitiveRuntimeError { location: move_location_opt.0.map(Into::into) },
            crate::execution_status::ExecutionFailureStatus::MoveAbort(move_location, code) => Self::MoveAbort { location: move_location.into(), code },
            crate::execution_status::ExecutionFailureStatus::VMVerificationOrDeserializationError => Self::VmVerificationOrDeserializationError,
            crate::execution_status::ExecutionFailureStatus::VMInvariantViolation => Self::VmInvariantViolation,
            crate::execution_status::ExecutionFailureStatus::FunctionNotFound => Self::FunctionNotFound,
            crate::execution_status::ExecutionFailureStatus::ArityMismatch => Self::ArityMismatch,
            crate::execution_status::ExecutionFailureStatus::TypeArityMismatch => Self::TypeArityMismatch,
            crate::execution_status::ExecutionFailureStatus::NonEntryFunctionInvoked => Self::NonEntryFunctionInvoked,
            crate::execution_status::ExecutionFailureStatus::CommandArgumentError { arg_idx, kind } => Self::CommandArgumentError { argument: arg_idx, kind: kind.into() },
            crate::execution_status::ExecutionFailureStatus::TypeArgumentError { argument_idx, kind } => Self::TypeArgumentError { type_argument: argument_idx, kind: kind.into() },
            crate::execution_status::ExecutionFailureStatus::UnusedValueWithoutDrop { result_idx, secondary_idx } => Self::UnusedValueWithoutDrop { result: result_idx, subresult: secondary_idx },
            crate::execution_status::ExecutionFailureStatus::InvalidPublicFunctionReturnType { idx } => Self::InvalidPublicFunctionReturnType { index: idx },
            crate::execution_status::ExecutionFailureStatus::InvalidTransferObject => Self::InvalidTransferObject,
            crate::execution_status::ExecutionFailureStatus::EffectsTooLarge { current_size, max_size } => Self::EffectsTooLarge { current_size, max_size },
            crate::execution_status::ExecutionFailureStatus::PublishUpgradeMissingDependency => Self::PublishUpgradeMissingDependency,
            crate::execution_status::ExecutionFailureStatus::PublishUpgradeDependencyDowngrade => Self::PublishUpgradeDependencyDowngrade,
            crate::execution_status::ExecutionFailureStatus::PackageUpgradeError { upgrade_error } => Self::PackageUpgradeError { kind: upgrade_error.into() },
            crate::execution_status::ExecutionFailureStatus::WrittenObjectsTooLarge { current_size, max_size } => Self::WrittenObjectsTooLarge { object_size: current_size, max_object_size:max_size },
            crate::execution_status::ExecutionFailureStatus::CertificateDenied => Self::CertificateDenied,
            crate::execution_status::ExecutionFailureStatus::SuiMoveVerificationTimedout => Self::SuiMoveVerificationTimedout,
            crate::execution_status::ExecutionFailureStatus::SharedObjectOperationNotAllowed => Self::SharedObjectOperationNotAllowed,
            crate::execution_status::ExecutionFailureStatus::InputObjectDeleted => Self::InputObjectDeleted,
            crate::execution_status::ExecutionFailureStatus::ExecutionCancelledDueToSharedObjectCongestion { congested_objects } => Self::ExecutionCanceledDueToSharedObjectCongestion { congested_objects: congested_objects.0.into_iter().map(Into::into).collect() },
            crate::execution_status::ExecutionFailureStatus::AddressDeniedForCoin { address, coin_type } => Self::AddressDeniedForCoin { address: address.into(), coin_type },
            crate::execution_status::ExecutionFailureStatus::CoinTypeGlobalPause { coin_type } => Self::CoinTypeGlobalPause { coin_type },
            crate::execution_status::ExecutionFailureStatus::ExecutionCancelledDueToRandomnessUnavailable => Self::ExecutionCanceledDueToRandomnessUnavailable,
            crate::execution_status::ExecutionFailureStatus::MoveVectorElemTooBig { value_size, max_scaled_size } => Self::MoveVectorElemTooBig { value_size, max_scaled_size },
            crate::execution_status::ExecutionFailureStatus::MoveRawValueTooBig { value_size, max_scaled_size } => Self::MoveRawValueTooBig { value_size, max_scaled_size },
        }
    }
}

impl From<crate::execution_status::MoveLocation> for MoveLocation {
    fn from(value: crate::execution_status::MoveLocation) -> Self {
        Self {
            package: ObjectId::new(value.module.address().into_bytes()),
            module: Identifier::new(value.module.name().as_str()).unwrap(),
            function: value.function,
            instruction: value.instruction,
            function_name: value
                .function_name
                .map(|name| Identifier::new(name).unwrap()),
        }
    }
}
