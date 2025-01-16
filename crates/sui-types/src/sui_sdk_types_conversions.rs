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
            UnchangedSharedKind::MutateDeleted { version } => Self::MutateDeleted(version.into()),
            UnchangedSharedKind::ReadDeleted { version } => Self::ReadDeleted(version.into()),
            UnchangedSharedKind::Cancelled { version } => Self::Cancelled(version.into()),
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
            crate::effects::UnchangedSharedKind::MutateDeleted(version) => Self::MutateDeleted {
                version: version.into(),
            },
            crate::effects::UnchangedSharedKind::ReadDeleted(version) => Self::ReadDeleted {
                version: version.into(),
            },
            crate::effects::UnchangedSharedKind::Cancelled(version) => Self::Cancelled {
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
