// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::Transaction;
use crate::message::{MessageField, MessageFields, MessageMerge};
use crate::proto::TryFromProtoError;
use tap::Pipe;

//
// Transaction
//

impl Transaction {
    const BCS_FIELD: &'static MessageField =
        &MessageField::new("bcs").with_message_fields(super::Bcs::FIELDS);
    const DIGEST_FIELD: &'static MessageField = &MessageField::new("digest");
    const VERSION_FIELD: &'static MessageField = &MessageField::new("version");
    const KIND_FIELD: &'static MessageField = &MessageField::new("kind");
    const SENDER_FIELD: &'static MessageField = &MessageField::new("sender");
    const GAS_PAYMENT_FIELD: &'static MessageField = &MessageField::new("gas_payment");
    const EXPIRATION_FIELD: &'static MessageField = &MessageField::new("expiration");
}

impl MessageFields for Transaction {
    const FIELDS: &'static [&'static MessageField] = &[
        Self::BCS_FIELD,
        Self::DIGEST_FIELD,
        Self::VERSION_FIELD,
        Self::KIND_FIELD,
        Self::SENDER_FIELD,
        Self::GAS_PAYMENT_FIELD,
        Self::EXPIRATION_FIELD,
    ];
}

impl From<sui_sdk_types::Transaction> for Transaction {
    fn from(value: sui_sdk_types::Transaction) -> Self {
        let mut message = Self::default();
        message.merge(value, &crate::field_mask::FieldMaskTree::new_wildcard());
        message
    }
}

impl MessageMerge<sui_sdk_types::Transaction> for Transaction {
    fn merge(
        &mut self,
        source: sui_sdk_types::Transaction,
        mask: &crate::field_mask::FieldMaskTree,
    ) {
        if mask.contains(Self::BCS_FIELD.name) {
            self.bcs = Some(super::Bcs::serialize(&source).unwrap());
        }

        if mask.contains(Self::DIGEST_FIELD.name) {
            self.digest = Some(source.digest().to_string());
        }

        if mask.contains(Self::VERSION_FIELD.name) {
            self.version = Some(1);
        }

        if mask.contains(Self::KIND_FIELD.name) {
            self.kind = Some(source.kind.into());
        }

        if mask.contains(Self::SENDER_FIELD.name) {
            self.sender = Some(source.sender.to_string());
        }

        if mask.contains(Self::GAS_PAYMENT_FIELD.name) {
            self.gas_payment = Some(source.gas_payment.into());
        }

        if mask.contains(Self::EXPIRATION_FIELD.name) {
            self.expiration = Some(source.expiration.into());
        }
    }
}

impl MessageMerge<&Transaction> for Transaction {
    fn merge(&mut self, source: &Transaction, mask: &crate::field_mask::FieldMaskTree) {
        let Transaction {
            bcs,
            digest,
            version,
            kind,
            sender,
            gas_payment,
            expiration,
        } = source;

        if mask.contains(Self::BCS_FIELD.name) {
            self.bcs = bcs.clone();
        }

        if mask.contains(Self::DIGEST_FIELD.name) {
            self.digest = digest.clone();
        }

        if mask.contains(Self::VERSION_FIELD.name) {
            self.version = *version;
        }

        if mask.contains(Self::KIND_FIELD.name) {
            self.kind = kind.clone();
        }

        if mask.contains(Self::SENDER_FIELD.name) {
            self.sender = sender.clone();
        }

        if mask.contains(Self::GAS_PAYMENT_FIELD.name) {
            self.gas_payment = gas_payment.clone();
        }

        if mask.contains(Self::EXPIRATION_FIELD.name) {
            self.expiration = *expiration;
        }
    }
}

impl TryFrom<&Transaction> for sui_sdk_types::Transaction {
    type Error = TryFromProtoError;

    fn try_from(value: &Transaction) -> Result<Self, Self::Error> {
        if let Some(bcs) = &value.bcs {
            return bcs.deserialize().map_err(TryFromProtoError::from_error);
        }

        match value.version {
            Some(1) => {}
            _ => {
                return Err(TryFromProtoError::from_error("unknown Transaction version"));
            }
        }

        let kind = value
            .kind
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("kind"))?
            .try_into()?;

        let sender = value
            .sender
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("sender"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        let gas_payment = value
            .gas_payment
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("gas_payment"))?
            .try_into()?;

        let expiration = value
            .expiration
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("expiration"))?
            .try_into()?;

        Ok(Self {
            kind,
            sender,
            gas_payment,
            expiration,
        })
    }
}

//
// GasPayment
//

impl From<sui_sdk_types::GasPayment> for super::GasPayment {
    fn from(value: sui_sdk_types::GasPayment) -> Self {
        Self {
            objects: value.objects.into_iter().map(Into::into).collect(),
            owner: Some(value.owner.to_string()),
            price: Some(value.price),
            budget: Some(value.budget),
        }
    }
}

impl TryFrom<&super::GasPayment> for sui_sdk_types::GasPayment {
    type Error = TryFromProtoError;

    fn try_from(value: &super::GasPayment) -> Result<Self, Self::Error> {
        let objects = value
            .objects
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        let owner = value
            .owner
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("owner"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;
        let price = value
            .price
            .ok_or_else(|| TryFromProtoError::missing("price"))?;
        let budget = value
            .budget
            .ok_or_else(|| TryFromProtoError::missing("budget"))?;
        Ok(Self {
            objects,
            owner,
            price,
            budget,
        })
    }
}

//
// TransactionExpiration
//

impl From<sui_sdk_types::TransactionExpiration> for super::TransactionExpiration {
    fn from(value: sui_sdk_types::TransactionExpiration) -> Self {
        use super::transaction_expiration::TransactionExpirationKind;
        use sui_sdk_types::TransactionExpiration::*;

        let mut message = Self::default();

        let kind = match value {
            None => TransactionExpirationKind::None,
            Epoch(epoch) => {
                message.epoch = Some(epoch);
                TransactionExpirationKind::Epoch
            }
        };

        message.set_kind(kind);
        message
    }
}

impl TryFrom<&super::TransactionExpiration> for sui_sdk_types::TransactionExpiration {
    type Error = TryFromProtoError;

    fn try_from(value: &super::TransactionExpiration) -> Result<Self, Self::Error> {
        use super::transaction_expiration::TransactionExpirationKind;

        match value.kind() {
            TransactionExpirationKind::Unknown => {
                return Err(TryFromProtoError::from_error(
                    "unknown TransactionExpirationKind",
                ))
            }
            TransactionExpirationKind::None => Self::None,
            TransactionExpirationKind::Epoch => Self::Epoch(value.epoch()),
        }
        .pipe(Ok)
    }
}

//
// TransactionKind
//

impl From<sui_sdk_types::TransactionKind> for super::TransactionKind {
    fn from(value: sui_sdk_types::TransactionKind) -> Self {
        use super::transaction_kind::Kind;
        use sui_sdk_types::TransactionKind::*;

        let kind = match value {
            ProgrammableTransaction(ptb) => Kind::ProgrammableTransaction(ptb.into()),
            ChangeEpoch(change_epoch) => Kind::ChangeEpoch(change_epoch.into()),
            Genesis(genesis) => Kind::Genesis(genesis.into()),
            ConsensusCommitPrologue(prologue) => Kind::ConsensusCommitPrologueV1(prologue.into()),
            AuthenticatorStateUpdate(update) => Kind::AuthenticatorStateUpdate(update.into()),
            EndOfEpoch(transactions) => Kind::EndOfEpoch(super::EndOfEpochTransaction {
                transactions: transactions.into_iter().map(Into::into).collect(),
            }),
            RandomnessStateUpdate(update) => Kind::RandomnessStateUpdate(update.into()),
            ConsensusCommitPrologueV2(prologue) => Kind::ConsensusCommitPrologueV2(prologue.into()),
            ConsensusCommitPrologueV3(prologue) => Kind::ConsensusCommitPrologueV3(prologue.into()),
            ConsensusCommitPrologueV4(prologue) => Kind::ConsensusCommitPrologueV4(prologue.into()),
        };

        Self { kind: Some(kind) }
    }
}

impl TryFrom<&super::TransactionKind> for sui_sdk_types::TransactionKind {
    type Error = TryFromProtoError;

    fn try_from(value: &super::TransactionKind) -> Result<Self, Self::Error> {
        use super::transaction_kind::Kind;

        match value
            .kind
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("kind"))?
        {
            Kind::ProgrammableTransaction(ptb) => Self::ProgrammableTransaction(ptb.try_into()?),
            Kind::ChangeEpoch(change_epoch) => Self::ChangeEpoch(change_epoch.try_into()?),
            Kind::Genesis(genesis) => Self::Genesis(genesis.try_into()?),
            Kind::ConsensusCommitPrologueV1(prologue) => {
                Self::ConsensusCommitPrologue(prologue.try_into()?)
            }
            Kind::AuthenticatorStateUpdate(update) => {
                Self::AuthenticatorStateUpdate(update.try_into()?)
            }
            Kind::EndOfEpoch(super::EndOfEpochTransaction { transactions }) => Self::EndOfEpoch(
                transactions
                    .iter()
                    .map(TryInto::try_into)
                    .collect::<Result<_, _>>()?,
            ),
            Kind::RandomnessStateUpdate(update) => Self::RandomnessStateUpdate(update.try_into()?),
            Kind::ConsensusCommitPrologueV2(prologue) => {
                Self::ConsensusCommitPrologueV2(prologue.try_into()?)
            }
            Kind::ConsensusCommitPrologueV3(prologue) => {
                Self::ConsensusCommitPrologueV3(prologue.try_into()?)
            }
            Kind::ConsensusCommitPrologueV4(prologue) => {
                Self::ConsensusCommitPrologueV4(prologue.try_into()?)
            }
        }
        .pipe(Ok)
    }
}

//
// ConsensusCommitPrologue
//

impl From<sui_sdk_types::ConsensusCommitPrologue> for super::ConsensusCommitPrologue {
    fn from(value: sui_sdk_types::ConsensusCommitPrologue) -> Self {
        Self {
            epoch: Some(value.epoch),
            round: Some(value.round),
            commit_timestamp: Some(crate::proto::types::timestamp_ms_to_proto(
                value.commit_timestamp_ms,
            )),
            consensus_commit_digest: None,
            sub_dag_index: None,
            consensus_determined_version_assignments: None,
            additional_state_digest: None,
        }
    }
}

impl TryFrom<&super::ConsensusCommitPrologue> for sui_sdk_types::ConsensusCommitPrologue {
    type Error = TryFromProtoError;

    fn try_from(value: &super::ConsensusCommitPrologue) -> Result<Self, Self::Error> {
        let epoch = value
            .epoch
            .ok_or_else(|| TryFromProtoError::missing("epoch"))?;
        let round = value
            .round
            .ok_or_else(|| TryFromProtoError::missing("round"))?;
        let commit_timestamp_ms = value
            .commit_timestamp
            .ok_or_else(|| TryFromProtoError::missing("commit_timestamp"))?
            .pipe(crate::proto::types::proto_to_timestamp_ms)?;

        Ok(Self {
            epoch,
            round,
            commit_timestamp_ms,
        })
    }
}

impl From<sui_sdk_types::ConsensusCommitPrologueV2> for super::ConsensusCommitPrologue {
    fn from(value: sui_sdk_types::ConsensusCommitPrologueV2) -> Self {
        Self {
            epoch: Some(value.epoch),
            round: Some(value.round),
            commit_timestamp: Some(crate::proto::types::timestamp_ms_to_proto(
                value.commit_timestamp_ms,
            )),
            consensus_commit_digest: Some(value.consensus_commit_digest.to_string()),
            sub_dag_index: None,
            consensus_determined_version_assignments: None,
            additional_state_digest: None,
        }
    }
}

impl TryFrom<&super::ConsensusCommitPrologue> for sui_sdk_types::ConsensusCommitPrologueV2 {
    type Error = TryFromProtoError;

    fn try_from(value: &super::ConsensusCommitPrologue) -> Result<Self, Self::Error> {
        let epoch = value
            .epoch
            .ok_or_else(|| TryFromProtoError::missing("epoch"))?;
        let round = value
            .round
            .ok_or_else(|| TryFromProtoError::missing("round"))?;
        let commit_timestamp_ms = value
            .commit_timestamp
            .ok_or_else(|| TryFromProtoError::missing("commit_timestamp"))?
            .pipe(crate::proto::types::proto_to_timestamp_ms)?;

        let consensus_commit_digest = value
            .consensus_commit_digest
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("consensus_commit_digest"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        Ok(Self {
            epoch,
            round,
            commit_timestamp_ms,
            consensus_commit_digest,
        })
    }
}

impl From<sui_sdk_types::ConsensusCommitPrologueV3> for super::ConsensusCommitPrologue {
    fn from(value: sui_sdk_types::ConsensusCommitPrologueV3) -> Self {
        Self {
            epoch: Some(value.epoch),
            round: Some(value.round),
            commit_timestamp: Some(crate::proto::types::timestamp_ms_to_proto(
                value.commit_timestamp_ms,
            )),
            consensus_commit_digest: Some(value.consensus_commit_digest.to_string()),
            sub_dag_index: value.sub_dag_index,
            consensus_determined_version_assignments: Some(
                value.consensus_determined_version_assignments.into(),
            ),
            additional_state_digest: None,
        }
    }
}

impl TryFrom<&super::ConsensusCommitPrologue> for sui_sdk_types::ConsensusCommitPrologueV3 {
    type Error = TryFromProtoError;

    fn try_from(value: &super::ConsensusCommitPrologue) -> Result<Self, Self::Error> {
        let epoch = value
            .epoch
            .ok_or_else(|| TryFromProtoError::missing("epoch"))?;
        let round = value
            .round
            .ok_or_else(|| TryFromProtoError::missing("round"))?;
        let commit_timestamp_ms = value
            .commit_timestamp
            .ok_or_else(|| TryFromProtoError::missing("commit_timestamp"))?
            .pipe(crate::proto::types::proto_to_timestamp_ms)?;

        let consensus_commit_digest = value
            .consensus_commit_digest
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("consensus_commit_digest"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        let consensus_determined_version_assignments = value
            .consensus_determined_version_assignments
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("consensus_determined_version_assignments"))?
            .try_into()?;

        Ok(Self {
            epoch,
            round,
            commit_timestamp_ms,
            sub_dag_index: value.sub_dag_index,
            consensus_commit_digest,
            consensus_determined_version_assignments,
        })
    }
}

impl From<sui_sdk_types::ConsensusCommitPrologueV4> for super::ConsensusCommitPrologue {
    fn from(
        sui_sdk_types::ConsensusCommitPrologueV4 {
            epoch,
            round,
            sub_dag_index,
            commit_timestamp_ms,
            consensus_commit_digest,
            consensus_determined_version_assignments,
            additional_state_digest,
        }: sui_sdk_types::ConsensusCommitPrologueV4,
    ) -> Self {
        Self {
            epoch: Some(epoch),
            round: Some(round),
            commit_timestamp: Some(crate::proto::types::timestamp_ms_to_proto(
                commit_timestamp_ms,
            )),
            consensus_commit_digest: Some(consensus_commit_digest.to_string()),
            sub_dag_index,
            consensus_determined_version_assignments: Some(
                consensus_determined_version_assignments.into(),
            ),
            additional_state_digest: Some(additional_state_digest.to_string()),
        }
    }
}

impl TryFrom<&super::ConsensusCommitPrologue> for sui_sdk_types::ConsensusCommitPrologueV4 {
    type Error = TryFromProtoError;

    fn try_from(value: &super::ConsensusCommitPrologue) -> Result<Self, Self::Error> {
        let epoch = value
            .epoch
            .ok_or_else(|| TryFromProtoError::missing("epoch"))?;
        let round = value
            .round
            .ok_or_else(|| TryFromProtoError::missing("round"))?;
        let commit_timestamp_ms = value
            .commit_timestamp
            .ok_or_else(|| TryFromProtoError::missing("commit_timestamp"))?
            .pipe(crate::proto::types::proto_to_timestamp_ms)?;

        let consensus_commit_digest = value
            .consensus_commit_digest
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("consensus_commit_digest"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        let consensus_determined_version_assignments = value
            .consensus_determined_version_assignments
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("consensus_determined_version_assignments"))?
            .try_into()?;

        let additional_state_digest = value
            .additional_state_digest
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("additional_state_digest"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        Ok(Self {
            epoch,
            round,
            commit_timestamp_ms,
            sub_dag_index: value.sub_dag_index,
            consensus_commit_digest,
            consensus_determined_version_assignments,
            additional_state_digest,
        })
    }
}

//
// ConsensusDeterminedVersionAssignments
//

impl From<sui_sdk_types::ConsensusDeterminedVersionAssignments>
    for super::ConsensusDeterminedVersionAssignments
{
    fn from(value: sui_sdk_types::ConsensusDeterminedVersionAssignments) -> Self {
        use super::consensus_determined_version_assignments::Kind;
        use sui_sdk_types::ConsensusDeterminedVersionAssignments::*;

        let kind = match value {
            CanceledTransactions {
                canceled_transactions,
            } => Kind::CanceledTransactions(super::CanceledTransactions {
                canceled_transactions: canceled_transactions.into_iter().map(Into::into).collect(),
            }),
        };

        Self { kind: Some(kind) }
    }
}

impl TryFrom<&super::ConsensusDeterminedVersionAssignments>
    for sui_sdk_types::ConsensusDeterminedVersionAssignments
{
    type Error = TryFromProtoError;

    fn try_from(value: &super::ConsensusDeterminedVersionAssignments) -> Result<Self, Self::Error> {
        use super::consensus_determined_version_assignments::Kind;

        match value
            .kind
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("kind"))?
        {
            Kind::CanceledTransactions(super::CanceledTransactions {
                canceled_transactions,
            }) => Self::CanceledTransactions {
                canceled_transactions: canceled_transactions
                    .iter()
                    .map(TryInto::try_into)
                    .collect::<Result<_, _>>()?,
            },
        }
        .pipe(Ok)
    }
}

//
// CanceledTransaction
//

impl From<sui_sdk_types::CanceledTransaction> for super::CanceledTransaction {
    fn from(value: sui_sdk_types::CanceledTransaction) -> Self {
        Self {
            digest: Some(value.digest.to_string()),
            version_assignments: value
                .version_assignments
                .into_iter()
                .map(Into::into)
                .collect(),
        }
    }
}

impl TryFrom<&super::CanceledTransaction> for sui_sdk_types::CanceledTransaction {
    type Error = TryFromProtoError;

    fn try_from(value: &super::CanceledTransaction) -> Result<Self, Self::Error> {
        let digest = value
            .digest
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("digest"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        let version_assignments = value
            .version_assignments
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        Ok(Self {
            digest,
            version_assignments,
        })
    }
}

//
// VersionAssignment
//

impl From<sui_sdk_types::VersionAssignment> for super::VersionAssignment {
    fn from(value: sui_sdk_types::VersionAssignment) -> Self {
        Self {
            object_id: Some(value.object_id.to_string()),
            version: Some(value.version),
        }
    }
}

impl TryFrom<&super::VersionAssignment> for sui_sdk_types::VersionAssignment {
    type Error = TryFromProtoError;

    fn try_from(value: &super::VersionAssignment) -> Result<Self, Self::Error> {
        let object_id = value
            .object_id
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("object_id"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;
        let version = value
            .version
            .ok_or_else(|| TryFromProtoError::missing("version"))?;

        Ok(Self { object_id, version })
    }
}

//
// GenesisTransaction
//

impl From<sui_sdk_types::GenesisTransaction> for super::GenesisTransaction {
    fn from(value: sui_sdk_types::GenesisTransaction) -> Self {
        Self {
            objects: value.objects.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<&super::GenesisTransaction> for sui_sdk_types::GenesisTransaction {
    type Error = TryFromProtoError;

    fn try_from(value: &super::GenesisTransaction) -> Result<Self, Self::Error> {
        let objects = value
            .objects
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        Ok(Self { objects })
    }
}

//
// RandomnessStateUpdate
//

impl From<sui_sdk_types::RandomnessStateUpdate> for super::RandomnessStateUpdate {
    fn from(value: sui_sdk_types::RandomnessStateUpdate) -> Self {
        Self {
            epoch: Some(value.epoch),
            randomness_round: Some(value.randomness_round),
            random_bytes: Some(value.random_bytes.into()),
            randomness_object_initial_shared_version: Some(
                value.randomness_obj_initial_shared_version,
            ),
        }
    }
}

impl TryFrom<&super::RandomnessStateUpdate> for sui_sdk_types::RandomnessStateUpdate {
    type Error = TryFromProtoError;

    fn try_from(
        super::RandomnessStateUpdate {
            epoch,
            randomness_round,
            random_bytes,
            randomness_object_initial_shared_version,
        }: &super::RandomnessStateUpdate,
    ) -> Result<Self, Self::Error> {
        let epoch = epoch.ok_or_else(|| TryFromProtoError::missing("epoch"))?;
        let randomness_round =
            randomness_round.ok_or_else(|| TryFromProtoError::missing("randomness_round"))?;
        let random_bytes = random_bytes
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("random_bytes"))?
            .to_vec();
        let randomness_obj_initial_shared_version = randomness_object_initial_shared_version
            .ok_or_else(|| {
                TryFromProtoError::missing("randomness_object_initial_shared_version")
            })?;
        Ok(Self {
            epoch,
            randomness_round,
            random_bytes,
            randomness_obj_initial_shared_version,
        })
    }
}

//
// AuthenticatorStateUpdate
//

impl From<sui_sdk_types::AuthenticatorStateUpdate> for super::AuthenticatorStateUpdate {
    fn from(value: sui_sdk_types::AuthenticatorStateUpdate) -> Self {
        Self {
            epoch: Some(value.epoch),
            round: Some(value.round),
            new_active_jwks: value.new_active_jwks.into_iter().map(Into::into).collect(),
            authenticator_object_initial_shared_version: Some(
                value.authenticator_obj_initial_shared_version,
            ),
        }
    }
}

impl TryFrom<&super::AuthenticatorStateUpdate> for sui_sdk_types::AuthenticatorStateUpdate {
    type Error = TryFromProtoError;

    fn try_from(
        super::AuthenticatorStateUpdate {
            epoch,
            round,
            new_active_jwks,
            authenticator_object_initial_shared_version,
        }: &super::AuthenticatorStateUpdate,
    ) -> Result<Self, Self::Error> {
        let epoch = epoch.ok_or_else(|| TryFromProtoError::missing("epoch"))?;
        let round = round.ok_or_else(|| TryFromProtoError::missing("round"))?;
        let authenticator_obj_initial_shared_version = authenticator_object_initial_shared_version
            .ok_or_else(|| {
                TryFromProtoError::missing("authenticator_object_initial_shared_version")
            })?;
        Ok(Self {
            epoch,
            round,
            new_active_jwks: new_active_jwks
                .iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            authenticator_obj_initial_shared_version,
        })
    }
}

//
// Jwk
//

impl From<sui_sdk_types::Jwk> for super::Jwk {
    fn from(sui_sdk_types::Jwk { kty, e, n, alg }: sui_sdk_types::Jwk) -> Self {
        Self {
            kty: Some(kty),
            e: Some(e),
            n: Some(n),
            alg: Some(alg),
        }
    }
}

impl TryFrom<&super::Jwk> for sui_sdk_types::Jwk {
    type Error = TryFromProtoError;

    fn try_from(super::Jwk { kty, e, n, alg }: &super::Jwk) -> Result<Self, Self::Error> {
        let kty = kty
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("kty"))?
            .into();
        let e = e
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("e"))?
            .into();
        let n = n
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("n"))?
            .into();
        let alg = alg
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("alg"))?
            .into();
        Ok(Self { kty, e, n, alg })
    }
}

//
// JwkId
//

impl From<sui_sdk_types::JwkId> for super::JwkId {
    fn from(sui_sdk_types::JwkId { iss, kid }: sui_sdk_types::JwkId) -> Self {
        Self {
            iss: Some(iss),
            kid: Some(kid),
        }
    }
}

impl TryFrom<&super::JwkId> for sui_sdk_types::JwkId {
    type Error = TryFromProtoError;

    fn try_from(super::JwkId { iss, kid }: &super::JwkId) -> Result<Self, Self::Error> {
        let iss = iss
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("iss"))?
            .into();
        let kid = kid
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("kid"))?
            .into();
        Ok(Self { iss, kid })
    }
}

//
// ActiveJwk
//

impl From<sui_sdk_types::ActiveJwk> for super::ActiveJwk {
    fn from(value: sui_sdk_types::ActiveJwk) -> Self {
        Self {
            id: Some(value.jwk_id.into()),
            jwk: Some(value.jwk.into()),
            epoch: Some(value.epoch),
        }
    }
}

impl TryFrom<&super::ActiveJwk> for sui_sdk_types::ActiveJwk {
    type Error = TryFromProtoError;

    fn try_from(value: &super::ActiveJwk) -> Result<Self, Self::Error> {
        let jwk_id = value
            .id
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("id"))?
            .try_into()?;

        let jwk = value
            .jwk
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("jwk"))?
            .try_into()?;

        let epoch = value
            .epoch
            .ok_or_else(|| TryFromProtoError::missing("epoch"))?;

        Ok(Self { jwk_id, jwk, epoch })
    }
}

//
// ChangeEpoch
//

impl From<sui_sdk_types::ChangeEpoch> for super::ChangeEpoch {
    fn from(value: sui_sdk_types::ChangeEpoch) -> Self {
        Self {
            epoch: Some(value.epoch),
            protocol_version: Some(value.protocol_version),
            storage_charge: Some(value.storage_charge),
            computation_charge: Some(value.computation_charge),
            storage_rebate: Some(value.storage_rebate),
            non_refundable_storage_fee: Some(value.non_refundable_storage_fee),
            epoch_start_timestamp: Some(crate::proto::types::timestamp_ms_to_proto(
                value.epoch_start_timestamp_ms,
            )),
            system_packages: value.system_packages.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<&super::ChangeEpoch> for sui_sdk_types::ChangeEpoch {
    type Error = TryFromProtoError;

    fn try_from(
        super::ChangeEpoch {
            epoch,
            protocol_version,
            storage_charge,
            computation_charge,
            storage_rebate,
            non_refundable_storage_fee,
            epoch_start_timestamp,
            system_packages,
        }: &super::ChangeEpoch,
    ) -> Result<Self, Self::Error> {
        let epoch = epoch.ok_or_else(|| TryFromProtoError::missing("epoch"))?;
        let protocol_version =
            protocol_version.ok_or_else(|| TryFromProtoError::missing("protocol_version"))?;
        let storage_charge =
            storage_charge.ok_or_else(|| TryFromProtoError::missing("storage_charge"))?;
        let computation_charge =
            computation_charge.ok_or_else(|| TryFromProtoError::missing("computation_charge"))?;
        let storage_rebate =
            storage_rebate.ok_or_else(|| TryFromProtoError::missing("storage_rebate"))?;
        let non_refundable_storage_fee = non_refundable_storage_fee
            .ok_or_else(|| TryFromProtoError::missing("non_refundable_storage_fee"))?;
        let epoch_start_timestamp_ms = epoch_start_timestamp
            .ok_or_else(|| TryFromProtoError::missing("epoch_start_timestamp_ms"))?
            .pipe(crate::proto::types::proto_to_timestamp_ms)?;

        Ok(Self {
            epoch,
            protocol_version,
            storage_charge,
            computation_charge,
            storage_rebate,
            non_refundable_storage_fee,
            epoch_start_timestamp_ms,
            system_packages: system_packages
                .iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        })
    }
}

//
// SystemPackage
//

impl From<sui_sdk_types::SystemPackage> for super::SystemPackage {
    fn from(value: sui_sdk_types::SystemPackage) -> Self {
        Self {
            version: Some(value.version),
            modules: value.modules.into_iter().map(Into::into).collect(),
            dependencies: value.dependencies.iter().map(ToString::to_string).collect(),
        }
    }
}

impl TryFrom<&super::SystemPackage> for sui_sdk_types::SystemPackage {
    type Error = TryFromProtoError;

    fn try_from(value: &super::SystemPackage) -> Result<Self, Self::Error> {
        Ok(Self {
            version: value
                .version
                .ok_or_else(|| TryFromProtoError::missing("version"))?,
            modules: value.modules.iter().map(|bytes| bytes.to_vec()).collect(),
            dependencies: value
                .dependencies
                .iter()
                .map(|s| s.parse())
                .collect::<Result<_, _>>()
                .map_err(TryFromProtoError::from_error)?,
        })
    }
}

//
// EndOfEpochTransactionkind
//

impl From<sui_sdk_types::EndOfEpochTransactionKind> for super::EndOfEpochTransactionKind {
    fn from(value: sui_sdk_types::EndOfEpochTransactionKind) -> Self {
        use super::end_of_epoch_transaction_kind::Kind;
        use sui_sdk_types::EndOfEpochTransactionKind::*;

        let kind = match value {
            ChangeEpoch(change_epoch) => Kind::ChangeEpoch(change_epoch.into()),
            AuthenticatorStateCreate => Kind::AuthenticatorStateCreate(()),
            AuthenticatorStateExpire(expire) => Kind::AuthenticatorStateExpire(expire.into()),
            RandomnessStateCreate => Kind::RandomnessStateCreate(()),
            DenyListStateCreate => Kind::DenyListStateCreate(()),
            BridgeStateCreate { chain_id } => Kind::BridgeStateCreate(chain_id.to_string()),
            BridgeCommitteeInit {
                bridge_object_version,
            } => Kind::BridgeCommitteeInit(bridge_object_version),
            StoreExecutionTimeObservations(observations) => {
                Kind::ExecutionTimeObservations(observations.into())
            }
        };

        Self { kind: Some(kind) }
    }
}

impl TryFrom<&super::EndOfEpochTransactionKind> for sui_sdk_types::EndOfEpochTransactionKind {
    type Error = TryFromProtoError;

    fn try_from(value: &super::EndOfEpochTransactionKind) -> Result<Self, Self::Error> {
        use super::end_of_epoch_transaction_kind::Kind;

        match value
            .kind
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("kind"))?
        {
            Kind::ChangeEpoch(change_epoch) => Self::ChangeEpoch(change_epoch.try_into()?),
            Kind::AuthenticatorStateExpire(expire) => {
                Self::AuthenticatorStateExpire(expire.try_into()?)
            }
            Kind::AuthenticatorStateCreate(()) => Self::AuthenticatorStateCreate,
            Kind::RandomnessStateCreate(()) => Self::RandomnessStateCreate,
            Kind::DenyListStateCreate(()) => Self::DenyListStateCreate,
            Kind::BridgeStateCreate(digest) => Self::BridgeStateCreate {
                chain_id: digest.parse().map_err(TryFromProtoError::from_error)?,
            },
            Kind::BridgeCommitteeInit(version) => Self::BridgeCommitteeInit {
                bridge_object_version: *version,
            },
            Kind::ExecutionTimeObservations(execution_time_observations) => {
                Self::StoreExecutionTimeObservations(execution_time_observations.try_into()?)
            }
        }
        .pipe(Ok)
    }
}

//
// AuthenticatorStateExpire
//

impl From<sui_sdk_types::AuthenticatorStateExpire> for super::AuthenticatorStateExpire {
    fn from(value: sui_sdk_types::AuthenticatorStateExpire) -> Self {
        Self {
            min_epoch: Some(value.min_epoch),
            authenticator_object_initial_shared_version: Some(
                value.authenticator_object_initial_shared_version,
            ),
        }
    }
}

impl TryFrom<&super::AuthenticatorStateExpire> for sui_sdk_types::AuthenticatorStateExpire {
    type Error = TryFromProtoError;

    fn try_from(
        super::AuthenticatorStateExpire {
            min_epoch,
            authenticator_object_initial_shared_version,
        }: &super::AuthenticatorStateExpire,
    ) -> Result<Self, Self::Error> {
        let min_epoch = min_epoch.ok_or_else(|| TryFromProtoError::missing("min_epoch"))?;
        let authenticator_object_initial_shared_version =
            authenticator_object_initial_shared_version.ok_or_else(|| {
                TryFromProtoError::missing("authenticator_object_initial_shared_version")
            })?;
        Ok(Self {
            min_epoch,
            authenticator_object_initial_shared_version,
        })
    }
}

// ExecutionTimeObservations

impl From<sui_sdk_types::ExecutionTimeObservations> for super::ExecutionTimeObservations {
    fn from(value: sui_sdk_types::ExecutionTimeObservations) -> Self {
        match value {
            sui_sdk_types::ExecutionTimeObservations::V1(vec) => Self {
                version: Some(1),
                observations: vec.into_iter().map(Into::into).collect(),
            },
        }
    }
}

impl TryFrom<&super::ExecutionTimeObservations> for sui_sdk_types::ExecutionTimeObservations {
    type Error = TryFromProtoError;

    fn try_from(value: &super::ExecutionTimeObservations) -> Result<Self, Self::Error> {
        Ok(Self::V1(
            value
                .observations
                .iter()
                .map(|observation| observation.try_into())
                .collect::<Result<_, _>>()?,
        ))
    }
}

impl
    From<(
        sui_sdk_types::ExecutionTimeObservationKey,
        Vec<sui_sdk_types::ValidatorExecutionTimeObservation>,
    )> for super::ExecutionTimeObservation
{
    fn from(
        value: (
            sui_sdk_types::ExecutionTimeObservationKey,
            Vec<sui_sdk_types::ValidatorExecutionTimeObservation>,
        ),
    ) -> Self {
        use super::execution_time_observation::ExecutionTimeObservationKind;
        use sui_sdk_types::ExecutionTimeObservationKey;

        let mut message = Self::default();

        let kind = match value.0 {
            ExecutionTimeObservationKey::MoveEntryPoint {
                package,
                module,
                function,
                type_arguments,
            } => {
                message.move_entry_point = Some(super::MoveCall {
                    package: Some(package.to_string()),
                    module: Some(module),
                    function: Some(function),
                    type_arguments: type_arguments
                        .into_iter()
                        .map(|ty| ty.to_string())
                        .collect(),
                    arguments: Vec::new(),
                });
                ExecutionTimeObservationKind::MoveEntryPoint
            }
            ExecutionTimeObservationKey::TransferObjects => {
                ExecutionTimeObservationKind::TransferObjects
            }
            ExecutionTimeObservationKey::SplitCoins => ExecutionTimeObservationKind::SplitCoins,
            ExecutionTimeObservationKey::MergeCoins => ExecutionTimeObservationKind::MergeCoins,
            ExecutionTimeObservationKey::Publish => ExecutionTimeObservationKind::Publish,
            ExecutionTimeObservationKey::MakeMoveVec => {
                ExecutionTimeObservationKind::MakeMoveVector
            }
            ExecutionTimeObservationKey::Upgrade => ExecutionTimeObservationKind::Upgrade,
        };

        message.validator_observations = value.1.into_iter().map(Into::into).collect();
        message.set_kind(kind);
        message
    }
}

impl TryFrom<&super::ExecutionTimeObservation>
    for (
        sui_sdk_types::ExecutionTimeObservationKey,
        Vec<sui_sdk_types::ValidatorExecutionTimeObservation>,
    )
{
    type Error = TryFromProtoError;

    fn try_from(value: &super::ExecutionTimeObservation) -> Result<Self, Self::Error> {
        use super::execution_time_observation::ExecutionTimeObservationKind;
        use sui_sdk_types::ExecutionTimeObservationKey;

        let key = match value.kind() {
            ExecutionTimeObservationKind::Unknown => {
                return Err(TryFromProtoError::from_error(
                    "unknown ExecutionTimeObservationKind",
                ))
            }
            ExecutionTimeObservationKind::MoveEntryPoint => {
                let move_call = value
                    .move_entry_point
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("move_entry_point"))?
                    .pipe(sui_sdk_types::MoveCall::try_from)?;
                ExecutionTimeObservationKey::MoveEntryPoint {
                    package: move_call.package,
                    module: move_call.module.to_string(),
                    function: move_call.function.to_string(),
                    type_arguments: move_call.type_arguments,
                }
            }
            ExecutionTimeObservationKind::TransferObjects => {
                ExecutionTimeObservationKey::TransferObjects
            }
            ExecutionTimeObservationKind::SplitCoins => ExecutionTimeObservationKey::SplitCoins,
            ExecutionTimeObservationKind::MergeCoins => ExecutionTimeObservationKey::MergeCoins,
            ExecutionTimeObservationKind::Publish => ExecutionTimeObservationKey::Publish,
            ExecutionTimeObservationKind::MakeMoveVector => {
                ExecutionTimeObservationKey::MakeMoveVec
            }
            ExecutionTimeObservationKind::Upgrade => ExecutionTimeObservationKey::Upgrade,
        };

        let observations = value
            .validator_observations
            .iter()
            .map(sui_sdk_types::ValidatorExecutionTimeObservation::try_from)
            .collect::<Result<_, _>>()?;

        Ok((key, observations))
    }
}

// ValidatorExecutionTimeObservation

impl From<sui_sdk_types::ValidatorExecutionTimeObservation>
    for super::ValidatorExecutionTimeObservation
{
    fn from(value: sui_sdk_types::ValidatorExecutionTimeObservation) -> Self {
        Self {
            validator: Some(value.validator.as_bytes().to_vec().into()),
            duration: Some(value.duration.try_into().unwrap()),
        }
    }
}

impl TryFrom<&super::ValidatorExecutionTimeObservation>
    for sui_sdk_types::ValidatorExecutionTimeObservation
{
    type Error = TryFromProtoError;

    fn try_from(value: &super::ValidatorExecutionTimeObservation) -> Result<Self, Self::Error> {
        Ok(Self {
            validator: value
                .validator
                .as_ref()
                .ok_or_else(|| TryFromProtoError::missing("validator"))?
                .as_ref()
                .pipe(sui_sdk_types::Bls12381PublicKey::from_bytes)
                .map_err(TryFromProtoError::from_error)?,
            duration: value
                .duration
                .ok_or_else(|| TryFromProtoError::missing("duration"))?
                .try_into()
                .map_err(TryFromProtoError::from_error)?,
        })
    }
}

//
// ProgrammableTransaction
//

impl From<sui_sdk_types::ProgrammableTransaction> for super::ProgrammableTransaction {
    fn from(value: sui_sdk_types::ProgrammableTransaction) -> Self {
        Self {
            inputs: value.inputs.into_iter().map(Into::into).collect(),
            commands: value.commands.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<&super::ProgrammableTransaction> for sui_sdk_types::ProgrammableTransaction {
    type Error = TryFromProtoError;

    fn try_from(value: &super::ProgrammableTransaction) -> Result<Self, Self::Error> {
        Ok(Self {
            inputs: value
                .inputs
                .iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            commands: value
                .commands
                .iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        })
    }
}

//
// Input
//

impl From<sui_sdk_types::Input> for super::Input {
    fn from(value: sui_sdk_types::Input) -> Self {
        use super::input::InputKind;
        use sui_sdk_types::Input::*;

        let mut message = Self::default();

        let kind = match value {
            Pure { value } => {
                message.pure = Some(value.into());
                InputKind::Pure
            }

            ImmutableOrOwned(reference) => {
                message.object_id = Some(reference.object_id().to_string());
                message.version = Some(reference.version());
                message.digest = Some(reference.digest().to_string());
                InputKind::ImmutableOrOwned
            }

            Shared {
                object_id,
                initial_shared_version,
                mutable,
            } => {
                message.object_id = Some(object_id.to_string());
                message.version = Some(initial_shared_version);
                message.mutable = Some(mutable);
                InputKind::Shared
            }

            Receiving(reference) => {
                message.object_id = Some(reference.object_id().to_string());
                message.version = Some(reference.version());
                message.digest = Some(reference.digest().to_string());
                InputKind::Receiving
            }
        };

        message.set_kind(kind);
        message
    }
}

impl TryFrom<&super::Input> for sui_sdk_types::Input {
    type Error = TryFromProtoError;

    fn try_from(value: &super::Input) -> Result<Self, Self::Error> {
        use super::input::InputKind;

        match value.kind() {
            InputKind::Unknown => return Err(TryFromProtoError::from_error("unknown InputKind")),

            InputKind::Pure => Self::Pure {
                value: value
                    .pure
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("pure"))?
                    .to_vec(),
            },
            InputKind::ImmutableOrOwned => {
                let object_id = value
                    .object_id
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("object_id"))?
                    .parse()
                    .map_err(TryFromProtoError::from_error)?;
                let version = value
                    .version
                    .ok_or_else(|| TryFromProtoError::missing("version"))?;
                let digest = value
                    .digest
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("digest"))?
                    .parse()
                    .map_err(TryFromProtoError::from_error)?;
                let reference = sui_sdk_types::ObjectReference::new(object_id, version, digest);
                Self::ImmutableOrOwned(reference)
            }
            InputKind::Shared => {
                let object_id = value
                    .object_id
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("object_id"))?
                    .parse()
                    .map_err(TryFromProtoError::from_error)?;
                let initial_shared_version = value
                    .version
                    .ok_or_else(|| TryFromProtoError::missing("version"))?;
                let mutable = value
                    .mutable
                    .ok_or_else(|| TryFromProtoError::missing("mutable"))?;
                Self::Shared {
                    object_id,
                    initial_shared_version,
                    mutable,
                }
            }
            InputKind::Receiving => {
                let object_id = value
                    .object_id
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("object_id"))?
                    .parse()
                    .map_err(TryFromProtoError::from_error)?;
                let version = value
                    .version
                    .ok_or_else(|| TryFromProtoError::missing("version"))?;
                let digest = value
                    .digest
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("digest"))?
                    .parse()
                    .map_err(TryFromProtoError::from_error)?;
                let reference = sui_sdk_types::ObjectReference::new(object_id, version, digest);
                Self::Receiving(reference)
            }
        }
        .pipe(Ok)
    }
}

//
// Argument
//

impl From<sui_sdk_types::Argument> for super::Argument {
    fn from(value: sui_sdk_types::Argument) -> Self {
        use super::argument::ArgumentKind;
        use sui_sdk_types::Argument::*;

        let mut message = Self::default();

        let kind = match value {
            Gas => ArgumentKind::Gas,
            Input(input) => {
                message.index = Some(input.into());
                ArgumentKind::Input
            }
            Result(result) => {
                message.index = Some(result.into());
                ArgumentKind::Result
            }
            NestedResult(result, subresult) => {
                message.index = Some(result.into());
                message.subresult = Some(subresult.into());
                ArgumentKind::Result
            }
        };

        message.set_kind(kind);
        message
    }
}

impl TryFrom<&super::Argument> for sui_sdk_types::Argument {
    type Error = TryFromProtoError;

    fn try_from(value: &super::Argument) -> Result<Self, Self::Error> {
        use super::argument::ArgumentKind;

        match value.kind() {
            ArgumentKind::Unknown => {
                return Err(TryFromProtoError::from_error("unknown ArgumentKind"))
            }
            ArgumentKind::Gas => Self::Gas,
            ArgumentKind::Input => {
                let input = value
                    .index
                    .ok_or_else(|| TryFromProtoError::missing("index"))?
                    .try_into()?;
                Self::Input(input)
            }
            ArgumentKind::Result => {
                let result = value
                    .index
                    .ok_or_else(|| TryFromProtoError::missing("index"))?
                    .try_into()?;

                if let Some(subresult) = value.subresult {
                    Self::NestedResult(result, subresult.try_into()?)
                } else {
                    Self::Result(result)
                }
            }
        }
        .pipe(Ok)
    }
}

//
// Command
//

impl From<sui_sdk_types::Command> for super::Command {
    fn from(value: sui_sdk_types::Command) -> Self {
        use super::command::Command;
        use sui_sdk_types::Command::*;

        let command = match value {
            MoveCall(move_call) => Command::MoveCall(move_call.into()),
            TransferObjects(transfer_objects) => Command::TransferObjects(transfer_objects.into()),
            SplitCoins(split_coins) => Command::SplitCoins(split_coins.into()),
            MergeCoins(merge_coins) => Command::MergeCoins(merge_coins.into()),
            Publish(publish) => Command::Publish(publish.into()),
            MakeMoveVector(make_move_vector) => Command::MakeMoveVector(make_move_vector.into()),
            Upgrade(upgrade) => Command::Upgrade(upgrade.into()),
        };

        Self {
            command: Some(command),
        }
    }
}

impl TryFrom<&super::Command> for sui_sdk_types::Command {
    type Error = TryFromProtoError;

    fn try_from(value: &super::Command) -> Result<Self, Self::Error> {
        use super::command::Command;

        match value
            .command
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("command"))?
        {
            Command::MoveCall(move_call) => Self::MoveCall(move_call.try_into()?),
            Command::TransferObjects(transfer_objects) => {
                Self::TransferObjects(transfer_objects.try_into()?)
            }
            Command::SplitCoins(split_coins) => Self::SplitCoins(split_coins.try_into()?),
            Command::MergeCoins(merge_coins) => Self::MergeCoins(merge_coins.try_into()?),
            Command::Publish(publish) => Self::Publish(publish.try_into()?),
            Command::MakeMoveVector(make_move_vector) => {
                Self::MakeMoveVector(make_move_vector.try_into()?)
            }
            Command::Upgrade(upgrade) => Self::Upgrade(upgrade.try_into()?),
        }
        .pipe(Ok)
    }
}

//
// MoveCall
//

impl From<sui_sdk_types::MoveCall> for super::MoveCall {
    fn from(value: sui_sdk_types::MoveCall) -> Self {
        Self {
            package: Some(value.package.to_string()),
            module: Some(value.module.to_string()),
            function: Some(value.function.to_string()),
            type_arguments: value
                .type_arguments
                .iter()
                .map(ToString::to_string)
                .collect(),
            arguments: value.arguments.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<&super::MoveCall> for sui_sdk_types::MoveCall {
    type Error = TryFromProtoError;

    fn try_from(value: &super::MoveCall) -> Result<Self, Self::Error> {
        let package = value
            .package
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("package"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        let module = value
            .module
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("module"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        let function = value
            .function
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("function"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        let type_arguments = value
            .type_arguments
            .iter()
            .map(|t| t.parse().map_err(TryFromProtoError::from_error))
            .collect::<Result<_, _>>()?;
        let arguments = value
            .arguments
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        Ok(Self {
            package,
            module,
            function,
            type_arguments,
            arguments,
        })
    }
}

//
// TransferObjects
//

impl From<sui_sdk_types::TransferObjects> for super::TransferObjects {
    fn from(value: sui_sdk_types::TransferObjects) -> Self {
        Self {
            objects: value.objects.into_iter().map(Into::into).collect(),
            address: Some(value.address.into()),
        }
    }
}

impl TryFrom<&super::TransferObjects> for sui_sdk_types::TransferObjects {
    type Error = TryFromProtoError;

    fn try_from(value: &super::TransferObjects) -> Result<Self, Self::Error> {
        let objects = value
            .objects
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        let address = value
            .address
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("address"))?
            .try_into()?;

        Ok(Self { objects, address })
    }
}

//
// SplitCoins
//

impl From<sui_sdk_types::SplitCoins> for super::SplitCoins {
    fn from(value: sui_sdk_types::SplitCoins) -> Self {
        Self {
            coin: Some(value.coin.into()),
            amounts: value.amounts.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<&super::SplitCoins> for sui_sdk_types::SplitCoins {
    type Error = TryFromProtoError;

    fn try_from(value: &super::SplitCoins) -> Result<Self, Self::Error> {
        let coin = value
            .coin
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("coin"))?
            .try_into()?;

        let amounts = value
            .amounts
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        Ok(Self { coin, amounts })
    }
}

//
// MergeCoins
//

impl From<sui_sdk_types::MergeCoins> for super::MergeCoins {
    fn from(value: sui_sdk_types::MergeCoins) -> Self {
        Self {
            coin: Some(value.coin.into()),
            coins_to_merge: value.coins_to_merge.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<&super::MergeCoins> for sui_sdk_types::MergeCoins {
    type Error = TryFromProtoError;

    fn try_from(value: &super::MergeCoins) -> Result<Self, Self::Error> {
        let coin = value
            .coin
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("coin"))?
            .try_into()?;

        let coins_to_merge = value
            .coins_to_merge
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        Ok(Self {
            coin,
            coins_to_merge,
        })
    }
}

//
// Publish
//

impl From<sui_sdk_types::Publish> for super::Publish {
    fn from(value: sui_sdk_types::Publish) -> Self {
        Self {
            modules: value.modules.into_iter().map(Into::into).collect(),
            dependencies: value.dependencies.iter().map(ToString::to_string).collect(),
        }
    }
}

impl TryFrom<&super::Publish> for sui_sdk_types::Publish {
    type Error = TryFromProtoError;

    fn try_from(value: &super::Publish) -> Result<Self, Self::Error> {
        let modules = value.modules.iter().map(|bytes| bytes.to_vec()).collect();

        let dependencies = value
            .dependencies
            .iter()
            .map(|s| s.parse())
            .collect::<Result<_, _>>()
            .map_err(TryFromProtoError::from_error)?;

        Ok(Self {
            modules,
            dependencies,
        })
    }
}

//
// MakeMoveVector
//

impl From<sui_sdk_types::MakeMoveVector> for super::MakeMoveVector {
    fn from(value: sui_sdk_types::MakeMoveVector) -> Self {
        Self {
            element_type: value.type_.map(|t| t.to_string()),
            elements: value.elements.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<&super::MakeMoveVector> for sui_sdk_types::MakeMoveVector {
    type Error = TryFromProtoError;

    fn try_from(value: &super::MakeMoveVector) -> Result<Self, Self::Error> {
        let element_type = value
            .element_type
            .as_ref()
            .map(|t| t.parse().map_err(TryFromProtoError::from_error))
            .transpose()?;

        let elements = value
            .elements
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        Ok(Self {
            type_: element_type,
            elements,
        })
    }
}

//
// Upgrade
//

impl From<sui_sdk_types::Upgrade> for super::Upgrade {
    fn from(value: sui_sdk_types::Upgrade) -> Self {
        Self {
            modules: value.modules.into_iter().map(Into::into).collect(),
            dependencies: value.dependencies.iter().map(ToString::to_string).collect(),
            package: Some(value.package.to_string()),
            ticket: Some(value.ticket.into()),
        }
    }
}

impl TryFrom<&super::Upgrade> for sui_sdk_types::Upgrade {
    type Error = TryFromProtoError;

    fn try_from(value: &super::Upgrade) -> Result<Self, Self::Error> {
        let modules = value.modules.iter().map(|bytes| bytes.to_vec()).collect();

        let dependencies = value
            .dependencies
            .iter()
            .map(|s| s.parse())
            .collect::<Result<_, _>>()
            .map_err(TryFromProtoError::from_error)?;

        let package = value
            .package
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("package"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        let ticket = value
            .ticket
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("ticket"))?
            .try_into()?;

        Ok(Self {
            modules,
            dependencies,
            package,
            ticket,
        })
    }
}

impl super::GetTransactionRequest {
    pub const READ_MASK_DEFAULT: &str = "digest";
}

impl super::BatchGetTransactionsRequest {
    pub const READ_MASK_DEFAULT: &str = super::GetTransactionRequest::READ_MASK_DEFAULT;
}
