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

use crate::crypto::SuiSignature as _;

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

impl From<std::array::TryFromSliceError> for SdkTypeConversionError {
    fn from(value: std::array::TryFromSliceError) -> Self {
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
bcs_convert_impl!(crate::effects::TransactionEffectsV1, TransactionEffectsV1);
bcs_convert_impl!(crate::effects::TransactionEffectsV2, TransactionEffectsV2);
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
bcs_convert_impl!(
    crate::crypto::ZkLoginPublicIdentifier,
    ZkLoginPublicIdentifier
);
bcs_convert_impl!(
    crate::crypto::ZkLoginAuthenticatorAsBytes,
    ZkLoginAuthenticator
);
bcs_convert_impl!(
    crate::zk_login_authenticator::ZkLoginAuthenticator,
    ZkLoginAuthenticator
);
bcs_convert_impl!(
    crate::crypto::PasskeyAuthenticatorAsBytes,
    PasskeyAuthenticator
);
bcs_convert_impl!(
    crate::passkey_authenticator::PasskeyAuthenticator,
    PasskeyAuthenticator
);
bcs_convert_impl!(crate::effects::TransactionEvents, TransactionEvents);
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
            crate::object::Owner::ConsensusAddressOwner {
                start_version,
                owner,
            } => Self::ConsensusAddress {
                start_version: start_version.value(),
                owner: owner.into(),
            },
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
            Owner::ConsensusAddress {
                start_version,
                owner,
            } => crate::object::Owner::ConsensusAddressOwner {
                start_version: start_version.into(),
                owner: owner.into(),
            },
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

impl TryFrom<crate::type_input::TypeInput> for TypeTag {
    type Error = SdkTypeConversionError;

    fn try_from(value: crate::type_input::TypeInput) -> Result<Self, Self::Error> {
        match value {
            crate::type_input::TypeInput::Bool => Self::Bool,
            crate::type_input::TypeInput::U8 => Self::U8,
            crate::type_input::TypeInput::U64 => Self::U64,
            crate::type_input::TypeInput::U128 => Self::U128,
            crate::type_input::TypeInput::Address => Self::Address,
            crate::type_input::TypeInput::Signer => Self::Signer,
            crate::type_input::TypeInput::Vector(type_input) => {
                Self::Vector(Box::new((*type_input).try_into()?))
            }
            crate::type_input::TypeInput::Struct(struct_input) => {
                Self::Struct(Box::new((*struct_input).try_into()?))
            }
            crate::type_input::TypeInput::U16 => Self::U16,
            crate::type_input::TypeInput::U32 => Self::U32,
            crate::type_input::TypeInput::U256 => Self::U256,
        }
        .pipe(Ok)
    }
}

impl TryFrom<crate::type_input::StructInput> for StructTag {
    type Error = SdkTypeConversionError;

    fn try_from(value: crate::type_input::StructInput) -> Result<Self, Self::Error> {
        Self {
            address: Address::new(value.address.into_bytes()),
            module: Identifier::new(value.module)?,
            name: Identifier::new(value.name)?,
            type_params: value
                .type_params
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        }
        .pipe(Ok)
    }
}

impl From<TypeTag> for crate::type_input::TypeInput {
    fn from(value: TypeTag) -> Self {
        match value {
            TypeTag::U8 => Self::U8,
            TypeTag::U16 => Self::U16,
            TypeTag::U32 => Self::U32,
            TypeTag::U64 => Self::U64,
            TypeTag::U128 => Self::U128,
            TypeTag::U256 => Self::U256,
            TypeTag::Bool => Self::Bool,
            TypeTag::Address => Self::Address,
            TypeTag::Signer => Self::Signer,
            TypeTag::Vector(type_tag) => Self::Vector(Box::new((*type_tag).into())),
            TypeTag::Struct(struct_tag) => Self::Struct(Box::new((*struct_tag).into())),
        }
    }
}

impl From<StructTag> for crate::type_input::StructInput {
    fn from(value: StructTag) -> Self {
        Self {
            address: move_core_types::account_address::AccountAddress::new(
                value.address.into_inner(),
            ),
            module: value.module.into_inner().into(),
            name: value.name.into_inner().into(),
            type_params: value.type_params.into_iter().map(Into::into).collect(),
        }
    }
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

impl From<crate::digests::Digest> for Digest {
    fn from(value: crate::digests::Digest) -> Self {
        Self::new(value.into_inner())
    }
}

impl From<Digest> for crate::digests::Digest {
    fn from(value: Digest) -> Self {
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
            UnchangedSharedKind::PerEpochConfigWithSequenceNumber { .. } => todo!(),
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

impl From<crate::effects::ObjectIn> for ObjectIn {
    fn from(value: crate::effects::ObjectIn) -> Self {
        match value {
            crate::effects::ObjectIn::NotExist => Self::NotExist,
            crate::effects::ObjectIn::Exist(((version, digest), owner)) => Self::Exist {
                version: version.value(),
                digest: digest.into(),
                owner: owner.into(),
            },
        }
    }
}

impl From<crate::effects::ObjectOut> for ObjectOut {
    fn from(value: crate::effects::ObjectOut) -> Self {
        match value {
            crate::effects::ObjectOut::NotExist => Self::NotExist,
            crate::effects::ObjectOut::ObjectWrite((digest, owner)) => Self::ObjectWrite {
                digest: digest.into(),
                owner: owner.into(),
            },
            crate::effects::ObjectOut::PackageWrite((version, digest)) => Self::PackageWrite {
                version: version.value(),
                digest: digest.into(),
            },

            // TODO implement accumulator in sdk. This feature is not live yet on any network
            crate::effects::ObjectOut::AccumulatorWriteV1(_) => todo!(),
        }
    }
}

impl From<crate::effects::IDOperation> for IdOperation {
    fn from(value: crate::effects::IDOperation) -> Self {
        match value {
            crate::effects::IDOperation::None => Self::None,
            crate::effects::IDOperation::Created => Self::Created,
            crate::effects::IDOperation::Deleted => Self::Deleted,
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

impl From<TypeArgumentError> for crate::execution_status::TypeArgumentError {
    fn from(value: TypeArgumentError) -> Self {
        match value {
            TypeArgumentError::TypeNotFound => Self::TypeNotFound,
            TypeArgumentError::ConstraintNotSatisfied => Self::ConstraintNotSatisfied,
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

impl From<PackageUpgradeError> for crate::execution_status::PackageUpgradeError {
    fn from(value: PackageUpgradeError) -> Self {
        match value {
            PackageUpgradeError::UnableToFetchPackage { package_id } => {
                Self::UnableToFetchPackage {
                    package_id: package_id.into(),
                }
            }
            PackageUpgradeError::NotAPackage { object_id } => Self::NotAPackage {
                object_id: object_id.into(),
            },
            PackageUpgradeError::IncompatibleUpgrade => Self::IncompatibleUpgrade,
            PackageUpgradeError::DigestDoesNotMatch { digest } => Self::DigestDoesNotMatch {
                digest: digest.into_inner().to_vec(),
            },
            PackageUpgradeError::UnknownUpgradePolicy { policy } => {
                Self::UnknownUpgradePolicy { policy }
            }
            PackageUpgradeError::PackageIdDoesNotMatch {
                package_id,
                ticket_id,
            } => Self::PackageIDDoesNotMatch {
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
            crate::execution_status::CommandArgumentError::InvalidTransferObject |
            crate::execution_status::CommandArgumentError::InvalidMakeMoveVecNonObjectArgument |
            crate::execution_status::CommandArgumentError::ArgumentWithoutValue |
            crate::execution_status::CommandArgumentError::CannotMoveBorrowedValue |
            crate::execution_status::CommandArgumentError::CannotWriteToExtendedReference |
            crate::execution_status::CommandArgumentError::InvalidReferenceArgument => {
                    todo!("New errors need to be added to SDK once stabilized")
            }

        }
    }
}

impl From<CommandArgumentError> for crate::execution_status::CommandArgumentError {
    fn from(value: CommandArgumentError) -> Self {
        match value {
            CommandArgumentError::TypeMismatch => Self::TypeMismatch,
            CommandArgumentError::InvalidBcsBytes => Self::InvalidBCSBytes,
            CommandArgumentError::InvalidUsageOfPureArgument => Self::InvalidUsageOfPureArg,
            CommandArgumentError::InvalidArgumentToPrivateEntryFunction => {
                Self::InvalidArgumentToPrivateEntryFunction
            }
            CommandArgumentError::IndexOutOfBounds { index } => {
                Self::IndexOutOfBounds { idx: index }
            }
            CommandArgumentError::SecondaryIndexOutOfBounds { result, subresult } => {
                Self::SecondaryIndexOutOfBounds {
                    result_idx: result,
                    secondary_idx: subresult,
                }
            }
            CommandArgumentError::InvalidResultArity { result } => {
                Self::InvalidResultArity { result_idx: result }
            }
            CommandArgumentError::InvalidGasCoinUsage => Self::InvalidGasCoinUsage,
            CommandArgumentError::InvalidValueUsage => Self::InvalidValueUsage,
            CommandArgumentError::InvalidObjectByValue => Self::InvalidObjectByValue,
            CommandArgumentError::InvalidObjectByMutRef => Self::InvalidObjectByMutRef,
            CommandArgumentError::SharedObjectOperationNotAllowed => {
                Self::SharedObjectOperationNotAllowed
            }
            CommandArgumentError::InvalidArgumentArity => Self::InvalidArgumentArity,
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
            crate::execution_status::ExecutionFailureStatus::InvalidLinkage => Self::InvalidLinkage,
            crate::execution_status::ExecutionFailureStatus::InsufficientBalanceForWithdraw => {
                todo!("Add InsufficientBalanceForWithdraw to sdk")
            }
        }
    }
}

impl From<ExecutionError> for crate::execution_status::ExecutionFailureStatus {
    fn from(value: ExecutionError) -> Self {
        match value {
            ExecutionError::InsufficientGas => Self::InsufficientGas,
            ExecutionError::InvalidGasObject => Self::InvalidGasObject,
            ExecutionError::InvariantViolation => Self::InvariantViolation,
            ExecutionError::FeatureNotYetSupported => Self::FeatureNotYetSupported,
            ExecutionError::ObjectTooBig {
                object_size,
                max_object_size,
            } => Self::MoveObjectTooBig {
                object_size,
                max_object_size,
            },
            ExecutionError::PackageTooBig {
                object_size,
                max_object_size,
            } => Self::MovePackageTooBig {
                object_size,
                max_object_size,
            },
            ExecutionError::CircularObjectOwnership { object } => Self::CircularObjectOwnership {
                object: object.into(),
            },
            ExecutionError::InsufficientCoinBalance => Self::InsufficientCoinBalance,
            ExecutionError::CoinBalanceOverflow => Self::CoinBalanceOverflow,
            ExecutionError::PublishErrorNonZeroAddress => Self::PublishErrorNonZeroAddress,
            ExecutionError::SuiMoveVerificationError => Self::SuiMoveVerificationError,
            ExecutionError::MovePrimitiveRuntimeError { location } => {
                Self::MovePrimitiveRuntimeError(crate::execution_status::MoveLocationOpt(
                    location.map(Into::into),
                ))
            }
            ExecutionError::MoveAbort { location, code } => Self::MoveAbort(location.into(), code),
            ExecutionError::VmVerificationOrDeserializationError => {
                Self::VMVerificationOrDeserializationError
            }
            ExecutionError::VmInvariantViolation => Self::VMInvariantViolation,
            ExecutionError::FunctionNotFound => Self::FunctionNotFound,
            ExecutionError::ArityMismatch => Self::ArityMismatch,
            ExecutionError::TypeArityMismatch => Self::TypeArityMismatch,
            ExecutionError::NonEntryFunctionInvoked => Self::NonEntryFunctionInvoked,
            ExecutionError::CommandArgumentError { argument, kind } => Self::CommandArgumentError {
                arg_idx: argument,
                kind: kind.into(),
            },
            ExecutionError::TypeArgumentError {
                type_argument,
                kind,
            } => Self::TypeArgumentError {
                argument_idx: type_argument,
                kind: kind.into(),
            },
            ExecutionError::UnusedValueWithoutDrop { result, subresult } => {
                Self::UnusedValueWithoutDrop {
                    result_idx: result,
                    secondary_idx: subresult,
                }
            }
            ExecutionError::InvalidPublicFunctionReturnType { index } => {
                Self::InvalidPublicFunctionReturnType { idx: index }
            }
            ExecutionError::InvalidTransferObject => Self::InvalidTransferObject,
            ExecutionError::EffectsTooLarge {
                current_size,
                max_size,
            } => Self::EffectsTooLarge {
                current_size,
                max_size,
            },
            ExecutionError::PublishUpgradeMissingDependency => {
                Self::PublishUpgradeMissingDependency
            }
            ExecutionError::PublishUpgradeDependencyDowngrade => {
                Self::PublishUpgradeDependencyDowngrade
            }
            ExecutionError::PackageUpgradeError { kind } => Self::PackageUpgradeError {
                upgrade_error: kind.into(),
            },
            ExecutionError::WrittenObjectsTooLarge {
                object_size,
                max_object_size,
            } => Self::WrittenObjectsTooLarge {
                current_size: object_size,
                max_size: max_object_size,
            },
            ExecutionError::CertificateDenied => Self::CertificateDenied,
            ExecutionError::SuiMoveVerificationTimedout => Self::SuiMoveVerificationTimedout,
            ExecutionError::SharedObjectOperationNotAllowed => {
                Self::SharedObjectOperationNotAllowed
            }
            ExecutionError::InputObjectDeleted => Self::InputObjectDeleted,
            ExecutionError::ExecutionCanceledDueToSharedObjectCongestion { congested_objects } => {
                Self::ExecutionCancelledDueToSharedObjectCongestion {
                    congested_objects: crate::execution_status::CongestedObjects(
                        congested_objects.into_iter().map(Into::into).collect(),
                    ),
                }
            }
            ExecutionError::AddressDeniedForCoin { address, coin_type } => {
                Self::AddressDeniedForCoin {
                    address: address.into(),
                    coin_type,
                }
            }
            ExecutionError::CoinTypeGlobalPause { coin_type } => {
                Self::CoinTypeGlobalPause { coin_type }
            }
            ExecutionError::ExecutionCanceledDueToRandomnessUnavailable => {
                Self::ExecutionCancelledDueToRandomnessUnavailable
            }
            ExecutionError::MoveVectorElemTooBig {
                value_size,
                max_scaled_size,
            } => Self::MoveVectorElemTooBig {
                value_size,
                max_scaled_size,
            },
            ExecutionError::MoveRawValueTooBig {
                value_size,
                max_scaled_size,
            } => Self::MoveRawValueTooBig {
                value_size,
                max_scaled_size,
            },
            ExecutionError::InvalidLinkage => Self::InvalidLinkage,
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

impl From<MoveLocation> for crate::execution_status::MoveLocation {
    fn from(value: MoveLocation) -> Self {
        Self {
            module: move_core_types::language_storage::ModuleId::new(
                move_core_types::account_address::AccountAddress::new(value.package.into_inner()),
                move_core_types::identifier::Identifier::new(value.module.into_inner()).unwrap(),
            ),
            function: value.function,
            instruction: value.instruction,
            function_name: value
                .function_name
                .map(Identifier::into_inner)
                .map(Into::into),
        }
    }
}

impl From<crate::execution_status::ExecutionStatus> for ExecutionStatus {
    fn from(value: crate::execution_status::ExecutionStatus) -> Self {
        match value {
            crate::execution_status::ExecutionStatus::Success => Self::Success,
            crate::execution_status::ExecutionStatus::Failure { error, command } => Self::Failure {
                error: error.into(),
                command: command.map(|c| c as u64),
            },
        }
    }
}

impl From<crate::messages_checkpoint::CheckpointCommitment> for CheckpointCommitment {
    fn from(value: crate::messages_checkpoint::CheckpointCommitment) -> Self {
        match value {
            crate::messages_checkpoint::CheckpointCommitment::ECMHLiveObjectSetDigest(digest) => {
                Self::EcmhLiveObjectSet {
                    digest: digest.digest.into(),
                }
            }
        }
    }
}

impl TryFrom<crate::crypto::PublicKey> for MultisigMemberPublicKey {
    type Error = SdkTypeConversionError;

    fn try_from(value: crate::crypto::PublicKey) -> Result<Self, Self::Error> {
        match value {
            crate::crypto::PublicKey::Ed25519(bytes_representation) => {
                Self::Ed25519(Ed25519PublicKey::new(bytes_representation.0))
            }
            crate::crypto::PublicKey::Secp256k1(bytes_representation) => {
                Self::Secp256k1(Secp256k1PublicKey::new(bytes_representation.0))
            }
            crate::crypto::PublicKey::Secp256r1(bytes_representation) => {
                Self::Secp256r1(Secp256r1PublicKey::new(bytes_representation.0))
            }
            crate::crypto::PublicKey::ZkLogin(z) => Self::ZkLogin(z.try_into()?),
            crate::crypto::PublicKey::Passkey(p) => {
                Self::Passkey(PasskeyPublicKey::new(Secp256r1PublicKey::new(p.0)))
            }
        }
        .pipe(Ok)
    }
}

impl TryFrom<crate::crypto::CompressedSignature> for MultisigMemberSignature {
    type Error = SdkTypeConversionError;

    fn try_from(value: crate::crypto::CompressedSignature) -> Result<Self, Self::Error> {
        match value {
            crate::crypto::CompressedSignature::Ed25519(bytes_representation) => {
                Self::Ed25519(Ed25519Signature::new(bytes_representation.0))
            }
            crate::crypto::CompressedSignature::Secp256k1(bytes_representation) => {
                Self::Secp256k1(Secp256k1Signature::new(bytes_representation.0))
            }
            crate::crypto::CompressedSignature::Secp256r1(bytes_representation) => {
                Self::Secp256r1(Secp256r1Signature::new(bytes_representation.0))
            }
            crate::crypto::CompressedSignature::ZkLogin(z) => {
                Self::ZkLogin(Box::new(z.try_into()?))
            }
            crate::crypto::CompressedSignature::Passkey(p) => Self::Passkey(p.try_into()?),
        }
        .pipe(Ok)
    }
}

impl TryFrom<crate::crypto::Signature> for SimpleSignature {
    type Error = SdkTypeConversionError;

    fn try_from(value: crate::crypto::Signature) -> Result<Self, Self::Error> {
        match value {
            crate::crypto::Signature::Ed25519SuiSignature(ed25519_sui_signature) => Self::Ed25519 {
                signature: Ed25519Signature::from_bytes(ed25519_sui_signature.signature_bytes())?,
                public_key: Ed25519PublicKey::from_bytes(ed25519_sui_signature.public_key_bytes())?,
            },
            crate::crypto::Signature::Secp256k1SuiSignature(secp256k1_sui_signature) => {
                Self::Secp256k1 {
                    signature: Secp256k1Signature::from_bytes(
                        secp256k1_sui_signature.signature_bytes(),
                    )?,
                    public_key: Secp256k1PublicKey::from_bytes(
                        secp256k1_sui_signature.public_key_bytes(),
                    )?,
                }
            }

            crate::crypto::Signature::Secp256r1SuiSignature(secp256r1_sui_signature) => {
                Self::Secp256r1 {
                    signature: Secp256r1Signature::from_bytes(
                        secp256r1_sui_signature.signature_bytes(),
                    )?,
                    public_key: Secp256r1PublicKey::from_bytes(
                        secp256r1_sui_signature.public_key_bytes(),
                    )?,
                }
            }
        }
        .pipe(Ok)
    }
}

impl From<crate::crypto::SignatureScheme> for SignatureScheme {
    fn from(value: crate::crypto::SignatureScheme) -> Self {
        match value {
            crate::crypto::SignatureScheme::ED25519 => Self::Ed25519,
            crate::crypto::SignatureScheme::Secp256k1 => Self::Secp256k1,
            crate::crypto::SignatureScheme::Secp256r1 => Self::Secp256r1,
            crate::crypto::SignatureScheme::BLS12381 => Self::Bls12381,
            crate::crypto::SignatureScheme::MultiSig => Self::Multisig,
            crate::crypto::SignatureScheme::ZkLoginAuthenticator => Self::ZkLogin,
            crate::crypto::SignatureScheme::PasskeyAuthenticator => Self::Passkey,
        }
    }
}

impl From<crate::transaction::CallArg> for Input {
    fn from(value: crate::transaction::CallArg) -> Self {
        match value {
            crate::transaction::CallArg::Pure(vec) => Self::Pure { value: vec },
            crate::transaction::CallArg::Object(object_arg) => match object_arg {
                crate::transaction::ObjectArg::ImmOrOwnedObject((id, version, digest)) => {
                    Self::ImmutableOrOwned(ObjectReference::new(
                        id.into(),
                        version.value(),
                        digest.into(),
                    ))
                }
                crate::transaction::ObjectArg::SharedObject {
                    id,
                    initial_shared_version,
                    mutable,
                } => Self::Shared {
                    object_id: id.into(),
                    initial_shared_version: initial_shared_version.value(),
                    mutable,
                },
                crate::transaction::ObjectArg::Receiving((id, version, digest)) => Self::Receiving(
                    ObjectReference::new(id.into(), version.value(), digest.into()),
                ),
            },
            crate::transaction::CallArg::BalanceWithdraw(_) => {
                // TODO(address-balances): Add support for balance withdraws.
                todo!("Convert balance withdraw reservation to sdk Input")
            }
        }
    }
}

impl From<Input> for crate::transaction::CallArg {
    fn from(value: Input) -> Self {
        use crate::transaction::ObjectArg;

        match value {
            Input::Pure { value } => Self::Pure(value),
            Input::ImmutableOrOwned(object_reference) => {
                let (id, version, digest) = object_reference.into_parts();
                Self::Object(ObjectArg::ImmOrOwnedObject((
                    id.into(),
                    version.into(),
                    digest.into(),
                )))
            }
            Input::Shared {
                object_id,
                initial_shared_version,
                mutable,
            } => Self::Object(ObjectArg::SharedObject {
                id: object_id.into(),
                initial_shared_version: initial_shared_version.into(),
                mutable,
            }),
            Input::Receiving(object_reference) => {
                let (id, version, digest) = object_reference.into_parts();
                Self::Object(ObjectArg::Receiving((
                    id.into(),
                    version.into(),
                    digest.into(),
                )))
            }
        }
    }
}

impl From<crate::transaction::Argument> for Argument {
    fn from(value: crate::transaction::Argument) -> Self {
        match value {
            crate::transaction::Argument::GasCoin => Self::Gas,
            crate::transaction::Argument::Input(idx) => Self::Input(idx),
            crate::transaction::Argument::Result(idx) => Self::Result(idx),
            crate::transaction::Argument::NestedResult(idx, sub_idx) => {
                Self::NestedResult(idx, sub_idx)
            }
        }
    }
}

impl From<Argument> for crate::transaction::Argument {
    fn from(value: Argument) -> Self {
        match value {
            Argument::Gas => Self::GasCoin,
            Argument::Input(idx) => Self::Input(idx),
            Argument::Result(idx) => Self::Result(idx),
            Argument::NestedResult(idx, sub_idx) => Self::NestedResult(idx, sub_idx),
        }
    }
}

impl TryFrom<TransactionEffects> for crate::effects::TransactionEffects {
    type Error = SdkTypeConversionError;

    fn try_from(value: TransactionEffects) -> Result<Self, Self::Error> {
        match value {
            TransactionEffects::V1(v1) => Self::V1((*v1).try_into()?),
            TransactionEffects::V2(v2) => Self::V2((*v2).try_into()?),
        }
        .pipe(Ok)
    }
}

impl TryFrom<crate::effects::TransactionEffects> for TransactionEffects {
    type Error = SdkTypeConversionError;

    fn try_from(value: crate::effects::TransactionEffects) -> Result<Self, Self::Error> {
        match value {
            crate::effects::TransactionEffects::V1(v1) => Self::V1(Box::new(v1.try_into()?)),
            crate::effects::TransactionEffects::V2(v2) => Self::V2(Box::new(v2.try_into()?)),
        }
        .pipe(Ok)
    }
}

impl TryFrom<crate::transaction::Command> for Command {
    type Error = SdkTypeConversionError;

    fn try_from(value: crate::transaction::Command) -> Result<Self, Self::Error> {
        match value {
            crate::transaction::Command::MoveCall(programmable_move_call) => {
                Self::MoveCall((*programmable_move_call).try_into()?)
            }
            crate::transaction::Command::TransferObjects(vec, argument) => {
                Self::TransferObjects(TransferObjects {
                    objects: vec.into_iter().map(Into::into).collect(),
                    address: argument.into(),
                })
            }
            crate::transaction::Command::SplitCoins(argument, vec) => {
                Self::SplitCoins(SplitCoins {
                    coin: argument.into(),
                    amounts: vec.into_iter().map(Into::into).collect(),
                })
            }
            crate::transaction::Command::MergeCoins(argument, vec) => {
                Self::MergeCoins(MergeCoins {
                    coin: argument.into(),
                    coins_to_merge: vec.into_iter().map(Into::into).collect(),
                })
            }
            crate::transaction::Command::Publish(vec, vec1) => Self::Publish(Publish {
                modules: vec,
                dependencies: vec1.into_iter().map(Into::into).collect(),
            }),
            crate::transaction::Command::MakeMoveVec(type_input, elements) => {
                Self::MakeMoveVector(MakeMoveVector {
                    type_: type_input.map(TryInto::try_into).transpose()?,
                    elements: elements.into_iter().map(Into::into).collect(),
                })
            }
            crate::transaction::Command::Upgrade(modules, deps, object_id, ticket) => {
                Self::Upgrade(Upgrade {
                    modules,
                    dependencies: deps.into_iter().map(Into::into).collect(),
                    package: object_id.into(),
                    ticket: ticket.into(),
                })
            }
        }
        .pipe(Ok)
    }
}

impl TryFrom<crate::transaction::ProgrammableMoveCall> for MoveCall {
    type Error = SdkTypeConversionError;

    fn try_from(value: crate::transaction::ProgrammableMoveCall) -> Result<Self, Self::Error> {
        Self {
            package: value.package.into(),
            module: Identifier::new(value.module)?,
            function: Identifier::new(value.function)?,
            type_arguments: value
                .type_arguments
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            arguments: value.arguments.into_iter().map(Into::into).collect(),
        }
        .pipe(Ok)
    }
}

impl From<MoveCall> for crate::transaction::ProgrammableMoveCall {
    fn from(value: MoveCall) -> Self {
        Self {
            package: value.package.into(),
            module: value.module.into_inner().into(),
            function: value.function.into_inner().into(),
            type_arguments: value.type_arguments.into_iter().map(Into::into).collect(),
            arguments: value.arguments.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<Command> for crate::transaction::Command {
    fn from(value: Command) -> Self {
        match value {
            Command::MoveCall(move_call) => Self::MoveCall(Box::new(move_call.into())),
            Command::TransferObjects(TransferObjects { objects, address }) => {
                Self::TransferObjects(
                    objects.into_iter().map(Into::into).collect(),
                    address.into(),
                )
            }
            Command::SplitCoins(SplitCoins { coin, amounts }) => {
                Self::SplitCoins(coin.into(), amounts.into_iter().map(Into::into).collect())
            }
            Command::MergeCoins(MergeCoins {
                coin,
                coins_to_merge,
            }) => Self::MergeCoins(
                coin.into(),
                coins_to_merge.into_iter().map(Into::into).collect(),
            ),
            Command::Publish(Publish {
                modules,
                dependencies,
            }) => Self::Publish(modules, dependencies.into_iter().map(Into::into).collect()),
            Command::MakeMoveVector(MakeMoveVector { type_, elements }) => Self::MakeMoveVec(
                type_.map(Into::into),
                elements.into_iter().map(Into::into).collect(),
            ),
            Command::Upgrade(Upgrade {
                modules,
                dependencies,
                package,
                ticket,
            }) => Self::Upgrade(
                modules,
                dependencies.into_iter().map(Into::into).collect(),
                package.into(),
                ticket.into(),
            ),
        }
    }
}

impl From<crate::transaction::StoredExecutionTimeObservations> for ExecutionTimeObservations {
    fn from(value: crate::transaction::StoredExecutionTimeObservations) -> Self {
        match value {
            crate::transaction::StoredExecutionTimeObservations::V1(vec) => Self::V1(
                vec.into_iter()
                    .map(|(key, value)| {
                        (
                            key.into(),
                            value
                                .into_iter()
                                .map(|(name, duration)| ValidatorExecutionTimeObservation {
                                    validator: name.into(),
                                    duration,
                                })
                                .collect(),
                        )
                    })
                    .collect(),
            ),
        }
    }
}

impl From<crate::execution::ExecutionTimeObservationKey> for ExecutionTimeObservationKey {
    fn from(value: crate::execution::ExecutionTimeObservationKey) -> Self {
        match value {
            crate::execution::ExecutionTimeObservationKey::MoveEntryPoint {
                package,
                module,
                function,
                type_arguments,
            } => Self::MoveEntryPoint {
                package: package.into(),
                module,
                function,
                type_arguments: type_arguments
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<_, _>>()
                    .unwrap(),
            },
            crate::execution::ExecutionTimeObservationKey::TransferObjects => Self::TransferObjects,
            crate::execution::ExecutionTimeObservationKey::SplitCoins => Self::SplitCoins,
            crate::execution::ExecutionTimeObservationKey::MergeCoins => Self::MergeCoins,
            crate::execution::ExecutionTimeObservationKey::Publish => Self::Publish,
            crate::execution::ExecutionTimeObservationKey::MakeMoveVec => Self::MakeMoveVec,
            crate::execution::ExecutionTimeObservationKey::Upgrade => Self::Upgrade,
        }
    }
}

impl From<crate::transaction::EndOfEpochTransactionKind> for EndOfEpochTransactionKind {
    fn from(value: crate::transaction::EndOfEpochTransactionKind) -> Self {
        match value {
            crate::transaction::EndOfEpochTransactionKind::ChangeEpoch(change_epoch) => {
                Self::ChangeEpoch(change_epoch.into())
            }
            crate::transaction::EndOfEpochTransactionKind::AuthenticatorStateCreate => {
                Self::AuthenticatorStateCreate
            }
            crate::transaction::EndOfEpochTransactionKind::AuthenticatorStateExpire(
                authenticator_state_expire,
            ) => Self::AuthenticatorStateExpire(authenticator_state_expire.into()),
            crate::transaction::EndOfEpochTransactionKind::RandomnessStateCreate => {
                Self::RandomnessStateCreate
            }
            crate::transaction::EndOfEpochTransactionKind::DenyListStateCreate => {
                Self::DenyListStateCreate
            }
            crate::transaction::EndOfEpochTransactionKind::BridgeStateCreate(chain_identifier) => {
                Self::BridgeStateCreate {
                    chain_id: CheckpointDigest::new(chain_identifier.as_bytes().to_owned()),
                }
            }
            crate::transaction::EndOfEpochTransactionKind::BridgeCommitteeInit(sequence_number) => {
                Self::BridgeCommitteeInit {
                    bridge_object_version: sequence_number.value(),
                }
            }
            crate::transaction::EndOfEpochTransactionKind::StoreExecutionTimeObservations(
                stored_execution_time_observations,
            ) => Self::StoreExecutionTimeObservations(stored_execution_time_observations.into()),
            crate::transaction::EndOfEpochTransactionKind::AccumulatorRootCreate => {
                Self::AccumulatorRootCreate
            }
        }
    }
}

impl From<crate::transaction::ChangeEpoch> for ChangeEpoch {
    fn from(
        crate::transaction::ChangeEpoch {
            epoch,
            protocol_version,
            storage_charge,
            computation_charge,
            storage_rebate,
            non_refundable_storage_fee,
            epoch_start_timestamp_ms,
            system_packages,
        }: crate::transaction::ChangeEpoch,
    ) -> Self {
        Self {
            epoch,
            protocol_version: protocol_version.as_u64(),
            storage_charge,
            computation_charge,
            storage_rebate,
            non_refundable_storage_fee,
            epoch_start_timestamp_ms,
            system_packages: system_packages
                .into_iter()
                .map(|(version, modules, dependencies)| SystemPackage {
                    version: version.value(),
                    modules,
                    dependencies: dependencies.into_iter().map(Into::into).collect(),
                })
                .collect(),
        }
    }
}

impl From<crate::transaction::AuthenticatorStateExpire> for AuthenticatorStateExpire {
    fn from(value: crate::transaction::AuthenticatorStateExpire) -> Self {
        Self {
            min_epoch: value.min_epoch,
            authenticator_object_initial_shared_version: value
                .authenticator_obj_initial_shared_version
                .value(),
        }
    }
}

impl From<crate::messages_consensus::ConsensusDeterminedVersionAssignments>
    for ConsensusDeterminedVersionAssignments
{
    fn from(value: crate::messages_consensus::ConsensusDeterminedVersionAssignments) -> Self {
        use crate::messages_consensus::ConsensusDeterminedVersionAssignments::*;
        match value {
            CancelledTransactions(vec) => Self::CanceledTransactions {
                canceled_transactions: vec
                    .into_iter()
                    .map(|(digest, assignments)| CanceledTransaction {
                        digest: digest.into(),
                        version_assignments: assignments
                            .into_iter()
                            .map(|(id, version)| VersionAssignment {
                                object_id: id.into(),
                                version: version.value(),
                            })
                            .collect(),
                    })
                    .collect(),
            },
            CancelledTransactionsV2(canceled_transactions) => Self::CanceledTransactionsV2 {
                canceled_transactions: canceled_transactions
                    .into_iter()
                    .map(|(digest, assignments)| CanceledTransactionV2 {
                        digest: digest.into(),
                        version_assignments: assignments
                            .into_iter()
                            .map(|((id, start_version), version)| VersionAssignmentV2 {
                                object_id: id.into(),
                                start_version: start_version.value(),
                                version: version.value(),
                            })
                            .collect(),
                    })
                    .collect(),
            },
        }
    }
}

impl From<crate::authenticator_state::ActiveJwk> for ActiveJwk {
    fn from(value: crate::authenticator_state::ActiveJwk) -> Self {
        let crate::authenticator_state::ActiveJwk { jwk_id, jwk, epoch } = value;
        Self {
            jwk_id: JwkId {
                iss: jwk_id.iss,
                kid: jwk_id.kid,
            },
            jwk: Jwk {
                kty: jwk.kty,
                e: jwk.e,
                n: jwk.n,
                alg: jwk.alg,
            },
            epoch,
        }
    }
}

// TODO remaining set of enums to add impls for to ensure new additions are caught during review
//
// impl From<crate::transaction::TransactionKind> for TransactionKind {
//     fn from(value: crate::transaction::TransactionKind) -> Self {
//         todo!()
//     }
// }
// src/object.rs:pub enum ObjectData {
