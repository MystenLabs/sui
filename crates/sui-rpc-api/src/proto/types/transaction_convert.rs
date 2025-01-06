use super::TryFromProtoError;
use tap::Pipe;

//
// Transaction
//

impl From<sui_sdk_types::Transaction> for super::Transaction {
    fn from(value: sui_sdk_types::Transaction) -> Self {
        let version = super::transaction::Version::V1(value.into());

        Self {
            version: Some(version),
        }
    }
}

impl TryFrom<&super::Transaction> for sui_sdk_types::Transaction {
    type Error = TryFromProtoError;

    fn try_from(value: &super::Transaction) -> Result<Self, Self::Error> {
        match value
            .version
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("version"))?
        {
            super::transaction::Version::V1(v1) => Self::try_from(v1)?,
        }
        .pipe(Ok)
    }
}

//
// TransactionV1
//

impl From<sui_sdk_types::Transaction> for super::transaction::TransactionV1 {
    fn from(value: sui_sdk_types::Transaction) -> Self {
        Self {
            kind: Some(value.kind.into()),
            sender: Some(value.sender.into()),
            gas_payment: Some(value.gas_payment.into()),
            expiration: Some(value.expiration.into()),
        }
    }
}

impl TryFrom<&super::transaction::TransactionV1> for sui_sdk_types::Transaction {
    type Error = TryFromProtoError;

    fn try_from(value: &super::transaction::TransactionV1) -> Result<Self, Self::Error> {
        let kind = value
            .kind
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("kind"))?
            .try_into()?;

        let sender = value
            .sender
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("sender"))?
            .try_into()?;

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
            owner: Some(value.owner.into()),
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
            .try_into()?;
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
        use super::transaction_expiration::Expiration;
        use sui_sdk_types::TransactionExpiration::*;

        let expiration = match value {
            None => Expiration::None(()),
            Epoch(epoch) => Expiration::Epoch(epoch),
        };

        Self {
            expiration: Some(expiration),
        }
    }
}

impl TryFrom<&super::TransactionExpiration> for sui_sdk_types::TransactionExpiration {
    type Error = TryFromProtoError;

    fn try_from(value: &super::TransactionExpiration) -> Result<Self, Self::Error> {
        use super::transaction_expiration::Expiration;

        match value
            .expiration
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("expiration"))?
        {
            Expiration::None(()) => Self::None,
            Expiration::Epoch(epoch) => Self::Epoch(*epoch),
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
            commit_timestamp_ms: Some(value.commit_timestamp_ms),
            consensus_commit_digest: None,
            sub_dag_index: None,
            consensus_determined_version_assignments: None,
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
            .commit_timestamp_ms
            .ok_or_else(|| TryFromProtoError::missing("commit_timestamp_ms"))?;

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
            commit_timestamp_ms: Some(value.commit_timestamp_ms),
            consensus_commit_digest: Some(value.consensus_commit_digest.into()),
            sub_dag_index: None,
            consensus_determined_version_assignments: None,
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
            .commit_timestamp_ms
            .ok_or_else(|| TryFromProtoError::missing("commit_timestamp_ms"))?;

        let consensus_commit_digest = value
            .consensus_commit_digest
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("consensus_commit_digest"))?
            .try_into()?;

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
            commit_timestamp_ms: Some(value.commit_timestamp_ms),
            consensus_commit_digest: Some(value.consensus_commit_digest.into()),
            sub_dag_index: value.sub_dag_index,
            consensus_determined_version_assignments: Some(
                value.consensus_determined_version_assignments.into(),
            ),
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
            .commit_timestamp_ms
            .ok_or_else(|| TryFromProtoError::missing("commit_timestamp_ms"))?;

        let consensus_commit_digest = value
            .consensus_commit_digest
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("consensus_commit_digest"))?
            .try_into()?;

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
            CancelledTransactions {
                cancelled_transactions,
            } => Kind::CancelledTransactions(super::CancelledTransactions {
                cancelled_transactions: cancelled_transactions
                    .into_iter()
                    .map(Into::into)
                    .collect(),
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
            Kind::CancelledTransactions(super::CancelledTransactions {
                cancelled_transactions,
            }) => Self::CancelledTransactions {
                cancelled_transactions: cancelled_transactions
                    .iter()
                    .map(TryInto::try_into)
                    .collect::<Result<_, _>>()?,
            },
        }
        .pipe(Ok)
    }
}

//
// CancelledTransaction
//

impl From<sui_sdk_types::CancelledTransaction> for super::CancelledTransaction {
    fn from(value: sui_sdk_types::CancelledTransaction) -> Self {
        Self {
            digest: Some(value.digest.into()),
            version_assignments: value
                .version_assignments
                .into_iter()
                .map(Into::into)
                .collect(),
        }
    }
}

impl TryFrom<&super::CancelledTransaction> for sui_sdk_types::CancelledTransaction {
    type Error = TryFromProtoError;

    fn try_from(value: &super::CancelledTransaction) -> Result<Self, Self::Error> {
        let digest = value
            .digest
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("digest"))?
            .try_into()?;

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
            object_id: Some(value.object_id.into()),
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
            .try_into()?;
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
            epoch_start_timestamp_ms: Some(value.epoch_start_timestamp_ms),
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
            epoch_start_timestamp_ms,
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
        let epoch_start_timestamp_ms = epoch_start_timestamp_ms
            .ok_or_else(|| TryFromProtoError::missing("epoch_start_timestamp_ms"))?;

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
            dependencies: value.dependencies.into_iter().map(Into::into).collect(),
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
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
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
            BridgeStateCreate { chain_id } => Kind::BridgeStateCreate(chain_id.into()),
            BridgeCommitteeInit {
                bridge_object_version,
            } => Kind::BridgeCommitteeInit(bridge_object_version),
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
                chain_id: digest.try_into()?,
            },
            Kind::BridgeCommitteeInit(version) => Self::BridgeCommitteeInit {
                bridge_object_version: *version,
            },
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
        use super::input::Kind;
        use sui_sdk_types::Input::*;

        let kind = match value {
            Pure { value } => Kind::Pure(value.into()),
            ImmutableOrOwned(reference) => Kind::ImmutableOrOwned(reference.into()),
            Shared {
                object_id,
                initial_shared_version,
                mutable,
            } => Kind::Shared(super::SharedObjectInput {
                object_id: Some(object_id.into()),
                initial_shared_version: Some(initial_shared_version),
                mutable: Some(mutable),
            }),
            Receiving(reference) => Kind::Receiving(reference.into()),
        };

        Self { kind: Some(kind) }
    }
}

impl TryFrom<&super::Input> for sui_sdk_types::Input {
    type Error = TryFromProtoError;

    fn try_from(value: &super::Input) -> Result<Self, Self::Error> {
        use super::input::Kind;

        match value
            .kind
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("kind"))?
        {
            Kind::Pure(value) => Self::Pure {
                value: value.to_vec(),
            },
            Kind::ImmutableOrOwned(reference) => Self::ImmutableOrOwned(reference.try_into()?),
            Kind::Shared(shared) => {
                let object_id = shared
                    .object_id
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("object_id"))?
                    .try_into()?;
                Self::Shared {
                    object_id,
                    initial_shared_version: shared
                        .initial_shared_version
                        .ok_or_else(|| TryFromProtoError::missing("initial_shared_version"))?,
                    mutable: shared
                        .mutable
                        .ok_or_else(|| TryFromProtoError::missing("mutable"))?,
                }
            }
            Kind::Receiving(reference) => Self::Receiving(reference.try_into()?),
        }
        .pipe(Ok)
    }
}

//
// Argument
//

impl From<sui_sdk_types::Argument> for super::Argument {
    fn from(value: sui_sdk_types::Argument) -> Self {
        use super::argument::Kind;
        use sui_sdk_types::Argument::*;

        let kind = match value {
            Gas => Kind::Gas(()),
            Input(input) => Kind::Input(input.into()),
            Result(result) => Kind::Result(result.into()),
            NestedResult(result, subresult) => Kind::NestedResult(super::NestedResult {
                result: Some(result.into()),
                subresult: Some(subresult.into()),
            }),
        };

        Self { kind: Some(kind) }
    }
}

impl TryFrom<&super::Argument> for sui_sdk_types::Argument {
    type Error = TryFromProtoError;

    fn try_from(value: &super::Argument) -> Result<Self, Self::Error> {
        use super::argument::Kind;

        match value
            .kind
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("kind"))?
        {
            Kind::Gas(()) => Self::Gas,
            Kind::Input(input) => Self::Input((*input).try_into()?),
            Kind::Result(result) => Self::Result((*result).try_into()?),
            Kind::NestedResult(super::NestedResult { result, subresult }) => Self::NestedResult(
                result
                    .ok_or_else(|| TryFromProtoError::missing("result"))?
                    .try_into()?,
                subresult
                    .ok_or_else(|| TryFromProtoError::missing("subresult"))?
                    .try_into()?,
            ),
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
            package: Some(value.package.into()),
            module: Some(value.module.into()),
            function: Some(value.function.into()),
            type_arguments: value.type_arguments.into_iter().map(Into::into).collect(),
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
            .try_into()?;

        let module = value
            .module
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("module"))?
            .try_into()?;

        let function = value
            .function
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("function"))?
            .try_into()?;

        let type_arguments = value
            .type_arguments
            .iter()
            .map(TryInto::try_into)
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
            dependencies: value.dependencies.into_iter().map(Into::into).collect(),
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
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

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
            element_type: value.type_.map(Into::into),
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
            .map(TryInto::try_into)
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
            dependencies: value.dependencies.into_iter().map(Into::into).collect(),
            package: Some(value.package.into()),
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
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        let package = value
            .package
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("package"))?
            .try_into()?;

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
