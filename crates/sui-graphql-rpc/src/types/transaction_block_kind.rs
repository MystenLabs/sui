// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{
    big_int::BigInt, date_time::DateTime, epoch::Epoch, object::Object, sui_address::SuiAddress,
};
use crate::context_data::db_data_provider::PgManager;
use async_graphql::{ComplexObject, Context, Result, ResultExt, SimpleObject, Union};
use sui_sdk::types::transaction::{GenesisObject, TransactionKind};

#[derive(Union, PartialEq, Clone, Eq)]
pub enum TransactionBlockKind {
    ConsensusCommitPrologueTransaction(ConsensusCommitPrologueTransaction),
    GenesisTransaction(GenesisTransaction),
    ChangeEpochTransaction(ChangeEpochTransaction),
    // ProgrammableTransactionBlock(ProgrammableTransactionBlock),
}

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
#[graphql(complex)]
pub struct ConsensusCommitPrologueTransaction {
    #[graphql(skip)]
    epoch_id: u64,
    // # TODO: This is the "leader round" -- does this line up with
    // # checkpoints? In which case, it may suffice to have a `Checkpoint`
    // # here.
    round: Option<u64>,
    timestamp: Option<DateTime>,
}

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub struct GenesisTransaction {
    objects: Option<Vec<Object>>,
}

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
#[graphql(complex)]
pub struct ChangeEpochTransaction {
    #[graphql(skip)]
    epoch_id: u64,
    timestamp: Option<DateTime>,
    storage_charge: Option<BigInt>,
    computation_charge: Option<BigInt>,
    storage_rebate: Option<BigInt>,
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

impl From<&TransactionKind> for TransactionBlockKind {
    fn from(value: &TransactionKind) -> Self {
        match value {
            TransactionKind::ConsensusCommitPrologue(x) => {
                let consensus = ConsensusCommitPrologueTransaction {
                    epoch_id: x.epoch,
                    round: Some(x.round),
                    timestamp: DateTime::from_ms(x.commit_timestamp_ms as i64),
                };
                TransactionBlockKind::ConsensusCommitPrologueTransaction(consensus)
            }
            TransactionKind::ChangeEpoch(x) => {
                let change = ChangeEpochTransaction {
                    epoch_id: x.epoch,
                    timestamp: DateTime::from_ms(x.epoch_start_timestamp_ms as i64),
                    storage_charge: Some(BigInt::from(x.storage_charge)),
                    computation_charge: Some(BigInt::from(x.computation_charge)),
                    storage_rebate: Some(BigInt::from(x.storage_rebate)),
                };
                TransactionBlockKind::ChangeEpochTransaction(change)
            }
            TransactionKind::Genesis(x) => {
                let genesis = GenesisTransaction {
                    objects: Some(
                        x.objects
                            .clone()
                            .into_iter()
                            .map(Object::from)
                            .collect::<Vec<_>>(),
                    ),
                };
                TransactionBlockKind::GenesisTransaction(genesis)
            }
            _ => todo!(),
        }
    }
}

impl From<GenesisObject> for Object {
    fn from(value: GenesisObject) -> Self {
        match value {
            GenesisObject::RawObject { data, owner } => Self {
                address: todo!(),
                version: todo!(),
                digest: todo!(),
                storage_rebate: todo!(),
                owner: Some(
                    SuiAddress::from_bytes(owner.get_owner_address().unwrap().to_vec()).unwrap(),
                ),
                bcs: todo!(),
                previous_transaction: todo!(),
                kind: todo!(),
            },
        }
    }
}
