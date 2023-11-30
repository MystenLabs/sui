// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{big_int::BigInt, date_time::DateTime, epoch::Epoch, sui_address::SuiAddress};
use crate::context_data::db_data_provider::PgManager;
use async_graphql::*;

#[derive(Union, PartialEq, Clone, Eq)]
pub(crate) enum TransactionBlockKind {
    ConsensusCommitPrologueTransaction(ConsensusCommitPrologueTransaction),
    GenesisTransaction(GenesisTransaction),
    ChangeEpochTransaction(ChangeEpochTransaction),
    ProgrammableTransactionBlock(ProgrammableTransaction),
    AuthenticatorStateUpdateTransaction(AuthenticatorStateUpdate),
    RandomnessStateUpdateTransaction(RandomnessStateUpdate),
    EndOfEpochTransaction(EndOfEpochTransaction),
}

// TODO: flesh out the programmable transaction block type
#[derive(SimpleObject, Clone, Eq, PartialEq)]
pub(crate) struct ProgrammableTransaction {
    pub value: String,
}

// TODO: flesh out the authenticator state update type
#[derive(SimpleObject, Clone, Eq, PartialEq)]
pub(crate) struct AuthenticatorStateUpdate {
    pub value: String,
}

// TODO: flesh out the randomness state update type
#[derive(SimpleObject, Clone, Eq, PartialEq)]
pub(crate) struct RandomnessStateUpdate {
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
