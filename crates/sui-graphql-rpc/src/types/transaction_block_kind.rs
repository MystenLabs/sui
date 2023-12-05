// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{big_int::BigInt, date_time::DateTime, epoch::Epoch, sui_address::SuiAddress};
use crate::context_data::db_data_provider::PgManager;
use async_graphql::*;
use sui_types::transaction::{GenesisObject, TransactionKind as NativeTransactionKind};

#[derive(Union, PartialEq, Clone, Eq)]
pub(crate) enum TransactionBlockKind {
    ConsensusCommitPrologue(ConsensusCommitPrologueTransaction),
    Genesis(GenesisTransaction),
    ChangeEpoch(ChangeEpochTransaction),
    Programmable(ProgrammableTransactionBlock),
    AuthenticatorState(AuthenticatorStateUpdateTransaction),
    Randomness(RandomnessStateUpdateTransaction),
    EndOfEpoch(EndOfEpochTransaction),
}

// TODO: flesh out the programmable transaction block type
#[derive(SimpleObject, Clone, Eq, PartialEq)]
pub(crate) struct ProgrammableTransactionBlock {
    pub value: String,
}

// TODO: flesh out the authenticator state update type
#[derive(SimpleObject, Clone, Eq, PartialEq)]
pub(crate) struct AuthenticatorStateUpdateTransaction {
    pub value: String,
}

// TODO: flesh out the randomness state update type
#[derive(SimpleObject, Clone, Eq, PartialEq)]
pub(crate) struct RandomnessStateUpdateTransaction {
    pub value: String,
}

// TODO: flesh out the end of epoch transaction type
#[derive(SimpleObject, Clone, Eq, PartialEq)]
pub(crate) struct EndOfEpochTransaction {
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct TxBlockKindNotImplementedYet {
    pub(crate) text: String,
}

// TODO: add ConsensusCommitPrologueTransactionV2 for TransactionKind::ConsensusCommitPrologueV2.
#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
#[graphql(complex)]
pub(crate) struct ConsensusCommitPrologueTransaction {
    #[graphql(skip)]
    pub(crate) epoch_id: u64,
    pub(crate) round: Option<u64>,
    pub(crate) timestamp: Option<DateTime>,
}

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct GenesisTransaction {
    pub(crate) objects: Option<Vec<SuiAddress>>,
}

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
#[graphql(complex)]
pub(crate) struct ChangeEpochTransaction {
    #[graphql(skip)]
    pub(crate) epoch_id: u64,
    pub(crate) timestamp: Option<DateTime>,
    pub(crate) storage_charge: Option<BigInt>,
    pub(crate) computation_charge: Option<BigInt>,
    pub(crate) storage_rebate: Option<BigInt>,
}

#[ComplexObject]
impl ConsensusCommitPrologueTransaction {
    async fn epoch(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        let epoch = ctx
            .data_unchecked::<PgManager>()
            .fetch_epoch_strict(self.epoch_id)
            .await
            .extend()?;

        Ok(Some(epoch))
    }
}

#[ComplexObject]
impl ChangeEpochTransaction {
    async fn epoch(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        let epoch = ctx
            .data_unchecked::<PgManager>()
            .fetch_epoch_strict(self.epoch_id)
            .await
            .extend()?;

        Ok(Some(epoch))
    }
}

impl From<NativeTransactionKind> for TransactionBlockKind {
    fn from(kind: NativeTransactionKind) -> Self {
        use NativeTransactionKind as K;
        use TransactionBlockKind as T;

        match kind {
            // TODO: flesh out type
            K::ProgrammableTransaction(pt) => T::Programmable(ProgrammableTransactionBlock {
                value: format!("{pt:?}"),
            }),

            K::ChangeEpoch(ce) => T::ChangeEpoch(ChangeEpochTransaction {
                epoch_id: ce.epoch,
                timestamp: DateTime::from_ms(ce.epoch_start_timestamp_ms as i64),
                storage_charge: Some(BigInt::from(ce.storage_charge)),
                computation_charge: Some(BigInt::from(ce.computation_charge)),
                storage_rebate: Some(BigInt::from(ce.storage_rebate)),
            }),

            K::Genesis(g) => T::Genesis(GenesisTransaction {
                objects: Some(g.objects.iter().cloned().map(SuiAddress::from).collect()),
            }),

            K::ConsensusCommitPrologue(ccp) => {
                T::ConsensusCommitPrologue(ConsensusCommitPrologueTransaction {
                    epoch_id: ccp.epoch,
                    round: Some(ccp.round),
                    timestamp: DateTime::from_ms(ccp.commit_timestamp_ms as i64),
                })
            }

            K::ConsensusCommitPrologueV2(ccp) => {
                T::ConsensusCommitPrologue(ConsensusCommitPrologueTransaction {
                    epoch_id: ccp.epoch,
                    round: Some(ccp.round),
                    timestamp: DateTime::from_ms(ccp.commit_timestamp_ms as i64),
                })
            }

            // TODO: flesh out type
            K::AuthenticatorStateUpdate(asu) => {
                T::AuthenticatorState(AuthenticatorStateUpdateTransaction {
                    value: format!("{asu:?}"),
                })
            }

            // TODO: flesh out type
            K::EndOfEpochTransaction(eoe) => T::EndOfEpoch(EndOfEpochTransaction {
                value: format!("{eoe:?}"),
            }),

            // TODO: flesh out type
            K::RandomnessStateUpdate(rsu) => T::Randomness(RandomnessStateUpdateTransaction {
                value: format!("{rsu:?}"),
            }),
        }
    }
}

// TODO fix this GenesisObject
impl From<GenesisObject> for SuiAddress {
    fn from(value: GenesisObject) -> Self {
        match value {
            GenesisObject::RawObject { data, owner: _ } => {
                SuiAddress::from_bytes(data.id().to_vec()).unwrap()
            }
        }
    }
}
