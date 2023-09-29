// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Result as GqlResult;
use async_trait::async_trait;
use diesel::{ExpressionMethods, OptionalExtension, PgConnection, QueryDsl, RunQueryDsl};
use sui_indexer::{
    indexer_reader::IndexerReader,
    models_v2::{
        checkpoints::StoredCheckpoint, epoch::StoredEpochInfo, transactions::StoredTransaction,
    },
    schema_v2::{checkpoints, epochs, transactions},
    PgConnectionPoolConfig,
};
use sui_json_rpc_types::SuiTransactionBlockEffects;
use sui_sdk::types::{
    effects::TransactionEffects,
    transaction::{SenderSignedData, TransactionDataAPI},
};

use super::data_provider::DataProvider;
use crate::{
    error::Error,
    types::{
        address::Address,
        base64::Base64,
        big_int::BigInt,
        checkpoint::Checkpoint,
        date_time::DateTime,
        digest::Digest,
        epoch::Epoch,
        gas::{GasCostSummary, GasInput},
        sui_address::SuiAddress,
        transaction_block::{TransactionBlock, TransactionBlockEffects},
    },
};

pub(crate) struct PgManager {
    pub inner: IndexerReader,
}

impl PgManager {
    pub(crate) fn new<T: Into<String>>(
        db_url: T,
        config: Option<PgConnectionPoolConfig>,
    ) -> Result<Self, Error> {
        // TODO (wlmyng): support config
        let mut config = config.unwrap_or(PgConnectionPoolConfig::default());
        config.set_pool_size(30);
        let inner = IndexerReader::new_with_config(db_url, config)
            .map_err(|e| Error::Internal(e.to_string()))?;

        Ok(Self { inner })
    }

    pub async fn run_query_async<T, E, F>(&self, query: F) -> Result<T, Error>
    where
        F: FnOnce(&mut PgConnection) -> Result<T, E> + Send + 'static,
        E: From<diesel::result::Error> + std::error::Error + Send + 'static,
        T: Send + 'static,
    {
        self.inner
            .run_query_async(query)
            .await
            .map_err(|e| Error::Internal(e.to_string()))
    }

    pub(crate) async fn fetch_tx(
        &self,
        digest: Vec<u8>,
    ) -> Result<Option<StoredTransaction>, Error> {
        self.run_query_async(|conn| {
            transactions::dsl::transactions
                .filter(transactions::dsl::transaction_digest.eq(digest))
                .get_result::<StoredTransaction>(conn) // Expect exactly 0 to 1 result
                .optional()
        })
        .await
    }

    pub(crate) async fn fetch_latest_epoch(&self) -> Result<StoredEpochInfo, Error> {
        self.run_query_async(|conn| {
            epochs::dsl::epochs
                .order_by(epochs::dsl::epoch.desc())
                .limit(1)
                .first::<StoredEpochInfo>(conn)
        })
        .await
    }

    pub(crate) async fn fetch_epoch(
        &self,
        epoch_id: i64,
    ) -> Result<Option<StoredEpochInfo>, Error> {
        self.run_query_async(move |conn| {
            epochs::dsl::epochs
                .filter(epochs::dsl::epoch.eq(epoch_id))
                .get_result::<StoredEpochInfo>(conn) // Expect exactly 0 to 1 result
                .optional()
        })
        .await
    }

    pub(crate) async fn fetch_latest_checkpoint(&self) -> Result<StoredCheckpoint, Error> {
        self.run_query_async(|conn| {
            checkpoints::dsl::checkpoints
                .order_by(checkpoints::dsl::sequence_number.desc())
                .limit(1)
                .first::<StoredCheckpoint>(conn)
        })
        .await
    }

    pub(crate) async fn fetch_checkpoint(
        &self,
        digest: Option<Vec<u8>>,
        sequence_number: Option<u64>,
    ) -> Result<Option<StoredCheckpoint>, Error> {
        let mut query = checkpoints::dsl::checkpoints.into_boxed();

        match (digest, sequence_number) {
            (Some(digest), None) => {
                query = query.filter(checkpoints::dsl::checkpoint_digest.eq(digest));
            }
            (None, Some(sequence_number)) => {
                query = query.filter(checkpoints::dsl::sequence_number.eq(sequence_number as i64));
            }
            _ => (), // No-op if invalid input
        }

        self.run_query_async(|conn| query.get_result::<StoredCheckpoint>(conn).optional())
            .await
    }
}

impl TryFrom<StoredCheckpoint> for Checkpoint {
    type Error = Error;
    fn try_from(c: StoredCheckpoint) -> Result<Self, Self::Error> {
        Ok(Self {
            digest: Digest::try_from(c.checkpoint_digest)?.to_string(),
            sequence_number: c.sequence_number as u64,
            validator_signature: Some(c.validator_signature.into()),
            previous_checkpoint_digest: c
                .previous_checkpoint_digest
                .map(|d| Digest::try_from(d).map(|digest| digest.to_string()))
                .transpose()?,
            live_object_set_digest: None,
            network_total_transactions: Some(c.network_total_transactions as u64),
            rolling_gas_summary: Some(GasCostSummary {
                computation_cost: c.computation_cost as u64,
                storage_cost: c.storage_cost as u64,
                storage_rebate: c.storage_rebate as u64,
                non_refundable_storage_fee: c.non_refundable_storage_fee as u64,
            }),
            epoch: Epoch::new(c.epoch as u64),
            end_of_epoch: None,
        })
    }
}

impl From<StoredEpochInfo> for Epoch {
    fn from(e: StoredEpochInfo) -> Self {
        Self {
            epoch_id: e.epoch as u64,
            system_state_version: None,
            protocol_configs: None,
            reference_gas_price: Some(BigInt::from(e.reference_gas_price as u64)),
            system_parameters: None,
            stake_subsidy: None,
            validator_set: None,
            storage_fund: None,
            safe_mode: None,
            start_timestamp: DateTime::from_ms(e.epoch_start_timestamp),
        }
    }
}

impl TryFrom<StoredTransaction> for TransactionBlock {
    type Error = Error;

    fn try_from(tx: StoredTransaction) -> Result<Self, Self::Error> {
        // TODO (wlmyng): Split the below into resolver methods
        let digest = Digest::try_from(tx.transaction_digest.as_slice())?;

        let sender_signed_data: SenderSignedData =
            bcs::from_bytes(&tx.raw_transaction).map_err(|e| {
                Error::Internal(format!(
                    "Can't convert raw_transaction into SenderSignedData. Error: {e}",
                ))
            })?;

        let sender = Address {
            address: SuiAddress::from_array(
                sender_signed_data
                    .intent_message()
                    .value
                    .sender()
                    .to_inner(),
            ),
        };

        let gas_input = GasInput::from(sender_signed_data.intent_message().value.gas_data());
        let effects: TransactionEffects = bcs::from_bytes(&tx.raw_effects).map_err(|e| {
            Error::Internal(format!(
                "Can't convert raw_effects into TransactionEffects. Error: {e}",
            ))
        })?;
        let effects = match SuiTransactionBlockEffects::try_from(effects) {
            Ok(effects) => Ok(Some(TransactionBlockEffects::from(
                &effects,
                Some(tx.checkpoint_sequence_number as u64),
            ))),
            Err(e) => Err(Error::Internal(format!(
                "Can't convert TransactionEffects into SuiTransactionBlockEffects. Error: {e}",
            ))),
        }?;

        Ok(Self {
            digest: digest.to_string(),
            effects,
            sender: Some(sender),
            bcs: Some(Base64::from(&tx.raw_transaction)),
            gas_input: Some(gas_input),
            checkpoint_sequence_number: Some(tx.checkpoint_sequence_number as u64),
        })
    }
}

#[async_trait]
impl DataProvider for PgManager {
    async fn fetch_tx(&self, digest: Vec<u8>) -> GqlResult<Option<TransactionBlock>> {
        let result = self.fetch_tx(digest).await?;
        match result {
            Some(tx) => Ok(Some(TransactionBlock::try_from(tx)?)),
            None => Ok(None),
        }
    }

    async fn fetch_latest_epoch(&self) -> GqlResult<Epoch> {
        let result = self.fetch_latest_epoch().await?;
        Ok(Epoch::from(result))
    }

    async fn fetch_epoch(&self, epoch_id: u64) -> GqlResult<Option<Epoch>> {
        let result = self.fetch_epoch(epoch_id as i64).await?;
        match result {
            Some(epoch) => Ok(Some(Epoch::from(epoch))),
            None => Ok(None),
        }
    }

    async fn fetch_latest_checkpoint(&self) -> GqlResult<Checkpoint> {
        let result = self.fetch_latest_checkpoint().await?;
        Ok(Checkpoint::try_from(result)?)
    }

    async fn fetch_checkpoint(
        &self,
        digest: Option<Vec<u8>>,
        sequence_number: Option<u64>,
    ) -> GqlResult<Option<Checkpoint>> {
        let result = self.fetch_checkpoint(digest, sequence_number).await?;
        match result {
            Some(checkpoint) => Ok(Some(Checkpoint::try_from(checkpoint)?)),
            None => Ok(None),
        }
    }
}
